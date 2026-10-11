# Internal AI and native skin repair

AURA automatically imports `D:\AI-Company\HERMES-AI-CONFIG.md`. On another
machine, set `AURA_AI_CONFIG_FILE` to the local file before launching AURA.
Credentials are stored through the operating system; Windows uses user-scoped
DPAPI. The file is read locally and is never uploaded or bundled with the app.

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
Catalogue success proves that discovery responded, not that a key can complete
an image request. Missing models, rejected keys and unreachable services retain
visible status. Image support must be declared by the catalogue or documented
for the exact supported endpoint/model family. Text-only routes can serve text
tasks. They are excluded from photo analysis.
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

Photo grading uses a validated reversible recipe. Skin advice can recommend up to
four residual healing passes and restrained pore refinement on face and visible
body skin. The renderer measures skin selections and protects eyes, nostrils,
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
