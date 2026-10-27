"""Dump LibRaw's reading of camera files, so aura-raw can be checked against it.

Usage: raw-reference-dump.py <out-dir> <raw file>...

For each file this writes <out-dir>/<n>.u16 (the whole sensor, margins included, as
little-endian 16-bit samples, row-major) and <out-dir>/<n>.json (the path, the sizes, the
margins, the black and white levels, the as-shot white balance, the CFA pattern and the
camera matrix LibRaw would use). `crates/aura-raw/tests/reference_decode.rs` reads them.

LibRaw (through the `rawpy` package) is a test-time oracle only. It is not linked into,
or shipped with, AURA; see ADR-0124.
"""
import json
import os
import sys

import numpy as np
import rawpy


def main() -> int:
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    for index, path in enumerate(sys.argv[2:]):
        record = {"path": path}
        try:
            with rawpy.imread(path) as raw:
                sizes = raw.sizes
                record.update(
                    raw_width=int(sizes.raw_width),
                    raw_height=int(sizes.raw_height),
                    width=int(sizes.width),
                    height=int(sizes.height),
                    left_margin=int(sizes.left_margin),
                    top_margin=int(sizes.top_margin),
                    black=[int(v) for v in raw.black_level_per_channel],
                    white=int(raw.white_level),
                    camera_wb=[float(v) for v in raw.camera_whitebalance],
                    pattern=raw.raw_pattern.tolist() if raw.raw_pattern is not None else None,
                    colour_desc=raw.color_desc.decode("ascii", "replace"),
                    rgb_xyz=[[float(v) for v in row] for row in raw.rgb_xyz_matrix[:3]],
                )
                image = raw.raw_image
                if image is not None and image.ndim == 2:
                    np.ascontiguousarray(image, dtype="<u2").tofile(os.path.join(out, f"{index}.u16"))
                    record["samples"] = f"{index}.u16"
        except Exception as error:  # noqa: BLE001 - recorded, not hidden
            record["error"] = str(error)
        with open(os.path.join(out, f"{index}.json"), "w", encoding="utf-8") as handle:
            json.dump(record, handle, indent=1)
        print(index, path, record.get("error", "ok"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
