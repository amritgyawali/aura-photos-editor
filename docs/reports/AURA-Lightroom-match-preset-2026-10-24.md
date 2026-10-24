# Match a look: reference replacement and Lightroom preset export

Date: 2026-10-24. Palette engine: `reference-palette-transport-v7`.

AURA now replaces an earlier reference treatment from its stable matching baseline, instead of accidentally treating that earlier look as the new starting photograph. A new **Use this look in Lightroom** section at the bottom of the reference-colour panel generates a named, standalone `.xmp` develop preset and exposes a **Download .xmp** link.

**Verified:** 20 independent reference replacements, 20 self-contained preset files, and actual desktop downloads at 100%, 75% and 0% strength. **Not established:** perfect reference matching, identical Lightroom rendering, or successful import in the installed Lightroom Classic. Those are separate questions; this report keeps their results separate.

## What changed

The UI requests `replacePrevious: true` for native analysis, including collection Sync. When a previous session still matches the saved recipe, its original matching baseline is reused even when choosing a different reference. If a later manual edit exists, that current recipe is retained and only the old `reference_grade_v1` treatment is removed before fitting. Exposure, masks, finishing and other manual changes are preserved. The observed current recipe hash still guards the final write against concurrent edits. Older native callers retain the original crossfade policy unless they explicitly request replacement.

Live matching and collection Sync refit older palette engines. Native integration verifies that switching from colour to monochrome and back recovers the original colour model. Baseline tests separately cover later manual work and a change during analysis. The per-photo native guards, bounded backtracking, skin/highlight protection and shadow-contour checks remain active. This change fixes the replacement workflow; it does not change the first-fit palette estimator or establish better colour accuracy on every scene.

The export is generated only from a current, applied, native-checked match. Unapplied controls disable generation. The backend verifies the photo, session, schema, saved recipe hash and checked hash, then checks the saved recipe again after encoding. Stale edits, invalid names and cancelled jobs return an error without modifying the photograph. Changing the photo, reference, controls or recipe revision discards the earlier download. Object URLs remain valid while the download is displayed and are revoked on replacement/unmount.

## What the preset actually contains

Numeric Lightroom sliders cannot describe AURA's reference palette transport. The export therefore contains an embedded RGB-to-RGB table and its complete profile parameters inside a normal develop preset. It does not point to an AURA sidecar, a source-photo path or a separately installed profile.

| Treatment | Representation |
|---|---|
| Saved reference grade, palette transfer and its applied strengths | Embedded 32 x 32 x 32 RGB table |
| Global white balance, exposure, contrast, white/black points, curves, HSL, saturation/vibrance, calibration, colour wheels and monochrome mixer | Baked through the native pointwise colour pipeline into that table |
| Image-dependent highlights/shadows, clarity, texture, dehaze, sharpening, noise reduction, grain and vignette | Supported Lightroom develop settings; Adobe's rendering can differ |
| Local masks, regional matching, face retouching, removal/restoration, crop and geometry | Excluded; they cannot be represented faithfully by this reusable global colour preset |
| Other AURA-only extras | Excluded and disclosed in export warnings |

White balance deserves special care: AURA's 5500 K neutral multiplier is not Lightroom's absolute 5500 K camera white balance. The exporter samples AURA's actual transform and leaves Lightroom's recipient white balance **As Shot**, with zero incremental temperature/tint. It resets already-baked global colour controls to neutral to avoid applying them twice. Image-dependent highlights/shadows are deliberately excluded from the colour sampler; sampling them on a synthetic colour grid would encode the grid's neighbourhood rather than the photographer's scene.

Sampling uses 16-bit native sRGB output. The table contains 32,768 RGB16 nodes, blue varying fastest, encoded as Adobe identity-relative deltas. The payload declares sRGB primaries, sRGB transfer, gamut clipping and an amount range of 0..1. Its MD5 is Adobe's content identifier, not a security mechanism. The payload is zlib compressed and encoded with Adobe's little-endian base85 alphabet. Distinct preset/profile UUIDs, escaped XML names and the `AURA Matched Looks` group are included. The file is SDR; identical RAW/HDR processing is not claimed. A 32-node edge preserves compatibility with the older serialized table-size limit rather than assuming support for the newest SDK's larger tables.

Generation checks 4,096 off-grid Halton colours and 256 neutral-ramp values using tetrahedral interpolation. The displayed mean/p95 OKLab x100 distances describe table approximation of AURA's sampled colour pipeline. They are **not** percentages of matching accuracy or measurements of Lightroom's rendered output.

## Native desktop assessment

Final executable SHA256: `3c090ed35068d18f55201991eb61e6e6349b4ac3528de1c2e947f6673b736408`.

