#!/usr/bin/env python3
"""Cross-check candidate string references against actual Ghidra disassembly references."""
import argparse
from collections import Counter
import json
from pathlib import Path


def summarize(root):
    trace=json.loads((root/'static-trace.json').read_text(encoding='utf-8'))
    binaries=[]
    functions=[]
    for name in ['targeted-decompilation.json','blemish-decompilation.json','caller-decompilation.json','eye-decompilation.json']:
        path=root/name
        if not path.exists():
            continue
        report=json.loads(path.read_text(encoding='utf-8'))
        if report['executable_sha256'] != trace['sha256']:
            raise ValueError('Different executable identities; refusing to combine evidence')
        binaries.append(name)
        for function in report['functions']:
            functions.append({**function,'evidence_file':name})
    all_links=[]
    seen=set()
    summary=[]
    for function in functions:
        start=int(function['address'],16)
        if start in seen:
            raise ValueError('Duplicate selected range requires explicit reconciliation')
        seen.add(start)
        actual={(int(r['from'],16),int(r['to'],16)) for r in function['references']}
        operands={(int(r['from'],16),int(r['to'],16)) for r in function.get('decoded_address_operands',[])}
        links=[]
        for string in trace['strings']:
            for ref in string['references']:
                if not ref['unwind_range'] or int(ref['unwind_range']['start'],16)!=start:
                    continue
                target=int(string['address'],16)+ref['literal_offset']
                source=int(ref['address'],16)
                validated=(source,target) in actual or (source,target) in operands
                link={'function':hex(start),'instruction':hex(source),'literal_address':hex(target),
                    'literal':string['text'],'validated_data_reference':validated,
                    'validation_kind':'listing-reference' if (source,target) in actual else 'decoded-address-operand' if (source,target) in operands else 'not-confirmed',
                    'evidence_file':function['evidence_file']}
                links.append(link); all_links.append(link)
        calls=Counter(r['to'] for r in function['references'] if 'CALL' in r['type'])
        summary.append({'address':hex(start),'label':function['label_evidence'],
            'disassembled':function['disassembled'],'decompiled':function.get('decompiled',False),
            'instructions':function['instruction_count'],'candidate_feature_links':len(links),
            'validated_feature_links':sum(l['validated_data_reference'] for l in links),
            'call_sites':sum(calls.values()),'distinct_call_targets':len(calls),
            'evidence_file':function['evidence_file']})
    decoded_calls={(int(r['from'],16),int(r['to'],16)) for function in functions
        for r in function['references'] if 'CALL' in r['type']}
    direct_calls=[]
    call_file=root/'direct-call-candidates.json'
    if call_file.exists():
        for candidate in json.loads(call_file.read_text(encoding='utf-8')):
            direct_calls.append({**candidate,'validated_call':(int(candidate['from'],16),int(candidate['to'],16)) in decoded_calls})
    result={'executable_sha256':trace['sha256'],'evidence_files':binaries,'functions':summary,'links':all_links,
        'direct_call_links':direct_calls,
        'limitations':['A validated data reference confirms a decoded instruction targets the recorded bytes.',
        'A decompiler-completed flag does not establish correct source-level types or all indirect call targets.',
        'Static reference validation is not runtime activation, a recovered model or an output-quality verdict.']}
    (root/'validated-feature-links.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'ranges':len(functions),'decompiled':sum(f['decompiled'] for f in summary),
        'candidate_links':len(all_links),'validated_links':sum(l['validated_data_reference'] for l in all_links)}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root',type=Path)
    summarize(parser.parse_args().root)
