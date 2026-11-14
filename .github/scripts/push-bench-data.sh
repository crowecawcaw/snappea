#!/usr/bin/env bash
# Usage: push-bench-data.sh <append|snapshot> <src-file> <dest-path> <commit message>
#
# Appends compact JSON to a JSONL history, or stores one gzip JSON snapshot, on
# the `benchmarks` branch. Retries on push races with other runs.
set -euo pipefail

case "$1" in append|snapshot) ;; *) echo "unknown mode: $1" >&2; exit 2 ;; esac
mode="$1"
writer="$(cd "$(dirname "$0")" && pwd)/store-bench-result.py"
src="$(realpath "$2")"
dest="$3"
msg="$4"

repo_dir="$(pwd)"
remote="$(git config --get remote.origin.url)"
work="$(mktemp -d)"
cd "$work"
git init -q
git remote add origin "$remote"
# Reuse the checkout's credentials (actions/checkout stores an auth header).
auth="$(git -C "$repo_dir" config --get http.https://github.com/.extraheader || true)"
if [ -n "$auth" ]; then git config http.https://github.com/.extraheader "$auth"; fi
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

for attempt in 1 2 3 4 5; do
  if git fetch -q --depth 1 origin benchmarks; then
    git checkout -q -B benchmarks FETCH_HEAD
  else
    git checkout -q --orphan benchmarks
    git rm -rqf . 2>/dev/null || true
    printf '# Benchmark data\n\nWritten by the Benchmarks workflows; see scripts/bench.py on main.\n' > README.md
  fi

  python3 "$writer" "$mode" "$src" "$dest"

  git add -A
  if git diff --cached --quiet; then
    echo "Already recorded: $msg"
    exit 0
  fi
  git commit -qm "$msg"
  if git push -q origin benchmarks; then
    echo "Pushed: $msg"
    exit 0
  fi
  echo "Push failed (attempt $attempt); retrying."
  sleep $((attempt * 3))
done
echo "Could not push to benchmarks." >&2
exit 1
