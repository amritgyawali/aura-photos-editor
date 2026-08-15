# What the emotion marks mean

AURA reads every photograph twice. Once for whether it *worked* - is the right person
sharp, are the eyes open, can the exposure be brought back; that is
[frame integrity](frame-integrity.md). And once for whether it is *worth delivering*,
which is this page.

## The one thing to know first

**AURA describes photographs. It does not read minds.**

When this product says "this face reads as crying", it means the pixels around somebody's
eyes look the way a crying face looks in a photograph. It is not saying they were sad. It
is not saying anything about them at all - it is saying something about the frame.

That distinction is not politeness. It is why every label on this page is written the way
it is, and it is a rule the code enforces rather than remembers: the sentences AURA can
say about a photograph are a closed list of twenty, they are all below, and there is no
way for a new one to be written without somebody adding it here.

## What AURA is not doing

- **It is not choosing.** The emotion score orders your photographs; it does not select
  them. Nothing on the Moments panel keeps, rejects or delivers anything, and there is no
  button that does.
- **It is not scoring people.** Every reading is about a frame. The same person is read
  differently in two photographs taken a second apart, because the photographs are
  different.
- **It is not assuming your weddings look like everybody else's.** See "Composure is not
  an absence" below, which is the most important section on this page.

---

## The emotion score

One number per photograph, from 0 % to 100 %.

It is a weighted combination of nine things: how much expression is on the faces, whether
the smiles read as caught rather than held, what the people are doing with one another,
whether this is the strongest frame of its moment, whether somebody is reacting to
something, whether two people are looking at each other, how composed the subjects are,
whether anybody reads as crying, and how many faces are expressing something.

**Every one of those nine is weighted differently depending on what part of the day the
photograph is from.** A broad smile is worth a lot on a dance floor and much less during a
rite. That is what makes a 71 % in a ceremony comparable with a 71 % on a dance floor.

The score is not a grade. A frame at 22 % may be the only photograph of the ring exchange,
and AURA knows nothing about that - the phase of the product that decides what you deliver
does.

---

## Composure is not an absence

This is the part of the design most likely to be got wrong by a product built somewhere
else, and it is worth explaining in full.

For long stretches of many weddings, nobody is smiling. A Hindu ceremony runs for hours
with the couple seated and often looking down. A nikah is short, quiet and still. A
Catholic vow exchange is two people being serious at each other. In every one of those, the
frames the family will look at in thirty years are the *still* ones.

An emotion model trained on "which photographs did Western photographers deliver" learns
that a moment is a big smile. Run over those weddings it produces an almost empty gallery,
and the honest conclusion a photographer draws is that the product does not understand
their work.

So AURA carries **composure as a positive reading**, not as the absence of one, and:

- in `ceremony`, `ritual`, `vows` and `rings`, composure is weighted **at or above** a
  smile;
- on top of that, AURA applies a per-tradition adjustment - a Hindu, Nepali or Muslim rite
  weights composure higher still;
- the settings that do this live in a file a person can read and edit
  (`emotion_weights.toml`), every line of it carries a written reason, and a test refuses
  to let a future change quietly reverse it.

If your galleries feel wrong in one direction, that file is the conversation - not a
retrain.

---

## Moments and peaks

AURA groups your frames into **moments** - the things you shot once, however many times you
pressed the shutter. Inside each moment it looks for the **peak**: the frame where the
action is strongest.

Three things can come back:

| What you see | What it means |
|---|---|
| A peak marker on one frame | That frame is clearly the strongest of the moment |
| "No single frame is the one" | AURA looked, and the frames are too close to separate |
| Nothing at all | AURA has not read this moment yet |

**The second one is a real answer and usually the right one.** Fourteen bracketed frames of
the rings genuinely have no apex, and a product that pointed at one anyway would be
pointing at a rounding error. You can always pick the peak frame yourself, and once you
have, AURA never overrules it - not on a re-analysis, not after a settings change, not
after a model update.

The peak is sometimes named more precisely: **kiss apex**, **tear release**, **bouquet in
the air**, **ring slide**. Those four come from combining what part of the day it is with
what AURA can see happening, so they are conservative - it takes two things agreeing before
AURA will use one of those names.

---

## Reactions

The feature no other culling product ships.

When something happens - a kiss, a ring going on, a toast - AURA looks four seconds either
side, **across every camera**, for frames where somebody is turned into the room and
reacting. The mother's face while the couple kiss. A sibling laughing at the best man's
line.

Those get linked as a pair. Later phases of the product use that link to make sure a keeper
and its reaction survive together, and to build album spreads that show cause and effect.

A link can point **backwards in time**, and that is not a bug: two cameras have two clocks,
and AURA aligns them to about a second.

---

## The face readings

Eight numbers per face, and they do not compete - a face can be laughing and crying at the
same time, and at a wedding many are.

