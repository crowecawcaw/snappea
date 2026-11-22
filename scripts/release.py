#!/usr/bin/env python3
"""Versions and binary archives for the release workflow (.github/workflows/release.yml).

  scripts/release.py next LEVEL             print the version a release would get
                                            (LEVEL: patch, minor, major or release)
  scripts/release.py set VERSION [--sums F] write VERSION into every package and the
                                            docs that pin it; with --sums, also Go's
                                            committed SHA256SUMS
  scripts/release.py docs                   check that the docs pin Cargo.toml's version
  scripts/release.py dev                    between releases: Go "dev" with no
                                            checksums, Java <next patch>-SNAPSHOT
  scripts/release.py package TARGET VERSION BINARY OUT
                                            archive BINARY as OUT/snappea-vVERSION-TARGET
                                            (.tar.gz, or .zip for Windows)
  scripts/release.py sums DIR VERSION       write DIR/SHA256SUMS for the five archives
  scripts/release.py verify DIR [SUMS]      check DIR's archives against SUMS
                                            (default DIR/SHA256SUMS)
  scripts/release.py natives DIR OUT        verify DIR's archives and extract each
                                            binary to OUT/natives-<os>-<arch>/, as
                                            java/pom.xml's natives profile expects

Rust (Cargo.toml) keeps the release version between releases; Go and Java go back to
development versions so a checkout never poses as a release. Standard library only;
runs on every release runner, Windows included.
"""

import hashlib
import io
import os
import pathlib
import re
import sys
import tarfile
import time
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
CARGO = ROOT / "Cargo.toml"
POM = ROOT / "java" / "pom.xml"
EXAMPLE_POM = ROOT / "examples" / "java" / "pom.xml"
GO_VERSION = ROOT / "go" / "internal" / "snappeabin" / "version.go"
GO_SUMS = ROOT / "go" / "internal" / "snappeabin" / "SHA256SUMS"

# Release target -> Java natives classifier. Go's snappeabin.targets lists the same five.
TARGETS = {
    "x86_64-unknown-linux-gnu": "natives-linux-x86_64",
    "aarch64-unknown-linux-gnu": "natives-linux-aarch64",
    "x86_64-apple-darwin": "natives-macos-x86_64",
    "aarch64-apple-darwin": "natives-macos-aarch64",
    "x86_64-pc-windows-msvc": "natives-windows-x86_64",
}

VERSION = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")
CARGO_VERSION = re.compile(r'^version = "([^"]*)"$', re.M)  # first one: [workspace.package]
POM_VERSION = re.compile(r"(<artifactId>snappea</artifactId>\s*<version>)([^<]*)(</version>)")
EXAMPLE_VERSION = re.compile(r"(<snappea\.version>)([^<]*)(</snappea\.version>)")
GO_CONST = re.compile(r'^(const Version = ")([^"]*)(")$', re.M)

# Version pins in docs and examples: Cargo dependencies name major.minor, install
# commands X.Y.Z. Each pattern's second group is the version.
DOC_PINS = [
    (re.compile(r'(snappea = (?:\{ ?version = )?")(\d+\.\d+)(")'), "minor"),
    (re.compile(r"(--version )(\d+\.\d+\.\d+)()"), "full"),
    (re.compile(r"(SNAPPEA_VERSION: ?)(\d+\.\d+\.\d+)()"), "full"),
]
DOC_SUFFIXES = {".md", ".rs", ".tsx", ".yml", ".sh", ".ps1"}
# Pins of older releases on purpose: the install test and recorded eval bundles.
DOC_EXEMPT = ("target", "node_modules", ".git", ".github/workflows/install-script.yml",
              "evals/certified-bundles")


def fail(message):
    sys.exit(f"release.py: {message}")


def parse(version):
    m = VERSION.match(version)
    if not m:
        fail(f"{version!r} is not a plain X.Y.Z version")
    return tuple(map(int, m.groups()))


def cargo_version():
    m = CARGO_VERSION.search(CARGO.read_text())
    if not m:
        fail("no version in Cargo.toml")
    parse(m.group(1))
    return m.group(1)


