"""Docs and examples pin the release version that Cargo.toml holds."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('release', ROOT / 'scripts/release.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class DocPinTests(unittest.TestCase):
    def test_docs_pin_the_cargo_version(self):
        wrong = MODULE.doc_pin_mismatches(MODULE.cargo_version())
        self.assertEqual(wrong, [], 'run `scripts/release.py set <Cargo.toml version>`')

    def test_set_rewrites_cargo_dependencies_and_install_pins(self):
        text = (
            'snappea = "0.1"\n'
            'snappea = { version = "0.1", default-features = false, features = ["test"] }\n'
            'sh -s -- --version 0.1.0\n'
            'SNAPPEA_VERSION: 0.1.0\n'
            'snappea-syntax = { version = "0.1.0" }\n'
        )
        self.assertEqual(
            MODULE.set_doc_pins(text, '1.4.2'),
            'snappea = "1.4"\n'
            'snappea = { version = "1.4", default-features = false, features = ["test"] }\n'
            'sh -s -- --version 1.4.2\n'
            'SNAPPEA_VERSION: 1.4.2\n'
            'snappea-syntax = { version = "0.1.0" }\n',
        )

    def test_older_pins_kept_on_purpose_are_exempt(self):
        files = {p.relative_to(ROOT).as_posix() for p in MODULE.doc_files()}
        self.assertNotIn('.github/workflows/install-script.yml', files)
        self.assertIn('crates/snappea/src/lib.rs', files)


if __name__ == '__main__':
    unittest.main()
