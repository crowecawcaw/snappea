#!/usr/bin/env python3
"""Freeze a P modeling skill tree for repeatable skill-treatment trials."""
import argparse
from pathlib import Path
from skill_snapshot import create_snapshot


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True,
                        help='directory containing SKILL.md and examples/')
    args = parser.parse_args()
    path, manifest = create_snapshot(args.source)
    print(f'SNAPSHOT={path}\nTREE_SHA256={manifest["tree_sha256"]}')


if __name__ == '__main__':
    main()
