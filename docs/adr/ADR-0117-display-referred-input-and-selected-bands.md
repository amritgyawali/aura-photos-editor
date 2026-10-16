# ADR-0117: Display-referred input, and bands that average only what they select

Status: Accepted
Date: 2026-10-13

The six public before/after pairs on a competitor's homepage were run through
the native commands in-process (`crates/aura-app/tests/reference_pairs.rs`) and
scored against the finished half (`scripts/compare-reference-pairs.py`). Four
defects were found that are independent of whose finished version is compared.
The measurements are in [the paired validation](../evoto-paired-validation.md).

## 1. A display-referred file was tone-mapped a second time

`output::map_one` rolls every channel off through `filmic_lite`, because the
working space is scene-referred. A JPEG, a PNG and the proxy phase 02 renders
are display-referred: they have been through a shoulder already. `CatalogFrames`
decoded their sRGB bytes straight into the working space, so the output stage
rolled them off again. With every control at zero a white wall at 239 was
delivered at 230 and paper white at 253 as 236. An untouched export was duller
than the file it was imported from, and every paired comparison this repository
had run carried that error as its largest term.

`output::unroll` is the inverse of step 1 of the output transform, applied when
a display-referred buffer becomes a frame. The untouched render is now the file
to within the 8-bit dither (at most one code), and an exposure lift on a JPEG
still rolls off as it does on a raw frame instead of clipping.

- **Unroll, not a flag that skips the shoulder.** A flag would have to travel
  through the frame, the plan, the tiler and every export path, and a lift
  would then clip hard. The inverse is one function beside the transform it
  inverts, and invariant 8 holds unchanged: tone is still baked in one place.
- **A ceiling at two stops over diffuse white.** `filmic_lite` reaches 1.0 only
  at infinity. `DISPLAY_PEAK = 4.0` rolls back off to within a hundredth of a
  code of white, which is what lets a file's own 255 come out as 255.
- **Resize before unrolling.** A frame is resized display-referred and unrolled
  afterwards, so a white backdrop at four scene units does not outweigh the
  hair in front of it in a downscaled preview.
- **A chosen colour is the colour delivered.** `retouch_color::to_working` goes
  through the same inverse, so a solid background chosen as `#d7e7ec` is
  delivered as that and not a shoulder darker.
- **`+display.N` in the engine string.** The same JPEG under the same recipe is
  a different picture, so cached previews keyed by the old render hash are
  stale. `DISPLAY_INPUT_VER` is folded in beside the profile and portrait
  versions. Nothing frozen moved: `ENGINE` in `aura-recipe` is unchanged.

Automatic planners still measure display bytes, and thresholds they hand the
renderer as linear stops are unchanged below the knee (sRGB 203). Above it the
renderer now sees larger values than a plain decode gives. That is the same
relationship a raw frame has always had with its proxy.

## 2. A band taken at the edge of a selection was the photograph's shading

An operation that evens a band adds the difference of a wide and a narrow mean.
Two things were wrong at the edge of a selection, and they needed different
fixes.

- **Bleed.** `Tool::Frequency` and its siblings averaged only selected pixels in
  one mode and used a plain blur for segmented selections; `SkinSmooth`,
  `SkinUniformity` and `PortraitDodgeBurn` always used a plain box mean. Lips,
  lashes, hair and whatever lies beyond a segmented edge coloured the skin
  beside them. Every band is now a mean over what the operation selects
  (coverage times the dark-detail guard), in every mode.
- **Lean.** A selected mean at the edge of its selection is centred inside it,
  the wide one further in than the narrow one, so their difference is the
  shading between those two points. Skin that darkens toward a jaw was
  lightened into a rim along the whole outline of a face, and a shadow under a
  lip was lifted. `retouch_planes::Lean` measures where each mean was actually
  read and `shading_between` takes exactly that share back out of the wide
  band. The guided wide band of `SkinSmooth` is not one plain mean, so it
  reports its own lean by being run on the coordinate planes.

Withholding the correction near an edge was tried first and removes the rim,
but at strong smoothing it leaves a border of unsmoothed skin along a hairline.
Subtracting the leaked shading has neither failure:
`evening_runs_to_the_edge_of_a_selection_without_flattening_its_shading` holds
evenly shaded skin with a one-sided selection to 0.6% for three tools, where a
plain or one-sided mean moved it by three to four per cent.

`Tool::Frequency` also bounds what it calls unevenness: where the wide and
narrow bands differ by more than a few per cent of luminance the difference is
a lip edge, a nostril or a fold, and is not evened.

## 3. Body skin the segmenter was unsure of

On a stretch-marked abdomen with no face in frame the segmenter gave the marked
centre to no label at all (other 0.52, body skin 0.27, clothes 0.14, against
0.89 to 0.92 for the wall, the blanket and the clothing). The selection had a
hole in it and healing stopped at a line across the belly.
`skin_body::complete` grows the selection into cells that touch it, are this
body's own skin colour as measured on the cells the network was sure of, and
that no competing label is sure of. Growth that would more than double the
selection is discarded whole.

Body spot healing is also capped at the planner's ordinary sensitivity and two
passes. At the face's full sensitivity it read the warm side of an abdomen as
one mark and rebuilt it as a pale patch with an edge.

**Stretch marks are not removed.** A multi-scale evening that clears them also
softens finger edges on the hands in the same selection, and was not shipped.

## 4. A white studio wall behind people

`respect_intent` keeps a high-key frame's whites but stands aside when people
are present, and `face_exposure_cap` answers for exposure alone. Highlights was
still taken down by the histogram, which in front of a white wall is the wall.
`keep_high_key` applies the same limit when the frame is high-key and has
people in it.

## What this does not establish

The comparison target is a set of lossy marketing exports from one vendor. The
figures are error against those files; none is a removal rate or a judgement of
quality, and one of the six pairs is not pixel-aligned with its own original.
Equality with another product's output is neither claimed nor possible: the
editor that made them is a different algorithm with unpublished models. The
gates added here are painted fixtures and the six pairs; accuracy on a real
wedding is still condition C1 of every phase that touches skin.
