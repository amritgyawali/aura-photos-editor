# How AURA culls

*What the engine does, what it guarantees, and what every reason in the panel means.*

This is the page to read before you deliver a gallery AURA chose. It is written for
photographers rather than for engineers; the arguments behind the numbers are in
`docs/adr/ADR-0025-culling-coverage-and-gallery-sizing.md`.

---

## The one-paragraph version

AURA does not go through your photographs one at a time and keep the good ones. It works
out what happened at the wedding — moments, chapters, who is in each frame — and then
chooses a gallery that covers all of it. That is why it will sometimes keep a slightly
soft photograph of the rings and drop a technically perfect frame of the dance floor: the
first is the only record of something, and the second is the eleventh frame of one song.

**Nothing is deleted.** A photograph that is not in the gallery is a row in a database
saying so, with the reason, and it is one click from being put back.

---

## What happens when you press Cull

Seven steps. The order matters more than any individual step.

1. **Score** — each photograph gets one number from four: whether it worked (sharpness,
   exposure, motion, eyes), what is happening in it, how it is framed, and who is in it.
   The four are *multiplied*, not averaged, so a photograph that fails badly on one of them
   cannot be rescued by the other three. A beautiful moment that is completely out of focus
   is still out of focus.
2. **Moments** — within each moment, the strongest frames win. How many depends on how much
   the moment *varied*: fourteen frames of one static instant earn one keeper, and four
   frames where the action moved can earn three.
3. **Chapters** — each part of the day gets a share of the gallery, based on how much was
   shot there and how much that part of the day matters.
4. **Coverage** — the guarantees. If the gallery is missing part of the wedding, frames are
   added back even if they scored badly.
5. **Spread** — repetition is removed. No more than a handful of delivered frames from any
   two-minute stretch of one chapter.
6. **Size** — the gallery is brought to the size you asked for, or to the size AURA
   predicted.
7. **Coverage, again** — because step 6 could have broken a guarantee, and a guarantee that
   only holds until you move a slider is not a guarantee.

---

## The guarantees

Twelve parts of a wedding are protected. If the gallery would not contain them, AURA puts
frames back in — even weak ones — and tells you it did.

| Guarantee | Frames | Why it is protected |
|---|---|---|
| The first look | 2 | Two photographs: the turn and the reaction. One is half the moment. |
| Walking in | 1 | The arrival at the door. |
| The vows | 2 | One for each person speaking. |
| The rings | 2 | The ring going on, and the hands afterwards. |
| The kiss | 2 | The wide shows the room reacting; the tight shows the couple. |
| Family groups | 3 | These are a list of families, not one moment. |
| Entering the reception | 1 | Kept separate from the ceremony entrance on purpose. |
| The cake | 1 | Nobody looks at it twice and everybody notices its absence. |
| The first dance | 2 | The room, and the two faces. |
| The venue | 1 | The frame an album opens on, and one a scoring engine would never choose. |
| The exit | 1 | The last page. Its absence ends the story mid-sentence. |
| Close family | 3 each | The rule that stops "my aunt isn't in the gallery". |

Each one comes out in one of three states:

* **covered** — it is in the gallery, from frames that met the usual bar.
* **weakly covered** — it is in the gallery, but only because AURA put back frames that were
  below the bar. Worth looking at.
* **missing** — **no photographs of it were found**. Not "AURA chose not to". If the frames
  exist but are labelled as something else, correcting the label and re-running fixes it.

AURA cannot invent coverage and will never imply that it did.

### What can and cannot break a guarantee

| Action | Can it drop a guarantee? |
|---|---|
| Moving the size slider all the way down | No |
| Switching to Aggressive mode | No |
| Changing the weights in `cull_weights.toml` | No |
| Removing every photograph of something by hand | **Yes** — and the panel says so, naming your removal |

The last row is deliberate. AURA does not silently overrule you and does not silently lose
the vows; it does what you asked and tells you what it cost.

---

## The three modes

| Mode | What it changes | What it never changes |
|---|---|---|
| **Conservative** | Keeps roughly a fifth more, and flags more of what it is unsure about. | The guarantees. |
| **Balanced** | The default, and the setting everything is calibrated against. | The guarantees. |
| **Aggressive** | A tighter gallery: a higher bar and one fewer keeper per moment. | The guarantees. |

