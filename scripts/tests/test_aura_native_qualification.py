import sys
from pathlib import Path
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from aura_native_qualification import ENGINE,MATERIAL,verify


def log(rows):
    counts={status:sum(value==status for _,value in rows) for status in ['ok','FAILED','ignored']}
    return '\n'.join(['Running target']+[f'test {name} ... {status}' for name,status in rows]+[
        f"test result: {'FAILED' if counts['FAILED'] else 'ok'}. {counts['ok']} passed; {counts['FAILED']} failed; {counts['ignored']} ignored"])


class NativeQualificationTests(unittest.TestCase):
    def test_explicit_repairs_preserve_the_original_failure_evidence(self):
        result=verify(log([(MATERIAL,'FAILED'),(ENGINE,'FAILED'),('existing','ok')]),log([(MATERIAL,'ok')]),log([(ENGINE,'ok')]))
        self.assertEqual(result['passed'],3)
        self.assertEqual(result['initial']['failed'],2)
        self.assertEqual(result['records'][0]['initialStatus'],'FAILED')
        self.assertEqual(result['records'][0]['resolvedBy'],'material-retest.log')

    def test_unrelated_failure_is_never_dropped(self):
        with self.assertRaises(ValueError):
            verify(log([(MATERIAL,'FAILED'),('new_bug','FAILED')]),log([(MATERIAL,'ok')]),log([(ENGINE,'ok')]))
        with self.assertRaises(ValueError):
            verify(log([(MATERIAL,'FAILED'),(ENGINE,'FAILED')])+'\nerror: could not compile another_target',log([(MATERIAL,'ok')]),log([(ENGINE,'ok')]))

    def test_missing_failed_or_incomplete_retests_cannot_qualify(self):
        initial=log([(MATERIAL,'FAILED'),(ENGINE,'FAILED')])
        for target in [log([('wrong_case','ok')]),log([(ENGINE,'FAILED')]),log([(ENGINE,'ok')])+'\ntest uncounted ... ok']:
            with self.subTest(target=target),self.assertRaises(ValueError):
                verify(initial,log([(MATERIAL,'ok')]),target)
