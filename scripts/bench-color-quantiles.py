"""Benchmark the prepared quantile map against the pre-upgrade implementation.

Compiles a small optimized Rust executable using the actual production module.
This isolates lookup cost; it is not a benchmark of full desktop matching.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import uuid

ROOT = Path(__file__).resolve().parents[1]
PROGRAM = r'''
fn main(){
 use std::{hint::black_box,time::Instant};
 let from:Vec<f32>=(0..65).map(|i|0.2+0.6*(i as f32/64.0).powf(1.2)).collect();
 let to:Vec<f32>=(0..65).map(|i|0.25+0.5*(i as f32/64.0).powf(0.9)).collect();
 let mut old_times=Vec::new();let mut new_times=Vec::new();let mut sum=0.0f32;
 for _ in 0..5{
  let start=Instant::now();
  for _ in 0..32{for i in 0..40_000{let x=black_box(0.05+i as f32/44_444.0);sum+=legacy(black_box(&from),black_box(&to),x,1.2,(0.25,2.0));}}
  old_times.push(start.elapsed().as_secs_f64()*1000.0);
  let start=Instant::now();
  for _ in 0..32{let map=quantile::QuantileMap::bounded(black_box(&from),black_box(&to),1.2,(0.25,2.0));for i in 0..40_000{let x=black_box(0.05+i as f32/44_444.0);sum+=map.apply(x);}}
  new_times.push(start.elapsed().as_secs_f64()*1000.0);
 }
 old_times.sort_by(f64::total_cmp);new_times.sort_by(f64::total_cmp);black_box(sum);
 let flat=[0.5;65];let refq:Vec<f32>=(0..65).map(|i|0.2+i as f32/80.0).collect();let map=quantile::QuantileMap::new(&flat,&refq,1.2,(0.25,2.0));
 let old_jump=legacy(&flat,&refq,0.500001,1.2,(0.25,2.0))-legacy(&flat,&refq,0.499999,1.2,(0.25,2.0));let new_jump=map.apply(0.500001)-map.apply(0.499999);
 println!("{{\"evaluationsPerRepeat\":1280000,\"repeats\":5,\"legacyMedianMs\":{},\"preparedMedianMs\":{},\"isolatedSpeedup\":{},\"flatLegacyJump\":{},\"flatPreparedJump\":{},\"legacySamplesMs\":{:?},\"preparedSamplesMs\":{:?}}}",old_times[2],new_times[2],old_times[2]/new_times[2],old_jump,new_jump,old_times,new_times);
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-ref', default='1f0d527')
    args = parser.parse_args()
    original = subprocess.check_output(['git', 'show', f'{args.baseline_ref}:crates/aura-app/src/color_match/transfer.rs'], cwd=ROOT, text=True, encoding='utf-8')
    start = original.index('fn quantile_map(')
    end = original.index('/// Evaluate a curve', start)
    legacy = original[start:end].replace('fn quantile_map(', 'fn legacy(')
    if 'partition_point' not in legacy:
        raise ValueError('Choose a baseline commit containing the original quantile implementation')
    out = ROOT / '.work-checks' / ('quantile-bench-' + uuid.uuid4().hex[:8])
    out.mkdir(parents=True)
    source = out / 'quantile-benchmark.rs'
    production = ROOT / 'crates/aura-app/src/color_match/transfer/quantile.rs'
    module = '#[path = ' + json.dumps(production.as_posix(), ensure_ascii=False) + ']\nmod quantile;\n'
    source.write_text(module + 'fn at(a:&[f32],i:usize)->f32{a.get(i).or_else(||a.last()).copied().unwrap_or(0.0)}\n' + legacy + PROGRAM, encoding='utf-8')
    exe = out / ('quantile-benchmark.exe' if os.name == 'nt' else 'quantile-benchmark')
    compile_result = subprocess.run(['rustc', '--crate-name', 'quantile_benchmark', '-O', str(source), '-o', str(exe)], check=True, capture_output=True, text=True)
    result = json.loads(subprocess.check_output([str(exe)], text=True))
    result.update(baselineRef=args.baseline_ref, module=str(production), compilerWarnings=compile_result.stderr,
                  meaning='Optimized isolated lookup cost including one prepared map per projection; not full image-match throughput.')
    (out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({'output': str(out), **result}), flush=True)


if __name__ == '__main__':
    main()
