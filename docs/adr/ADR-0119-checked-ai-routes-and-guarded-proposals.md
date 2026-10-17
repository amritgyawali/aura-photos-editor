# ADR-0119: AI routes are checked with a picture, and a model's proposal is held to the photograph

Status: Accepted
Date: 2026-10-14

ADR-0114 imports a file of AI providers and routes photo tasks through the
governed gateway. The first time a real file was imported, with eleven providers
in it, the settings reported seven photo routes and not one of twenty-four calls
was answered. Every photograph fell back to a local stand-in that made it
visibly worse. This records what was wrong and what replaced it.

## 1. The provider file is found beside the work

`AI-CONFIG.md` is looked for in the folder AURA was started from, then beside
the application and in the five folders above it, then at the original fixed
path. `AURA_AI_CONFIG_FILE` names one file and nothing else is tried. Only the
`aura-desktop` executable looks beside itself: a test binary or command-line
tool built in the same checkout sits just as close to a photographer's real
keys and must never import them because of where it was built.

The file holds live keys in plain text. It is in `.gitignore`.

## 2. A route is offered only after it read a picture

A model list says a model exists. Asked directly, the seven "available" routes
were:

| Route | What a real image request returned |
|---|---|
| Kilo, free vision model | 401, the token is not valid |
| OpenRouter, free vision model | 429, the free daily allowance was spent |
| Mistral medium | 429, rate limited |
| Google, Gemini 3.8 Flash | no answer in 60 s |
| Google, Gemini 3.6 Flash | an answer after 47 s, past the 30 s deadline |
| NVIDIA, Llama 3.2 90B vision (two keys) | no answer in 60 s |

None of that is visible in a catalogue, and a route that fails this way takes
one of a photograph's three attempts and up to thirty seconds with it.

At import, each route that would be sent photographs is shown a generated
64 px picture, yellow on the left and blue on the right, and asked to name the
two colours. It stays a route only if the answer names yellow and then blue
within 15 seconds. Otherwise its status says what happened (`check_http_401`,
`check_http_429`, `check_no_answer_in_time`, `check_image_not_read`), the
settings turn that into a sentence, and it is not used until the next import.

- **No photograph is sent to find out whether a provider works.** The picture
  is made in code.
- **A blind model does not pass by guessing.** A model that cannot see answers
  a refusal or a common pair; "yellow, then blue" in that order is not one.
- **Silence is tried once more; a refusal is not.** Every provider in the file
  is asked at the same moment, so the first silence is as likely to be this
  machine as that service. A refusal is an answer, and asking again would spend
  the allowance it is about.
- **Providers do not wait for each other.** Each has its own thread. Sequential
  discovery made the slowest provider everyone's delay.
- **When the configured model does not read images, up to three alternatives
  are shown the picture** and the first that reads it becomes the photo route.
  Before, the first alternative in sort order was taken on the catalogue's word.

Two models that answer image requests but whose catalogues declare no
modalities were added to the documented list: `meta/llama-3.2-11b-vision-instruct`
on NVIDIA's endpoint and `qwen/qwen3.8-27b` on Groq's. The check is what
confirms them; the list only makes them candidates.

## 3. How long a failed route rests

A failed route rested 60 seconds whatever the cause. Three causes, three rests:

- **Rate limited:** as long as the provider said, between 5 seconds and 10
  minutes. One free tier allows 8,000 tokens a minute, an image request is about
  2,300, and its refusal says "try again in 18 s". Resting it a minute wastes
  the allowance; resting it ten wastes the provider.
- **No answer in time, or an answer that could not be used after the repair:**
  10 minutes. Neither changes in a minute, and both are the slowest way there
  is to reach the local answer: a weak model cost 20 to 60 seconds a photograph
  failing the same schema each time.
- **Anything else:** 60 seconds, as before.

The pause travels in the audit row's note beside the error code. At most three
routes are tried for one photograph, unchanged: a refused request still carried
the photograph's derivative to that provider.