| Reading | What it measures |
|---|---|
| **smile** | how much of a smile |
| **genuineness** | how much the smile reads as caught rather than held for the camera |
| **laughter** | open mouth, raised cheeks, usually a head tilt |
| **tears** | wet eyes and an expression that agrees with them |
| **surprise** | how much the face reads as surprised |
| **tenderness** | softened eyes, a small mouth, a lowered chin - the first-look expression |
| **composed** | how still and attentive the face is. **A positive reading** |
| **discomfort** | how much the face reads as awkward or caught mid-word |

**genuineness is not a judgement about a person.** In a family portrait everybody is
holding a smile because you asked them to, and that is exactly right - so AURA weights
genuineness almost to zero there. It matters in candids, where the difference between a
caught smile and a held one is the difference between a photograph and a snapshot.

**tears is deliberately cautious.** AURA will not use the word unless it is very sure, and
the threshold is high enough that it will miss some rather than invent any. A wrongly
detected tear is embarrassing in a way a missed one is not.

---

## What people are doing

Nine things AURA looks for between the people in a frame:

**Hug** · **Kiss** · **Holding hands** · **Dancing** · **Ring exchange** · **Blessing** ·
**Toast** · **Tears wiped** · **Group reaction**

A frame can have more than one - somebody wiping somebody else's tears is also holding
them. Four of them (kiss, ring exchange, blessing, tears wiped) count for more, because they
are the frames clients buy prints of.

---

## Every reason AURA can give

The complete list. AURA cannot say anything about a photograph that is not on it.

| Reason | What it means |
|---|---|
| `unremarkable` | Nothing here stands out as a moment, which is not the same as anything being wrong with it |
| `genuine_smile` | Somebody is smiling, and it reads as unposed |
| `posed_smile` | The smile reads as held for the camera - right in a posed frame, less so in a candid |
| `laughter` | Somebody is laughing |
| `tears` | A face reads as crying: wet eyes and the expression around them agree |
| `surprise` | A face reads as surprised |
| `tenderness` | The expression reads as tender rather than as a smile |
| `composure` | The faces are composed, and in this part of the day that is what the moment asks for |
| `discomfort` | A face reads as awkward, or caught mid-word - usually a frame either side of a better one |
| `mutual_gaze` | Two people are looking at each other |
| `looking_at_camera` | The subjects are looking at the lens |
| `interaction_detected` | Something is happening between the people in the frame |
| `peak_frame` | Of everything shot in this moment, this is where the action is strongest |
| `near_peak` | Close to the strongest frame of its moment |
| `off_peak` | Either side of the strongest frame - often the run-up or the settle, rather than a fault |
| `reaction_to` | Somebody here is reacting to something in another frame |
| `action_of` | Another frame catches somebody reacting to this one |
| `narrative_weight` | AURA asked a cloud model how important this moment is to the day's story |
| `no_faces` | No face was found, so the reading is about the frame rather than about anybody in it |
| `no_scene` | AURA has not worked out what part of the day this is, so it weighed things neutrally |

The last three are **notes about the reading** rather than statements about the photograph,
and the panel shows them separately for that reason.

---

## Telling AURA it is wrong

Two things you can say, and AURA keeps both.

**"This frame is the one."** Pick a different peak for a moment. It survives every
re-analysis, every settings change and every model update.

**"I would deliver this one."** Shown two frames, say which. AURA records it and - today -
changes nothing with it. That is deliberate: a product that re-tuned its ranking while you
were working would reorder the grid under your hands. Those answers are being collected for
the learning loop that arrives later, where they will move the numbers on purpose and with
a version stamp.

---

## What AURA cannot do yet

Said plainly, because a product that hides its limits is a product you find them in at the
worst moment.

**The two models behind the face and interaction readings are not trained.** They have the
right shape and no learning. On a real photograph they produce numbers that describe the
pixels arbitrarily rather than describing an expression. Everything *around* them is real
and measured - the weighting, the peak detection, the reaction linking, the cultural
adjustment, the caution about tears - and every number this product publishes about its own
accuracy is measured against synthetic frames whose answer is known in advance.

Training them needs a labelled set of wedding faces across traditions, and ten thousand
"which of these two would you deliver" comparisons from working photographers. Neither
exists yet.

**AURA reads where a head is pointed, not where the eyes are looking.** A person can look
sideways without turning their head. Every threshold is set cautiously because of it, and
gaze is one of nine things in the score rather than a decision on its own.

---

## Related

- [What the technical marks mean](frame-integrity.md) - the other half of how AURA reads a
  photograph
- [Moments, bursts and duplicates](moments-bursts-and-duplicates.md) - how frames become
  moments in the first place
- [Using your own AI key](using-your-own-ai-key.md) - the optional cloud call, and what
  leaves your machine when you make it
