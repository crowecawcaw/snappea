#!/bin/bash
# Grade every frozen run (has submission/) that has no grade.json yet. Runs until killed.
cd "$(dirname "$0")"
while true; do
  for d in runs/*/; do
    id=$(basename "$d")
    if [ -d "$d/submission" ] && [ ! -f "$d/grade.json" ] && [ ! -f "$d/.grading" ]; then
      touch "$d/.grading"; trap 'rm -f "$d/.grading"' EXIT
      python3 harness.py grade "$id"; rm -f "$d/.grading"
    fi
  done
  sleep 20
done
