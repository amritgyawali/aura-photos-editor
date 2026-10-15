# Automatic GPU photo output

AURA automatically uses an available hardware graphics card for the final output conversion
in edited previews and exports. It processes the edited image's highlight roll-off, output
colour space, display encoding and pixel quantization together on the card. Retouch planning,
other editing stages and file decoding continue through their existing implementations.

The card is checked against AURA's CPU colour reference and timed with transfer costs included.
Small images stay on the CPU below the measured crossover. Unsupported adapters, slower cards
and driver failures fall back automatically; originals and saved edit recipes are preserved.
The renderer's capabilities show `wgpu` and an engine identity beginning with `wgpu.output`
when a qualified GPU is available. Actual render results report the backend that ran.

For troubleshooting, start AURA with `AURA_DISABLE_GPU=1` to force CPU output. Remove the
variable and restart to enable automatic selection again. This switch changes rendering,
not your photos or edit recipes. Driver and qualification details are logged under `render.gpu`.

The local GTX 1050 Ti output-stage benchmark improved from approximately 1.9 seconds to
60–77 milliseconds for a 6.3-megapixel synthetic frame. Total editing time also includes
decoding, analysis, retouching and encoding, so the whole job's gain will be smaller.

Implementation and qualification policy: [ADR-0122](adr/ADR-0122-qualified-gpu-output-and-cpu-fallback.md).
