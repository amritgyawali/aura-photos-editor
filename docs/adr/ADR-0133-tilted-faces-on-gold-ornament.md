# ADR-0133: A tilted face at its window floor needs both eyes confirmed

Status: accepted. Date: 2026-10-20.

## What happened

A whole-application test (`scripts/test-everything-gui.py`) ran portrait analysis and the
portrait retouch styles on a photograph of a bride's hands with mehndi, resting on a gold silk
outfit. There is no face in it. The portrait analyser reported one, and the Natural, Soft and
Polished portrait retouch styles planned skin smoothing and face operations on it - on the gold
silk. Auto retouch, which detects faces with its own YuNet model, correctly found none.

## Why

`aura-portrait` scans six ways and accepts a candidate on evidence (ADR-0065). The tilted scan's
rule was: skin fraction at least 0.6, at least five agreeing windows, two eyes located and a mouth
located. Gold silk and gilded carving fall inside the broad skin prior, and a pair of sequins or
studs is located as two eyes - the darkest blobs where eyes belong - with a crease below for a
mouth. The false face was found on fifteen of forty-five brightness, contrast and saturation
variants of the photograph, which is why it appeared after Auto edit and not on the original file.

On 67 wedding photographs (1024 px canvases, the analysis size), the only tilted detections were
two at exactly five windows, and both were gold ornament: the dress above, and carving between two
guests at a reception. Every frontal and profile detection at eight windows or fewer on the same
photographs - eighteen, including faces with glasses, a turban and black-and-white frames - was a
real face, so the other rules were left alone.

## Decision

A tilted candidate at five windows is accepted only when the eye cascade agrees with both of its
eyes. From six windows the rule is unchanged. The two ornament detections had one and no eye
confirmations; a real tilted face, whose eyes the eye cascade recognises, still passes at five.

`PARSE_VER` goes from 1 to 2, so every cached portrait parse and the renderer's engine string are
renewed and no photograph keeps a region derived from the old rule.

## Measured

| Set | Before | After |
|---|---|---|
| 67 wedding photographs | 90 faces, 1 false (gold carving) | 89 faces, 0 false found |
| 45 variants of the mehndi photograph | 15 false faces | 0 |

`a_tilted_face_at_its_window_floor_needs_both_eyes_confirmed` pins the rule. The evaluation set
is local (`crates/aura-portrait/tests/local_eval.rs`, `AURA_PORTRAIT_EVAL_DIR`) and the photographs
are not in this repository.

## What this does not establish

Recall on tilted faces at exactly five windows is not measured: the 67 photographs contained no
true tilted face at that floor, so a real face like that, without both eyes confirmed, will now be
missed - and can still be drawn by hand with the face tool. Small faces in wide group shots are
missed by both this detector and Auto retouch's; that is a separate, unchanged limitation.
