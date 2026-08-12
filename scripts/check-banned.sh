#!/usr/bin/env bash
# R1 enforcement. Runs in stage 1 of CI and in the pre-commit hook.
# Exit 1 on any banned pattern outside permitted locations.
set -euo pipefail

fail=0

# Files where a panic is acceptable: test code, benches, xtask, and main().
is_exempt() {
  case "$1" in
    *"/tests/"*|*"/benches/"*|xtask/*|*"/main.rs") return 0 ;;
    *) return 1 ;;
  esac
}

banned_rs='\.unwrap\(\)|\.expect\(|panic!\(|todo!\(|unimplemented!\(|unreachable!\(|process::exit'

while IFS= read -r file; do
  if is_exempt "$file"; then continue; fi
  if matches=$(grep -nE "$banned_rs" "$file" | grep -v '// ALLOW-BANNED:'); then
    echo "BANNED PATTERN in $file"
    echo "$matches"
    fail=1
  fi
done < <(find crates -name '*.rs' -type f)

# Determinism patterns
det_banned='HashMap::new|HashSet::new|SystemTime::now|Instant::now|rand::random|thread_rng'

while IFS= read -r file; do
  if is_exempt "$file"; then continue; fi
  if matches=$(grep -nE "$det_banned" "$file" | grep -v '// DETERMINISM:'); then
    echo "DETERMINISM RISK in $file (add a // DETERMINISM: justification if intentional)"
    echo "$matches"
    fail=1
  fi
done < <(find crates -name '*.rs' -type f)

# Phase 03: nothing outside aura-infer may link ONNX Runtime directly.
# Acceptance criterion 7 and section 12 of PHASE-03: one crate owns the runtime,
# or twenty-five models become twenty-five different answers to "why is it slow
# on this laptop". The lint is here rather than in review because a direct `ort::`
# call is exactly the shortcut a hurried change makes.
ort_users=$(grep -rlnE '(^|[^a-zA-Z_])ort::|onnxruntime_sys|use ort\b' crates --include='*.rs' | grep -v '^crates/aura-infer/' || true)
if [ -n "$ort_users" ]; then
  echo "BANNED: ONNX Runtime used outside crates/aura-infer"
  echo "$ort_users"
  fail=1
fi

# TypeScript: no any, no non-null assertion on IPC boundaries
if [ -d ui/src ]; then
  if matches=$(grep -rnE ':\s*any\b|as any' ui/src --include='*.ts' --include='*.tsx' || true); then
    if [ -n "$matches" ]; then
      echo "BANNED: 'any' in UI source"
      echo "$matches"
      fail=1
    fi
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo ""
  echo "check-banned failed. Do not add // ALLOW-BANNED: to make this pass"
  echo "unless a reviewer has approved it in the PR description."
  exit 1
fi

echo "check-banned: clean"
