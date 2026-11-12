#!/bin/sh
# Linker for the PGO-instrumented Linux release build (scripts/pgo.py, release.yml).
#
# rustc links the profiler runtime with `-u __llvm_profile_runtime`, which `zig cc`
# reads as an input file. This passes it as `-Wl,-u,...` and links as cargo-zigbuild
# does, for the glibc target in PGO_ZIG_TARGET (e.g. x86_64-linux-gnu.2.17). Use it as
# CARGO_TARGET_<TRIPLE>_LINKER for the instrumented `cargo zigbuild` only: the linker
# does not change symbol names, so the profile still matches the release build.
for arg; do
  shift
  if [ -n "$undefined" ]; then
    set -- "$@" "-Wl,-u,$arg"
    undefined=
  elif [ "$arg" = -u ]; then
    undefined=1
  else
    set -- "$@" "$arg"
  fi
done
exec cargo-zigbuild zig cc -- -g -fno-sanitize=all -target "${PGO_ZIG_TARGET:?set PGO_ZIG_TARGET, e.g. x86_64-linux-gnu.2.17}" "$@"
