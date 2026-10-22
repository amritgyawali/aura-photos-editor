"""Record one validation command, its complete UTF-8 log and real exit status."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--cwd', type=Path, default=ROOT)
parser.add_argument('name')
parser.add_argument('command', nargs=argparse.REMAINDER)
args = parser.parse_args()
if not args.command or not args.name.replace('-', '').replace('_', '').isalnum():
    parser.error('A simple result name and explicit command are required')
folder = ROOT / '.work-checks/aura-advanced-2026-10-22'
folder.mkdir(parents=True, exist_ok=True)
path = folder / (args.name + '.json')
if path.exists():
    raise ValueError('Choose a fresh result name; earlier attempts must stay recorded')
started = time.monotonic()
env = dict(os.environ, CARGO_INCREMENTAL='0', AURA_PERF_HOST_SCALE='6')
with (folder / (args.name + '.log')).open('w', encoding='utf-8') as stream:
    result = subprocess.run(args.command, cwd=args.cwd, env=env, stdout=stream, stderr=subprocess.STDOUT)
record = dict(command=args.command, cwd=str(args.cwd), exitCode=result.returncode,
              environment={'CARGO_INCREMENTAL':'0', 'AURA_PERF_HOST_SCALE':'6'}, seconds=round(time.monotonic()-started, 3))
path.write_text(json.dumps(record, indent=2)+'\n', encoding='utf-8')
print(json.dumps(dict(name=args.name, **record)), flush=True)
raise SystemExit(result.returncode)