Run: [run-d9694705](../../output/aura-lightroom-preset-2026-10-24/run-d9694705/results.json). [Comparison gallery](../../output/aura-lightroom-preset-2026-10-24/run-d9694705/index.html), [actual desktop screenshot](../../output/aura-lightroom-preset-2026-10-24/run-d9694705/native-preset-workspace.png), [20-case numerical CSV](AURA-Lightroom-preset-cases-2026-10-24.csv), [example downloaded preset](../../output/aura-lightroom-preset-2026-10-24/run-d9694705/downloads/AURA-AURA_Final_Warm_Matte.xmp).

The final tested AURA executable was reopened on this disposable catalogue with the first target's warm-matte match and a generated download ready for review. [Reopened workspace](../../output/aura-lightroom-preset-2026-10-24/run-d9694705/native-final-open.png).

The existing reviewed dataset contains 20 distinct target photographs and six separately sourced reference photographs: 26 unique normalized-pixel hashes, no exact duplicates. The six references have distinct warm-matte, cool-cyan, teal-orange, pastel, warm-punch and monochrome edits. Every target is different from every reference. Photo credits and duplicate-screening evidence remain in the [earlier assessment](AURA-twenty-image-colour-assessment-2026-10-24.md).

The script backs up the previous assessment catalogue through SQLite's backup API into an isolated catalogue. It replaces each target's saved monochrome reference with one of the five colour references, four targets per colour style, without a user reset. It compares every resulting pixel against the earlier fresh-original native fit for the same target/reference pair. It then generates a preset, independently decodes the XML/base85/zlib/RGB16 payload in Python, and applies its tetrahedral interpolation to that target's native original-baseline render. All input/source bytes are checked afterward. No personal catalogue is reset.

| Check | Result |
|---|---:|
| Reference replacement equals fresh-original fit, complete RGB8 pixels | 20/20 |
| Independent XML/table/fingerprint/self-contained format validation | 20/20 |
| Export leaves saved recipe unchanged | 20/20 |
| Decoded table exactly equals AURA's complete render | 0/20 |
| Decoded table close to AURA: mean CIE76 <=3 and p95 <=8 | 20/20 |
| Actual native UI downloads at 100%, 75%, 0% | 3/3 |
| Stale export refuses and preserves manual exposure | Passed |
| Original source/input hashes unchanged | Passed |
| JavaScript page errors | 0 |

The table/native closeness cutoffs are declared QA thresholds, not universal perceptual guarantees. Native render versus decoded-table mean CIE76 ranges **0.285-1.417**, median **0.582**. P95 ranges **0.766-4.203**, median **1.484**. Held-out sampler p95 reaches **0.536 OKLab x100**; the largest isolated held-out error is **9.588**, so a small number of strong/saturated boundary colours remain much less accurate than the median. Neither file validity nor these approximate colour comparisons prove Lightroom parity.

| Target | Pexels source | New colour reference | Table/native mean CIE76 | P95 CIE76 | Reference-matching assessment inherited from the identical fresh fit |
|---|---|---|---:|---:|---|
| 01 | 33380315 | warm-matte | 0.548 | 1.463 | Partial palette match |
| 02 | 16646551 | cool-cyan | 1.417 | 3.706 | Partial palette match |
| 03 | 18688314 | teal-orange | 1.410 | 4.203 | Partial palette match |
| 04 | 15069828 | pastel | 0.885 | 3.094 | Partial palette match |
| 05 | 30154480 | warm-punch | 0.719 | 2.423 | Partial palette match |
| 06 | 29209958 | warm-matte | 0.536 | 1.500 | Partial palette match |
| 07 | 11008002 | cool-cyan | 1.396 | 3.427 | Partial palette match |
| 08 | 3455934 | teal-orange | 0.615 | 1.468 | Partial palette match |
| 09 | 26651573 | pastel | 0.383 | 0.864 | Partial palette match |
| 10 | 37164686 | warm-punch | 0.321 | 0.891 | Weak/worse colour match |
| 11 | 7808457 | warm-matte | 0.346 | 0.878 | Weak/worse colour match |
| 12 | 7322073 | cool-cyan | 0.337 | 0.798 | Partial palette match |
| 13 | 17000433 | teal-orange | 0.953 | 2.138 | Weak/worse colour match |
| 14 | 37461742 | pastel | 0.947 | 2.646 | Partial palette match |
| 15 | 20699589 | warm-punch | 0.415 | 1.076 | Partial palette match |
| 16 | 36701322 | warm-matte | 0.334 | 0.766 | Partial palette match |
| 17 | 32972578 | cool-cyan | 1.083 | 2.579 | Partial palette match |
| 18 | 20788937 | teal-orange | 0.338 | 0.813 | Weak/worse colour match |
| 19 | 20756240 | pastel | 0.285 | 0.864 | Weak/worse colour match |
| 20 | 31714366 | warm-punch | 1.142 | 3.532 | Partial palette match |

The last column concerns matching an unrelated edited reference, not exporting AURA's saved treatment. These output pixels are identical to the earlier v6 fresh fits, so their reference and known-edit accuracy is unchanged. Across that earlier complete 120-pair assessment, there were zero exact/close known-edit recoveries and 12 close reference-palette-and-tone cases. An exported approximation of an imperfect fit does not become a perfect reference clone. See the [full prior results and limitations](AURA-continuous-palette-upgrade-2026-10-24.md).

