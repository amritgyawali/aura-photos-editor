import importlib.util
from pathlib import Path
import unittest
import tempfile

import numpy as np
from PIL import Image, PngImagePlugin

spec = importlib.util.spec_from_file_location('aura_colour_metrics', Path(__file__).resolve().parents[1] / 'aura_colour_metrics.py')
metrics = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metrics)


class ColourMeasurements(unittest.TestCase):
    def test_png_srgb_chunk_is_recognized_without_an_icc_profile(self):
        pixels = np.full((16, 16, 3), 128, dtype=np.uint8)
        chunks = PngImagePlugin.PngInfo()
        chunks.add(b'sRGB', b'\x00')
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'tagged.png'
            Image.fromarray(pixels).save(path, pnginfo=chunks)
            decoded, info = metrics.load_srgb(path)
        self.assertFalse(info['embeddedIcc'])
        self.assertEqual(info['interpretation'], 'PNG sRGB chunk')
        self.assertEqual(info['srgbRenderingIntent'], 0)
        np.testing.assert_array_equal(decoded, pixels)

    def test_srgb_black_white_and_red_use_the_expected_lab_axes(self):
        black, white, red = metrics.lab(np.array([[0, 0, 0], [255, 255, 255], [255, 0, 0]], dtype=np.uint8))
        np.testing.assert_allclose(black, [0, 0, 0], atol=.001)
        np.testing.assert_allclose(white, [100, 0, 0], atol=.001)
        np.testing.assert_allclose(red, [53.2408, 80.0925, 67.2032], atol=.002)

    def test_aligned_errors_reject_different_scene_sizes_and_detect_export_casts(self):
        neutral = np.full((16, 16, 3), 128, dtype=np.uint8)
        self.assertTrue(metrics.aligned_error(neutral, neutral)['exactPixels'])
        warm = neutral.copy()
        warm[:, :, 0] += 15
        shifted = metrics.aligned_error(warm, neutral)
        self.assertFalse(shifted['exactPixels'])
        self.assertGreater(shifted['meanDeltaE76'], 1)
        self.assertGreater(shifted['meanCastShiftA'], 0)
        with self.assertRaises(ValueError):
            metrics.aligned_error(neutral[:2], neutral)

    def test_distribution_gaps_are_symmetric_and_do_not_invent_white_balance(self):
        gray = metrics.descriptor(np.full((32, 32, 3), 128, dtype=np.uint8))
        red = metrics.descriptor(np.full((32, 32, 3), [180, 50, 40], dtype=np.uint8))
        self.assertEqual(metrics.distribution_gap(gray, gray)['toneQuantileMaeL'], 0)
        self.assertEqual(metrics.distribution_gap(gray, gray)['paletteJensenShannonDistance'], 0)
        a, b = metrics.distribution_gap(gray, red), metrics.distribution_gap(red, gray)
        self.assertAlmostEqual(a['paletteJensenShannonDistance'], b['paletteJensenShannonDistance'])
        self.assertGreater(a['paletteJensenShannonDistance'], 0)
        self.assertIsNone(a['nearNeutralCastGapAb'])
        self.assertIn('unverified', a['calibratedWhiteBalance'])


if __name__ == '__main__':
    unittest.main()