def replace(path, pattern, value):
    text = path.read_text()
    new, n = pattern.subn(lambda m: m.group(1) + value + m.group(3), text, count=1)
    if n != 1:
        fail(f"no version to replace in {path.relative_to(ROOT)}")
    # newline="" keeps the file's line endings on Windows runners.
    path.write_text(new, newline="")


def doc_files():
    """The files DOC_PINS applies to: docs, examples and sources, outside DOC_EXEMPT."""
    def exempt(rel):
        return any(rel == e or rel.startswith(e + "/") or rel.endswith("/" + e) for e in DOC_EXEMPT)

    for top, dirs, files in os.walk(ROOT):
        base = pathlib.Path(top)
        dirs[:] = sorted(d for d in dirs if not exempt((base / d).relative_to(ROOT).as_posix()))
        for name in sorted(files):
            path = base / name
            if path.suffix in DOC_SUFFIXES and not exempt(path.relative_to(ROOT).as_posix()):
                yield path


def doc_pin(version, kind):
    major, minor, _ = parse(version)
    return f"{major}.{minor}" if kind == "minor" else version


def set_doc_pins(text, version):
    for pattern, kind in DOC_PINS:
        text = pattern.sub(lambda m: m[1] + doc_pin(version, kind) + m[3], text)
    return text


def doc_pin_mismatches(version):
    """`file:line: pin` for every doc pin that names another version."""
    out = []
    for path in doc_files():
        text = path.read_text(errors="replace")
        for pattern, kind in DOC_PINS:
            for m in pattern.finditer(text):
                if m[2] != doc_pin(version, kind):
                    line = text.count("\n", 0, m.start()) + 1
                    out.append(f"{path.relative_to(ROOT).as_posix()}:{line}: {m[0]}")
    return out


def asset(target, version):
    ext = ".zip" if "windows" in target else ".tar.gz"
    return f"snappea-v{version}-{target}{ext}"


def exe(target):
    return "snappea.exe" if "windows" in target else "snappea"


def cmd_next(level):
    major, minor, patch = parse(cargo_version())
    if level == "major":
        major, minor, patch = major + 1, 0, 0
    elif level == "minor":
        minor, patch = minor + 1, 0
    elif level == "patch":
        patch += 1
    elif level != "release":
        fail(f"unknown level {level!r}")
    print(f"{major}.{minor}.{patch}")


def cmd_set(version, sums=None):
    parse(version)
    text = CARGO.read_text()
    m = CARGO_VERSION.search(text)
    text = text[: m.start(1)] + version + text[m.end(1) :]
    # Workspace crates share a release version, including inter-crate requirements.
    text = re.sub(r'^(snappea-(?:syntax|lint) = .*version = ")[^"]*(")',
                  lambda m: m[1] + version + m[2], text, flags=re.M)
    CARGO.write_text(text, newline="")
    replace(POM, POM_VERSION, version)
    replace(EXAMPLE_POM, EXAMPLE_VERSION, version)
    replace(GO_VERSION, GO_CONST, version)
    for path in doc_files():
        text = path.read_text(errors="replace")
        new = set_doc_pins(text, version)
        if new != text:
            path.write_text(new, newline="")
    if sums is not None:
        text = pathlib.Path(sums).read_text()
        check_sums(text, version)
        GO_SUMS.write_text(text, newline="")


def cmd_docs():
    version = cargo_version()
    wrong = doc_pin_mismatches(version)
    if wrong:
        fail(f"these pins differ from Cargo.toml's {version}; run "
             f"`scripts/release.py set {version}`:\n  " + "\n  ".join(wrong))


def cmd_dev():
    major, minor, patch = parse(cargo_version())
    snapshot = f"{major}.{minor}.{patch + 1}-SNAPSHOT"
    replace(POM, POM_VERSION, snapshot)
    replace(EXAMPLE_POM, EXAMPLE_VERSION, snapshot)
    replace(GO_VERSION, GO_CONST, "dev")
    GO_SUMS.write_text("")


