import unittest
from source_assembly import assemble

PUBLIC='event ePut: (id: string, accepted: bool);\nevent eSnapshot: (items: seq[int]);\n'

class SourceAssemblyTests(unittest.TestCase):
    def test_legal_types_before_and_events_after_public_schema(self):
        candidate='type Memo = (accepted: bool, previous: int);\ntype Record = (memos: map[string, Memo], q: seq[int]);\n'+PUBLIC+'\nevent eLoad: machine;\nevent eDone;\n'
        self.assertEqual(assemble(PUBLIC,candidate),candidate)
    def test_original_and_no_candidate(self):
        self.assertEqual(assemble(PUBLIC,PUBLIC),PUBLIC)
        self.assertEqual(assemble(PUBLIC,None),PUBLIC)
    def test_conflicting_or_duplicate_declarations(self):
        for extra in ['event ePut;','type eSnapshot = int;','type Memo = int; type Memo = bool;']:
            with self.subTest(extra=extra),self.assertRaises(ValueError):assemble(PUBLIC,PUBLIC+extra)
    def test_changed_or_repeated_public_schema(self):
        for candidate in [PUBLIC.replace('accepted: bool','accepted: int'),PUBLIC+PUBLIC]:
            with self.subTest(candidate=candidate),self.assertRaises(ValueError):assemble(PUBLIC,candidate)
    def test_executable_code_and_suppressions_rejected(self):
        for extra in ['machine Rogue {}','spec Ignore observes ePut {}','fun evade() {}','type Evil = int; assert true;','type Evil = (x: int) { assert true; };','// snappea-ignore lint\nevent eDone;','event eDone: bool = true;']:
            with self.subTest(extra=extra),self.assertRaises(ValueError):assemble(PUBLIC,PUBLIC+extra)
    def test_invalid_type_syntax_rejected(self):
        for extra in ['type Bad = map[int];','type Bad = seq[int, bool];','type Bad = foo[int];','type Bad = ;','event eDone: (x: int;']:
            with self.subTest(extra=extra),self.assertRaises(ValueError):assemble(PUBLIC,PUBLIC+extra)

if __name__=='__main__':unittest.main()
