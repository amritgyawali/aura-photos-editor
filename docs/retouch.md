# What AURA does to skin

Retouching is the part of a delivery clients notice most and photographers argue about most. This
page says exactly what AURA does, what it will never do, and how you can check.

## The short version

AURA removes things that were passing through - a spot, a patch of redness, a scratch, the shadow
under tired eyes - and leaves everything that is *the person*. Pores, fine lines, freckles, moles,
scars, birthmarks and tattoos stay. It is measured rather than promised: every photograph carries a
number saying how much of the skin's own texture survived, and if it could not keep enough, AURA
does nothing to that photograph and says so.

## What is removed

**Temporary marks.** A spot is a colour event with ordinary skin texture on top of it, and that is
how AURA finds one: something a few millimetres across, redder than the skin around it, sitting at
a scale between a pore and the shape of a face. Removing it borrows skin from a couple of
millimetres away - the same person, the same light - and puts that skin's own texture back on top,
matched in strength to the skin around the mark.

**The shadow under the eyes.** Lifted a little, never closed. There is a hard cap of a quarter of a
stop, and AURA aims at about three fifths of what it measures rather than at all of it. An
under-eye lifted until it matches the cheek is a face with no eye sockets, which is the second most
recognisable sign of automated retouching after plastic skin.

**Blotchy patches.** Flush across the cheeks, a makeup line at the jaw, a neck two shades from the
chin. These are calmed by moving one band of detail and leaving the others exactly where they were,
so the pores in a calmed patch are the same pores that were there before.

## What is kept, always

Freckles, moles, birthmarks, scars, dimples and tattoos. AURA finds them and puts them on a list
you can see, with a note saying why each one is there.

The strongest evidence comes from the whole wedding rather than from one frame: **a mark in the
same place on the same face across hours is part of that person.** A spot that shows up in four
frames over ninety seconds is a spot; one that shows up all day is not. AURA needs both - at least
four photographs and at least forty-five minutes - before it decides.

You can remove something from the protect list if you disagree. With one exception:

> **AURA never alters tattoos.** There is no setting, no preset and no slider that changes this.
> The refusal is written into the type, into the service and into the database, and a photograph
> AURA cannot retouch without touching a tattoo is a photograph it leaves alone.

When AURA is not sure whether a mark is temporary, it leaves it. That is deliberate: leaving a spot
is a small annoyance, and removing somebody's beauty mark is a photograph of somebody else.

## What AURA will never do

Body reshaping, slimming, skin lightening, face swapping, or anything else that changes who
somebody is. This is not a setting that ships switched off - there is nowhere in the product to
express it. The edit recipe has no field for it, the catalog has no column for it, and a test fails
the build if the words appear in the code.

## The texture guarantee, as a number

Skin has texture at a scale finer than any mark AURA removes: pores, fine lines, the grain of the
photograph. AURA measures how much of that survives, by rendering the retouch and comparing.

| Preset | Skin must keep |
|---|---|
| Light | 94 % of its texture |
| Natural (the default) | 90 % |
| Polished | 84 % |

Never below 80 %, on any setting, ever.

If a retouch would cost more than that, AURA reduces it and measures again - up to three times.
If it still cannot get there, it **applies nothing to that photograph** and tells you. A frame that
ships unretouched is a much smaller problem than a frame that ships looking plastic.

The panel shows the measured number for the photograph you are looking at, along with how much
skin it was measured over. A number measured across a whole face means something; a number
measured on a face forty pixels tall does not, and AURA says which one you are reading.

## The four presets

**Off.** Nothing is retouched.

**Light.** Confident blemishes only, and a little under-eye work. For a photographer whose look is
"as shot".

**Natural.** The default. Marks go, dark circles lift, blotches calm, texture stays.

**Polished.** The most this product will do, and still inside the texture floor.

## Why the same person looks the same everywhere

AURA decides how much care a person gets **once for the whole wedding**, from four things: who they
are to this wedding, the preset, how large their face usually is across the frames they appear in,
and the kind of photograph they mostly appear in. Every frame in the gallery then uses that same
number.

