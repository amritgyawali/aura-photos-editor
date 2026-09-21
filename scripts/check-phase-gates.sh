#!/usr/bin/env bash
# Run every phase gate, 01 to 30, and fail on the first one that does not.
#
# ## Why this file exists
#
# The independent review of phases 01 to 30 (`docs/progress/PHASE-01-30-REVIEW.md`, section 6.1)
# found that sixteen of the thirty phase gates were invoked by nothing on a push. They passed -
# the review ran all of them by hand - but nothing would have caught it if they had stopped.
# Among the sixteen were phase 30's gate, which checks the delivery guarantee, and phase 13's,
# which checks that nothing acts unattended while the product is uncalibrated.
#
# The fix is deliberately a list of one thing rather than thirty CI steps: a workflow that names
# each gate individually is a workflow somebody forgets to extend, which is exactly how the
# omission happened. This script enumerates the gate modules that `aura-cli` actually compiles
# and refuses to run if the count is not thirty, so adding a phase without wiring its gate in is
# a red build rather than a silent gap.
#
# ## Usage
#
#   bash scripts/check-phase-gates.sh              # all thirty, release
#   bash scripts/check-phase-gates.sh 06 07 08     # a subset
#   AURA_PHASE_GATE_PROFILE=debug bash scripts/check-phase-gates.sh 12
#
# On the reference Windows machine the MSVC linker is absent, so the GNU host toolchain has to be
# named - the same requirement CLAUDE.md records for the whole workspace:
#
#   RUSTUP_TOOLCHAIN=1.97.1-x86_64-pc-windows-gnu bash scripts/check-phase-gates.sh
#
# It is deliberately not set here. CI runs on a host whose default toolchain is the right one, and
# a script that pinned a Windows triple would fail everywhere else.
#
# `AURA_PERF_HOST_SCALE` is honoured by the gates themselves. Phase 14's proxy-render guardrail
# was measured on a development machine; a shared runner is several times slower, so CI sets
# `AURA_PERF_HOST_SCALE=4` for the same reason lane 2 does. Sizes, counts and costs are never
# scaled - a byte is a byte on any host.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

profile="${AURA_PHASE_GATE_PROFILE:-release}"
case "$profile" in
  release) profile_flag="--release" ;;
  debug)   profile_flag="" ;;
  *) echo "check-phase-gates: AURA_PHASE_GATE_PROFILE must be release or debug" >&2; exit 2 ;;
esac

# The gates that exist, taken from the source rather than from a list in this file. `main.rs`
# declares one `mod phaseNN;` per gate; phases 01 and 02 are the two that predate the numbering
# and live in other modules, so they are named here and asserted below.
mapfile -t declared < <(grep -oE '^mod phase[0-9]{2};' crates/aura-cli/src/main.rs \
  | grep -oE '[0-9]{2}' | sort -u)
all=(01 02 "${declared[@]}")

if [ "${#all[@]}" -ne 30 ]; then
  echo "check-phase-gates: found ${#all[@]} gate modules, expected 30" >&2
  printf 'check-phase-gates:   %s\n' "${all[@]}" >&2
  exit 1
fi

if [ "$#" -gt 0 ]; then
  wanted=("$@")
else
  wanted=("${all[@]}")
fi

echo "check-phase-gates: building aura-cli ($profile)"
# shellcheck disable=SC2086
cargo build $profile_flag --package aura-cli >/dev/null

failed=()
for phase in "${wanted[@]}"; do
  work="target/phase${phase}-verify"
  echo
  echo "----- phase $phase -----"
  if [ "$phase" = "01" ]; then
    # Phase 01's gate is the bare `verify`, from before `--phase` existed.
    # shellcheck disable=SC2086
    if cargo run $profile_flag --quiet --package aura-cli -- verify --work "$work"; then
      continue
    fi
  else
    # shellcheck disable=SC2086
    if cargo run $profile_flag --quiet --package aura-cli -- verify --phase "$phase" --work "$work"; then
      continue
    fi
  fi
  failed+=("$phase")
done

echo
if [ "${#failed[@]}" -gt 0 ]; then
  echo "check-phase-gates: FAILED - ${failed[*]}" >&2
  exit 1
fi
echo "check-phase-gates: ${#wanted[@]} / ${#wanted[@]} gates pass"
