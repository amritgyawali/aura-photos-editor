"""Run with python -m unittest discover -s crates/aura-cloud/tests -p test_instagram_fetch.py."""
import importlib.util
import pathlib
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("instagram_fetch", pathlib.Path(__file__).parents[1] / "src/instagram_fetch.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


class Response:
    def __enter__(self): return self
    def __exit__(self, *_): pass
    def iter_content(self, _): yield b"fake-jpeg-data"


def post(code, video=False):
    return types.SimpleNamespace(shortcode=code, is_video=video, typename="GraphImage", url="https://images.cdninstagram.com/photo.jpg")


class FetchTests(unittest.TestCase):
    def run_fetch(self, posts=(), private=False, failure=None, limit=8):
        profile = types.SimpleNamespace(is_private=private, get_posts=lambda: iter(posts))
        def from_username(*_):
            if failure: raise RuntimeError(failure)
            return profile
        context = types.SimpleNamespace(get_raw=lambda _: Response())
        module = types.SimpleNamespace(Instaloader=lambda **_: types.SimpleNamespace(context=context), Profile=types.SimpleNamespace(from_username=from_username))
        with tempfile.TemporaryDirectory() as folder, patch.dict(sys.modules, {"instaloader": module}):
            report = helper.fetch("photographer", folder, limit)
            count = len(list(pathlib.Path(folder).glob("*.jpg")))
            self.assertEqual(count, report["fetched"])
            self.assertFalse(list(pathlib.Path(folder).glob("*.part")))
            return report

    def test_counts_stills_and_skips_videos(self):
        result = self.run_fetch([post("one"), post("two", True), post("three")])
        self.assertEqual(result["fetched"], 2)
        self.assertEqual(result["skipped"], 1)
        self.assertTrue(result["complete"])

    def test_cap_does_not_claim_full_profile(self):
        result = self.run_fetch([post(str(i)) for i in range(10)], limit=8)
        self.assertEqual(result["fetched"], 8)
        self.assertFalse(result["complete"])

    def test_blocked_access_is_actionable_and_not_success(self):
        result = self.run_fetch(failure="401 Please wait a few minutes")
        self.assertEqual(result["fetched"], 0)
        self.assertFalse(result["complete"])
        self.assertIn("saved reference", result["message"])

    def test_private_profile_and_invalid_media_paths_are_not_downloaded(self):
        self.assertEqual(self.run_fetch(private=True)["fetched"], 0)
        result = self.run_fetch([post("../../outside")])
        self.assertEqual(result["fetched"], 0)
        self.assertEqual(result["skipped"], 1)


if __name__ == "__main__": unittest.main()
