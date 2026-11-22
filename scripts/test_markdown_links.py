"""Relative links in the repository's Markdown point at files and headings that exist."""
import os
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent
SKIP = {'target', 'node_modules', '.git', 'evals'}
# [text](target), not images' remote URLs; the target ends at whitespace or ')'.
LINK = re.compile(r'(?<!\!)\[[^\]]*\]\(([^)\s]+)\)')
FENCE = re.compile(r'^(```|~~~).*?^\1', re.S | re.M)


def markdown_files():
    for top, dirs, files in os.walk(ROOT):
        dirs[:] = [d for d in dirs if d not in SKIP]
        for name in files:
            if name.endswith('.md'):
                yield Path(top) / name


def slug(heading):
    """GitHub's anchor for a heading."""
    heading = re.sub(r'[`*_]|\[([^\]]*)\]\([^)]*\)', r'\1', heading.strip().lower())
    return re.sub(r'[^\w\- ]', '', heading).replace(' ', '-')


def anchors(path):
    text = FENCE.sub('', path.read_text(errors='replace'))
    found, seen = set(), {}
    for m in re.finditer(r'^#{1,6}\s+(.*?)\s*#*$', text, re.M):
        s = slug(m[1])
        n = seen.get(s, 0)
        seen[s] = n + 1
        found.add(s if n == 0 else f'{s}-{n}')
    found.update(re.findall(r'<a (?:name|id)="([^"]+)"', text))
    return found


def broken_links():
    out, cache = [], {}
    for path in markdown_files():
        text = FENCE.sub('', path.read_text(errors='replace'))
        for m in LINK.finditer(text):
            target = m[1].strip('<>')
            if re.match(r'^[a-z][a-z0-9+.-]*:', target, re.I):
                continue
            file, _, anchor = target.partition('#')
            dest = (path.parent / file).resolve() if file else path
            rel = path.relative_to(ROOT).as_posix()
            if not dest.exists():
                out.append(f'{rel}: {target}: no such file')
            elif anchor and dest.suffix == '.md':
                if dest not in cache:
                    cache[dest] = anchors(dest)
                if anchor.lower() not in cache[dest]:
                    out.append(f'{rel}: {target}: no such heading')
    return out


class LinkTests(unittest.TestCase):
    def test_relative_links_resolve(self):
        self.assertEqual(broken_links(), [])

    def test_anchors_follow_github(self):
        self.assertEqual(slug('Observed `monitor` coverage!'), 'observed-monitor-coverage')
        self.assertEqual(slug('What fails a check'), 'what-fails-a-check')


if __name__ == '__main__':
    unittest.main()
