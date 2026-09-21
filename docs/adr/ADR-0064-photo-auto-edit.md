# ADR-0064: Usable photograph editing

Accepted 2026-09-09 for the local desktop workflow.

Add a `photo_auto_edit` command accepting a project and photograph. It reads a
bounded sRGB preview and asks the existing governed cloud gateway for conservative
global adjustments. The configured provider, consent, budget, cache and offline
switches remain authoritative. Without a provider response, a deterministic
histogram enhancement runs locally and is labeled as such. This is not a trained
retouch model, and does not claim calibrated confidence or semantic recognition.

Only exposure, contrast, highlights, shadows and vibrance are proposed. Every
response is validated, carries reasons, and is merged through the recipe service
with manual-field protection. Repeated requests analyse the original preview,
so they do not compound earlier adjustments. The history label records provenance
and reasons. Originals remain read-only. Failed decoding must return an error,
never export a grey replacement photograph.

The additive IPC result contains the updated recipe, actual source, model and
reasons. Existing raw RGB render payloads retain their contract; the UI encodes
them for display. Native dialogs provide both individual-file and folder import.
PNG decoding joins the existing JPEG and camera-preview paths. Full RAW sensor
support remains camera-dependent and is not expanded by this decision.