Preset generation took **80.5-102.6 ms**, median **89.6 ms**, on this machine's debug build. Colour replacement analysis plus Apply took **3.04-33.33 s**, median **5.12 s**. Long-tail fitting/backtracking remains. The 20 preset files are 216,792-262,782 bytes; the downloaded zero-strength identity file is 6,158 bytes. These are observations under local activity, not controlled performance guarantees.

## Lightroom compatibility: not yet certified

Adobe Lightroom Classic **15.2.1** is installed. Two attempts used a new disposable `AURA Preset QA` catalogue and a copied test photograph. The application exited unexpectedly before a preset import/application result could be obtained. The reason was not established. No successful Lightroom import, profile recognition, SDK application or before/after Lightroom export is claimed.

The temporary test plugin and copied preset were moved out of Adobe's global folders back into the isolated QA directory after confirming Lightroom had exited. No pre-existing plugin/preset was replaced. The generated XMP independently passes the Adobe interchange checks, but that is not a substitute for loading it in Lightroom.

To finish compatibility qualification, import a generated file using Lightroom's **Import Presets**, confirm its group/name and embedded look, then compare Lightroom's exported sRGB pixels against AURA on processed JPEG/TIFF and camera RAW fixtures separately. Test multiple Lightroom versions and pre-existing creative settings. RAW camera matrices, baseline profiles, scene transforms, local tone operators and gamut handling can cause differences even when the preset parses correctly. Do not promise import with zero issues or an exact cross-application clone without that evidence.

The table interchange was independently implemented from the [Adobe DNG SDK](https://www.adobe.com/support/downloads/dng/dng_sdk.html), version 1.7.1 build 2768, specifically its RGB-table serialization and XMP encoding. Enhanced-profile scope is described in Adobe's [Camera Raw and Lightroom Profile SDK](https://download.adobe.com/pub/adobe/lightroom/profile-sdk/ACR_and_Lightroom_Profile_SDK.zip). Preset metadata follows Adobe's [normal-preset example](https://github.com/AdobeDocs/cis-photoshop-api-docs/blob/main/sample-code/lr-sample-app/crs.xml) and [Camera Raw namespace documentation](https://github.com/adobe/xmp-docs/blob/master/XMPNamespaces/crs.md). No Adobe look, LUT or proprietary application algorithm is copied into AURA.

## Verification and remaining work

- Focused UI suite: **49 passed**, including generation, error handling, download URL lifetime, cancellation and the bottom-of-panel integration.
- Native colour-reference integration: **10 passed, one optional RAW fixture skipped**. Covers reference replacement, zero-strength export, XML escaping, self-contained data, stale refusal and source/recipe preservation.
- Colour-match unit suite: **86 passed, one existing Natural-mode accuracy failure**. The six Lightroom tests pass, including native neutral/white-balance/exposure sampling and separation of baked colour from spatial settings.
- The unchanged failing test is `a_batch_matched_to_one_reference_converges_on_one_look`: known-grade errors 1.319 -> 0.868 and 1.281 -> 1.791 do not meet its asserted recovery threshold. Its assertion is retained. The complete colour suite is not green.
- Production UI build, native desktop build and strict workspace Clippy passed. Existing frontend chunk-size and GNU resource-manifest linker warnings remain. The 83 locked IPC contracts pass. Formatting and banned-pattern checks are enforced by the commit hook.
- New dependencies: `md5` for Adobe's table identifier and a direct `flate2` dependency reusing the existing zlib implementation. The earlier TLS-related dependency-policy advisories are not fixed by this feature; CI status must be checked separately.

Useful next improvements are multi-reference or aligned original/edited calibration with held-out scene validation, explicit reference colour-coverage checks, denser/profile-shaped table approximation around high-error colours when host compatibility permits, and real Lightroom RAW/processed-image qualification. A single unrelated edited reference cannot uniquely reveal the original white balance, colour history or semantic correspondence, so universal exact cloning is not an achievable correctness guarantee.

## Reproduce

With the existing reviewed 20-photo baseline and Python dependencies installed:

```powershell
npm.cmd --prefix ui run build
cargo build --locked --manifest-path ui/src-tauri/Cargo.toml --target-dir target/desktop-launch --features custom-protocol -j 1
python scripts/test-lightroom-match-export.py
python scripts/validate-lightroom-preset.py output/aura-lightroom-preset-2026-10-24/run-d9694705/downloads/AURA-AURA_Final_Warm_Matte.xmp
cargo test -p aura-app --test color_reference_match --locked
cargo test -p aura-app color_match:: --lib --locked
```

The last command intentionally exposes the outstanding Natural accuracy failure. `--resume <run-directory>` resumes only this script's isolated run with an identical executable. Photos, executable, generated presets and screenshots remain local ignored evidence; code, report and numerical CSV are tracked.
