# Hot Features

Open **Hot Features** in the sidebar. Choose one of the five cards, then select a photograph in the filmstrip. Adding photos from this section imports them without automatically editing them. Apply or discard a precision-retouch draft before switching tools or photographs.

| Tool | Workflow |
| --- | --- |
| Liquify Background Repair | Make a face/body shape or liquify edit, then choose **Repair background automatically**. Adjust **Background repair** from 0 to 100. Requires a detected subject and background; use the Restore brush for manual repairs. |
| Face Freckles & Acne | **Acne · keep freckles** starts with skin-only healing, protected moles and retained pore texture. **Acne & freckles** explicitly includes dark marks. Run automatic retouch on the photograph or collection, compare, inspect saved coverage, and paint remaining spots with the Blemish brush. |
| Remove Glasses Glare | The native Glare softening tool opens selected. Place a small ellipse or paint only the reflection, preview, adjust its amount, and apply. Multiple lenses can have separate saved operations. |
| Train Your AI Look | Scan folders containing originals and finished exports, review paired coverage, train, inspect the measured profile report, then adopt it. Changing the folders or profile name invalidates the previous scan. Training is local. |
| AI Color Match | Upload an edited reference in Reference matching, analyse it, adjust the match, and apply to the photograph or collection. The existing paired reference workflow can use an original and its edited version for calibrated transfer. |

## Native background repair

`studio_finish_v1.backgroundRepair` is a finite amount from 0 to 100. Zero is omitted when serialized, preserving older recipe encodings. It modifies the combined face/body/liquify displacement field before its single resampling pass. A smooth confidence ramp reduces displacement only where both the destination and displaced source classify as background; foreground and uncertain boundaries retain their displacement. The original pre-warp background pixels are reused, retaining the current colour, retouch and light edits. Undo restores the previous recipe; export uses the same renderer. The render engine version includes `background-repair.1` to invalidate old cached results.

The save command refuses enabling/changing repair when it cannot find both a foreground subject and a background. A zero setting remains available to disable it. Segmentation quality still affects boundary quality. It cannot recover pixels missing from an imported flattened liquify edit without a clean original. Glare softening cannot reconstruct an eye hidden by opaque or clipped reflections. Skin-mark classification and single-reference colour transfer also require review; no universal perfect result is claimed.

Cards use local vector artwork before a photo is selected and the selected photo's cached thumbnail afterward. These are tool illustrations, not fabricated before/after results; working comparisons are inside the native editor. No external stock-image requests or customer photographs are bundled.
