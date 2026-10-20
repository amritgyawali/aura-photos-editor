#!/usr/bin/env python3
"""Bounded read-only PE feature tracing. Strings and instruction candidates are evidence,
not original source or runtime proof. Never scans profiles, projects or account storage.
"""
import argparse
import bisect
import hashlib
import json
from pathlib import Path
import re
import struct

FEATURES = re.compile(r'(?:FaceAcne|BodyAcne|MakeupProtect|BeardProtect|MatteRefinement|RemoveEyeBag|'
    r'FaceDermabrasion|SkinTexture|SkinUnify|NeutralGray|WhiteBalance|white_balance|NoseFlaw|'
    r'face_pars|face_landmark|skin_mask|skinmask|skin.?mask|blemish|eyebag|eye.?bag|'
    r'acne|pore|under.?eye|dark.?circle|eyelid|makeup.?protect|beard.?protect|landmark|auto.?wb|onnxruntime::InferenceSession)', re.I)


class PE:
    def __init__(self, data):
        self.data = data
        pe = struct.unpack_from('<I', data, 0x3c)[0]
        if data[:2] != b'MZ' or data[pe:pe+4] != b'PE\0\0':
            raise ValueError('Not a PE executable')
        machine, count = struct.unpack_from('<HH', data, pe+4)
        opt = pe+24
        if machine != 0x8664 or struct.unpack_from('<H', data, opt)[0] != 0x20b:
            raise ValueError('Only x64 PE32+ is supported')
        self.base = struct.unpack_from('<Q', data, opt+24)[0]
        self.sections = []
        section_start = opt + struct.unpack_from('<H', data, pe+20)[0]
        for index in range(count):
            at = section_start + 40*index
            name = data[at:at+8].rstrip(b'\0').decode('ascii', 'replace')
            virtual, rva, size, raw = struct.unpack_from('<IIII', data, at+8)
            flags = struct.unpack_from('<I', data, at+36)[0]
            self.sections.append(dict(name=name, rva=rva, size=size, raw=raw, virtual=virtual, executable=bool(flags & 0x20000000)))
        exception_rva, exception_size = struct.unpack_from('<II', data, opt+112+3*8)
        self.functions = []
        offset = self.offset(exception_rva)
        if offset is not None:
            for at in range(offset, min(len(data)-11, offset+exception_size), 12):
                start, end, unwind = struct.unpack_from('<III', data, at)
                if start and start < end:
                    self.functions.append((start, end, unwind))
        self.functions.sort()
        self.starts = [f[0] for f in self.functions]

    def offset(self, rva):
        for s in self.sections:
            if s['rva'] <= rva < s['rva']+s['size']:
                return s['raw']+rva-s['rva']
        return None

    def function(self, rva):
        index = bisect.bisect_right(self.starts, rva)-1
        if index >= 0 and rva < self.functions[index][1]:
            a,b,u = self.functions[index]
            return {'start':hex(self.base+a), 'end_exclusive':hex(self.base+b), 'bytes':b-a, 'unwind_rva':hex(u)}
        return None

    def rva_from_offset(self, offset):
        for s in self.sections:
            if s['raw'] <= offset < s['raw']+s['size']:
                return s['rva']+offset-s['raw']
        return None

    def executable(self, address):
        rva=address-self.base
        return any(s['executable'] and s['rva']<=rva<s['rva']+s['size'] for s in self.sections)


