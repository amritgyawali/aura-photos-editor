# Bundled lens profiles

One TOML file per manufacturer. Every file carries an attribution header, and **every lens
row carries a `measured_by`** - `ProfileTable::merge_str` refuses a row without one.

## The profiles in this repository are synthetic

There are no measured lens profiles here, for the same reason there is no photographed
ColorChecker in `crates/aura-render/config/camera_profiles.toml`: measuring one needs the
lens, a calibration target and a rig, and none of the three is in a git repository.

Every row below is **fabricated on a plausible lens id**, with coefficients of the right
order of magnitude and the right sign for the focal length. Each is marked `synthetic = true`,
which reaches `ProfileTable::is_synthetic`, the IPC surface and the Geometry panel - so a
photographer is never told a lens was profiled when it was invented.

That is condition C2 in `docs/progress/PHASE-23-EXIT.md`, it is a Sev 2 trigger, and **no
later phase may claim a lens correction result that depends on a profile being measured until
it closes.** The first real measured profile reopens phase 23's acceptance criteria whatever
phase is in flight, exactly as the first real camera file reopens phase 02's.

## Format

```toml
[table]
version     = 1                      # bumps PROFILE_VER; re-plans every corrected frame
attribution = "who produced this file, and under what terms"

[[lens]]
id          = "Canon RF 50mm F1.2 L USM"   # matched against EXIF, trimmed and case-folded
mount       = "RF"
measured_by = "required - a profile is a measurement somebody made"
synthetic   = true                          # omit only when it really was measured

[[lens.entry]]        # one for a prime, several for a zoom, ascending focal length
focal_mm = 50.0
k1       = -0.012     # Brown-Conrady radial terms in normalised radius, 1.0 at the corner
k2       =  0.004     # positive k1 is barrel; negative is pincushion
k3       =  0.0
vignette =  0.38      # full correction strength, 0..1
ca_red   =  1.00022   # radial scale relative to green; green is never scaled
ca_blue  =  0.99978
```

Between two entries a zoom interpolates **in log focal length**, because distortion follows
the field of view rather than the millimetres: 24 to 34 mm is a much larger change of view
than 60 to 70. Outside the entries the nearest is used unchanged - extrapolating a polynomial
fitted over 24-70 to a 200 mm frame produces a correction with the right shape and the wrong
magnitude, which is worse than none because it looks deliberate.

## Adding a profile

1. Measure it. A chart shot at each marked focal length, corners included, at the widest
   aperture and two stops down.
2. Add the rows with a real `measured_by` and no `synthetic` flag.
3. Bump `[table].version`. That bumps `PROFILE_VER`, which raises `AURA-ML-5090` and
   re-plans every frame the profile touches. A profile added without the bump is a profile
   that only applies to photographs imported afterwards.
4. `cargo run --package aura-cli -- verify --phase 23` re-checks the table.

A duplicate lens id across two files is refused rather than resolved: a duplicate would
resolve by directory iteration order, and a correction that depends on a file system's
ordering is not deterministic (invariant 4).
