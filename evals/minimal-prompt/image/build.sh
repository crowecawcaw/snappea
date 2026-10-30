#!/bin/bash
# Build the solver/grader image: pytest wheels plus a pure-Python pysnappea wheel made
# from this checkout's python/src/snappea (the snappea binary is mounted at run time).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
build="$(mktemp -d)"
trap 'rm -rf "$build"' EXIT
mkdir -p "$here/wheels"
pip download -q --only-binary=:all: --python-version 3.12 --dest "$here/wheels" pytest==8.4.2
cp -r "$root/python/src/snappea" "$build/"
cat > "$build/pyproject.toml" <<'TOML'
[build-system]
requires = ["setuptools"]
build-backend = "setuptools.build_meta"
[project]
name = "pysnappea"
version = "0.1.0"
[tool.setuptools]
packages = ["snappea"]
[tool.setuptools.package-data]
snappea = ["py.typed"]
TOML
pip wheel -q --no-deps "$build" -w "$here/wheels"
docker build -q -t snappea-min-eval "$here"