The alternative - deciding per frame - is how galleries end up with a bride whose skin looks
different between the ceremony and the reception. Nobody can point at the frame where it went
wrong, and that is exactly what makes it hard to fix.

What each photograph *does* decide is which operations run at all. A face too small in the frame
gets nothing, because at that size there is nothing to correct. A ritual frame is never
"evened out", because turmeric, sindoor, mehndi and festival colour are the reason the photograph
exists, not blotches. Details, rings and venue frames are not retouched at all - there is no skin
in them.

## Where you can check

- The retouch panel lists what was removed **and what was left alone**, in sentences.
- Every protected feature is listed with its evidence: how many photographs it was seen in and
  over how long.
- The texture number is on the photograph, with the number of skin samples behind it.
- `retouch_plan` in the catalog carries all of it, so "did the skin keep its texture across this
  wedding" is a question with a numeric answer rather than an opinion.

## Every sentence AURA can say about a retouch

Twenty-six of them, and thirteen describe something the product decided *not* to do. That is the
shape of a careful retoucher: most of what it does on most photographs is leave things alone, and
a product that could not say so out loud would look like one that was not paying attention.

| Code | What it means |
|---|---|
| `blemish_removed` | a temporary mark was removed and the skin around it kept its own texture |
| `no_blemish_found` | there was nothing temporary on this skin to remove *(withdrawal)* |
| `anomaly_uncertain` | AURA was not sure whether this mark was temporary or part of how this person looks, so it left it alone *(withdrawal)* |
| `anomaly_too_large` | this mark is too large to be a blemish, so AURA left it for you to decide about *(withdrawal)* |
| `no_donor_patch` | there was no nearby skin under the same light to borrow from, so this mark was left alone *(withdrawal)* |
| `feature_protected` | this is part of how this person looks, so it is protected |
| `protected_by_cross_frame` | this mark is in the same place on this person's face through the day, so AURA treats it as part of them |
| `protected_by_user` | you asked AURA to keep this |
| `tattoo_protected` | AURA never alters tattoos *(withdrawal)* |
| `vetoed_by_protection` | this was next to something AURA protects, so it was left alone *(withdrawal)* |
| `under_eye_corrected` | the shadow under the eyes was lightened a little |
| `under_eye_capped` | AURA stopped lightening under the eyes before it would start to look retouched |
| `no_eye_landmarks` | AURA could not find the eyes here, so it did no under-eye work *(withdrawal)* |
| `tone_evened` | uneven patches in the skin tone were calmed, without smoothing |
| `skin_already_even` | this skin was already even, so nothing was changed *(withdrawal)* |
| `already_evened_by_local` | this face had already been evened out earlier in the edit, so AURA did not do it twice *(withdrawal)* |
| `texture_held` | the skin kept its own texture through the retouch |
| `texture_resolved` | AURA used a gentler retouch here so that the skin kept its own texture |
| `texture_floor_unreachable` | AURA could not retouch this photograph without losing skin texture, so it left it alone *(withdrawal)* |
| `texture_unmeasurable` | there was not enough skin visible here to check the texture, so AURA was cautious *(withdrawal)* |
| `identity_strength` | this person is retouched the same way everywhere in this wedding |
| `identity_unknown` | AURA does not know who this is yet, so it used its gentlest settings *(withdrawal)* |
| `face_too_small` | this face is too small in the frame to retouch *(withdrawal)* |
| `scene_limited` | this kind of photograph is retouched more gently |
| `mask_unavailable` | AURA was not sure enough where the skin is here, so it did not retouch it |
| `head_untrained` | AURA is using its measured retouching rather than a learned model in this build |

## What this build cannot claim

AURA's face detection is not trained yet in this build, so on a real photograph there are no faces
to retouch and no cross-frame evidence to protect anybody with. The blemish and permanent-feature
models that ship are placeholders and are not consulted: what runs is a measurement, which finds
fewer marks than a trained model would and is careful about the ones it does find.

Everything above is real, tested and enforced - the protect veto, the texture floor, the withdrawal,
the per-person consistency. What has not happened yet is a blind comparison against the retouching
tools this product means to beat, and a per-skin-tone study of the detector. Until both exist,
nothing here is a claim about how AURA compares with anybody else.
