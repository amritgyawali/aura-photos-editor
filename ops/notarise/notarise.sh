#!/usr/bin/env bash
# Notarise and staple a signed macOS artefact.
#
#   bash ops/notarise/notarise.sh dist/AURA.dmg
#
# Sign, then notarise, then staple - in that order, and this refuses to do the middle one on
# something that is not signed. Each wrong order costs a submission round trip to discover:
# notarising an unsigned build is rejected, stapling before the ticket exists silently does
# nothing, and signing after stapling invalidates the ticket.
set -euo pipefail

artefact="${1:-}"
if [ -z "$artefact" ] || [ ! -e "$artefact" ]; then
  echo "usage: notarise.sh <signed artefact>" >&2
  exit 1
fi

for name in AURA_NOTARY_KEY_ID AURA_NOTARY_ISSUER_ID AURA_NOTARY_KEY_PATH; do
  if [ -z "${!name:-}" ]; then
    echo "notarise: $name is not set. See ops/notarise/README.md." >&2
    exit 1
  fi
done

if ! command -v xcrun >/dev/null 2>&1; then
  echo "notarise: xcrun is not on PATH; this runs on macOS only" >&2
  exit 1
fi

# The precondition, checked rather than assumed. A rejected submission twenty minutes from now is
# a worse way to find this out.
if ! codesign --verify --strict "$artefact" >/dev/null 2>&1; then
  echo "notarise: $artefact is not signed. Run ops/sign/sign.sh macos first." >&2
  exit 1
fi

echo "notarise: submitting $artefact"
xcrun notarytool submit "$artefact" \
  --key "$AURA_NOTARY_KEY_PATH" \
  --key-id "$AURA_NOTARY_KEY_ID" \
  --issuer "$AURA_NOTARY_ISSUER_ID" \
  --wait

# Stapling is what makes the ticket verify offline. Without it, a photographer installing on a
# venue's guest wifi gets a dialogue that reads exactly like the one for unsigned software.
echo "notarise: stapling"
xcrun stapler staple "$artefact"
xcrun stapler validate "$artefact"

echo "notarise: $artefact notarised and stapled"
