# 11 - The Zero-Budget Build (and the ladder to scale)

## Short answer

**Yes.** You can build, ship and sell AURA for roughly **USD 0 up to your first paying customer**, and about
**USD 120-250 per year** after that. Nothing in the 30-phase plan requires a server, a GPU cluster, a paid
API, or a licence fee to get to a sellable Windows product.

This is not luck. It is the direct consequence of three decisions already baked into the architecture:

1. **Local-first desktop app.** All processing happens on the customer's machine. You have **zero marginal
   cost per user and per image**. Aftershoot and Imagen pay GPU money every time a customer imports a wedding.
   You pay nothing. Ever.
2. **Bring-your-own API key.** The customer's key pays for the cloud reasoning calls. You never see the bill,
   never hold the liability, and never need a backend to proxy it.
3. **Fitted models, not giant neural networks, wherever possible.** Exposure targets, white balance priors,
   tone intent, culling weights and the entire personal style profile are small statistical fits that train in
   **minutes on a laptop CPU**. No GPU rental, no cloud training bill.

The honest caveat: free in *money* is not free in *time*. Budget 4-7 months of focused solo work with a
coding agent for a sellable V1. That is the real currency you are spending.

## 1. The software stack: licence and cost audit

Everything in the plan is free for commercial use. The only items with a catch are marked.

| Component | Licence | Cost | Catch |
| --- | --- | --- | --- |
| Rust + Cargo | MIT / Apache-2.0 | 0 | none |
| Tauri 2 | MIT / Apache-2.0 | 0 | none |
| React + TypeScript + Vite | MIT | 0 | none |
| wgpu (Vulkan / Metal / DX12) | MIT / Apache-2.0 | 0 | none |
| ONNX Runtime (CUDA, DirectML, CoreML, CPU) | MIT | 0 | none |
| SQLite | Public domain | 0 | none |
| OpenCV 4.5+ | Apache-2.0 | 0 | none |
| Little CMS (lcms2) | MIT | 0 | none |
| PyTorch | BSD-3 | 0 | none |
| **LibRaw** | LGPL-2.1 / CDDL-1.0 / commercial | 0 | **Must dynamically link** (ship it as a DLL/dylib and allow relinking) to stay LGPL-compliant in a closed-source app. Static linking requires the paid commercial licence. Design for a DLL from day one - it costs nothing and removes the problem permanently. |
| EXIF parsing | use `kamadak-exif` (BSD-2) | 0 | Avoid `exiv2` (GPL) unless you are prepared to open-source. |
| XMP sidecars | you write the XML yourself | 0 | XMP is a documented open format; no SDK needed. |

**Rule:** before adding any dependency, check the licence. GPL and "research only" are the two words that can
force you to either open-source your product or pay a licence fee later, when you have the least leverage.

## 2. The real trap: model licences, not software licences

This is where most "free AI product" plans quietly break. A pretrained model can be free to download and
**illegal to sell**. Audit every single one before you ship.

| Need | Free and commercially usable | Avoid (research-only) |
| --- | --- | --- |
| Face detection | **YuNet** (OpenCV Zoo, permissive) | InsightFace pretrained weights (non-commercial) |
| Face embedding | **SFace** (OpenCV Zoo, Apache-2.0) | ArcFace/InsightFace release weights |
| Perceptual embedding | **CLIP** (OpenAI, MIT) or **OpenCLIP** (MIT) | - |
| Aesthetic scoring | LAION aesthetic predictor (MIT) | - |
| Segmentation | **MobileSAM / SAM** (Apache-2.0) | BiSeNet face-parsing weights trained on CelebAMask-HQ |
| Denoise / sharpen | **train your own** small U-Net on synthetic noise | most published restoration weights |
| Generative inpainting | sibling-frame borrowing (your own code) or FLUX.1 schnell (Apache-2.0) | LaMa weights (Places2, research) |
| Datasets | **your own archive**, partner-licensed weddings, Open Images (CC BY) | WIDER FACE, FFHQ, CelebA - all research-only |

Two cheap habits that make this a non-issue:

- **Heuristics first, models later.** Laplacian variance for focus, histogram statistics for exposure,
  gray-world and white-patch for WB, dHash plus timestamp proximity for bursts and duplicates, Hough lines for
  horizons. These are free, instant, explainable, and good enough to *ship*. Phase by phase you replace them
  with learned models once you have data. Nothing in the plan's contracts changes when you do.
- **Fitted models over trained models.** Ridge regression, robust estimators and lookup surfaces reproduce a
  photographer's editing behaviour with a few hundred examples and train on CPU. This is exactly how Phase 17
  is specified. It is also why your style learning can beat Imagen's 2,000-photo requirement with 300 pairs.