## 4. When no model answers, the edit is AURA's own measurement

The task's built-in stand-in reads one histogram and aims its median at middle
grey. On a bright studio frame that is most of a stop of darkening. Measured as
mean absolute error against the finished photograph, before any retouch:

| Photograph | Untouched | The stand-in | Untouched | AURA's measured correction |
|---|---|---|---|---|
| | 1600 px | 1600 px | full size | full size |
| Beauty | 2.32 | 29.12 | 1.75 | 2.01 |
| Headshots | 1.89 | 30.71 | 1.25 | 4.94 |
| Maternity | 1.92 | 21.37 | 2.20 | 3.45 |
| Newborn | 2.12 | 6.84 | 2.01 | 3.29 |
| Events | 13.18 | 17.03 | 13.17 | 13.83 |
| Wedding | 13.58 | 23.04 | 13.54 | 12.25 |

The stand-in was measured on a 1600 px render before it was replaced and the
measured correction at full size after, so each is beside the untouched figure
at its own scale.

`photo_auto_edit` now uses the light half of Auto enhance when the answer is a
local fallback: the measured correction, the face and intent guards, the scene
analysis and white balance, with the decisions in words as the reasons. The
stand-in stays in the task because a task without a local fallback does not
compile, and is no longer what a photographer receives.

## 5. A model proposes; the photograph decides

A vision model's answer was written into the recipe as given, inside the
schema's ranges. Two things showed that is not enough. The same model, shown
the same frame twice, answered "well exposed" and "-0.4, significantly
overexposed". And a small model asked for -0.50 EV on a studio headshot while
its own reasons said nothing needed changing.

- **The proposal goes through AURA's own guards.** `guard_tone` is the three
  rules Auto enhance already applies to its histogram reading: skin neither
  clipped nor lifted past its headroom, a white wall not mistaken for
  overexposed people, a frame dark or bright by design left that way. On the
  headshot the -0.50 became 0, with the reason shown.
- **Exposure is taken only as far as two estimates agree.** The model's
  exposure is limited to AURA's own guarded measurement plus 0.15 EV when they
  point the same way, and to 0.15 EV when they do not or the measurement asks
  for nothing. Exposure is the one control where a wrong answer spoils the
  whole photograph, and a model reading a 512 px preview is not a light meter.
- **The prompt says a bright median is not overexposure** (task version 3, so
  no answer cached under the old wording is served).

The reasons shown with an edit say when a proposal was adjusted and why.

## 6. What the window did with real providers

`scripts/test-ai-config-gui.py`, on the six published before and after pairs,
through the desktop window's own commands, 2026-10-14:

- Sixteen routes imported; three photo routes passed the check (one Groq, two
  NVIDIA). The rest say why: key not accepted, allowance spent, no answer.
- Five of six photographs were graded by a model and five retouches used a
  model's skin advice; ten calls answered, six fell back. One model answers in
  1 to 5 seconds.
- Error against the finished photograph, untouched then after the AI edit and
  AI-advised retouch, full size: Beauty 1.75 to 1.77, Headshots 1.25 to 2.50,
  Maternity 2.20 to 2.17, Wedding 13.54 to 13.52, Events 13.17 to 14.59,
  Newborn 2.01 to 3.26 (no model answered; AURA's measured correction).
- No grade left a photograph more than 4 further from its finished version.
  With the stand-in, four of six were 9 to 29 further.

## What this does not establish

That an AI-guided edit is better than AURA's own. On these six pairs the best
model mostly, and correctly, proposed no change to light and colour, and the
skin advice it gave was gentler than the calibrated presets of ADR-0118. The
finished photographs it is scored against were retouched, not regraded, so any
change of tone counts against it.

That a provider which passed the check will answer the next photograph. Free
allowances run out mid-shoot, and the check is once a day or on refresh.

That the two estimates of exposure are independent enough for their agreement
to mean much. Both read the same pixels.

Six photographs, one day, one set of keys.
