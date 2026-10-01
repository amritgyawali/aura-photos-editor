//! Bundled, offline portrait detection. No identity embeddings are produced.
use std::sync::{Mutex, OnceLock};

use aura_core::AuraResult;
use aura_infer::{
    contract::infer::{Precision, Tensor, TensorView},
    onnx::Executable,
};
use serde::{Deserialize, Serialize};

pub const VERSION: &str = "yunet-2023mar-aura320-rotation-v2";
pub const MODEL_HASH: &str = "3d5938c4cd5a02dc416f1cd1f7fc1f662a22adc370477112c871954587e63431";
const SIDE: usize = 320;
const MODEL: &[u8] =
    include_bytes!("../../../assets/models/yunet/face_detection_yunet_2023mar.onnx");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitFace {
    /// Normalized left, top, right, bottom in the oriented original.
    pub bounds: [f32; 4],
    /// Two eyes, nose, two mouth corners, normalized to the original.
    pub landmarks: [[f32; 2]; 5],
    pub confidence: f32,
}

fn invalid(message: impl Into<String>) -> aura_core::AuraError {
    let mut error = aura_core::errors::ml::parse_failed(message);
    error.user_message =
        "Automatic portrait analysis could not finish. Try again or use the manual retouch tools."
            .into();
    error
}

fn graph() -> AuraResult<&'static Mutex<Executable>> {
    static GRAPH: OnceLock<Result<Mutex<Executable>, String>> = OnceLock::new();
    GRAPH
        .get_or_init(|| {
            if blake3::hash(MODEL).to_hex().as_str() != MODEL_HASH {
                return Err("Portrait model checksum mismatch".into());
            }
            let mut model = aura_infer::onnx::parse(MODEL).map_err(|e| e.to_string())?;
            let input = model
                .graph
                .inputs
                .first_mut()
                .ok_or("Portrait model has no input")?;
            input.shape = vec![Some(1), Some(3), Some(SIDE), Some(SIDE)];
            Executable::compile(&model, Precision::Fp32)
                .map(Mutex::new)
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|message| invalid(message.clone()))
}

/// Detect confident faces from oriented, packed sRGB pixels. Calls are serialized
/// to bound memory use; the 320-pixel model intentionally skips small crowd faces.
/// When upright detection is empty, try three quarter-turn views without
/// allocating rotated full-size photographs. Return original-space geometry.
/// # Errors
/// Invalid pixels, a damaged bundled model, or an inference failure.
pub fn detect(rgb: &[u8], width: u32, height: u32) -> AuraResult<Vec<PortraitFace>> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || w.checked_mul(h).and_then(|n| n.checked_mul(3)) != Some(rgb.len()) {
        return Err(invalid("Invalid portrait analysis pixels"));
    }
    for turns in [0, 1, 3, 2] {
        let mut faces = detect_view(rgb, w, h, turns)?;
        if !faces.is_empty() {
            for face in &mut faces {
                face.landmarks = face.landmarks.map(|p| original_point(p, turns));
                let [l, t, r, b] = face.bounds;
                let points = [[l, t], [r, t], [l, b], [r, b]].map(|p| original_point(p, turns));
                face.bounds = [
                    points.iter().map(|[x, _]| *x).fold(1.0, f32::min),
                    points.iter().map(|[_, y]| *y).fold(1.0, f32::min),
                    points.iter().map(|[x, _]| *x).fold(0.0, f32::max),
                    points.iter().map(|[_, y]| *y).fold(0.0, f32::max),
                ];
            }
            faces.sort_by(|a, b| {
                let [ax, ay, _, _] = a.bounds;
                let [bx, by, _, _] = b.bounds;
                ax.total_cmp(&bx).then(ay.total_cmp(&by))
            });
            return Ok(faces);
        }
    }
    Ok(Vec::new())
}

fn original_point([x, y]: [f32; 2], turns: u8) -> [f32; 2] {
    match turns {
        1 => [y, 1.0 - x],
        2 => [1.0 - x, 1.0 - y],
        3 => [1.0 - y, x],
        _ => [x, y],
    }
}

