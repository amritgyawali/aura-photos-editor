# Business and Go-To-Market

## Value proposition in one sentence

AURA gives a wedding photographer their evenings back: 8-20 hours of post-production per wedding becomes
one button, one review pass, and a delivered gallery.

## Pricing (recommended)

| Plan | Price | Includes |
| --- | --- | --- |
| Trial | Free, 14 days | Two full weddings, watermark-free, all features except cloud reasoning |
| Solo | USD 29 / month or 290 / year | Unlimited weddings, local AI, one style profile set, Lightroom/Photoshop integration |
| Studio | USD 79 / month or 790 / year | Three seats, second-shooter matching, shared profiles, team delivery, priority support |
| Enterprise | Quote | Outsourcing studios: volume seats, audit reports, custom profiles, SLA |

Cloud reasoning uses the customer's own API key, so inference cost is never our margin risk.
This is a deliberate structural advantage over cloud-only competitors whose gross margin shrinks with usage.

## Unit economics (illustrative)

| Line | Solo |
| --- | --- |
| Revenue per year | USD 290 |
| Cloud cost to us | ~0 (customer key) |
| Model distribution and updates | ~USD 6 |
| Support (0.4 tickets/month at USD 3) | ~USD 15 |
| Payment and platform fees | ~USD 12 |
| Gross margin | ~USD 257 (89 %) |

High gross margin funds the dataset, which is the moat. Protect it: avoid features that require us to pay
for per-image cloud inference.

## Launch sequence

1. **Alpha (private, 10 photographers).** Phases 01-13. Culling only, positioned as "the most explainable
   wedding culler that exists". Collect licensed weddings.
2. **Beta (closed, 20 photographers).** Phases 01-17 plus 28 and 30. Culling plus grading plus style plus autopilot plus export.
   Publish honest benchmarks against competitor culling times.
3. **V1 public launch.** Zero-Touch Autopilot. Message: *shoot the wedding, import the RAWs, click once, deliver.*
4. **V2/V3 within 9 months.** Retouch depth and gallery intelligence, marketed as "the only tool that edits
   the gallery, not the photo".

## Distribution

- Wedding photography communities and Facebook groups (where this audience actually lives).
- Educators and workshop leaders: free studio licences in exchange for honest teaching.
- YouTube long-form: full 3,000-frame wedding processed in real time, unedited. This category buys on proof.
- Comparison content: side-by-side against the tools they already pay for, with our failures shown too.
  Credibility converts better than polish in this market.

## Retention levers

1. Style profiles get better the longer they use it (switching cost rises honestly).
2. The learning loop makes their corrections compound.
3. Delivery integrations put us in the middle of their workflow.
4. QC reports become part of their studio quality process.

## Key business risks

| Risk | Response |
| --- | --- |
| Adobe ships wedding-aware automation | Depth of wedding domain plus offline plus dataset; be the specialist they cannot justify being |
| Aftershoot/Imagen add gallery consistency | Ship it first and make it measurable; publish the metrics |
| Photographer backlash against AI editing | Radical transparency: explainability, disclosure, refusal list, no identity alteration |
| Support burden of GPU diversity | Hardware tiers, pre-flight checks, honest performance expectations, DirectML/CPU fallbacks |
