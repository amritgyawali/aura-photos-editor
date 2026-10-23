"""Run one qualification command with a fresh log and its actual exit status."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('name')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not args.command or not args.name.replace('-', '').replace('_', '').isalnum():
        parser.error('A simple name and an explicit command are required')
    args.output.mkdir(parents=True, exist_ok=True)
    record_path = args.output / (args.name + '.json')
    log_path = args.output / (args.name + '.log')
    if record_path.exists() or log_path.exists():
        parser.error('Choose a fresh name; earlier evidence must remain intact')
    started = time.monotonic()
    environment = {'CARGO_INCREMENTAL': '0', 'AURA_PERF_HOST_SCALE': '6'}
    with log_path.open('x', encoding='utf-8') as stream:
        result = subprocess.run(args.command, cwd=ROOT,
                                env=dict(os.environ, **environment),
                                stdout=stream, stderr=subprocess.STDOUT)
    record = dict(command=args.command, cwd=str(ROOT), exitCode=result.returncode,
                  environment=environment, seconds=round(time.monotonic() - started, 3))
    record_path.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(dict(name=args.name, **record)), flush=True)
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
