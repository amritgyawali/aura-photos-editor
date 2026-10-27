"""The IPC gate must reject collisions even when its three name sets agree."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


@unittest.skipUnless(shutil.which('bash'), 'requires the repository Bash tooling')
class IpcSurfaceTests(unittest.TestCase):
    def gate(self, duplicate_definition=False, duplicate_registration=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            shutil.copyfile(Path(__file__).resolve().parents[1] / 'check-ipc-surface.sh', root / 'scripts/check-ipc-surface.sh')
            (root / 'ui/src-tauri/src').mkdir(parents=True)
            (root / 'ui/src/ipc').mkdir(parents=True)
            definition = '#[tauri::command]\nfn inspect_photo() {}\n\n\n'
            main = definition * (2 if duplicate_definition else 1)
            main += 'tauri::generate_handler![\ninspect_photo,\n'
            if duplicate_registration:
                main += 'inspect_photo,\n'
            main += ']);\n'
            (root / 'ui/src-tauri/src/main.rs').write_text(main, encoding='utf-8')
            (root / 'ui/src/ipc/client.ts').write_text("invoke('inspect_photo', {});\n", encoding='utf-8')
            return subprocess.run([shutil.which('bash'), 'scripts/check-ipc-surface.sh'], cwd=root, text=True, capture_output=True, check=False)

    def test_matching_unique_surface_passes(self):
        result = self.gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_duplicate_definition_fails_even_when_sets_match(self):
        result = self.gate(duplicate_definition=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('duplicate commands defined', result.stdout)

    def test_duplicate_registration_fails_even_when_sets_match(self):
        result = self.gate(duplicate_registration=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('duplicate commands registered', result.stdout)


if __name__ == '__main__':
    unittest.main()