Conservative is the default because the two mistakes are not equally expensive. Too many
photographs costs you ten minutes with the slider. Too few costs a frame nobody can get
back.

---

## The size slider

Moving it re-chooses the gallery. It does **not** re-analyse anything, which is why it is
fast — a second or two on a full wedding — and why the photographs it adds back are ones
AURA had already scored.

If you ask for fewer photographs than the guarantees need, you get the guarantees. The
panel shows both numbers so the difference is visible rather than surprising.

---

## What every reason means

Every photograph carries at least one of these, in both directions.

### Why a photograph is in the gallery

| Reason | What it means |
|---|---|
| `moment_winner` | The strongest frame of its moment. The ordinary reason. |
| `peak_frame` | The instant of a sequence where the most was happening. |
| `coverage_protected` | Part of the wedding's story that would otherwise be missing. |
| `identity_coverage` | Somebody who should be in the gallery is in very few other frames. |
| `diversity_spread` | It shows something the other keepers from this moment do not. |
| `chapter_quota` | This part of the day still had room. |
| `size_target` | The best frame left while the gallery was still short of its size. |
| `only_candidate` | The only frame there was. |
| `user_kept` | You asked for it. |

### Why a photograph never reached the arithmetic

These three are *measurements*, not scores. If you disagree, you can look and say so.

| Reason | What it means |
|---|---|
| `veto_out_of_focus` | The subject is not in focus. Not "a bit soft" — not in focus. |
| `veto_exposure_lost` | The information is not in the file and editing cannot bring it back. |
| `veto_eyes_closed` | The main subject's eyes are closed and nothing in the moment explains it. Only in posed scenes, and never at a kiss, a prayer or a moment of tears. |

### Why a photograph is not in the gallery

| Reason | What it means |
|---|---|
| `lost_moment_rank` | Another frame of the same moment is stronger. |
| `near_duplicate` | Effectively the same photograph as one already in the gallery. |
| `chapter_full` | This part of the day was already well covered by stronger frames. |
| `below_floor` | Weaker than this kind of photograph usually needs to be. |
| `diversity_cap` | The gallery already carries several frames very like this one. |
| `size_trim` | The weakest frame left when the gallery reached its size. |
| `user_rejected` | You asked for it to be left out. |

### Notes rather than decisions

| Reason | What it means |
|---|---|
| `not_analysed` | **Nobody has checked this photograph yet.** It was not judged and not rejected. Run the analysis and choose again to include it. |
| `peak_rejected` | This was the strongest instant of its moment, and a different frame of the same instant was delivered instead. AURA never drops a peak silently. |
| `runner_up` | The closest alternative to the frame that was delivered. |
| `no_scene` | AURA does not know what kind of photograph this is, so it was judged cautiously. |
| `no_moment` | It was not grouped with any others, so it was judged on its own. |

---

## The number to check before you deliver

At the top of the coverage panel: **how much of the wedding the gallery was chosen from.**

A gallery chosen from 60 % of a wedding looks exactly like a gallery chosen from all of it.
The photographs that were never analysed are not in it, they were not rejected, and nothing
in the grid shows the difference. If that number is below about 95 %, finish the analysis
and choose again before delivering.

The line underneath it says how much of the judgement was real: what fraction of the
considered photographs had an expression reading, a framing judgement and a moment. A
gallery chosen at 3 % expression-aware was chosen on two signals instead of four.

---

## What AURA will not do

* It will not delete, move, or rename a photograph. Ever. A rejection is a row.
* It will not deliver a gallery. That is a separate, explicit step.
* It will not edit the photographs it chose. That is the next phase of the pipeline.
* It will not claim coverage it does not have.

---

## A caveat this release has to make

The four scores underneath every decision come from models that are **not yet trained on
real wedding photographs**. The arithmetic in this page is real, measured and tested. The
numbers it works on are, for now, placeholders — so this release's galleries prove that the
engine does what it says, not that it has good taste. That limitation is recorded as
condition C1 in `docs/progress/PHASE-12-EXIT.md` and it lifts when the models do.
