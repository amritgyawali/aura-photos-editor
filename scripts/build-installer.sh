#!/usr/bin/env bash
# Build the Windows installer (NSIS) around an already built aura-desktop.exe. ADR-0105.
#
#   CARGO_TARGET_DIR=C:/Users/me/aura-target scripts/build-installer.sh [runtime-dir]
#
# runtime-dir holds what scripts/fetch-ai-models.sh writes: onnxruntime.dll,
# onnxruntime_providers_shared.dll, DirectML.dll, their licence files and models/. Default: ./app.
#
# Every file that goes into the installer is checked against its pinned SHA-256 first, so a
# download that was cut short or a disk that corrupted a model cannot ship. The executable is the
# one cargo built in $CARGO_TARGET_DIR/debug - the shell's optimised dev profile, which is what
# this machine can build (ui/src-tauri/Cargo.toml says why).
#
# Signing: set AURA_SIGN_COMMAND to a command that signs one file passed as %1, for example
#   AURA_SIGN_COMMAND='signtool sign /sha1 <thumbprint> /fd sha256 /tr http://timestamp.digicert.com /td sha256 %1'
# Tauri runs it on the executable, the uninstaller and the installer. Without it the installer is
# built UNSIGNED, says so, and Windows SmartScreen will warn whoever runs it.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
runtime="$(cd "${1:-$root/app}" && pwd)"
target="${CARGO_TARGET_DIR:?set CARGO_TARGET_DIR to where aura-desktop.exe was built}"
target="$(cd "$target" && pwd)"
exe="$target/debug/aura-desktop.exe"
loader="$target/debug/WebView2Loader.dll"

[ -f "$exe" ] || { echo "build-installer: $exe not found - build the shell first" >&2; exit 1; }
[ -f "$loader" ] || { echo "build-installer: $loader not found" >&2; exit 1; }

check() { # file sha256
  [ -f "$1" ] || { echo "build-installer: missing $1" >&2; exit 1; }
  local got
  got="$(sha256sum "$1" | cut -c1-64)"
  [ "$got" = "$2" ] || { echo "build-installer: $1 is $got, expected $2 - refusing to ship it" >&2; exit 1; }
  echo "ok    $(basename "$1")"
}
check "$runtime/onnxruntime.dll" e7eedec6a6f26dc39dc948276a75ef6d2bee3fff944d874ceed0bbd3b97bff40
check "$runtime/onnxruntime_providers_shared.dll" 265c8daf29637cb259cac8be9f08f2cd45f3883f0f0e4949cbfddd5b4cbec3b6
check "$runtime/DirectML.dll" 9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1
check "$runtime/models/isnet_general.onnx" 60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a
check "$runtime/models/skyseg.onnx" ab9c34c64c3d821220a2886a4a06da4642ffa14d5b30e8d5339056a089aa1d39
check "$runtime/models/sam21_tiny_encoder.onnx" 667384d1e686de6828b841ac8a24db0fafa2b3452494225f82eeedac56141230
check "$runtime/models/sam21_tiny_decoder.onnx" c40f5aa7d37b681cd500481a85d44839fd81c93dce1e86271a2c866470d22105
for f in ONNXRUNTIME-LICENSE.txt ONNXRUNTIME-ThirdPartyNotices.txt DIRECTML-LICENSE.txt; do
  [ -f "$runtime/$f" ] || { echo "build-installer: missing $runtime/$f" >&2; exit 1; }
done

win() { cygpath -m "$1" 2>/dev/null || echo "$1"; }
conf="$(mktemp -d)/tauri.installer.conf.json"
sign_json=""
if [ -n "${AURA_SIGN_COMMAND:-}" ]; then
  sign_json=",\"signCommand\": $(printf '%s' "$AURA_SIGN_COMMAND" | python -c 'import json,sys; print(json.dumps(sys.stdin.read()))')"
else
  echo "warn  AURA_SIGN_COMMAND is not set: this installer will be UNSIGNED (ops/sign/README.md)" >&2
fi
cat > "$conf" <<EOF
{
  "bundle": {
    "resources": {
      "$(win "$loader")": "WebView2Loader.dll",
      "$(win "$runtime/onnxruntime.dll")": "onnxruntime.dll",
      "$(win "$runtime/onnxruntime_providers_shared.dll")": "onnxruntime_providers_shared.dll",
      "$(win "$runtime/DirectML.dll")": "DirectML.dll",
      "$(win "$runtime/ONNXRUNTIME-LICENSE.txt")": "licences/ONNXRUNTIME-LICENSE.txt",
      "$(win "$runtime/ONNXRUNTIME-ThirdPartyNotices.txt")": "licences/ONNXRUNTIME-ThirdPartyNotices.txt",
      "$(win "$runtime/DIRECTML-LICENSE.txt")": "licences/DIRECTML-LICENSE.txt",
      "$(win "$runtime/models/isnet_general.onnx")": "models/isnet_general.onnx",
      "$(win "$runtime/models/skyseg.onnx")": "models/skyseg.onnx",
      "$(win "$runtime/models/sam21_tiny_encoder.onnx")": "models/sam21_tiny_encoder.onnx",
      "$(win "$runtime/models/sam21_tiny_decoder.onnx")": "models/sam21_tiny_decoder.onnx"
    },
    "windows": { "nsis": { "installMode": "currentUser" }$sign_json }
  }
}
EOF

cd "$root/ui"
npx tauri bundle --debug --bundles nsis --config "$conf" --ci

out="$(ls -t "$target"/debug/bundle/nsis/*-setup.exe | head -1)"
echo
echo "installer: $out"
echo "size:      $(du -h "$out" | cut -f1)"
echo "sha256:    $(sha256sum "$out" | cut -c1-64)"
[ -n "${AURA_SIGN_COMMAND:-}" ] || echo "signed:    NO - see ops/sign/README.md before giving this to anybody"
