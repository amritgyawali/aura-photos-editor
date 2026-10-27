"""Synthetic fixtures for analysis provenance and address mapping, never Evoto assets."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest


def module(name):
    path=Path(__file__).resolve().parents[1]/(name+'.py')
    spec=importlib.util.spec_from_file_location(name,path)
    loaded=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


scan=module('evoto-static-trace')
summary=module('summarize-evoto-traces')


def fixture():
    data=bytearray(0xb00)
    data[:2]=b'MZ'
    struct.pack_into('<I',data,0x3c,0x80)
    data[0x80:0x84]=b'PE\0\0'
    struct.pack_into('<HH',data,0x84,0x8664,3)
    struct.pack_into('<H',data,0x94,0xf0)
    struct.pack_into('<H',data,0x98,0x20b)
    struct.pack_into('<Q',data,0x98+24,0x140000000)
    struct.pack_into('<II',data,0x98+112+24,0x3000,12)
    for i,(name,rva,raw,size,flags) in enumerate([
        (b'.text',0x1000,0x400,0x200,0x60000020),
        (b'.rdata',0x2000,0x600,0x400,0x40000040),
        (b'.pdata',0x3000,0xa00,0x100,0x40000040),
    ]):
        at=0x188+i*40
        data[at:at+len(name)]=name
        struct.pack_into('<IIII',data,at+8,size,rva,size,raw)
        struct.pack_into('<I',data,at+36,flags)
    data[0x410:0x413]=b'\x48\x8d\x0d'
    struct.pack_into('<i',data,0x413,0x2000-(0x1010+7))
    data[0x600:0x609]=b'FaceAcne\0'
    struct.pack_into('<III',data,0xa00,0x1000,0x1040,0x3080)
    return bytes(data)


class StaticTraceTest(unittest.TestCase):
    def test_file_offsets_virtual_addresses_and_half_open_ranges(self):
        pe=scan.PE(fixture())
        self.assertEqual(pe.offset(0x2000),0x600)
        self.assertEqual(pe.rva_from_offset(0x600),0x2000)
        self.assertIsNone(pe.offset(0x4000))
        self.assertEqual(pe.function(0x103f)['start'],'0x140001000')
        self.assertIsNone(pe.function(0x1040))
        self.assertTrue(pe.executable(0x140001010))
        self.assertFalse(pe.executable(0x140002000))

    def test_scan_records_candidate_without_claiming_instruction_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            source=root/'synthetic.exe'
            original=fixture()
            source.write_bytes(original)
            with contextlib.redirect_stdout(io.StringIO()): scan.trace(source,root/'out')
            report=json.loads((root/'out/static-trace.json').read_text())
            string=report['strings'][0]
            self.assertEqual(string['text'],'FaceAcne')
            self.assertEqual(string['address'],'0x140002000')
            self.assertEqual(string['references'][0]['address'],'0x140001010')
            self.assertIn('requires disassembly',string['references'][0]['status'])
            self.assertEqual(source.read_bytes(),original)

    def test_cross_check_rejects_wrong_operand_and_mixed_executable_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            source=root/'synthetic.exe'; source.write_bytes(fixture())
            with contextlib.redirect_stdout(io.StringIO()): scan.trace(source,root/'out')
            out=root/'out'; trace=json.loads((out/'static-trace.json').read_text())
            function={'address':'140001000','label_evidence':'synthetic','disassembled':True,
                'decompiled':True,'instruction_count':2,'references':[],
                'decoded_address_operands':[{'from':'140001010','to':'140002001'}]}
            report={'executable_sha256':trace['sha256'],'functions':[function]}
            path=out/'targeted-decompilation.json'; path.write_text(json.dumps(report))
            with contextlib.redirect_stdout(io.StringIO()): summary.summarize(out)
            result=json.loads((out/'validated-feature-links.json').read_text())
            self.assertFalse(result['links'][0]['validated_data_reference'])
            function['decoded_address_operands'][0]['to']='140002000'
            path.write_text(json.dumps(report))
            with contextlib.redirect_stdout(io.StringIO()): summary.summarize(out)
            self.assertTrue(json.loads((out/'validated-feature-links.json').read_text())['links'][0]['validated_data_reference'])
            report['executable_sha256']='0'*64; path.write_text(json.dumps(report))
            with self.assertRaisesRegex(ValueError,'Different executable'): summary.summarize(out)

    def test_x86_is_not_silently_parsed_as_x64(self):
        data=bytearray(fixture())
        struct.pack_into('<H',data,0x84,0x14c)
        with self.assertRaisesRegex(ValueError,'Only x64'): scan.PE(data)


if __name__=='__main__': unittest.main()
