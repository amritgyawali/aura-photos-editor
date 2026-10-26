//! Antialiased working-space input for the main reference-first fitter.
use aura_core::AuraResult;
use aura_render::Frame;

pub(super) fn reduce(full: &Frame, edge: u32) -> AuraResult<Frame> {
    let length = (full.width as usize)
        .checked_mul(full.height as usize)
        .and_then(|n| n.checked_mul(3));
    if full.width == 0 || full.height == 0 || edge == 0 || length != Some(full.rgb.len()) {
        return Err(super::invalid("The native photo dimensions are invalid."));
    }
    // Use every source pixel. Point/bilinear sampling can lock onto just one stripe of
    // fabric and give both the optimizer and correspondence detector a false colour.
    let (rgb, width, height) = aura_render::edge_aware::reduce(
        &full.rgb,
        full.width as usize,
        full.height as usize,
        edge as usize,
    );
    Ok(Frame::working(
        rgb,
        width as u32,
        height as u32,
        &full.camera,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternating_fabric_keeps_both_colours_instead_of_aliasing_to_one() {
        let rgb = (0..1024_usize * 512)
            .flat_map(|i| {
                if i.is_multiple_of(2) {
                    [0.8, 0.1, 0.2]
                } else {
                    [0.2, 0.3, 0.8]
                }
            })
            .collect();
        let full = Frame::working(rgb, 1024, 512, "test camera");
        for edge in [256, 512] {
            let small = reduce(&full, edge).unwrap();
            assert_eq!((small.width, small.height), (edge, edge / 2));
            assert_eq!(small.camera, full.camera);
            for pixel in small.rgb.chunks_exact(3) {
                for (actual, expected) in pixel.iter().zip([0.5, 0.2, 0.5]) {
                    assert!((actual - expected).abs() < 1e-6);
                }
            }
        }
        assert_eq!(reduce(&full, 2048).unwrap().rgb, full.rgb);
    }

    #[test]
    fn malformed_and_empty_frames_cannot_be_used_as_fit_evidence() {
        for (rgb, width, height, edge) in [
            (vec![], 0, 1, 256),
            (vec![0.5; 3], 2, 2, 256),
            (vec![0.5; 12], 2, 2, 0),
        ] {
            assert!(reduce(&Frame::working(rgb, width, height, ""), edge).is_err());
        }
    }
}
