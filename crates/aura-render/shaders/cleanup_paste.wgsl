// PHASE-24. Putting a stored patch back where an object was.
//
// The GPU half of `apply` in `crates/aura-render/src/cleanup.rs`, and the simplest stage shader
// in the pipeline by a wide margin. That is not an accident: **every decision this operation
// embodies was made somewhere else.**
//
// `aura-generative` decided the rectangle, proved it safe against phase 18's regions, chose the
// source, produced the pixels and ran the artefact self-check over the result. A photographer
// then accepted it. What is left for a renderer to do is copy the approved samples into the
// frame, and anything more than that would be a second opinion about a decision that has already
// been taken and disclosed.
//
// Everything here is LINEAR and nothing encodes. Invariant 8.
//
// WHAT THIS SHADER MUST NOT DO, AND WHY THE LIST IS THE INTERESTING PART
//
// It must not re-run the fill. The patch in the recipe's `cleanup[]` operation refers to pixels
// that were measured, checked and approved; re-deriving them at render time would put *different*
// pixels into the delivered file from the ones the self-check passed, and the disclosure would
// then describe a removal that never happened. A render that cannot find the patch leaves the
// object in the photograph and reports `SkipReason::CleanupPatchAbsent`.
//
// It must not blend toward the original inside the region. The patch already covers the whole
// object at full weight and its feather - where it has one - was baked in by the borrow, on the
// band of real background *outside* the object. Feathering here would blend the outermost pixels
// of the replacement back toward the thing being removed, which is a rim of the exit sign left by
// the code that exists to hide the seam. Both removal modules shipped that defect once;
// `aura_generative::pixels::feather_out` is where it is written down.
//
// It must not scale. A patch whose dimensions do not match its rectangle at this render level is
// refused on the host rather than resampled, because a resampled patch is a different set of
// samples from the one the self-check saw.
//
// NO ATOMICS. Each invocation writes one sample.

struct CleanupParams {
    // Frame dimensions at this render level.
    width: u32,
    height: u32,
    // The region, in pixels at this level: top-left and size.
    patch_x: u32,
    patch_y: u32,
    patch_w: u32,
    patch_h: u32,
    // How many operations this dispatch covers. One dispatch per operation; the count is here so
    // the shader can be validated against the host's own loop rather than trusting it.
    op_count: u32,
    // Which operation this dispatch is, zero-based.
    op_index: u32,
};

@group(5) @binding(0) var<uniform> cleanup_params: CleanupParams;
@group(5) @binding(1) var<storage, read> cleanup_patch: array<f32>;
@group(5) @binding(2) var<storage, read_write> cleanup_out: array<f32>;

// The linear index of one sample of the frame.
fn cleanup_frame_index(x: u32, y: u32) -> u32 {
    return (y * cleanup_params.width + x) * 3u;
}

// The linear index of one sample of the patch.
fn cleanup_patch_index(x: u32, y: u32) -> u32 {
    return (y * cleanup_params.patch_w + x) * 3u;
}

@compute @workgroup_size(8, 8, 1)
fn stage_cleanup(@builtin(global_invocation_id) id: vec3<u32>) {
    let px = id.x;
    let py = id.y;
    if (px >= cleanup_params.patch_w || py >= cleanup_params.patch_h) {
        return;
    }

    let fx = cleanup_params.patch_x + px;
    let fy = cleanup_params.patch_y + py;
    if (fx >= cleanup_params.width || fy >= cleanup_params.height) {
        // A region that leaves the frame is a region about a different photograph. The host
        // refuses one before it dispatches; this is the second layer, and it drops the sample
        // rather than wrapping it onto the opposite edge.
        return;
    }

    let source = cleanup_patch_index(px, py);
    let target = cleanup_frame_index(fx, fy);

    // A straight copy. See the header for the three things this deliberately is not.
    cleanup_out[target + 0u] = cleanup_patch[source + 0u];
    cleanup_out[target + 1u] = cleanup_patch[source + 1u];
    cleanup_out[target + 2u] = cleanup_patch[source + 2u];
}
