# Where the lens profiles in this directory come from

Short answer: **nobody measured them.** Every row in `profiles.toml` is a reference model for a
class of lens, not a measurement of a copy of that lens, and every row says so in its own
`measured = false` field. This file is the long answer, because a profile database is exactly the
kind of asset that acquires an authority it never earned once it has been shipped twice.

## What a real lens profile is

A measured profile is produced by photographing a calibration target - a flat grid, evenly lit,
at a known distance - at several focal lengths and several apertures on one physical copy of one
lens, and fitting a distortion polynomial, a vignette falloff and a pair of lateral chromatic
aberration scales to the result. Adobe, Lensfun and the camera manufacturers each maintain such a
database. The numbers differ between copies of the same lens, which is why a serious profile
records the body and the sample it was taken from.

None of that happened here. This repository contains no camera files and no calibration target
photographs - the same gap PHASE-02's exit report records as conditions C1 and C2, and the same
one that leaves `crates/aura-render/config/camera_profiles.toml` holding synthetic bench bodies.

## What is in `profiles.toml` instead

Two kinds of row, and the difference is in the `kind` field:

* `kind = "class"` - eight rows, one per focal-length class from ultra-wide to super-telephoto.
  The coefficients are the sign and the order of magnitude that class of lens has: barrel at the
  wide end, pincushion at the long end, lateral chromatic aberration falling with focal length,
  and vignetting worst wide open at the wide end. They are a **default**, in the way that
  `camera_profiles.toml`'s reference matrix is a default, and a frame corrected through one is
  labelled as corrected through a reference model.

* `kind = "family"` - six rows for lens families that are common at weddings. They are still
  reference models. What they add over the class row is the *shape* - a fast wide prime and a
  wide zoom at its wide end distort differently even at the same focal length - and they were
  written from the published behaviour of those families rather than measured here.

Every row in both sets carries `measured = false`. `aura_geometry::profiles` refuses to load a
file whose row claims otherwise, because the one thing this directory must never do is let a
plausible number be mistaken for a measurement.

## What that means for a photograph

A frame corrected through one of these rows is corrected in the right direction by roughly the
right amount, and `GeometryPlan::lens.source` is `database` rather than `embedded`. The panel
says which of the three sources a correction came from, and `docs/geometry.md` says in the
product's own words that a database correction here is a reference model.

**A camera that wrote its own correction data into the file always wins**, which is
`LensSource::Embedded` and section 6.1's first preference. On the bodies most wedding
photographers shoot, that is the common case, and it is a measurement by the manufacturer of the
lens that was actually mounted.

## Replacing this directory with measurements

The file format is stable and the loader is in `crates/aura-geometry/src/profiles.rs`. A measured
profile is a row with `measured = true`, a `source` naming who measured it and when, and the
licence it arrives under recorded here. `aura_geometry::profiles::LensDatabase::parse` accepts
`measured = true` only when `source` and `licence` are both present and non-empty, so a row
cannot be promoted by editing one field.

Bump `profiles_ver` on any change to any row. It is written into `geometry_plan.profile_ver`, and
`AURA-ML-5109` exists so that a plan made under one table is never silently compared with a plan
made under another.

## Licence of what is here

The reference models in this directory were written for this repository and carry the same
licence as the rest of it. They incorporate no third party's measurements, which is the one
advantage of not having any.
