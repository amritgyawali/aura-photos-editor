# How confidence works

Every decision AURA makes carries a number and a list of reasons. This page explains what the
number means, what it does *not* mean yet, and what AURA is allowed to do at each level.

## The short version

| What you see | What AURA did | What it means |
|---|---|---|
| **Applied** | Did it, without asking | AURA is confident enough that asking would waste your time |
| **Applied in Zero-Touch** | Did it only if you turned Zero-Touch on | Confident, not certain |
| **Applied - worth a look** | Did it, and put it in your review queue | It went ahead and thinks you should see this one |
| **Waiting for you** | Did nothing | Not confident enough to act, so it is asking |

The thresholds are 0.98, 0.90 and 0.75, and they live in a file a person can read and argue
with: `crates/aura-explain/config/autonomy_bands.toml`. Every row in it carries a written
reason.

## The honest part

**AURA has not yet learned how often it is right.** The confidence you see is what the code
that made the decision believed at the time. It has not been checked against outcomes,
because checking it needs a large number of real weddings where somebody recorded which
decisions were correct.

Until that happens, AURA does two things:

1. **It says so.** Every panel that shows an uncalibrated confidence says it is a rough guide
   rather than a percentage.
2. **It asks more often.** Every decision is moved one band toward review. A decision that
   would be applied silently is applied and flagged; one that would be applied in Zero-Touch
   waits for you.

That is deliberately cautious, and it will be relaxed in the release that ships the first
calibration - not before.

## What calibration will change

Calibration is a mapping from what the code believed to what that belief is worth. If AURA
says 0.90 on a thousand decisions and is right on 750 of them, the calibration learns to
report those decisions as 0.75 - and the bands then mean what they say.

Two things it will never do:

* **It will not reorder anything.** The mapping is monotone: if AURA was more confident about
  A than about B before calibration, it still is afterwards. A review queue that reshuffled
  itself after an upgrade would be a review queue nobody trusts.
* **It will not change history.** Calibration applies to new decisions. A decision recorded
  last year keeps the confidence it was made with, because the ledger records what happened
  rather than what would happen now.

## Two decisions are treated more carefully than their number suggests

**Anything hard to undo.** A retouch, an album layout and an export are held to a higher bar
than a parameter edit, because reverting a parameter costs a click and reverting a gallery a
client has already opened costs a conversation.

**Anything that touches a part of the day the gallery may not be missing.** The rings, the
vows, the kiss, the first dance. A wrong decision about a dance frame costs a photograph; a
wrong decision about the only frame of the ring exchange costs the thing your client will ask
for first.

In both cases the *number* is unchanged - AURA never quietly lowers a confidence to make a
policy work - and the band is raised, with the reason shown beside it.

## What the reasons are

Every decision carries up to six of them, and each one is a code with a documented meaning, a
sentence, a weight and, where there is something to look at, the evidence. The full list is
in [the reason-code reference](reason-codes.md).

Reasons come in four kinds, and the third is the one worth knowing about:

* **In its favour** - something good about the photograph.
* **Worth knowing** - neutral, or a suspicion explicitly cleared. "Closed eyes belong to this
  moment" is one of these.
* **Not checked** - AURA did not measure something. **This is never a criticism of the
  photograph.** A frame nobody analysed is not a frame that failed, and everywhere in this
  product the two are drawn differently.
* **Counted against it** - something is wrong with the photograph.

## Where the record lives

Every decision is written to a ledger in your catalog, with its reasons, its evidence, the
versions of every model and setting underneath it, and a hash of the question it answered.
Nothing is ever overwritten: if a decision is corrected, the correction is a new entry that
points back at the old one.

That is what makes two things possible:

* **Replay.** `aura-cli replay --decision <id>` asks the same question again and tells you
  whether the answer moved - and if it did, whether that is because the pipeline changed
  (an upgrade) or because the same question got two answers (a bug).
* **Support.** An anonymised slice of the ledger can be exported and sent. It carries no
  photographs, no names and no file paths; every identifier in it is replaced with a handle
  before the file exists.
