"""Create and validate content-addressed, read-only modeling-skill snapshots."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SNAPSHOT_ROOT = ROOT / 'runs' / 'skill-snapshots'
GENERATED_DIRS = {'__pycache__', 'PCheckerOutput', 'coverage', 'htmlcov'}
GENERATED_FILES = {'PLAN.md', 'snapshot.json', '.coverage', 'coverage.xml'}


def file_manifest(source):
    source = Path(source).resolve()
    if not (source / 'SKILL.md').is_file():
        raise ValueError(f'{source} must contain SKILL.md')
    files = {}
    for path in sorted(source.rglob('*')):
        if path.is_symlink():
            raise ValueError(f'symlinks are not allowed in a skill snapshot: {path}')
        if path.is_file():
            rel = path.relative_to(source).as_posix()
            parts = Path(rel).parts
            if (any(part in GENERATED_DIRS for part in parts)
                    or path.name in GENERATED_FILES
                    or path.suffix in {'.pyc', '.log'}):
                continue
            files[rel] = hashlib.sha256(path.read_bytes()).hexdigest()
    if not any(name.startswith('examples/') for name in files):
        raise ValueError(f'{source} must include at least one file under examples/')
    digest_input = ''.join(f'{name}\0{digest}\n' for name, digest in files.items()).encode()
    return source, files, hashlib.sha256(digest_input).hexdigest()


def verify_snapshot(snapshot):
    snapshot = Path(snapshot).resolve()
    manifest_path = snapshot / 'snapshot.json'
    if not manifest_path.is_file():
        raise ValueError(f'{snapshot} has no snapshot.json')
    saved = json.loads(manifest_path.read_text())
    files = {}
    for path in sorted(snapshot.rglob('*')):
        if path.is_symlink():
            raise ValueError(f'symlink found in skill snapshot: {path}')
        if path.is_file() and path.name != 'snapshot.json':
            rel = path.relative_to(snapshot).as_posix()
            parts = Path(rel).parts
            if (any(part in GENERATED_DIRS for part in parts)
                    or path.name in GENERATED_FILES
                    or path.suffix in {'.pyc', '.log'}):
                continue
            files[rel] = hashlib.sha256(path.read_bytes()).hexdigest()
    digest_input = ''.join(f'{name}\0{digest}\n' for name, digest in files.items()).encode()
    digest = hashlib.sha256(digest_input).hexdigest()
    if files != saved.get('files') or digest != saved.get('tree_sha256'):
        raise ValueError(f'skill snapshot hash mismatch: {snapshot}')
    if 'SKILL.md' not in files or not any(name.startswith('examples/') for name in files):
        raise ValueError(f'{snapshot} must contain SKILL.md and examples/')
    return saved


def create_snapshot(source, snapshot_root=None):
    source, files, digest = file_manifest(source)
    root = Path(snapshot_root) if snapshot_root is not None else SNAPSHOT_ROOT
    destination = root / digest
    root.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        saved = verify_snapshot(destination)
        if saved['tree_sha256'] != digest:
            raise ValueError(f'content-addressed snapshot collision: {destination}')
        return destination, saved

    staging = Path(tempfile.mkdtemp(prefix='.skill-snapshot-', dir=root))
    try:
        for rel in files:
            target = staging / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / rel, target)
            target.chmod(stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
        saved = {'tree_sha256': digest, 'files': files}
        manifest_path = staging / 'snapshot.json'
        manifest_path.write_text(json.dumps(saved, indent=2) + '\n')
        manifest_path.chmod(stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
        for directory in sorted((p for p in staging.rglob('*') if p.is_dir()), reverse=True):
            directory.chmod(stat.S_IRUSR | stat.S_IXUSR | stat.S_IRGRP | stat.S_IXGRP | stat.S_IROTH | stat.S_IXOTH)
        staging.chmod(stat.S_IRUSR | stat.S_IXUSR | stat.S_IRGRP | stat.S_IXGRP | stat.S_IROTH | stat.S_IXOTH)
        os.rename(staging, destination)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    return destination, saved
