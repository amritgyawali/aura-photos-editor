# Local-light texture healing validation - 2026-10-06

## Code and automated checks

The implementation is in PR #48; follow-up validation and fixes are in PR #49.
PR #48 was merged externally before validation finished. No merge was performed
by this work session.

At `ece073e`, all GitHub checks passed: strict formatting/lint/banned-pattern checks,
Windows/macOS/Linux Rust tests, phase gates, benchmarks, frontend tests/build,
desktop shell type-check, dependency policy and security scanning.

Local checks passed: eight deep-cleanup tests, 35 renderer integration tests, the
669-test frontend suite, and the additional preset regression. Later donor-boundary
and lesion-rim regressions passed in the full GitHub suites. Exposure-level fixtures
check lighting and texture; they are not a demographic accuracy study.

## Actual desktop workflow

Built the native Windows shell with the production UI and `custom-protocol`.
After a disk-space interruption, moved the temporary build cache and resumed
successfully. The source checkout is `C:\Users\amrit\aura-professional-retouch`.

The test used an isolated catalog and WebView profile. AURA controls imported the
existing 1024 x 1536 synthetic acne portrait, ran automatic editing followed by
Deep acne cleanup, saved the recipe, and exported a JPEG with readback verification.
The automation script did not edit pixels or generate a replacement face.

- 236 saved operations, including 220 `textureHeal` patch repairs and a
  `preserveMicrotexture` frequency finish.
- Retouch: 12.26 seconds; export and verification: 9.30 seconds on this PC.
  These are single-run timings, not a controlled performance comparison.
- Verified export: 486328 bytes; BLAKE3
  `2812aacede67338c71ad8f7f27d4beb9d7ac7cb8258f9215e8f2f3f52d263028`.
- Original SHA-256 unchanged:
  `c11925539965ac39f67a01b46f5275f6cac1f25e7a2f079767ea4f97a34ddd9a`.
- Visual inspection: substantially cleaner left cheek and lower cheek marks than
  the previous export, with visible pores and unchanged facial geometry. Some
  shadowed right-cheek marks and strong specular shine remain. Lowering mask
  precision from 50 to 15 did not materially improve them; restored 50.
- Restarting the installed bundle preserved the recipe hash, 220 texture repairs,
  and the permanent six-section studio layout.

Evidence is local under `output/professional-retouch`: `verification.json`,
`recipe.json`, before/after restart snapshots, screenshots, and
`export/gallery/restored-studio-verification_0001.jpg`. Portraits are not committed.
Reproduce with `scripts/verify-restored-studio.py --require-texture-heal` against
an isolated native debug build.

## Installation and limits

Installed `D:\aura-restore-studio\app\aura-desktop.exe`, SHA-256
`80705e750da1b9496906f41ba350474af9308ae678353b0154f5b359463e3918`.
The previous executable is backed up beside it as
`aura-desktop-before-texture-heal.exe`. Existing launch shortcuts use this bundle
on the next launch. Only the isolated test process was stopped for verification.

This validates improved local repair, persistence and export on this portrait.
It does not establish removal of every mark on every photograph, exact recovery
of obscured pores, or equivalence to a professional human retoucher. Borrowed
texture comes from the same photo; hidden original texture cannot be recovered.
