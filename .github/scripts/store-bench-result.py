#!/usr/bin/env python3
"""Store one benchmark result as an append-only JSONL row or a compressed snapshot."""
import gzip
import json
from pathlib import Path
import sys


def store(mode: str, source: Path, destination: Path) -> None:
    if mode not in ('append', 'snapshot'):
        raise ValueError(f'unknown storage mode: {mode}')
    row = (json.dumps(json.loads(source.read_text()), separators=(',', ':')) + '\n').encode()
    destination.parent.mkdir(parents=True, exist_ok=True)
    if mode == 'append':
        with destination.open('ab') as output:
            output.write(row)
    else:
        # One file per workflow run: corpus history cannot grow one Git blob past
        # GitHub's 100 MiB limit. Fixed mtime keeps retries byte-identical.
        destination.write_bytes(gzip.compress(row, mtime=0))


if __name__ == '__main__':
    store(sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3]))
