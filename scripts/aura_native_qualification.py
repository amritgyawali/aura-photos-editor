"""Fail-closed accounting for a complete run followed by test-only target repairs."""
from collections import Counter
import json
from pathlib import Path
import re

MATERIAL='material_stack_and_inspection_use_native_pixels_and_reversible_atomic_history'
ENGINE='graph::tests::the_engine_string_carries_the_profile_version'


def text(path):
    raw=path.read_bytes()
    return raw.decode('utf-16' if raw.startswith(b'\xff\xfe') else 'utf-8-sig',errors='replace')


def parse(log):
    suite=''
    records=[]
    for line in log.splitlines():
        line=line.strip()
        if line.startswith(('Running ','Doc-tests ')):suite=line
        match=re.match(r'^test (.+?) \.\.\. (ok|FAILED|ignored)(?:, (.*))?$',line)
        if match:records.append(dict(suite=suite,test=match[1],status=match[2],reason=match[3] or ''))
    summaries=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',log)
    totals=[sum(int(row[index]) for row in summaries) for index in range(3)]
    actual=Counter(row['status'] for row in records)
    if not summaries or [actual['ok'],actual['FAILED'],actual['ignored']]!=totals:
        raise ValueError('Native case records do not match complete result summaries')
    return dict(records=records,passed=totals[0],failed=totals[1],ignored=totals[2],groups=len(summaries))


def verify(initial, material, render):
    for line in initial.splitlines():
        line=line.strip()
        if line.startswith('error:') and not line.startswith(('error: test failed,','error: 2 targets failed:')):
            raise ValueError('A compiler, runner or unknown target error cannot be resolved by these case retests')
    first=parse(initial)
    material_result=parse(material)
    render_result=parse(render)
    failures=[row for row in first['records'] if row['status']=='FAILED']
    if len(failures)!=2 or {row['test'] for row in failures}!={MATERIAL,ENGINE}:
        raise ValueError('Unexpected failures cannot be resolved by the two known test-only repairs')
    retests=[('material-retest.log',MATERIAL,material_result),('render-retest.log',ENGINE,render_result)]
    resolved={}
    for log_name,name,result in retests:
        matches=[row for row in result['records'] if row['test']==name]
        if result['failed'] or len(matches)!=1 or matches[0]['status']!='ok':
            raise ValueError('A corrected target failed or did not execute the expected case')
        resolved[name]=log_name
    records=[]
    for original in first['records']:
        row=dict(original,initialStatus=original['status'],resolvedBy='')
        if original['status']=='FAILED':row.update(status='ok',resolvedBy=resolved[original['test']])
        records.append(row)
    return dict(verified=True,scope='Complete workspace run plus two explicit test-only target retests; not a single clean full-run log.',
                passed=first['passed']+2,failed=0,ignored=first['ignored'],groups=first['groups'],records=records,
                initial={key:value for key,value in first.items() if key!='records'},
                retests={name:{key:value for key,value in result.items() if key!='records'} for name,_,result in retests},
                resolvedCases=resolved)


def main():
    root=Path(__file__).resolve().parents[1]
    checks=root/'.work-checks/aura-next100'
    for name,expected in [('native-tests-initial','101'),('material-retest','0'),('render-retest','0')]:
        if text(checks/(name+'.exit')).strip()!=expected:raise ValueError('Unexpected native exit: '+name)
    result=verify(*(text(checks/name) for name in ['native-tests-initial.log','material-retest.log','render-retest.log']))
    (checks/'native-verification.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    (checks/'native-verified.exit').write_text('0',encoding='utf-8')
    print(json.dumps({key:value for key,value in result.items() if key!='records'}))


if __name__=='__main__':main()
