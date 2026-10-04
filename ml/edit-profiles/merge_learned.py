"""Merge profiles fitted from FiveK RAW before/after pairs into the shipped profile table.

Reads `ml/edit-profiles/fivek_expert_<x>.json` (written by `crates/aura-app/tests/profile_fit.rs`)
and replaces or appends the matching `fivek-expert-<x>` entry in
`crates/aura-app/config/edit_profiles.json`. The prose (name, tagline) is authored here after
reading the fitted medians; every number comes from the fit.

Usage:  python ml/edit-profiles/merge_learned.py c a b d e
"""

from __future__ import annotations

import io
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
TABLE = ROOT / "crates/aura-app/config/edit_profiles.json"

# How each retoucher's measured style reads, written after looking at the fitted medians.
PROSE = {
    "a": ("Pro Retoucher A", "Rich colour, deeper blacks, clean neutral white balance."),
    "b": ("Pro Retoucher B", "Warm, gentle and light-handed."),
    "c": ("Pro Retoucher C", "Vivid and punchy with open shadows - the FiveK reference edit."),
    "d": ("Pro Retoucher D", "Warm light, open shadows and deep blacks."),
    "e": ("Pro Retoucher E", "Golden warmth, rich colour and open shadows."),
}

SWATCH = {
    "a": ["#e9e4dc", "#c9b6a2", "#7f9a7a", "#8fa8c4", "#3c3a38"],
    "b": ["#f1e9df", "#d8b28f", "#6fa06a", "#7da2d0", "#2f2c2a"],
    "c": ["#ece6de", "#d1b39a", "#789a6c", "#86a4c8", "#34312e"],
    "d": ["#efe2d2", "#d9a77c", "#88985c", "#8c9fb8", "#2b2522"],
    "e": ["#f4eee6", "#e0b48c", "#6aa65e", "#6f9fd6", "#2a2826"],
}


def describe(adjust: dict) -> list[str]:
    """The technique, read back off the measured numbers."""
    steps = []
    ev = adjust.get("exposure", 0)
    if abs(ev) >= 0.05:
        steps.append(f"Exposure {ev:+.2f} EV beyond AURA's own correction")
    for key, label in [("contrast", "Contrast"), ("highlights", "Highlights"), ("shadows", "Shadows"),
                       ("whites", "Whites"), ("blacks", "Blacks"), ("vibrance", "Vibrance"),
                       ("saturation", "Saturation"), ("tint", "Tint")]:
        value = adjust.get(key, 0)
        if value:
            steps.append(f"{label} {value:+d}")
    temperature = adjust.get("temperature", 0)
    if temperature:
        steps.append(f"White balance {temperature:+d} K ({'warmer' if temperature > 0 else 'cooler'})")
    if adjust.get("curve"):
        steps.append("A tone curve through " + ", ".join(f"({x}, {y})" for x, y in adjust["curve"][1:-1]))
    return steps or ["Very close to AURA's own automatic correction"]


def main(experts: list[str]) -> int:
    table = json.loads(TABLE.read_text(encoding="utf-8"))
    for expert in experts:
        fitted = json.loads((ROOT / f"ml/edit-profiles/fivek_expert_{expert}.json").read_text(encoding="utf-8"))
        adjust = {k: v for k, v in fitted["adjust"].items() if v not in (0, 0.0, [], {}, None)}
        if isinstance(adjust.get("exposure"), float):
            adjust["exposure"] = round(adjust["exposure"], 2)
        # Learned from RAW: a camera JPEG already carries the contrast and colour the retoucher
        # lifted out of a flat sensor render, so it gets half of the look.
        adjust["developedStrength"] = 0.5
        name, tagline = PROSE[expert]
        ev = {k: round(v, 2) if isinstance(v, float) else v for k, v in fitted["evidence"].items()}
        entry = {
            "id": f"fivek-expert-{expert}",
            "name": name,
            "category": "Learned",
            "tagline": tagline,
            "description": (
                f"Measured, not written: AURA decoded {ev['trainingPairs']} camera RAW files, recovered the settings "
                f"that reproduce professional retoucher {expert.upper()}'s finished photographs through its own renderer, "
                "and kept the median of what the retoucher did beyond AURA's automatic correction. "
                f"On {ev['heldOutPairs']} RAW photos it never learned from, it brings AURA from "
                f"{ev['autoDe00']:.1f} to {ev['profileDe00']:.1f} dE00 of the retoucher's final."
            ),
            "bestFor": ["Everyday", "Travel", "Portraits", "Landscape"],
            "technique": describe(adjust),
            "origin": "learned",
            "sources": [
                {"title": "MIT-Adobe FiveK dataset (Bychkovsky et al., CVPR 2011)",
                 "url": "https://data.csail.mit.edu/graphics/fivek/"}
            ],
            "evidence": ev,
            "swatch": SWATCH[expert],
            "adjust": adjust,
        }
        table["profiles"] = [p for p in table["profiles"] if p["id"] != entry["id"]] + [entry]
        print(f"{entry['id']}: {ev['autoDe00']} -> {ev['profileDe00']} dE00 on {ev['heldOutPairs']} held-out pairs")
    text = json.dumps(table, indent=2, ensure_ascii=False) + "\n"
    io.open(TABLE, "w", encoding="utf-8", newline="\n").write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:] or ["c"]))
