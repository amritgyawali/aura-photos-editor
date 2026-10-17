# ADR-0122: Qualified GPU output with a CPU fallback

Status: accepted. Date: 2026-10-14.

The user requested automatic GPU photo processing. ADR-0100's per-retouch-pass transfers did
not earn enough speed to enable them. The terminal photographic output is a better first
boundary: filmic roll-off, gamut conversion, transfer encoding and quantization execute in
one compute dispatch over already edited linear Rec.2020 pixels. Other editing stages retain
their CPU implementations. This supersedes the decision to ship no GPU renderer for this stage.

`wgpu` 27.0.1 preserves the workspace's Rust 1.88 minimum and uses Vulkan, DirectX 12 or Metal.
Automatic detection prefers a hardware adapter and rejects a software adapter. One device,
pipeline and mutex-protected scratch set are reused. Row stripes bound GPU scratch to 72 MiB
independently of image resolution, preserve absolute dither coordinates and permit large output
buffers without relying on the card's maximum texture size. No model or source file is uploaded
to a network service.

Before accepting an adapter, run the real shader against the CPU output in all three colour
spaces at both bit depths, with negative samples and HDR values. Maximum qualified differences
are one 8-bit code or four 16-bit codes. These quantization bounds describe output rounding;
they do not establish perceptual preference. The 16-bit bound is below the existing 1/1024
stage tolerance. Repeated execution on the tested GPU produces identical codes.

Then time CPU and GPU output including upload, computation, readback and conversion. Probe
512, 1024 and 2048 square pixels, require 15% headroom, and use the smallest passing size as
the crossover. Below it, preserve CPU output. A card that fails parity or never beats the CPU
remains disabled. Initialization happens once on an image worker; unavailable hardware and
driver errors leave the CPU reference usable. A GPU execution failure disables that device
for the session and restarts output from the untouched CPU input. GPU completion/readback waits
are bounded. `AURA_DISABLE_GPU=1` disables probing and forces the reference path.

The application enables this hybrid path for saved previews, live previews and whole-frame
exports. A reference `CpuEngine::new` still defaults to CPU for qualification and callers needing
the exact reference. CPU-streamed, memory-limited renders remain supported. Actual results
identify `wgpu` only when the output used it; capabilities name `wgpu.output`, adapter, driver
and selected crossover. GPU cache identities are separate from CPU identities. A fallback
that changes the render hash is returned without being cached under the preceding GPU key.

Executing the old specification shader exposed a stale rational highlight shoulder. It was
corrected to the reference's exponential shoulder; 8-bit half-value rounding also follows
Rust's rule explicitly rather than WGSL even-tie rounding. The existing CPU output is unchanged.

Qualification on this machine's NVIDIA GTX 1050 Ti (Vulkan, driver 560.94) passed. A synthetic
2048-by-3072 output conversion took about 1.9 seconds on the CPU and 60–77 milliseconds on
the GPU over three alternating warm runs, with transfers included. This is the output stage
on this host in a development build, not an end-to-end photo-editing or cross-platform benchmark.
Colour parity on odd stripe boundaries covers sRGB, Adobe RGB and Display P3 at 8/16 bits.
Separate-process qualification verifies the disable switch returns identical CPU pixels/hashes.
Real desktop import/edit/export and cache checks complete the local qualification report.
