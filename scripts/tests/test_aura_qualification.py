"""Qualification must reject partial or stale evidence instead of reporting success."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from aura_qualification import exact_import, photo_success, separate_output


class QualificationTests(unittest.TestCase):
    def report(self):
        return dict(inputManifest=[{'file': 'one.CR3'}], photos=[{'file': 'one.CR3', 'passed': True}],
                    originalsUnchanged=True, pageErrors=[], fullExportRequested=True,
                    export=dict(requested=1, written=1, verified=1, unverified=0, corrupt=0,
                                renderFailed=0, manifestSealed=True), exportFiles=[{'file': 'one.jpg'}])

    def test_partial_edits_or_exports_never_qualify(self):
        self.assertTrue(photo_success(self.report()))
        for key in ('requested', 'written', 'verified', 'manifestSealed'):
            report = self.report(); report['export'][key] = 0
            self.assertFalse(photo_success(report), key)
        for key in ('unverified', 'corrupt', 'renderFailed'):
            report = self.report(); report['export'][key] = 1
            self.assertFalse(photo_success(report), key)
        for update in ({'photos': []}, {'originalsUnchanged': False}, {'pageErrors': ['error']},
                       {'exportFiles': []}, {'exportFiles': [{'file': 'stale.jpg'}]}):
            report = self.report(); report.update(update)
            self.assertFalse(photo_success(report), update)

    def test_equal_count_wrong_import_or_duplicate_id_is_refused(self):
        sources = [Path('one.CR3'), Path('two.CR3')]
        exact_import(sources, [{'id': '1', 'fileName': 'one.CR3'}, {'id': '2', 'fileName': 'two.CR3'}])
        with self.assertRaises(ValueError):
            exact_import(sources, [{'id': '1', 'fileName': 'one.CR3'}, {'id': '2', 'fileName': 'other.CR3'}])
        with self.assertRaises(ValueError):
            exact_import(sources, [{'id': '1', 'fileName': 'one.CR3'}, {'id': '1', 'fileName': 'two.CR3'}])

    def test_original_tree_and_old_evidence_are_protected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); source = root / 'originals'; output = root / 'output'
            separate_output(source, output)
            for path in (source, source / 'edits', root):
                with self.assertRaises(ValueError):
                    separate_output(source, path)
            output.mkdir(); (output / 'results.json').write_text('{}')
            with self.assertRaises(ValueError):
                separate_output(source, output)


if __name__ == '__main__':
    unittest.main()
