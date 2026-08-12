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

# Phase 04: nothing outside aura-cloud may open a socket.
#
# Acceptance criterion in section 3: "the gateway is the only crate allowed to
# make outbound network calls; a CI lint enforces this". The lint is here rather
# than in review for the same reason the ONNX one is: a direct `TcpStream` in a
# phase that needed one thing from an API is exactly the shortcut a hurried
# change makes, and by the time review notices there are three of them.
net_users=$(grep -rlnE 'std::net::(TcpStream|TcpListener|UdpSocket)|ToSocketAddrs|\breqwest::|\bureq::|\bhyper::|\bcurl::|\btungstenite::|\brustls::|\bnative_tls::' \
  crates --include='*.rs' | grep -v '^crates/aura-cloud/' || true)
if [ -n "$net_users" ]; then
  echo "BANNED: outbound network APIs used outside crates/aura-cloud"
  echo "$net_users"
  echo "Every cloud call goes through aura-cloud. See docs/adr/ADR-0009-cloud-ai-policy.md."
  fail=1
fi

# Phase 04: a key must never be written where it can be read back.
#
# The credential store is the only place an API key lives. A key in an
# environment variable, a config file or a catalog row is a key in a backup, a
# support bundle and a process listing.
key_sinks=$(grep -rnE 'env::set_var\("[A-Z_]*(KEY|TOKEN|SECRET)|INSERT INTO .*api_key|api_key *=' \
  crates --include='*.rs' | grep -v '// ALLOW-BANNED:' || true)
if [ -n "$key_sinks" ]; then
  echo "BANNED: an API key is being written somewhere other than the credential store"
  echo "$key_sinks"
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
