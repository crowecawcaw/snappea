#!/bin/bash
# Score transplanted defects for runs whose helper wrote mutants and whose base grade exists.
cd "$(dirname "$0")"
while true; do
  for d in runs/*/; do
    id=$(basename "$d")
    if ls "$d"/transplant/mutant_* >/dev/null 2>&1 && [ -f "$d/grade.json" ] && [ ! -f "$d/.grading" ] \
       && [ ! -f "$d/.transplant-failed" ] && ! grep -q '"transplant"' "$d/grade.json"; then
      python3 harness.py grade_transplant "$id" || touch "$d/.transplant-failed"  # do not retry forever
    fi
  done
  sleep 30
done
