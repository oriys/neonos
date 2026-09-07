"""Exercise lab generation, isolated grading and non-destructive packaging."""
from pathlib import Path
import contextlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
import zipfile
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import lab


class LabRunnerTests(unittest.TestCase):
    def test_rust_method_template_preserves_neighbors(self):
        text='impl Thing {\n    fn push(&mut self) {\n        if true {\n            go();\n        }\n    }\n    fn next() {}\n}\n'
        actual=lab.remove_body(text,'push','todo!();','rust')
        self.assertNotIn('go()',actual)
        self.assertIn('fn next() {}',actual)
        self.assertIn('todo!();',actual)
    def test_python_template_keeps_other_definitions(self):
        text='def simulate(jobs):\n    return jobs\n\ndef main():\n    pass\n'
        actual=lab.remove_body(text,'simulate','raise NotImplementedError()','python')
        self.assertIn('def main():',actual)
        self.assertNotIn('return jobs',actual)
    def test_missing_marker_is_rejected(self):
        with self.assertRaises(ValueError):lab.remove_body('fn other() {}','missing','todo!();','rust')
    def test_existing_work_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'my work';path.mkdir();(path/'answer').write_text('keep')
            with self.assertRaises(ValueError):lab.start('lab1',path)
            self.assertEqual((path/'answer').read_text(),'keep')
    def test_snapshot_skeleton_and_submission(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            path=lab.start('lab1',Path(temp)/'student with spaces')
            self.assertIn('TODO(lab1.console)',(path/'src/console.rs').read_text())
            self.assertNotIn('console.write_fmt(args)',(path/'src/console.rs').read_text())
            self.assertFalse((path/'tools').exists())
            result=subprocess.run(['git','status','--porcelain'],cwd=path,capture_output=True,text=True,check=True)
            self.assertEqual(result.stdout,'')
            spec=lab.LABS['lab1']
            outputs=lab.output_dir(path)
            (outputs/'grade.json').write_text(json.dumps({'source_sha256':lab.fingerprint(path,spec),'score':100}))
            package=lab.submit(path)
            with zipfile.ZipFile(package) as z:
                self.assertTrue(json.loads(z.read('submission.json'))['grade_current'])
                self.assertNotIn('tests/lesson-02.sh',z.namelist())
            with (path/'src/main.rs').open('a') as f:f.write('\n// changed\n')
            with zipfile.ZipFile(lab.submit(path)) as z:
                self.assertFalse(json.loads(z.read('submission.json'))['grade_current'])
            (path/'src/main.rs').unlink()
            (path/'src/main.rs').symlink_to(Path(temp)/'outside')
            with self.assertRaises(ValueError):lab.fingerprint(path,spec)
    def test_timeout_is_a_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            passed,status=lab.execute([sys.executable,'-c','import time; time.sleep(10)'],Path(temp),Path(temp)/'timeout.log',timeout=.1)
            self.assertFalse(passed)
            self.assertIn('TIMEOUT',status)


if __name__=='__main__':unittest.main()
