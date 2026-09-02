#!/usr/bin/env bash
# Sign one artefact. Refuses rather than producing something unsigned that looks signed.
#
#   bash ops/sign/sign.sh windows dist/aura-setup.exe
#   bash ops/sign/sign.sh macos   dist/AURA.app
#
# ## Why this refuses instead of warning
#
# A signing script that warns and exits 0 produces an unsigned artefact that a release pipeline
# treats as signed, and the first person to find out is a photographer whose operating system
# refuses to launch it. Every missing precondition here is an exit 1.
#
# ## What it never does
#
# It never reads a key. The key is in a hardware token or a cloud HSM and this calls a signing
# service; a script that could read a key is a script whose CI runner holds one, and a key on a
# runner is a key in every snapshot of that runner.
set -euo pipefail

platform="${1:-}"
artefact="${2:-}"

if [ -z "$platform" ] || [ -z "$artefact" ]; then
  echo "usage: sign.sh <windows|macos> <artefact>" >&2
  exit 1
fi

if [ ! -e "$artefact" ]; then
  echo "sign: $artefact does not exist" >&2
  exit 1
fi

require() {
  local name="$1"
  if [ -z "${!name:-}" ]; then
    echo "sign: $name is not set. This build cannot be signed, and an unsigned build must not" >&2
    echo "      be released. See ops/sign/README.md." >&2
    exit 1
  fi
}

case "$platform" in
  windows)
    # The timestamp URL is required, not optional. Without it every installed copy stops verifying
    # the day the certificate expires, which for somebody who installed two years ago is an
    # operating system refusing to launch software that has not changed.
    require AURA_SIGN_CERT_THUMBPRINT
    require AURA_SIGN_TIMESTAMP_URL
    if ! command -v signtool >/dev/null 2>&1; then
      echo "sign: signtool is not on PATH" >&2
      exit 1
    fi
    signtool sign \
      /sha1 "$AURA_SIGN_CERT_THUMBPRINT" \
      /fd sha256 \
      /tr "$AURA_SIGN_TIMESTAMP_URL" \
      /td sha256 \
      "$artefact"
    signtool verify /pa /v "$artefact"
    ;;

  macos)
    require AURA_SIGN_IDENTITY
    if ! command -v codesign >/dev/null 2>&1; then
      echo "sign: codesign is not on PATH" >&2
      exit 1
    fi
    # `--options runtime` is the hardened runtime, which notarisation requires. AURA loads no
    # third-party code at runtime, so it can take the strict option without an entitlement that
    # weakens it.
    codesign \
      --force \
      --deep \
      --timestamp \
      --options runtime \
      --sign "$AURA_SIGN_IDENTITY" \
      "$artefact"
    codesign --verify --strict --verbose=2 "$artefact"
    ;;

  *)
    echo "sign: unknown platform '$platform'" >&2
    exit 1
    ;;
esac

echo "sign: $artefact signed and verified for $platform"
