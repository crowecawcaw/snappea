#!/bin/sh
# start.sh PREFIX TASK MODE N [LABEL BINARY [CHECK_FLAGS]]: start N runs per arm (or N runs of one snappea variant).
# Prints the run ids; give each solver `runs/<id>/prompt.txt`.
set -e
cd "$(dirname "$0")"
prefix=$1 task=$2 mode=$3 n=$4 label=$5 binary=$6 flags=$7
V=../../target/variants
for i in $(seq -w 1 "$n"); do
  if [ -n "$label" ]; then
    id=$prefix-$label-0$i
    python3 harness.py start --mode "$mode" --task "$task" --arm snappea --run-id "$id" --model haiku \
      --label "$label" --binary "$binary" ${flags:+--check-flags="$flags"} >/dev/null && echo "$id"
  else
    for arm in snappea compat; do
      id=$prefix-$arm-0$i
      python3 harness.py start --mode "$mode" --task "$task" --arm $arm --run-id "$id" --model haiku \
        --binary $V/$([ $arm = snappea ] && echo snappea-base || echo compat-base) >/dev/null && echo "$id"
    done
  fi
done
