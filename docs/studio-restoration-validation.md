# Permanent studio validation ? 2026-10-05

- Production React build passed. The complete retained UI suite passed 667 tests; additional deep-cleanup and limit-toggle tests passed afterward.
- Source reachability check passed: advanced tools remain available within the studio shell. Alternate theme, onboarding and stage-navigation components were removed.
- Banned-pattern check passed. Initial local native test/Clippy attempts encountered corrupted build metadata and compiler crashes; these attempts are not reported as passing.
- Native Windows executable built with bundled production UI. It was launched with a separate debug-only catalog and WebView profile, so the user's unsaved draft was not used for testing.
- Actual AURA controls imported the supplied synthetic acne test portrait and ran local automatic editing, then Deep acne cleanup. The saved recipe contained 236 operations, including 220 patch repairs and a fine-texture-preserving surface finish.
- AURA exported a 1024 ? 1536 JPEG and read it back for verification. Manifest: one file, 488502 bytes, verified true. Source SHA-256 matched the imported copy afterward.
- The fixed plum layout survived a page reload, light OS color-scheme emulation and a stale saved light-theme preference. Native screenshots show the six-section sidebar, collections and photo workspace.
- Visual review: many forehead/cheek marks were reduced; some cheek blemishes and pronounced shine remain. This is evidence of functioning editing and export, not perfect blemish recall or exact pore reconstruction.

Local artifacts are in `output/desktop-check` (screenshots, recipe, export and manifest). Run `scripts/verify-restored-studio.py` against an isolated debug catalog to reproduce the control-driven workflow. All pixel editing is performed by AURA, not the automation script.

The original PR #43 was merged externally during validation. Follow-up fixes are delivered in PR #44 targeting main; no merge was performed by this work session.
