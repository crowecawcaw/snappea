#!/bin/bash
# Grade every frozen run (has submission/) that has no grade.json yet. Runs until killed.
cd "$(dirname "$0")"
export SNAPPEA_EVAL_BINARY=${SNAPPEA_EVAL_BINARY:-$PWD/bin/snappea-c2}
while true; do
  for d in runs/*/; do
    id=$(basename "$d")
    if [ -d "$d/submission" ] && [ ! -f "$d/grade.json" ] && [ ! -f "$d/.grading" ]; then
      touch "$d/.grading"; python3 harness.py grade "$id"; rm -f "$d/.grading"
    fi
  done
  sleep 20
done