## 3. The three things that actually cost money

| Item | Real cost | Zero-cost workaround | When to actually pay |
| --- | --- | --- | --- |
| **Windows code signing** | USD 120-500 / year, or Azure Trusted Signing at ~USD 10 / month | Ship unsigned during alpha and closed beta. SmartScreen shows a warning; technical early users click through when you warn them first. | The day you take money from a stranger. An unsigned paid installer destroys conversion. |
| **Apple Developer Program** | USD 99 / year (required for notarisation) | **Launch Windows-only.** Most wedding photographers with NVIDIA GPUs are on Windows, and that is where your CUDA speed advantage shows. | ~USD 300 MRR, or when 3+ prospects ask for macOS. |
| **Training data and GPU** | The moat | Your own archive + Kaggle's free GPU hours + the learning loop | When paid customers start opting in - then it is free again, just bigger. |

Everything else on the list below is genuinely free at your scale.

## 4. Free-tier infrastructure map

| Need | Free tool | Free limit | What breaks first | Paid successor |
| --- | --- | --- | --- | --- |
| Source control + CI | GitHub Actions | 2,000 min/month private (public unlimited) | macOS runners burn minutes 10x | USD 4/month Team, or run macOS builds locally |
| Model + installer hosting | **Cloudflare R2** | 10 GB storage, **zero egress fees** | storage, not bandwidth | USD 0.015/GB/month |
| Marketing site + docs | Cloudflare Pages | unlimited bandwidth | nothing realistically | - |
| Licence key server | Cloudflare Workers + D1 | 100k requests/day | nothing at your scale | USD 5/month |
| Payments | Lemon Squeezy or Paddle | no fixed fee, ~5% + 0.50 per sale | nothing - it scales with revenue | - |
| Crash reporting | Sentry free tier | 5k errors/month | a bad release | USD 26/month |
| Product analytics | PostHog free tier | 1M events/month | nothing at your scale | usage-based |
| Transactional email | Resend | 3,000/month | nothing at your scale | USD 20/month |
| Support | Discord server + Notion docs | - | your own time | help desk later |
| Training GPU | **Kaggle Notebooks** | ~30 GPU-hours/week, free | long training runs | rent hourly, see ladder |
| Domain | Cloudflare Registrar | at-cost, ~USD 10/year | - | - |

**Total fixed cost to run a live, paid product on this map: about USD 10 per year (the domain), until you buy
a signing certificate.**

## 5. Training everything on zero budget

1. **Kaggle Notebooks give roughly 30 GPU-hours per week for free.** That is more than enough for the handful
   of small vision models in this plan. Train in 6-hour chunks, checkpoint to the notebook's output, download
   the ONNX file. Google Colab's free tier is a backup.
2. **Most of the intelligence needs no GPU at all.** Culling weights, exposure targets, WB priors, tone intent,
   camera transforms, style deltas and consistency solving are all statistics. They train on your laptop in
   minutes and they are the features customers actually notice.
3. **The learning loop is free labelled data.** Every correction a photographer makes is a perfectly targeted
   training example that arrives at zero acquisition cost. This is Phase 30, and it is why the plan puts it in V1
   instead of treating it as a nice-to-have.
4. **Synthetic data is free.** Noise models, motion blur, defocus, lens distortion and colour casts can all be
   simulated from clean images you already own. Denoise, deblur and integrity models train fine on synthetic
   degradation of your own weddings.

## 6. Your unfair advantage costs nothing

You run a wedding photography studio. That means you already own the single most expensive asset in this
business plan: **thousands of real weddings with RAW originals and photographer-approved final edits.**

A venture-funded competitor has to buy that. You have it on a hard drive.

- Phase 17 is written to fit edit recipes from **JPEG finals alone** when XMP sidecars are missing, so old
  archives still work.
- Twenty of your own weddings, properly labelled, get every model in the plan to its quality gate.
- Nepali, Hindu and mixed-tradition weddings are exactly the coverage that Aftershoot and Imagen are weakest
  at. Your archive is not just free data, it is *differentiated* data.

Do one thing this month that costs nothing: pick 20 finished weddings, and store them with RAW plus final
plus your delivery decisions. That is the seed of the Wedding Intelligence Dataset.

## 7. The lean V1 cut

Do not build 30 phases before selling anything. Build **16** and charge for them.

