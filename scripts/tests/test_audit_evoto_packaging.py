"""Synthetic carrier fixtures only; no Evoto assets or private payloads."""
import importlib.util
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zlib

SPEC = importlib.util.spec_from_file_location('packaging_audit', Path(__file__).resolve().parents[1] / 'audit-evoto-packaging.py')
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def encoded_config(value, spaces):
    body = json.dumps(value).encode() + b' ' * spaces
    prefix = AUDIT.xor_words(body)
    tail = bytes(value ^ mask for value, mask in
                 zip(body[len(prefix):], AUDIT.JSON_TAIL_MASKS[len(body) % 4]))
    return prefix + tail + AUDIT.JSON_MARKER


class PackagingAuditTest(unittest.TestCase):
    def test_strict_json_round_trip_for_all_four_tail_lengths(self):
        value = {'Effects': [{'EffectName': 'SyntheticCallback', 'Type': 'Synthetic'}]}
        self.assertEqual(AUDIT.decode_config(json.dumps(value).encode()), (value, 'plain_utf8_json'))
        for spaces in range(4):
            with self.subTest(spaces=spaces):
                self.assertEqual(AUDIT.decode_config(encoded_config(value, spaces)), (value, 'xor_words_json_tail'))

    def test_unknown_markers_corruption_and_size_limit_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'marker'):
            AUDIT.decode_config(b'not-json')
        with self.assertRaisesRegex(ValueError, 'strict UTF-8 JSON'):
            AUDIT.decode_config(b'broken' + AUDIT.JSON_MARKER)
        with self.assertRaisesRegex(ValueError, 'limit'):
            AUDIT.decode_config(b' ' * (AUDIT.MAX_CONFIG_BYTES + 1))
        with self.assertRaises(ValueError):
            AUDIT.decode_config(encoded_config({'value': float('nan')}, 0))

    def test_audit_binds_build_hash_and_does_not_follow_external_asset_reference(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Evoto-worker.exe').write_bytes(b'synthetic worker')
            (root / 'Evoto.exe').write_bytes(b'synthetic gui')
            resource = root / 'resources/effect/config/synthetic/effect.json'
            resource.parent.mkdir(parents=True)
            value = {'Effects': [{'EffectName': 'Synthetic', 'Path': '../../../../outside.key'}]}
            resource.write_bytes(encoded_config(value, 0))
            original = resource.read_bytes()
            with self.assertRaisesRegex(ValueError, 'SHA-256'):
                AUDIT.audit(root, '0' * 64)
            expected = hashlib.sha256(b'synthetic worker').hexdigest()
            result = AUDIT.audit(root, expected)
            self.assertEqual(result['worker_sha256'], expected)
            reference = result['configs'][0]['metadata']['references'][0]
            self.assertFalse(reference['within_resources'])
            self.assertIsNone(reference['exists'])
            self.assertEqual(resource.read_bytes(), original)

    def test_metadata_omits_shader_source_weights_and_tuning_arrays(self):
        value = {'Effects': [{'EffectName': 'Synthetic', 'DefaultDegree': 50, 'DegreeRange': [0, 100],
                             'Paths': ['mask.png'], 'ShaderSource': 'PRIVATE_SOURCE', 'Weights': [1, 2, 3],
                             'Radius': 1.2345}]}
        result = AUDIT.metadata(value)
        text = json.dumps(result)
        self.assertNotIn('PRIVATE_SOURCE', text)
        self.assertNotIn('1.2345', text)
        self.assertEqual(result['ranges'][1]['value'], [0, 100])
        self.assertEqual(result['references'][0]['path'], 'mask.png')

    def test_png_proof_checks_ihdr_crc_and_does_not_mutate_resource(self):
        ihdr = b'IHDR' + struct.pack('>IIBBBBB', 64, 48, 8, 2, 0, 0, 0)
        png = b'\x89PNG\r\n\x1a\n' + struct.pack('>I', 13) + ihdr + struct.pack('>I', zlib.crc32(ihdr))
        body = png + b'\x00' * 3
        encoded = AUDIT.xor_words(body) + AUDIT.IMAGE_MARKER
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'synthetic.png'
            path.write_bytes(encoded)
            result = AUDIT.png_header(path)
            self.assertTrue(result['ihdr_crc_valid'])
            self.assertEqual((result['width'], result['height']), (64, 48))
            self.assertEqual(path.read_bytes(), encoded)
            damaged = bytearray(encoded)
            damaged[20] ^= 1
            path.write_bytes(damaged)
            self.assertFalse(AUDIT.png_header(path)['ihdr_crc_valid'])


if __name__ == '__main__':
    unittest.main()