def cmd_package(target, version, binary, out):
    if target not in TARGETS:
        fail(f"unknown target {target!r}")
    parse(version)
    data = pathlib.Path(binary).read_bytes()
    out = pathlib.Path(out)
    out.mkdir(parents=True, exist_ok=True)
    name = asset(target, version)
    folder = f"snappea-v{version}-{target}"
    files = [(exe(target), data, 0o755)]
    for extra in ("LICENSE-MIT", "LICENSE-APACHE"):
        files.append((extra, (ROOT / extra).read_bytes(), 0o644))
    path = out / name
    now = int(time.time())
    if name.endswith(".zip"):
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
            for file, body, mode in files:
                info = zipfile.ZipInfo(f"{folder}/{file}", time.localtime(now)[:6])
                info.external_attr = (0o100000 | mode) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                zf.writestr(info, body)
    else:
        with tarfile.open(path, "w:gz") as tf:
            for file, body, mode in files:
                info = tarfile.TarInfo(f"{folder}/{file}")
                info.size, info.mode, info.mtime = len(body), mode, now
                tf.addfile(info, io.BytesIO(body))
    print(path)


def sha256(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def check_sums(text, version):
    """SHA256SUMS lists exactly the five archives of version."""
    names = sorted(line.split()[1] for line in text.splitlines() if line.strip())
    want = sorted(asset(t, version) for t in TARGETS)
    if names != want:
        fail(f"SHA256SUMS lists {names}, want {want}")


def cmd_sums(directory, version):
    directory = pathlib.Path(directory)
    lines = []
    for target in TARGETS:
        path = directory / asset(target, version)
        if not path.is_file():
            fail(f"missing {path}")
        lines.append(f"{sha256(path)}  {path.name}\n")
    text = "".join(sorted(lines, key=lambda line: line.split()[1]))
    (directory / "SHA256SUMS").write_text(text, newline="")
    print(text, end="")


def cmd_verify(directory, sums=None):
    directory = pathlib.Path(directory)
    text = pathlib.Path(sums or directory / "SHA256SUMS").read_text()
    checked = []
    for line in text.splitlines():
        if not line.strip():
            continue
        want, name = line.split()
        name = name.lstrip("*")
        path = directory / name
        if not path.is_file():
            fail(f"missing {path}")
        if sha256(path) != want:
            fail(f"{name}: sha256 {sha256(path)}, SHA256SUMS says {want}")
        checked.append(name)
    if not checked:
        fail("SHA256SUMS is empty")
    return checked


def cmd_natives(directory, out):
    directory, out = pathlib.Path(directory), pathlib.Path(out)
    names = cmd_verify(directory)
    for target, classifier in TARGETS.items():
        name = next((n for n in names if n.endswith(f"-{target}.zip") or n.endswith(f"-{target}.tar.gz")), None)
        if name is None:
            fail(f"no archive for {target}")
        body = None
        if name.endswith(".zip"):
            with zipfile.ZipFile(directory / name) as zf:
                for info in zf.infolist():
                    if not info.is_dir() and info.filename.rsplit("/", 1)[-1] == exe(target):
                        body = zf.read(info)
        else:
            with tarfile.open(directory / name, "r:gz") as tf:
                for member in tf.getmembers():
                    if member.isfile() and member.name.rsplit("/", 1)[-1] == exe(target):
                        body = tf.extractfile(member).read()
        if body is None:
            fail(f"{name} holds no {exe(target)}")
        dest = out / classifier / exe(target)
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(body)
        dest.chmod(0o755)
        print(dest)


def main(argv):
    commands = {
        "next": (cmd_next, 1, 1),
        "set": (None, 1, 3),
        "dev": (cmd_dev, 0, 0),
        "docs": (cmd_docs, 0, 0),
        "package": (cmd_package, 4, 4),
        "sums": (cmd_sums, 2, 2),
        "verify": (cmd_verify, 1, 2),
        "natives": (cmd_natives, 2, 2),
    }
    if not argv or argv[0] not in commands:
        sys.exit(__doc__)
    name, args = argv[0], argv[1:]
    if name == "set":
        if len(args) == 1:
            return cmd_set(args[0])
        if len(args) == 3 and args[1] == "--sums":
            return cmd_set(args[0], args[2])
        sys.exit(__doc__)
    func, lo, hi = commands[name]
    if not lo <= len(args) <= hi:
        sys.exit(__doc__)
    func(*args)


if __name__ == "__main__":
    main(sys.argv[1:])
