#!/usr/bin/env python3
"""Every third-party component AURA ships, its licence, and the notice text that must go with it.

    python scripts/third-party-notices.py [--out THIRD-PARTY-NOTICES.txt] [--runtime <dir>]

Reads three sources:

* the Rust crates compiled into ``aura-desktop.exe`` - the shell's normal dependencies, resolved for
  Windows, from ``cargo metadata`` (build scripts and test-only crates are not shipped and are left
  out);
* the npm packages bundled into the window - ``dependencies`` of ``ui/package.json`` and everything
  they pull in, never ``devDependencies``;
* the files beside the executable: ONNX Runtime, DirectML and the masking models (ADR-0103), and the
  vendored detectors, from a fixed table below.

It writes one text file with every licence text, de-duplicated, and **exits 1 when anything is under
a licence that a closed-source, paid product cannot ship** (GPL, LGPL, AGPL, SSPL, a non-commercial
Creative Commons licence, or no licence at all), naming the component - and when a shipped edit
profile was derived from a research-only data set such as MIT-Adobe FiveK. ADR-0106.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Licences a closed-source commercial product may ship, with notices.
ALLOWED = {
    "MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "0BSD",
    "Unicode-3.0", "Unicode-DFS-2016", "CC0-1.0", "BSL-1.0", "Unlicense", "MPL-2.0",
    "CDLA-Permissive-2.0", "LLVM-exception", "Apache-2.0 WITH LLVM-exception", "BlueOak-1.0.0",
    "OFL-1.1", "CC-BY-4.0", "Python-2.0", "WTFPL",
}
# Shippable, but with an obligation beyond keeping the notice.
OBLIGATIONS = {
    "MPL-2.0": "file-level copyleft: if these files are modified, their source must be offered. AURA uses them unmodified.",
}
FORBIDDEN = re.compile(r"\b(A?GPL|LGPL|SSPL|CC-BY-NC|CC-BY-SA|EUPL|OSL|CPAL)", re.I)

# Shipped beside the executable or compiled into it, with where their licence text lives.
BUNDLED = [
    ("ONNX Runtime 1.24.4", "MIT", "runtime:ONNXRUNTIME-LICENSE.txt"),
    ("ONNX Runtime third-party notices", "MIT", "runtime:ONNXRUNTIME-ThirdPartyNotices.txt"),
    ("DirectML 1.15.4", "Microsoft DirectML redistributable licence", "runtime:DIRECTML-LICENSE.txt"),
    ("IS-Net general-use segmentation model (DIS)", "Apache-2.0", "text:apache"),
    ("Sky segmentation model (skyseg)", "MIT", "text:mit:Copyright (c) the skyseg authors"),
    ("SAM 2.1 Hiera Tiny, ONNX export", "Apache-2.0", "text:apache"),
    ("MediaPipe selfie multiclass segmenter", "Apache-2.0", "file:assets/models/selfie_multiclass/LICENSE"),
    ("YuNet face detector (OpenCV Zoo)", "MIT", "file:assets/models/yunet/LICENSE"),
    ("OpenCV Haar cascades (frontal face alt2, profile face, eye)", "BSD-3-Clause (Intel License Agreement)", "cascade"),
]

MIT_TEMPLATE = """{copyright}

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
"""

LICENCE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|COPYRIGHT|UNLICENSE)([-_.].*)?$", re.I)


def allowed(expression: str) -> bool:
    """An SPDX expression is shippable when some OR-branch has every AND-term allowed."""
    expression = expression.replace("/", " OR ").strip()
    if not expression or FORBIDDEN.search(expression) and " OR " not in expression:
        return False
    for branch in re.split(r"\s+OR\s+", expression.strip("() ")):
        terms = [t.strip("() ") for t in re.split(r"\s+AND\s+", branch)]
        if all(t in ALLOWED or t.split(" WITH ")[0] in ALLOWED for t in terms if t):
            return True
    return False


def licence_texts(directory: Path) -> list[tuple[str, str]]:
    out = []
    try:
        names = sorted(os.listdir(directory))
    except OSError:
        return out
    for name in names:
        path = directory / name
        if path.is_file() and LICENCE_FILE.match(name):
            try:
                out.append((name, path.read_text(encoding="utf-8", errors="replace").strip()))
            except OSError:
                pass
    return out


def rust_components() -> list[dict]:
    manifest = ROOT / "ui" / "src-tauri" / "Cargo.toml"
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--manifest-path", str(manifest),
         "--filter-platform", "x86_64-pc-windows-gnu"],
        cwd=ROOT,
    ))
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    shipped, stack = set(), [root]
    while stack:
        current = stack.pop()
        if current in shipped:
            continue
        shipped.add(current)
        for dep in nodes[current]["deps"]:
            kinds = {k.get("kind") for k in dep.get("dep_kinds", [])}
            if None in kinds:  # a normal dependency, compiled into the executable
                stack.append(dep["pkg"])
    out = []
    for pid in sorted(shipped, key=lambda i: (packages[i]["name"], packages[i]["version"])):
        p = packages[pid]
        if p.get("source") is None:  # AURA's own crates
            continue
        out.append({
            "name": f"{p['name']} {p['version']}",
            "licence": p.get("license") or ("file: " + p["license_file"] if p.get("license_file") else ""),
            "texts": licence_texts(Path(p["manifest_path"]).parent),
            "origin": "crate",
        })
    return out


def npm_components() -> list[dict]:
    ui = ROOT / "ui"
    listing = subprocess.run(
        "npm ls --omit=dev --all --parseable", cwd=ui, capture_output=True, text=True, shell=True
    ).stdout.split()
    out, seen = [], set()
    for line in listing:
        path = Path(line)
        if path.resolve() == ui.resolve():
            continue
        package = path / "package.json"
        try:
            data = json.loads(package.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        key = (data.get("name"), data.get("version"))
        if key in seen:
            continue
        seen.add(key)
        licence = data.get("license") or ""
        if isinstance(licence, dict):
            licence = licence.get("type", "")
        out.append({"name": f"{key[0]} {key[1]}", "licence": licence, "texts": licence_texts(path), "origin": "npm"})
    return out


def bundled_components(runtime: Path | None) -> list[dict]:
    apache = (ROOT / "assets" / "models" / "selfie_multiclass" / "LICENSE")
    out = []
    for name, licence, source in BUNDLED:
        texts: list[tuple[str, str]] = []
        kind, _, rest = source.partition(":")
        if kind == "runtime" and runtime and (runtime / rest).is_file():
            texts = [(rest, (runtime / rest).read_text(encoding="utf-8", errors="replace").strip())]
        elif kind == "file" and (ROOT / rest).is_file():
            texts = [(Path(rest).name, (ROOT / rest).read_text(encoding="utf-8", errors="replace").strip())]
        elif kind == "text" and rest == "apache" and apache.is_file():
            texts = [("Apache-2.0", apache.read_text(encoding="utf-8", errors="replace").strip())]
        elif kind == "text" and rest.startswith("mit:"):
            texts = [("MIT", MIT_TEMPLATE.format(copyright=rest[4:]).strip())]
        elif kind == "cascade":
            cascade = ROOT / "crates" / "aura-portrait" / "data" / "frontalface_alt2.cascade"
            header = []
            if cascade.is_file():
                for line in cascade.read_text(encoding="utf-8", errors="replace").splitlines()[:60]:
                    if line.startswith("#"):
                        header.append(line.lstrip("# "))
            texts = [("Intel License Agreement", "\n".join(header).strip() or "See docs/model-cards/haar_cascades.md")]
        out.append({"name": name, "licence": licence, "texts": texts, "origin": "bundled"})
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=ROOT / "THIRD-PARTY-NOTICES.txt")
    parser.add_argument("--runtime", type=Path, default=None, help="folder holding the ONNX Runtime and DirectML licence files")
    args = parser.parse_args()

    components = rust_components() + npm_components() + bundled_components(args.runtime)
    problems, obligations = [], set()
    # Data sets licensed for research only must not reach the product, even as derived numbers.
    profiles = json.loads((ROOT / "crates" / "aura-app" / "config" / "edit_profiles.json").read_text(encoding="utf-8"))
    for profile in profiles.get("profiles", []):
        dataset = json.dumps(profile.get("evidence") or {}) + json.dumps(profile.get("sources") or [])
        if re.search(r"fivek|research[- ]only", dataset, re.I):
            problems.append(f"edit profile {profile.get('id')}: derived from a research-only data set (FiveK)")
    for c in components:
        if c["origin"] == "bundled":
            if not c["texts"]:
                problems.append(f"{c['name']}: its licence text was not found")
            continue
        if not allowed(c["licence"]):
            problems.append(f"{c['name']} ({c['origin']}): licence '{c['licence'] or 'none declared'}' cannot ship in a closed-source paid product")
        for spdx, note in OBLIGATIONS.items():
            if spdx in c["licence"] and not allowed(c["licence"].replace(spdx, "")):
                obligations.add(f"{c['name']}: {note}")

    # A package that declares its licence but ships no file of its own gets the standard text.
    apache_text = next((t for c in components for n, t in c["texts"] if n == "Apache-2.0"), "")
    for c in components:
        if c["texts"] or c["origin"] == "bundled":
            continue
        if "MIT" in c["licence"]:
            author = c["name"].rsplit(" ", 1)[0]
            c["texts"] = [("MIT", MIT_TEMPLATE.format(copyright=f"Copyright (c) the {author} authors").strip())]
        elif "Apache-2.0" in c["licence"] and apache_text:
            c["texts"] = [("Apache-2.0", apache_text)]

    texts: dict[str, list[str]] = {}
    lines = [
        "AURA Photo Studio - third-party notices",
        "",
        "AURA includes the following third-party software and data. Each is used under the licence",
        "named beside it; the full licence texts follow. AURA itself is proprietary software.",
        "",
    ]
    for c in components:
        lines.append(f"  {c['name']}  -  {c['licence'] or 'see text'}")
        for _, text in c["texts"]:
            texts.setdefault(text, []).append(c["name"])
    if obligations:
        lines += ["", "Obligations beyond keeping these notices:"] + [f"  {o}" for o in sorted(obligations)]
    lines += ["", "=" * 78, ""]
    for text, names in texts.items():
        shown = ", ".join(names[:12]) + (f" and {len(names) - 12} more" if len(names) > 12 else "")
        lines += [f"Applies to: {shown}", "-" * 78, text, "", "=" * 78, ""]
    args.out.write_text("\n".join(lines), encoding="utf-8", newline="\n")

    crates = sum(c["origin"] == "crate" for c in components)
    npm = sum(c["origin"] == "npm" for c in components)
    print(f"{crates} crates, {npm} npm packages, {len(components) - crates - npm} bundled components, "
          f"{len(texts)} distinct licence texts -> {args.out}")
    for o in sorted(obligations):
        print(f"note  {o}")
    for p in problems:
        print(f"FAIL  {p}", file=sys.stderr)
    missing = [c["name"] for c in components if c["origin"] != "bundled" and not c["texts"]]
    if missing:
        print(f"note  {len(missing)} packages carry no licence file of their own; their SPDX licence is listed: "
              + ", ".join(missing[:8]) + (" ..." if len(missing) > 8 else ""))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
