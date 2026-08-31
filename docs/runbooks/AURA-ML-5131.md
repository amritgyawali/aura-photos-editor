# AURA-ML-5131 - A photographer's camera-matching decision could not be recorded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The camera choice was not saved. Nothing about the photographs from that camera has changed.

## What actually happened

One of the three decision commands on the frozen `CameraMatchService` refused. Five ways, and none
of them is anything a person did wrong:

1. **The body is not in this project.** Every camera id on this surface arrives from a row the same
   surface handed out, so an unknown one is the panel and the catalog disagreeing rather than a
   typo.
2. **`set_reference` on a body that shot no measurable photographs.** A reference is the body
   everything else is matched *to*, so a body with no fingerprint would be a target nothing could be
   measured against.
3. **`set_override` with nothing set.** An empty override that still set `user_edited` would take a
   camera out of automation without changing anything about it.
4. **`set_override` with a value outside its bound.** Refused rather than clamped - see below.
5. **A database trigger aborted the statement.** `camera_reference_keep_user` refuses an automatic
   write over a photographer's chosen reference, and `camera_transform_keep_user_edit` refuses an
   UPDATE that would clear `user_edited`.

## Why a value outside its bound is refused rather than clamped

Phase 21's rule, applied to a surface a photographer touches: **a ceiling can be lowered by a studio
and raised by nobody.** There is no strength field anywhere on this surface, and a camera that needs
to move further than 900 K is a camera whose *per-frame* estimates are wrong - phase 15's own
override is where that is fixed, one photograph at a time and visibly.

Clamping instead would silently give a photographer a different correction from the one they asked
for and report success.

## The four bounds on the override surface

| Field | Ceiling |
|---|---|
| `dCct` | 900 K |
| `dTint` | 20 |
| `dExposure` | 0.6 stops |
| `dSaturation` | 12 |

## Fixing it

The detail line names the camera and the reason. Reopen the panel, which re-reads the project's
transforms, and try again. If the same body refuses twice, `camera_transforms` will show whether it
has a row at all.
