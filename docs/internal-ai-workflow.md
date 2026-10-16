# Internal AI and native skin repair

AURA automatically imports a file named `AI-CONFIG.md`. It is looked for in the
folder AURA was started from, then beside the application and in the few folders
above it (so a file in the project folder is found by a build inside it and by
the installed copy in `app/`), and last at `D:\AI-Company\HERMES-AI-CONFIG.md`.
Set `AURA_AI_CONFIG_FILE` to use one specific file and nothing else. Only the
desktop application looks beside itself; tests and command-line tools never do.
Credentials are stored through the operating system; Windows uses user-scoped
DPAPI. The file is read locally and is never uploaded or bundled with the app.
It holds live keys in plain text: `AI-CONFIG.md` is in `.gitignore`, and it
should not be shared, synced or attached anywhere.

The file uses one group per provider:

```text
STUDIO_API_KEY=<provider API key>
STUDIO_BASE_URL=https://provider.example/v1
STUDIO_MODEL=<model identifier>
```

The base URL includes the API's complete base path. AURA appends `/models` for
discovery and `/chat/completions` for requests, without adding a second `/v1`.
HTTPS, complete groups and unique assignment fields are required. Markdown
headings/comments beginning with `#` are ignored; commands are never executed.
The source file remains under your control and is not rewritten by AURA.

AI settings show every configured profile and any discovered vision alternative.
A model list proves that discovery responded, not that a key can complete an
image request, so every route that would be sent photographs is first shown a
small generated picture (a yellow half and a blue half) and asked to name the
two colours. Only a route that answers correctly within 15 seconds is used. No
photograph is sent to find out whether a provider works. The settings say what
each check found in plain words: the provider did not accept this key, the
account has no balance, over its quota or rate limit today, did not answer
within 15 seconds, or answered without reading the picture. The check runs when
the file is imported: once a day at startup, and when you press **Refresh
internal providers**. A provider that was over its quota this morning is
offered again after a refresh.

Image support must be declared by the catalogue or documented for the exact
supported endpoint/model family, and is then confirmed by the check. When the
model you configured does not read images, AURA looks for another model the
same key reaches and shows up to three of them the check picture, keeping the
first that reads it. Text-only routes can serve text tasks. They are excluded
from photo analysis.
If a profile explicitly selects a free model, discovered vision alternatives
must also be free. AURA does not silently upgrade that profile to a paid model.

Unknown prices block a route. A provider with a catalogue price uses that price;
supported published rates and explicit free development endpoints are recognized.
For another billing arrangement, add both rates in dollars per million tokens:

```text
STUDIO_INPUT_USD_PER_MTOK=1.0
STUDIO_OUTPUT_USD_PER_MTOK=4.0
```

Confirm those rates with your provider. A zero rate is appropriate only for an
actually free service. The spending meter estimates token charges; discounts,
credits, provider tools and unknown usage from a failed network response may
differ from the provider invoice. Current known services are based on official
[Gemini pricing](https://ai.google.dev/gemini-api/docs/pricing),
[Mistral pricing](https://docs.mistral.ai/inference/pricing),
[Groq models](https://console.groq.com/docs/models),
[Kilo free routing](https://kilo.ai/docs/getting-started/using-kilo-for-free) and
[NVIDIA development endpoints](https://docs.nvidia.com/ai-workbench/user-guide/latest/how-to/integrations/nvidia-integrations.html).

Refresh internal providers reloads configuration and retries discovery. Startup
and collection-run boundaries refresh discovery on a new UTC day. Offline mode,
project privacy settings and spending caps continue to control photo uploads.
Only downscaled derivatives are sent; native full-resolution originals stay local.
Existing explicit privacy choices, expiry and revocation are preserved.
A discovery-listed Gemini 3.6 Flash route can serve as an alternative to Gemini
3.8 Flash when its image service is busy. Both are restricted to the documented
Google endpoint and remain subject to spending caps. A failed configuration
refresh is reported in the collection run; saved routes or local editing continue.
An explicit refresh in settings still shows its error for correction.

A route that fails during editing rests before it is asked again. One that was
rate-limited rests for as long as the provider itself said (a per-minute free
allowance is usually free again in seconds); one that did not answer in time,
or whose answer could not be used, rests for ten minutes. At most three routes
are tried for one photograph, as before.

Photo grading uses a validated reversible recipe. The model proposes and AURA
decides: the proposal goes through the same guards as AURA's own measurement
(skin not clipped, a white wall not mistaken for overexposed people, a frame
that is dark or bright by design left that way), and exposure starts from AURA's own measurement of the photograph: the model
may move it by at most 0.15 EV either way. Measured on 22 photographs and the
same photographs overexposed, the models did not tell the two apart reliably
and the measurement did, so a model can neither darken a photograph nothing
says is bright nor undo an overexposure the measurement found. The reasons shown with the edit say when a proposal was
adjusted and why. When no provider answers, the automatic edit is AURA's own
measured light and colour correction, and the reasons say so. Skin advice says how hard to look for marks on face and visible
body skin: two to four healing passes, and a sensitivity that is never under half.
How texture is handled is not the model's to say; it is the finish calibrated
against published retouching, the same one Portrait skin finish applies, with
freckles and dark marks kept. The renderer measures skin selections and protects eyes, nostrils,
contours and other excluded features. Saved custom preferences and manual edits
take precedence; the AI cannot issue arbitrary operations or redraw the person.
Provider failure uses measured local editing and is recorded per photo. A selected
reference or saved look retains precedence over the AI's grading preference.
Fresh photos use visible face and body skin in local fallback as well as AI-guided
retouch, including local one-click batch editing. Body healing is enabled in the
shared automatic defaults. An explicitly saved face-only choice remains face-only,
even when the run finds no face and creates no skin operations. Repeating an
unchanged AI grade leaves the saved recipe and its undo history unchanged.

Use **Detailed skin repair and review** to opt into four native cleanup passes.
In Retouch, choose a detected skin selection or draw a brush, then **Check remaining
marks**. The check reads the saved image. Candidate regions can prepare a focused
brush, which you preview and apply yourself. It is a review aid: pores, freckles,
hair, lighting and false positives need visual judgement. A zero-candidate result
does not establish 100% blemish removal. Existing recipes retain two passes.

In the AI decision history, rate a successful result **Useful result** or **Needs
improvement**. AURA reviews recent reliability and explicit ratings once per day
per project to rank future routes. It does not train a foundation model or rewrite
its code. Learning is local, bounded and based on recorded outcomes; professional
retouch quality and full Evoto feature parity still need separate validation.

See [ADR-0114](adr/ADR-0114-internal-ai-routing-and-residual-review.md) for the
implementation decisions and boundaries.
See [native validation](internal-ai-native-validation.md) for the tested build,
real-photo exports, provider availability and remaining quality limitations.
