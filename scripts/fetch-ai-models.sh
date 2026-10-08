#!/usr/bin/env bash
# Fetch ONNX Runtime (DirectML) and the learned masking models, each checked against its
# pinned SHA-256. ADR-0103.
#
#   scripts/fetch-ai-models.sh <dir>
#
# Writes into <dir>: onnxruntime.dll, onnxruntime_providers_shared.dll, DirectML.dll, their
# licences, and models/ with the four model files. Copy <dir> beside aura-desktop.exe (the
# DLLs next to the executable, models/ as a folder next to it). Nothing here is committed:
# the files are 550 MB, and the hashes below are what the application checks on every load.
set -euo pipefail

out="${1:?usage: fetch-ai-models.sh <dir>}"
mkdir -p "$out/models"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fetch() { # url file sha256
  local url="$1" file="$2" sum="$3"
  if [ -f "$file" ] && [ "$(sha256sum "$file" | cut -c1-64)" = "$sum" ]; then
    echo "have  $(basename "$file")"
    return
  fi
  echo "fetch $(basename "$file")"
  curl -fsSL -o "$file.part" "$url"
  local got
  got="$(sha256sum "$file.part" | cut -c1-64)"
  if [ "$got" != "$sum" ]; then
    rm -f "$file.part"
    echo "fetch-ai-models: $(basename "$file") is $got, expected $sum" >&2
    exit 1
  fi
  mv "$file.part" "$file"
}

# ONNX Runtime 1.24.4 with DirectML (MIT), and DirectML 1.15.4 (Microsoft redistributable
# terms). The packages are not pinned; the DLLs taken out of them are, below.
curl -fsSL -o "$tmp/ort.nupkg" "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.24.4/microsoft.ml.onnxruntime.directml.1.24.4.nupkg"
unzip -jo "$tmp/ort.nupkg" runtimes/win-x64/native/onnxruntime.dll runtimes/win-x64/native/onnxruntime_providers_shared.dll LICENSE ThirdPartyNotices.txt -d "$tmp/ort" >/dev/null
curl -fsSL -o "$tmp/dml.nupkg" "https://api.nuget.org/v3-flatcontainer/microsoft.ai.directml/1.15.4/microsoft.ai.directml.1.15.4.nupkg"
unzip -jo "$tmp/dml.nupkg" bin/x64-win/DirectML.dll LICENSE.txt -d "$tmp/dml" >/dev/null

check() { # file sha256
  [ "$(sha256sum "$1" | cut -c1-64)" = "$2" ] || { echo "fetch-ai-models: $1 does not match" >&2; exit 1; }
}
check "$tmp/ort/onnxruntime.dll" e7eedec6a6f26dc39dc948276a75ef6d2bee3fff944d874ceed0bbd3b97bff40
check "$tmp/ort/onnxruntime_providers_shared.dll" 265c8daf29637cb259cac8be9f08f2cd45f3883f0f0e4949cbfddd5b4cbec3b6
check "$tmp/dml/DirectML.dll" 9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1
cp "$tmp/ort/onnxruntime.dll" "$tmp/ort/onnxruntime_providers_shared.dll" "$tmp/dml/DirectML.dll" "$out/"
cp "$tmp/ort/LICENSE" "$out/ONNXRUNTIME-LICENSE.txt"
cp "$tmp/ort/ThirdPartyNotices.txt" "$out/ONNXRUNTIME-ThirdPartyNotices.txt"
cp "$tmp/dml/LICENSE.txt" "$out/DIRECTML-LICENSE.txt"

# The models, pinned to the hashes in crates/aura-vision/src/ai.rs.
fetch "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx" \
  "$out/models/isnet_general.onnx" 60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a
fetch "https://huggingface.co/JianyuanWang/skyseg/resolve/main/skyseg.onnx" \
  "$out/models/skyseg.onnx" ab9c34c64c3d821220a2886a4a06da4642ffa14d5b30e8d5339056a089aa1d39
fetch "https://huggingface.co/vietanhdev/segment-anything-2.1-onnx-models/resolve/main/sam2.1_hiera_tiny_20260221.zip" \
  "$tmp/sam.zip" c602cb3f6fd297312a415f885f0df3f0eef9fbb334b069c9d30e828f2ae7c69a
unzip -jo "$tmp/sam.zip" -d "$tmp/sam" >/dev/null
mv "$tmp/sam/sam2.1_hiera_tiny.encoder.onnx" "$out/models/sam21_tiny_encoder.onnx"
mv "$tmp/sam/sam2.1_hiera_tiny.decoder.onnx" "$out/models/sam21_tiny_decoder.onnx"
check "$out/models/sam21_tiny_encoder.onnx" 667384d1e686de6828b841ac8a24db0fafa2b3452494225f82eeedac56141230
check "$out/models/sam21_tiny_decoder.onnx" c40f5aa7d37b681cd500481a85d44839fd81c93dce1e86271a2c866470d22105
echo "fetch-ai-models: runtime and four models in $out"
