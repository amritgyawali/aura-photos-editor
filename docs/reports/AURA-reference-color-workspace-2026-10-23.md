# Reference colour workspace

AURA now offers the reference/active workflow shown in the supplied Evoto screenshot through **Match a look**, or **Hot Features → AI Color Match**. This is an independent implementation using AURA's native reference-grade renderer. Proprietary algorithm parity and perfect transfer across arbitrary photographs have not been established.

## Using it

1. Open a collection and a photograph. Upload finished JPEG, PNG or TIFF references, or use **Create Reference** to capture the current saved edit.
2. Choose a reference. The reference and active photograph appear together. Zoom and pan can be linked, or adjusted independently. Older thumbnail-only references can be uploaded again for a larger preview.
3. Choose **Quick Mode** or **Precise Mode**, analyse, then apply. Alternatively enable **Real-Time Color Adjustments**: deliberate reference, mode and strength changes queue an analysis/apply after a 350 ms pause. Opening the workspace never starts an edit. Stop and leaving the workspace stop queued work; completed saves remain in Undo.
4. Adjust tone, colour, warmth and the fine controls. Preferences are AURA presets: Subtle 50%, Standard 100%, Strong 150%, Extreme 200% overall strength, with component strengths at 100%. Native skin/highlight checks remain enabled and can reduce a requested correction.
5. **Save Preset** stores matching strengths on that reference. **Sync** refits selected collection members, or the entire collection when explicitly selected. It never copies another photograph's fitted correction. Empty selections do not expand to the collection. Results identify matched, unchanged, failed and stopped photos.

Recommended references come from the user's local library, ranked by overlapping measured regions and favourites. They are not an Evoto stock collection. Reference preview JPEGs are capped at 512 pixels and 32 KiB; these comparison images are not full-resolution originals. Portable colour descriptors omit embedded previews.

## Native fitting

Quick uses a 256-pixel fitting frame, up to 10,000 target samples and two transfer iterations. Precise defaults to 512 pixels, 40,000 samples and four iterations. Unrelated scenes retain the existing conservative one-iteration transfer. Both modes use the same full-resolution application validation and immutable baseline, so changing strength does not compound the treatment. Older requests and diagnostics default to Precise.

Fresh imports can be matched directly. Analysis reads the implicit neutral camera recipe without creating an edit or Undo step. First Apply checks that a saved recipe is still absent in the same transaction that saves the match; subsequent edits require the measured recipe hash to remain current. A concurrent manual correction therefore wins on both fresh and previously edited photos.

Live requests are serialised; newer strengths replace queued intentions. Hidden panels, photo switches, cancellation, and external editor locks guard writes. Native recipe candidates are canonicalised before verification and equality checks so stored precision cannot create false stale sessions or duplicate Undo steps.

## Investigation and limits

The installed `F:\evoto\Evoto.exe` was inspected read-only with the [reverse-engineer-anything skill](C:/Users/Admin/.agents/skills/reverse-engineer-anything/SKILL.md). Its SHA-256 was `7f020192f3b659ca4a1e4f8631b21879982bb21d97d26196a604ad70e2f3b2fa`. REA PE-resource evidence `ev_852354417e70a5e5efaa912879f8058ed77ab90be2a57fd6add2bcce87e0b558` describes icons, version and manifest resources. Native decompilation providers were unavailable; no installation or configuration was changed. Bounded ASCII observations include `AIColorMatchMode`, `ColorMatchPreference` and `loadColorMatchesFromServer`. Names establish interface vocabulary, not executed behaviour or recovered algorithms. No Evoto models, code or bundled reference images were copied.

An edited reference alone cannot uniquely identify the original lighting, scene colours, white balance or editing operations. Similar lighting and subject matter remain important. Existing demanding single-reference accuracy regressions remain visible in `a_batch_matched_to_one_reference_converges_on_one_look` and `a_new_photograph_takes_an_edited_references_look_through_the_renderer`; this workspace does not remove or weaken their thresholds. Original/edited-pair calibration remains available for better constrained transfers.

The user's already-running AURA process is not replaced or closed by this change. Reopen the rebuilt application to load the updated interface.

## Verification

- Frontend: 119 files, 1,119 tests passed; TypeScript and production build passed.
- Native integration: `color_reference_match` nine passed, one optional local-RAW review ignored; `reference_style_native` four passed, one optional local-photo review ignored. These cover native application, masks, preview/export, manual edits, zero-strength restoration, duplicate refusal and noncompounding strengths. The repeated-control regression was observed failing before the precision fix and passing afterward.
- The first real desktop pass exposed a pre-existing requirement to save an initial correction before matching. Two new pristine-import regressions pass after the fix: analysis/zero create no edit, first Apply has exactly one Undo step, Undo/Redo restore the correct recipes, and concurrent first manual edits remain protected. Both transfer and older descriptor-only matching paths are covered.
- Native reference unit coverage: 64 tests passed with the existing demanding unpaired batch accuracy gate explicitly excluded; its failure is disclosed above rather than reclassified as passing.
- Repository: formatting, banned-pattern scan, strict workspace Clippy and all 83 locked contracts passed.
- `scripts/test-reference-workspace-gui.py` runs a real desktop smoke check with its own catalogue, resized test copies and WebView profile. It records live strengths, fitting modes, persisted presets, collection refits, source hashes, screenshot and page errors under `.work-checks/reference-workspace-*`.
- Final distinct-photo testing is recorded in [the ten-photo assessment](AURA-unique-color-match-2026-10-24.md). It found six small Standard changes and four native safety refusals, with zero strong statistical matches. This is evidence of a remaining appearance-transfer limitation, even though workflow and preservation checks pass.
