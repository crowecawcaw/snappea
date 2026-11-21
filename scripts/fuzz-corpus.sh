#!/usr/bin/env bash
# Seeds fuzz/corpus/<target>/ for cargo-fuzz from the repository's own inputs: every P file
# under tests/, examples/ and the built-in guide for the P targets, and fuzz/seeds/observe/
# plus the guide's logs for `observe`. Existing corpus entries are kept.
#
# Usage: scripts/fuzz-corpus.sh
set -euo pipefail
cd "$(dirname "$0")/.."

for target in parse compile check observe; do
  mkdir -p "fuzz/corpus/$target"
done

# P sources: small files only, so each input stays fast
find tests examples crates/snappea/src/cli/guide -name '*.p' -size -16k -print0 |
  while IFS= read -r -d '' f; do
    name=$(sha1sum "$f" | cut -c1-16)
    for target in parse compile check; do
      cp "$f" "fuzz/corpus/$target/$name"
    done
  done

# observe: a mode byte (see snappea_fuzz::observe), then the log
seed_log() {
  local mode=$1 file=$2
  local name
  name=$(printf '%s-%s' "$mode" "$(sha1sum "$file" | cut -c1-16)")
  { printf "\\x$(printf '%02x' "$mode")"; cat "$file"; } > "fuzz/corpus/observe/$name"
}
for f in fuzz/seeds/observe/*.jsonl; do
  for mode in 0 2 4 6 8 16; do seed_log "$mode" "$f"; done
done
for f in fuzz/seeds/observe/*.json; do
  for mode in 1 3 5; do seed_log "$mode" "$f"; done
done
# other models' logs: unknown events, as warnings
find crates/snappea/src/cli/guide -name '*.jsonl' -size -16k -print0 |
  while IFS= read -r -d '' f; do seed_log 8 "$f"; done

for target in parse compile check observe; do
  echo "$target: $(find "fuzz/corpus/$target" -type f | wc -l) inputs"
done