fn original_pixel(x: usize, y: usize, w: usize, h: usize, turns: u8) -> (usize, usize) {
    match turns {
        1 => (y, h - 1 - x),
        2 => (w - 1 - x, h - 1 - y),
        3 => (w - 1 - y, x),
        _ => (x, y),
    }
}

fn detect_view(
    rgb: &[u8],
    original_w: usize,
    original_h: usize,
    turns: u8,
) -> AuraResult<Vec<PortraitFace>> {
    let (w, h) = if turns % 2 == 1 {
        (original_h, original_w)
    } else {
        (original_w, original_h)
    };
    let scale = SIDE as f32 / w.max(h) as f32;
    let rw = (w as f32 * scale).round().max(1.0) as usize;
    let rh = (h as f32 * scale).round().max(1.0) as usize;
    let mut input = vec![0.0; 3 * SIDE * SIDE];
    // Half-pixel bilinear sampling preserves aspect ratio. Actual rounded width
    // and height are also used to map detections back, avoiding letterbox drift.
    for y in 0..rh {
        let fy = ((y as f32 + 0.5) * h as f32 / rh as f32 - 0.5).max(0.0);
        let y0 = (fy as usize).min(h - 1);
        let y1 = (y0 + 1).min(h - 1);
        for x in 0..rw {
            let fx = ((x as f32 + 0.5) * w as f32 / rw as f32 - 0.5).max(0.0);
            let x0 = (fx as usize).min(w - 1);
            let x1 = (x0 + 1).min(w - 1);
            for c in 0..3 {
                let at = |xx, yy| {
                    let (sx, sy) = original_pixel(xx, yy, original_w, original_h, turns);
                    f32::from(
                        rgb.get((sy * original_w + sx) * 3 + (2 - c))
                            .copied()
                            .unwrap_or(0),
                    )
                };
                let dx = fx - x0 as f32;
                let dy = fy - y0 as f32;
                let top = at(x0, y0) * (1.0 - dx) + at(x1, y0) * dx;
                let bottom = at(x0, y1) * (1.0 - dx) + at(x1, y1) * dx;
                if let Some(v) = input.get_mut(c * SIDE * SIDE + y * SIDE + x) {
                    *v = top * (1.0 - dy) + bottom * dy;
                }
            }
        }
    }
    let graph = graph()?
        .lock()
        .map_err(|_| invalid("Portrait analysis lock failed"))?;
    let outputs = graph.run(&[TensorView::new(vec![1, 3, SIDE, SIDE], &input)?])?;
    let output = |name: &str| -> AuraResult<&Tensor> {
        graph
            .outputs()
            .iter()
            .position(|o| o.name == name)
            .and_then(|i| outputs.get(i))
            .ok_or_else(|| invalid(format!("Missing portrait output {name}")))
    };
    let mut faces = Vec::new();
    for stride in [8, 16, 32] {
        let cls = output(&format!("cls_{stride}"))?;
        let obj = output(&format!("obj_{stride}"))?;
        let boxes = output(&format!("bbox_{stride}"))?;
        let points = output(&format!("kps_{stride}"))?;
        let grid = SIDE / stride;
        if cls.data.len() != grid * grid
            || obj.data.len() != grid * grid
            || boxes.data.len() != grid * grid * 4
            || points.data.len() != grid * grid * 10
        {
            return Err(invalid("Unexpected portrait model output shape"));
        }
        for (i, ((class, object), (bbox, kps))) in cls
            .data
            .iter()
            .zip(&obj.data)
            .zip(boxes.data.chunks_exact(4).zip(points.data.chunks_exact(10)))
            .enumerate()
        {
            let confidence = (class.clamp(0.0, 1.0) * object.clamp(0.0, 1.0)).sqrt();
            if !confidence.is_finite() || confidence < 0.85 {
                continue;
            }
            let [bx, by, bw, bh] = bbox else {
                continue;
            };
            if !bbox.iter().chain(kps).all(|v| v.is_finite()) {
                continue;
            }
            let cx = ((i % grid) as f32 + bx) * stride as f32;
            let cy = ((i / grid) as f32 + by) * stride as f32;
            let fw = bw.exp() * stride as f32;
            let fh = bh.exp() * stride as f32;
            if fw < 20.0 || fh < 24.0 || fw > rw as f32 || fh > rh as f32 {
                continue;
            }
            let bounds = [
                (cx - fw * 0.5) / rw as f32,
                (cy - fh * 0.5) / rh as f32,
                (cx + fw * 0.5) / rw as f32,
                (cy + fh * 0.5) / rh as f32,
            ];
            if bounds
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0)
            {
                continue;
            }
            let mut landmarks = [[0.0; 2]; 5];
            for (point, pair) in landmarks.iter_mut().zip(kps.chunks_exact(2)) {
                let [x, y] = pair else {
                    continue;
                };
                *point = [
                    ((i % grid) as f32 + x) * stride as f32 / rw as f32,
                    ((i / grid) as f32 + y) * stride as f32 / rh as f32,
                ];
            }
            let [left, top, right, bottom] = bounds;
            if landmarks
                .iter()
                .any(|[x, y]| !(*x >= left && *x <= right && *y >= top && *y <= bottom))
            {
                continue;
            }
            faces.push(PortraitFace {
                bounds,
                landmarks,
                confidence,
            });
        }
    }
    faces.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    let mut kept: Vec<PortraitFace> = Vec::new();
    for face in faces {
        if kept
            .iter()
            .all(|other| overlap(face.bounds, other.bounds) <= 0.3)
        {
            kept.push(face);
        }
        if kept.len() == 16 {
            break;
        }
    }
    // Stable spatial IDs for persisted editing operations.
    kept.sort_by(|a, b| {
        let [ax, ay, _, _] = a.bounds;
        let [bx, by, _, _] = b.bounds;
        ax.total_cmp(&bx).then(ay.total_cmp(&by))
    });
    Ok(kept)
}

