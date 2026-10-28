"""Validate the narrow model-source assembly correction."""
import tempfile
import unittest
from pathlib import Path
from recheck_model_sources import corrected_events

class SourceAssemblyTests(unittest.TestCase):
    def assemble(self, original, candidate):
        with tempfile.TemporaryDirectory() as tmp:
            public=Path(tmp)/'public.p';public.write_text(original)
            submitted=Path(tmp)/'submitted.p';submitted.write_text(candidate)
            return corrected_events(public,submitted)

    def test_unchanged_public_schema(self):
        self.assertEqual(self.assemble('event eA;\n','event eA;\n'),'event eA;\n')

    def test_internal_fields_preserved(self):
        text='event eA;\nevent eLoad: machine;\nevent eCAS: (version: int, client: machine);\n'
        self.assertEqual(self.assemble('event eA;\n',text),text)

    def test_modified_schema_is_not_accepted(self):
        with self.assertRaises(ValueError): self.assemble('event eA: int;\n','event eA: bool;\n')

    def test_duplicate_public_event_is_not_accepted(self):
        with self.assertRaises(ValueError): self.assemble('event eA;\n','event eA;\nevent eA: int;')

    def test_non_event_code_is_not_accepted(self):
        with self.assertRaises(ValueError): self.assemble('event eA;\n','event eA;\nmachine Main {}')

if __name__=='__main__': unittest.main()
