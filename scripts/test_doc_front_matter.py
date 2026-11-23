"""Documentation pages start with the front matter AGENTS.md requires."""
from pathlib import Path
import re
import subprocess
import unittest

ROOT = Path(__file__).resolve().parent.parent
PAGES = [':(glob)docs/**/*.md', ':(glob)examples/**/*.md', ':(glob)crates/snappea/src/cli/guide/**/*.md']
TYPES = {'tutorial', 'how-to', 'explanation', 'reference'}
FRONT = re.compile(r'\A---\n(.*?)\n---\n', re.S)


def pages():
    out = subprocess.run(['git', 'ls-files', '--', *PAGES], cwd=ROOT, capture_output=True, text=True, check=True)
    return [ROOT / line for line in out.stdout.splitlines()]


class FrontMatter(unittest.TestCase):
    def test_pages_declare_type_personas_and_questions(self):
        self.assertTrue(pages(), 'no pages found')
        for path in pages():
            with self.subTest(path=str(path.relative_to(ROOT))):
                m = FRONT.match(path.read_text())
                self.assertIsNotNone(m, 'add YAML front matter: diataxis, personas, questions')
                fields = dict(re.findall(r'^(\w+):\s*(.*)$', m.group(1), re.M))
                self.assertIn(fields.get('diataxis'), TYPES)
                self.assertTrue(fields.get('personas'), 'personas is empty')
                self.assertIn('questions', fields)
                self.assertRegex(m.group(1), r'\nquestions:\s*\n\s+- ', 'list at least one question')


if __name__ == '__main__':
    unittest.main()
