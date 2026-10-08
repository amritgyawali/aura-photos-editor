# ADR-0106: Third-party licences, research-only data, and the licence agreement

Status: accepted.
Date: 2026-10-08

## Problem

A paid, closed-source product may only ship third-party code and data whose licences allow it, and
must carry their notices. Nobody had checked. AURA compiles in 362 Rust crates, bundles 164 npm
packages into its window, and ships ONNX Runtime, DirectML, six models and OpenCV's cascades.

The check found one real problem. The five "learned" edit profiles (`fivek-expert-a` to `-e`) were
fitted on MIT-Adobe FiveK pairs taken only from the `LicenseAdobeMIT` subset, on the understanding
that it was MIT-licensed. It is not: its text is a **research licence** that forbids exercising the
granted rights "in any manner that is intended for or directed toward commercial advantage or
monetary compensation". No image was shipped, only medians fitted from them, but a product sold for
money that ships numbers derived from those images is the use the licence rules out.

## Decision

1. **The FiveK profiles are removed from the shipped table** and kept in
   `ml/edit-profiles/fivek-research-profiles.json` for research. Sixteen researched profiles remain,
   written from published editing technique. A photographer's own learned style (ADR-0104) is
   unaffected: it learns from the photographer's own catalogue.
2. **`scripts/third-party-notices.py`** resolves what is actually shipped - the shell's normal Rust
   dependencies for Windows, the npm `dependencies` tree, and a table of bundled runtime files and
   models - collects every licence text, de-duplicates them, and writes
   `THIRD-PARTY-NOTICES.txt`. It **fails** on GPL, LGPL, AGPL, SSPL, non-commercial or share-alike
   Creative Commons, a package with no licence, or a shipped edit profile derived from a
   research-only data set. MPL-2.0 (cssparser, selectors, dtoa-short, option-ext) is allowed and
   noted: used unmodified, its only obligation is the notice.
3. **The installer carries both documents.** `scripts/build-installer.sh` runs the audit, refuses
   to build on a failure, ships the notices and the agreement under `licences/`, and shows
   `legal/EULA.txt` as the installer's licence page.
4. **`legal/EULA.txt` is a plain-language draft** matching ADR-0105: a 14-day trial, a personal
   licence on the computers the licensee uses, the photographs remain the photographer's, nothing
   is uploaded, third-party components under their own licences, the usual warranty and liability
   limits with consumer rights preserved. Its bracketed placeholders - the seller's legal name and
   address, governing law and support address - must be filled, and a release build
   (`AURA_RELEASE=1`) refuses to ship them unfilled. **It should be reviewed by a lawyer in the
   seller's jurisdiction before sale**; it is a starting point, not legal advice.
5. The audit is release gate `licences` in `ops/release/release.toml`.

## The Instagram downloader is removed

"Instagram style" fetched a public profile's photos by running Python with Instaloader. That cannot
ship: a customer's computer has no Python, so the button failed for everybody but the developer,
and Instagram's terms forbid collecting its content by automated means - a risk a paid product
carries for every copy sold. The command, `aura_cloud::instagram` and its helper script are gone.
The look matching itself is unchanged and now takes what the photographer already has: a folder of
reference photos, or their own Instagram data export, read by its layout
(`MediaSource::InstagramExport`). The tab is now **Match a look**.

## Result on 2026-10-08

362 crates, 164 npm packages and 9 bundled components; no forbidden licence; 333 distinct licence
texts in a 1.2 MB notices file.
