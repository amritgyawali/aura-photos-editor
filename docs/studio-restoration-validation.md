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

## Final checks and installation

All GitHub checks passed for b54b0b0: Rust tests on Windows/macOS/Linux, phase gates, benchmarks, strict lints, UI tests/build, desktop type-check and dependency policy.

Installed-bundle launcher verification passed with an isolated catalog. Restart preserved the recipe hash, all 236 operations, Deep blemish cleanup enabled, and the 220-spot preference. The user's existing session was never terminated by the verification; only the separate test process was closed.

The installed executable is the successfully built and visually tested restored-studio bundle (SHA-256 `9b83c43d49a9623b8b7a36489474c94a329d4d89b56dda541615c15db998e003`). A later local rebuild hit corrupted compiler metadata, so the launcher retains this tested bundle until an explicit successful `-Rebuild`. Later source changes include lint/test fixes, wording, and main's complete-workflow portrait pass; they are not claimed to be in that installed snapshot.
