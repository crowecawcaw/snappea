"""Links in the repository's Markdown point at files, headings and site anchors that exist."""
import subprocess
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent
# [text](target), not images' remote URLs; the target ends at whitespace or ')'.
LINK = re.compile(r'(?<!\!)\[[^\]]*\]\(([^)\s]+)\)')
FENCE = re.compile(r'^(```|~~~).*?^\1', re.S | re.M)
# The project site, under either of its hosts.
SITE = re.compile(r'https://(?:snappea\.dev|crowecawcaw\.github\.io/snappea)/docs/([\w-]+)\.html#([\w-]+)')
WEB = ROOT / 'web/src/site'


def markdown_files():
    # tracked files only: agent worktrees and build output hold stale copies
    out = subprocess.run(['git', 'ls-files', '-z', '--', '*.md', ':(exclude)evals'],
                         cwd=ROOT, capture_output=True, text=True, check=True).stdout
    return [ROOT / p for p in out.split('\0') if p]


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


def site_pages():
    """Docs page name -> its .tsx source, from the PAGES table web/src/site/render.tsx renders."""
    render = (WEB / 'render.tsx').read_text()
    files = {m[1]: m[2] for m in re.finditer(r'import \{ (\w+) \} from "\./([^"]+)"', render)}
    table = re.search(r'const PAGES = \{([^}]*)\}', render)[1]
    pages = {}
    for m in re.finditer(r'"?([\w-]+)"?: (\w+)', table):
        pages[m[1]] = WEB / f'{files[m[2]]}.tsx'
    return pages


def broken_site_anchors():
    out, pages, ids = [], site_pages(), {}
    for path in markdown_files():
        rel = path.relative_to(ROOT).as_posix()
        for m in SITE.finditer(path.read_text(errors='replace')):
            page, frag = m[1], m[2]
            if page not in pages:
                out.append(f'{rel}: {m[0]}: no such page')
                continue
            if page not in ids:
                ids[page] = set(re.findall(r'\bid="([^"]+)"', pages[page].read_text()))
            if frag not in ids[page]:
                out.append(f'{rel}: {m[0]}: no id="{frag}" in {pages[page].relative_to(ROOT)}')
    return out


class LinkTests(unittest.TestCase):
    def test_relative_links_resolve(self):
        self.assertEqual(broken_links(), [])

    def test_site_anchors_resolve(self):
        self.assertEqual(broken_site_anchors(), [])

    def test_site_pages_map_to_sources(self):
        pages = site_pages()
        self.assertEqual(pages['python'], WEB / 'docs/languages/Python.tsx')
        self.assertTrue(all(p.exists() for p in pages.values()), pages)

    def test_anchors_follow_github(self):
        self.assertEqual(slug('Observed `monitor` coverage!'), 'observed-monitor-coverage')
        self.assertEqual(slug('What fails a check'), 'what-fails-a-check')


if __name__ == '__main__':
    unittest.main()
