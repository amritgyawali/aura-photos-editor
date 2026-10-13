# ADR-0113: Explicit color, local proportions and provider dashboard links

Accepted 2026-10-11.

The native retouch extension gains Colorize, BackgroundColor and Reshape tools.
Color tools carry optional `targetColor` as normalized display sRGB. Existing
edits omit the new field and retain their rendering; new color tools require a
finite three-channel target. Source samples, per-photo selections and masks remain
photo-specific. Tool presets may retain a color but never selection coordinates.

Colorize converts sRGB to the linear Rec.2020 working space and replaces chroma
while maintaining photographed luminance. Gamut compression limits highlight
clipping without smoothing the photographed detail. Colorize's explicit brightness
control reuses `warmth` as an exposure offset of up to four stops; its default zero
preserves luminance, while negative values can darken gray or white hair.
BackgroundColor performs selected solid-color compositing. Both use the same selection, preview, history
and full-resolution render/export path as existing native operations.

Reshape is a manual inverse-map width/height adjustment inside an ellipse, limited
to 25% local scale change. It samples an immutable input with bilinear interpolation,
with displacement tapering to zero at the boundary. Unselected destination pixels
remain exact, and masked-off source pixels cannot be interpolated into the selected
area. Zero width/height change is an exact identity. Resampling changes geometry
and can reduce local detail; this is not an automatic skin repair or anatomical model.

The provider browser command accepts only a provider identity, resolves a published
HTTPS key dashboard from the compiled catalogue, and launches the system browser
without shell interpolation, credentials or user-provided URLs. Unknown identities
and providers without dashboards are refused. Browser visits never mark a key as
saved or checked, enable cloud processing, or implement OAuth. API keys continue
through the existing OS credential-store surface only. Chat-session cookies and
subscription access tokens are not API keys.

The Evoto catalogue inventory is a conservative gap tracker. Public comparison
images are visual references, not shared algorithms or an equivalence certificate.