/// Add faces too small for the whole-frame pass by running the detector on overlapping tiles.
///
/// The model sees a fixed 320-pixel square, so in a group photo or a full-length portrait a
/// face can shrink below its 20-pixel minimum. Tiles of a larger rendition (for example the
/// 2048-pixel proxy) give each face several times more pixels. Tiles are only run when the
/// frame does not already hold a large face, which keeps close-up portraits as fast as before.
/// `known` faces are kept; a tile face that overlaps one of them is dropped.
/// # Errors
/// Invalid pixels or a failed model run.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn detect_small_faces(
    rgb: &[u8],
    width: u32,
    height: u32,
    known: &[PortraitFace],
) -> AuraResult<Vec<PortraitFace>> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || w.checked_mul(h).and_then(|n| n.checked_mul(3)) != Some(rgb.len()) {
        return Err(invalid("Invalid portrait analysis pixels"));
    }
    let mut faces = known.to_vec();
    let largest = known
        .iter()
        .map(|f| (f.bounds[2] - f.bounds[0]) * (f.bounds[3] - f.bounds[1]))
        .fold(0.0_f32, f32::max);
    if largest > 0.06 || w.max(h) < 2 * SIDE {
        return Ok(faces);
    }
    let grid = if w.max(h) >= 1600 { 3 } else { 2 };
    let tile_w = ((w as f32 / grid as f32) * 1.35).ceil().min(w as f32) as usize;
    let tile_h = ((h as f32 / grid as f32) * 1.35).ceil().min(h as f32) as usize;
    for gy in 0..grid {
        for gx in 0..grid {
            let x0 = if grid == 1 {
                0
            } else {
                (w - tile_w) * gx / (grid - 1)
            };
            let y0 = if grid == 1 {
                0
            } else {
                (h - tile_h) * gy / (grid - 1)
            };
            let mut tile = Vec::with_capacity(tile_w * tile_h * 3);
            for y in y0..y0 + tile_h {
                let start = (y * w + x0) * 3;
                tile.extend_from_slice(rgb.get(start..start + tile_w * 3).unwrap_or(&[]));
            }
            if tile.len() != tile_w * tile_h * 3 {
                continue;
            }
            for mut face in detect_view(&tile, tile_w, tile_h, 0)? {
                let map = |[x, y]: [f32; 2]| {
                    [
                        (x0 as f32 + x * tile_w as f32) / w as f32,
                        (y0 as f32 + y * tile_h as f32) / h as f32,
                    ]
                };
                let [l, t, r, b] = face.bounds;
                let [l, t] = map([l, t]);
                let [r, b] = map([r, b]);
                face.bounds = [l, t, r, b];
                face.landmarks = face.landmarks.map(map);
                // A face cut by the tile edge is found whole by a neighbouring tile.
                let touches_edge = (face.bounds[0] * w as f32 - x0 as f32) < 2.0 && x0 > 0
                    || (face.bounds[1] * h as f32 - y0 as f32) < 2.0 && y0 > 0
                    || ((x0 + tile_w) as f32 - face.bounds[2] * w as f32) < 2.0 && x0 + tile_w < w
                    || ((y0 + tile_h) as f32 - face.bounds[3] * h as f32) < 2.0 && y0 + tile_h < h;
                if touches_edge {
                    continue;
                }
                if faces
                    .iter()
                    .any(|other| overlap(face.bounds, other.bounds) > 0.3)
                {
                    continue;
                }
                if faces.len() < 16 {
                    faces.push(face);
                }
            }
        }
    }
    faces.sort_by(|a, b| {
        let [ax, ay, _, _] = a.bounds;
        let [bx, by, _, _] = b.bounds;
        ax.total_cmp(&bx).then(ay.total_cmp(&by))
    });
    Ok(faces)
}

