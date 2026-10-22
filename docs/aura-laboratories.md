# Colour, spatial and inspection laboratories

Open a collection, enter **Auto edit**, select a photo and use **Editing panels**.
The new methods are independent AURA implementations. They do not contain Evoto
models or reproduce a verified proprietary preset.

| Panel | Current choices | What it does |
| --- | --- | --- |
| Colour lab | 50 methods, including 25 new ones | Changes colour or luminance inside an explicit selection |
| Spatial repair | 50 methods, including 25 new ones | Restores or reshapes selected image structure on a bounded correction grid |
| Masks → Selection laboratory | 50 workflows, including 25 new ones | Constructs, measures or transforms editable selection coverage |
| Palette & tone review | 8 new measurements | Shows palette populations, channel dependence, tonal moments and colour relationships |
| Detail review | 9 new measurements | Shows edge support, overshoot, residual correlation and multiscale energy |
| Composition review | 8 new measurements | Shows light/detail placement, boundary activity, symmetry and bright-region connectivity |

The new 100 consist of 50 image methods, 25 selection workflows and 25 inspection
tools. Inspection tools measure the saved photograph; they do not apply an edit.

## Edit a selected region

1. In **Masks**, create or choose the region to edit. A brush, radial gradient or
   existing stored selection can be used. Review its overlay before continuing.
2. In **Colour lab** or **Spatial repair**, choose the destination selection and
   method, then add an operation. Each stack holds at most 32 operations.
3. Adjust the controls and read the selected method's explanation. Source and
   reference coordinates are normalized across the uncropped frame. They are
   deliberate sample locations, including when a reference lies outside the mask.
4. Apply the stack. Applying saves one reversible history step; an unchanged stack
   does not add another step. A stale saved revision is rejected instead of
   overwriting another edit. Undo/Redo restores the stored operation stack.

Changing panel tabs retains drafts. Changing photos or the saved recipe revision
reloads the panel from that revision. Apply or discard a draft before switching
photos. Disabled operations remain in the saved stack and can be re-enabled.

Colour controls use linear Rec.2020 working values where stated. A pivot of 0.18
is a linear gray anchor, not an 18% display brightness setting. Statistical
illuminant estimators describe the selected colour population; they do not know
whether a photographed object should actually be neutral. Choose a suitable
selection and compare the result before applying a strong correction.

Spatial methods calculate corrections on a grid with a longest edge of 256
pixels, then interpolate selected correction taps. Excluded pixels contribute
neither colour samples nor upsampled correction taps. The full-resolution mask
controls the final blend. These methods suit regional repair and optical effects;
review fine skin texture and sensor-scale defects at full resolution with the
existing precision tools. Deconvolution and channel correction are mathematical
approximations, not calibrated camera/lens models.

## Inspect a saved edit

Choose **Palette & tone review**, **Detail review**, **Composition review**, or
**Photo inspection**, then run the inspection explicitly. One report contains
45 measurements, including the earlier 20, and is shared across the review tabs.
The report is invalidated when the photo or its saved revision changes.

Inspection uses a working-space preview with a longest edge of 512 pixels.
Measurements include their units and sample counts; unsupported or empty
populations are described in the control's explanation. A large edge residual,
unbalanced composition or concentrated palette can be intentional. These are
review aids rather than learned defect labels or universal quality scores.
Check the full-resolution output before print or delivery.

The [new capability ledger](reports/AURA-next-hundred-capabilities-2026-10-21.json)
maps every addition to its mathematics and native implementation. The
[validation report](reports/AURA-next-hundred-validation-2026-10-21.md) records
what passed, what remains unverified and where the supplied-photo exports live.