| Keep for V1 | Why |
| --- | --- |
| 01, 02, 03 | Foundation, RAW decode, inference runtime - unavoidable |
| 05, 08 | Embeddings, bursts and duplicates - the culling backbone |
| 09 | Frame integrity: focus, motion, exposure, eyes - the reason people buy cullers |
| 06, 07 (light) | People and scene understanding at heuristic depth first |
| 12, 13 | Autonomous culling with coverage guard, plus Explain My Edit |
| 14, 15, 16 | Develop engine, exposure, white balance, tone, colour |
| 17 | Personal style profiles - the retention feature |
| 28, 30 | Autopilot button, export, XMP, learning loop |

That is a complete, sellable product: **"Import 3,000 RAWs, click once, get a culled and graded gallery in
your own style, with XMP for Lightroom."** At USD 19-29 per month it competes directly with the culling tools,
and it needs no retouching, no generative AI and no cloud.

Phases 18-27 and 29 are your V2 and V3 - funded by V1 revenue, not by savings.

**Honest timeline, solo with a coding agent:** 20 weeks aggressive, 30 weeks realistic, 12 months part-time.

## 8. The spend ladder - buy only when revenue triggers it

| Trigger | Spend | Why then |
| --- | --- | --- |
| Day 0 | **USD 0** | Build, alpha with yourself and 3 photographer friends, unsigned installer |
| First beta users | **~USD 10/year** - domain | Credibility for the download page |
| First paying customer | **~USD 10/month** - code signing | Removes the SmartScreen warning that kills conversion |
| ~USD 300 MRR | **USD 99/year** - Apple Developer | Unlock the macOS half of the market |
| ~USD 500 MRR | **USD 4-26/month** - CI minutes, Sentry | Faster releases, real crash visibility |
| ~USD 1,000 MRR | **~USD 600-800 once** - used RTX 3090/4070 | Owning a training GPU beats renting once you train weekly |
| ~USD 3,000 MRR | **usage-based** - rented cloud GPU for optional heavy features, resold as credits | Only if customers ask for it, and only at a margin |
| Never | **servers for core processing** | Local-first is the moat. Do not give it away. |

Every line item is triggered by money coming in, never by hope.

## 9. Why your unit economics beat the funded competitors

| | Cloud competitor | AURA |
| --- | --- | --- |
| Cost per 3,000-image wedding | GPU seconds + storage + egress | **0** |
| Cost of a customer processing 40 weddings/year | scales linearly | **0** |
| Cloud reasoning cost | on their P&L | on the customer's own API key |
| Gross margin | shrinks with heavy users | ~89 %, flat |
| Break-even customers at USD 29/month | dozens, with a burn rate | **1** |

This is the whole argument. A heavy user is a *cost* to your competitors and a *pure profit* to you. It means
you can price lower, offer unlimited weddings, and still make money on day one - and it means you never need
funding to survive growth.

## 10. Scaling later without breaking the model

When users grow, **nothing on your side grows**. There is no queue, no cluster, no autoscaling group. What
actually changes:

1. **Model distribution** - more downloads. Cloudflare R2's zero egress fee means this stays nearly free;
   ship delta updates so a model revision is 20 MB, not 900 MB.
2. **Licence checks** - Cloudflare Workers handles 100k/day free. A 10,000-customer base checking once a day
   is 10k requests. You are fine.
3. **Support** - the first real cost, and it is time, not money. Invest in the Explain My Edit surfaces and
   support bundles from Phase 13 and 30; they cut ticket volume more than any help desk.
4. **Optional cloud GPU** - only for the heaviest generative work, only if customers ask, and only sold as
   prepaid credits at a markup. Rent per-second (spot instances) instead of running anything permanently.

## 11. Five rules that keep the cost at zero

1. **No servers in the critical path.** If the product cannot finish a wedding with the network unplugged, it
   is a design bug.
2. **The customer's key pays for the customer's AI.** Never proxy their cloud calls through your infrastructure.
3. **Heuristic first, fitted second, neural last.** Only spend GPU hours where the cheaper method demonstrably fails.
4. **Every dependency and every model gets a licence check before it is merged.** Add it to the pull request checklist.
5. **Every purchase needs a revenue trigger.** If you cannot name the customer whose payment justifies it, do not buy it.

## 12. What not to cheap out on

- **Code signing, once you charge money.** A scary Windows warning on a paid download is the single most
  expensive USD 10 you will ever save.
- **Backups of your training archive.** It is your only irreplaceable asset. Two local copies and one offsite.
- **Colour correctness.** Getting skin wrong is the one bug wedding photographers do not forgive. The plan gives
  the Colour Scientist role veto power for exactly this reason - honour it even when you are the one wearing that hat.
- **Honesty in marketing.** Free tools and no funding are fine. Overclaiming what the AI does is what actually
  kills products in this market.
