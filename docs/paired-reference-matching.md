# Match a finished version of one photo

In the photo studio, save your edit or portrait healing first, then choose
**Choose finished reference**. Select a JPEG or PNG export of that same photo.
The reference and original must have the same pixel dimensions, crop and
orientation. This bounded workflow supports photo dimensions up to
16 megapixels and a reference file below 64 MB. A different scene is refused
using signed luminance-gradient alignment. This is an alignment heuristic,
not proof that two images came from the same camera original. If a JPEG/PNG
import lacks catalogue dimensions, a bounded decode checks its real dimensions
and imported content hash before the full native frame is requested.

AURA searches exposure, temperature, tint, contrast, highlights, shadows,
whites, blacks and saturation through its native CPU renderer. Four
deterministic coarse-to-fine sweeps evaluate at most 72 candidates at a
384-pixel edge. Every candidate passes recipe validation and the existing
automated merge rules, including protection of manually edited fields.
Each direction starts from the same current best candidate. The search is
bounded around the saved baseline, and only strictly better candidates advance.
The final sweep resolves exposure changes as small as 0.0105 stops; integer
controls retain their schema's quantization. A native-gradient regression checks
a small correction the previous 0.033-stop final step missed, with the other
eight global fields manually protected.

For a saved portrait, the search weights the mean detected-face RGB error at
80% and whole-image error at 20%. Every detected face must also remain no worse
than its baseline at preview resolution. This prevents a brighter background
from hiding a worse face match.

The chosen settings are rendered again at the original export dimensions.
They are saved only if mean absolute RGB error is lower there too and each
detected face's full-resolution RGB error is no worse. A worse or
equal result keeps the existing recipe without a history entry. An accepted
result creates one normal Undo/Redo step, labelled with the reference content
hash and measured errors. The recipe and history write use a transactional
baseline-hash check so a newer concurrent edit cannot be overwritten.

The matching worker uses the shared image-work lane for decode and rendering,
and checks cancellation between candidates, before full-resolution rendering,
and before saving. Stop or changing the open photo requests cancellation.
Source and reference files are read only. No reference bitmap is saved in a
recipe, pasted into an export, bundled as an application asset, or sent to an
AI provider. Saved healing operations, masks, anatomy and geometry settings
are retained; tone and colour changes can intentionally affect their rendered
pixel values.

The displayed whole-image error and reported face errors are in RGB byte
values out of 255. They are not an accuracy,
skin-removal or perceptual-quality percentage. Exact equality requires every
RGB pixel to match, without alignment, resizing or tolerance at delivered
dimensions. Skin defects still require the portrait healing tools and review.
Matching cannot recover another editor's models, hidden settings or proprietary
algorithm, and it does not establish full feature parity.

## Keep the look for the rest of the shoot

Type a name under **Keep the look as** before choosing the reference. When the
match changes the edit, the look it found is kept under Presets as a personal
look. It holds what the finished version adds beyond AURA's own measured
correction of that photo: the exposure, contrast, highlight and shadow moves
beyond the measured ones, the white-balance offset, and the whites, blacks and
saturation as they stand. No pixels from the reference are stored.

Applied to another photo, the look starts from that photo's own measured
correction and passes the same scene guards as every preset, so a look kept
from a bright frame does not brighten a dark one by the same amount. The guards
can make it gentler than the match was: on the published wedding pair the match
itself ended 4.75 from the finished image and the kept look, applied again
through the preset path with its warmth held to a fifth, 5.36 (13.54 untouched).
It carries tone and colour only; healing is planned on each photo separately.
Leave the name empty to match this photo and keep nothing.

The paired comparison harness exercises the native command and full
export/readback. With `--native-pid`, it also exercises the studio button and
the actual owned Windows file picker; it does not replace Tauri IPC:

```powershell
python scripts/verify-evoto-reference.py before.webp after.webp output/paired-match --collection "Paired match verification" --port 9363 --portrait-finish --match-reference
```

For the Windows studio path, add `--native-pid` followed by the isolated AURA
process ID. For an existing harness collection, add `--resume`. Both public WebP files are
losslessly converted into identical-pixel PNG files because the import and
paired target workflow accept JPEG/PNG. Original/reference hashes, all detected
face comparisons, protected feature checks and exact equality are reported
separately. Published marketing pairs are comparison targets, not camera RAW
files or independent ground truth for every defect.