fn overlap([ax, ay, ar, ab]: [f32; 4], [bx, by, br, bb]: [f32; 4]) -> f32 {
    let intersection = (ar.min(br) - ax.max(bx)).max(0.0) * (ab.min(bb) - ay.max(by)).max(0.0);
    intersection / ((ar - ax) * (ab - ay) + (br - bx) * (bb - by) - intersection).max(1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_views_map_every_pixel_and_landmark_to_the_original() {
        for turns in 0..4 {
            let (w, h) = if turns % 2 == 1 { (3, 5) } else { (5, 3) };
            let mut visited = std::collections::BTreeSet::new();
            for y in 0..h {
                for x in 0..w {
                    let (sx, sy) = original_pixel(x, y, 5, 3, turns);
                    assert!(sx < 5 && sy < 3);
                    assert!(visited.insert((sx, sy)));
                    let point = original_point(
                        [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32],
                        turns,
                    );
                    assert!((point[0] - (sx as f32 + 0.5) / 5.0).abs() < 1e-6);
                    assert!((point[1] - (sy as f32 + 0.5) / 3.0).abs() < 1e-6);
                }
            }
            assert_eq!(visited.len(), 15);
        }
    }
    #[test]
    fn rejects_invalid_buffers_and_blank_frames_have_no_faces() {
        assert!(detect(&[0; 3], 2, 2).is_err());
        assert!(detect(&[], 0, 0).is_err());
        for value in [0, 128, 255] {
            assert!(detect(&vec![value; 64 * 96 * 3], 64, 96)
                .unwrap()
                .is_empty());
        }
    }
    #[test]
    fn tiled_search_keeps_known_faces_and_skips_close_ups() {
        let known = PortraitFace {
            bounds: [0.1, 0.1, 0.5, 0.6],
            landmarks: [[0.2, 0.3], [0.4, 0.3], [0.3, 0.4], [0.22, 0.5], [0.38, 0.5]],
            confidence: 0.9,
        };
        // A large known face: no tiles, nothing changes.
        let blank = vec![120_u8; 700 * 700 * 3];
        let faces = detect_small_faces(&blank, 700, 700, &[known.clone()]).unwrap();
        assert_eq!(faces.len(), 1);
        // No faces at all on a plain frame: tiles run and still find none.
        assert!(detect_small_faces(&blank, 700, 700, &[])
            .unwrap()
            .is_empty());
        assert!(detect_small_faces(&[0; 3], 2, 2, &[]).is_err());
    }
    #[test]
    fn nms_intersection_is_geometric() {
        assert!((overlap([0., 0., 1., 1.], [0., 0., 1., 1.]) - 1.).abs() < 1e-6);
        assert!(overlap([0., 0., 0.2, 0.2], [0.3, 0.3, 1., 1.]).abs() < 1e-6);
    }
}