def trace(path, output):
    data = path.read_bytes()
    pe = PE(data)
    strings = []
    for section in pe.sections:
        if section['name'] not in ('.rdata', '.data', '_RDATA'):
            continue
        raw = section['raw']
        for match in re.finditer(rb'[\x20-\x7e]{5,768}\x00', data[raw:raw+section['size']]):
            value = match.group()[:-1].decode('ascii')
            if FEATURES.search(value):
                rva = section['rva']+match.start()
                strings.append({'text':value, 'address':hex(pe.base+rva), 'file_offset':raw+match.start(), 'bytes':len(value), 'references':[]})
    strings.sort(key=lambda s:int(s['address'],16))
    addresses = [int(s['address'],16) for s in strings]
    # Candidate RIP-relative LEA encoding. A reference can point into a literal.
    # It may be data, unreachable code or an instruction interior: validate before treating
    # it as a real instruction or caller/callee relation.
    for section in pe.sections:
        if not section['executable']:
            continue
        raw = section['raw']
        for match in re.finditer(rb'[\x48\x4c]\x8d[\x05\x0d\x15\x1d\x25\x2d\x35\x3d][\x00-\xff]{4}', data[raw:raw+section['size']]):
            source_rva = section['rva']+match.start()
            target = pe.base+source_rva+7+struct.unpack_from('<i', match.group(), 3)[0]
            index = bisect.bisect_right(addresses, target)-1
            if index >= 0 and target < addresses[index]+strings[index]['bytes']:
                strings[index]['references'].append({'address':hex(pe.base+source_rva),
                    'encoding':match.group().hex(), 'literal_offset':target-addresses[index],
                    'unwind_range':pe.function(source_rva), 'status':'encoding candidate; requires disassembly validation'})
    report = {'path':str(path.resolve()), 'sha256':hashlib.sha256(data).hexdigest(),
        'image_base':hex(pe.base), 'runtime_function_ranges':len(pe.functions),
        'strings':strings, 'limitations':['ASCII strings in selected data sections only; no UTF-16 inventory.',
        'Candidates are bounded encoding matches, not validated instructions or dynamic execution.',
        'PE unwind ranges can be function fragments; they are not guaranteed source-level function boundaries.',
        'No private weights or algorithms recovered merely by matching names.']}
    output.mkdir(parents=True, exist_ok=True)
    (output/'static-trace.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    candidates = {}
    # Prioritise task configuration names over generic runtime diagnostic strings.
    for s in sorted(strings, key=lambda s:(len(s['text'])>120, len(s['text']))):
        for ref in s['references']:
            f = ref['unwind_range']
            if f and 32 <= f['bytes'] <= 16000 and f['start'] not in candidates:
                candidates[f['start']] = (f, s['text'])
    all_candidates = list(candidates.values())
    (output/'all-candidate-functions.tsv').write_text(''.join(f"{f['start']}\t{f['end_exclusive']}\t{label[:180]}\n" for f,label in all_candidates),encoding='utf-8')
    priorities = ['GetAutoWhiteBalance', 'FaceDermabrasionHLF::Render GetSkinMask',
        'NoseFlawRemovalPipeline Failed to load', 'NoseFlawRemovalPipeline::check_data_valid',
        'CPU composition_face', 'HAVE_FACE_PARSING', 'EnsureResultImage: removeBGEdge',
        'SkinUnifyPipeline', 'MatteRefinementMix::Render', 'AutoWBPipeline']
    selected = []
    seen = set()
    for priority in priorities:
        count = 0
        for s in strings:
            if priority not in s['text']:
                continue
            for ref in s['references']:
                f=ref['unwind_range']
                if f and 32 <= f['bytes'] <= 16000 and f['start'] not in seen:
                    selected.append((f,s['text'])); seen.add(f['start']); count += 1
                    if count >= 2: break
            if count >= 2: break
    selected = selected[:20]
    (output/'target-functions.tsv').write_text(''.join(f"{f['start']}\t{f['end_exclusive']}\t{label[:180]}\n" for f,label in selected),encoding='utf-8')
    blemishes={}
    for s in strings:
        if not any(term in s['text'] for term in ['AdultFaceBlemishPipeline::inference',
            'AdultFaceBlemishPipeline::initCache', 'NewBornFaceBlemishPipelineV2::faceFeatureProtect',
            'AdultFaceBlemishPipeline::postProcess', 'AdultFaceBlemishPipeline::Total Skin Mask']):
            continue
        for ref in s['references']:
            f=ref['unwind_range']
            if f and 32<=f['bytes']<=16000:
                blemishes.setdefault(f['start'],(f,s['text']))
    (output/'blemish-functions.tsv').write_text(''.join(f"{f['start']}\t{f['end_exclusive']}\t{label}\n" for f,label in blemishes.values()),encoding='utf-8')
    # Follow caller candidates of explicitly selected inference/protection entry ranges.
    # E8 can also occur inside another instruction; Ghidra must validate these edges.
    entries={int(address,16):label for address,(_,label) in blemishes.items()}
    direct_calls=[]
    callers={}
    for section in pe.sections:
        if not section['executable']: continue
        for match in re.finditer(rb'\xe8[\x00-\xff]{4}', data[section['raw']:section['raw']+section['size']]):
            source_rva=section['rva']+match.start()
            target=pe.base+source_rva+5+struct.unpack_from('<i',match.group(),1)[0]
            if target not in entries: continue
            f=pe.function(source_rva)
            direct_calls.append({'from':hex(pe.base+source_rva),'to':hex(target),'target_label':entries[target],
                'caller_range':f,'status':'relative-call encoding candidate; requires disassembly'})
            if f and 32<=f['bytes']<=16000 and f['start'] not in blemishes:
                callers.setdefault(f['start'],(f,'caller candidate of '+entries[target]))
    (output/'direct-call-candidates.json').write_text(json.dumps(direct_calls,indent=2)+'\n',encoding='utf-8')
    (output/'caller-functions.tsv').write_text(''.join(f"{f['start']}\t{f['end_exclusive']}\t{label}\n" for f,label in list(callers.values())[:24]),encoding='utf-8')
    # MSVC x64 RTTI complete-object locators: require signature and self-RVA, not
    # merely a pointer-looking byte pattern. Vtable slots still need disassembly review.
    rtti=[]
    eye_targets={}
    for s in strings:
        if s['text'] not in ['.?AVDarkCirclesV1Pipeline@TSAI@@','.?AVEyebagV1Pipeline@TSAI@@']:
            continue
        descriptor=int(s['address'],16)-pe.base-16
        locators=[]
        for hit in iter_matches(data,struct.pack('<I',descriptor)):
            at=hit-12
            if at<0 or at+24>len(data): continue
            fields=struct.unpack_from('<IIIIII',data,at)
            rva=pe.rva_from_offset(at)
            if fields[0]!=1 or rva is None or fields[5]!=rva: continue
            tables=[]
            for pointer in iter_matches(data,struct.pack('<Q',pe.base+rva)):
                slots=[]
                for i in range(40):
                    slot=pointer+8+i*8
                    if slot+8>len(data): break
                    address=struct.unpack_from('<Q',data,slot)[0]
                    if not pe.executable(address): break
                    f=pe.function(address-pe.base)
                    slots.append({'slot':i,'address':hex(address),'unwind_range':f})
                    if f and int(f['start'],16)==address and 32<=f['bytes']<=16000:
                        eye_targets.setdefault(f['start'],(f,s['text']+' vtable slot '+str(i)))
                if slots: tables.append({'table_address':hex(pe.base+pe.rva_from_offset(pointer+8)), 'slots':slots})
            locators.append({'address':hex(pe.base+rva),'object_offset':fields[1],'tables':tables})
        rtti.append({'type':s['text'],'type_descriptor_rva':hex(descriptor),'locators':locators})
    (output/'eye-rtti-vtables.json').write_text(json.dumps(rtti,indent=2)+'\n',encoding='utf-8')
    for s in strings:
        if s['text'] not in ['DarkCirclesV1Pipeline','EyebagV1Pipeline']: continue
        for ref in s['references']:
            f=ref['unwind_range']
            if f and 32<=f['bytes']<=16000: eye_targets.setdefault(f['start'],(f,s['text']))
    (output/'eye-functions.tsv').write_text(''.join(f"{f['start']}\t{f['end_exclusive']}\t{label}\n" for f,label in list(eye_targets.values())[:24]),encoding='utf-8')
    print(json.dumps({'strings':len(strings), 'referenced_strings':sum(bool(s['references']) for s in strings),
        'candidate_functions':len(candidates),'selected':len(selected),'unwind_ranges':len(pe.functions)}))


def iter_matches(data, needle):
    start=0
    while (at:=data.find(needle,start))>=0:
        yield at
        start=at+1


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    trace(args.binary,args.output)
