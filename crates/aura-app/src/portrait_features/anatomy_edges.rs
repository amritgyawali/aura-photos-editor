//! Preserve coherent nasal, expression and jaw contours without classifying dark spots as anatomy.
//!
//! Edges are measured on the original photograph in an eye-distance-normalized,
//! rotated grid. Hysteresis retains weak portions of supported contours. Tensor
//! coherence and minimum spatial extent reject pores and small closed blemishes.
#![allow(clippy::indexing_slicing)]
use super::{Geometry, Pixels};
use std::collections::VecDeque;

const STEP: f32 = 1.0 / 64.0;
const LEFT: f32 = -1.35;
const TOP: f32 = -0.60;
const WIDTH: usize = 174;
const WEAK: f32 = 0.035;
const STRONG: f32 = 0.085;

pub(super) struct Edges {
    origin: [f32; 2],
    u: [f32; 2],
    v: [f32; 2],
    scale: f32,
    height: usize,
    distance: Vec<f32>,
    pub(super) measured: bool,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Edges {
    /// Preserve an inward core and blend outside it; masks never gain new coverage.
    pub(super) fn weight(&self, point: [f32; 2], margin: f32, healing: bool) -> f32 {
        if !self.measured {
            return 1.0;
        }
        let delta = [point[0] - self.origin[0], point[1] - self.origin[1]];
        let x = ((delta[0] * self.u[0] + delta[1] * self.u[1]) / self.scale - LEFT) / STEP;
        let y = ((delta[0] * self.v[0] + delta[1] * self.v[1]) / self.scale - TOP) / STEP;
        if x < 0.0 || y < 0.0 || x >= (WIDTH - 1) as f32 || y >= (self.height - 1) as f32 {
            return 1.0;
        }
        let (xx, yy) = (x.floor() as usize, y.floor() as usize);
        // The minimum of the four surrounding distances is conservative under
        // bilinear mask upsampling. Half a cell diagonal also protects the line itself.
        let distance = [
            yy * WIDTH + xx,
            yy * WIDTH + xx + 1,
            (yy + 1) * WIDTH + xx,
            (yy + 1) * WIDTH + xx + 1,
        ]
        .into_iter()
        .map(|i| self.distance[i])
        .fold(f32::INFINITY, f32::min)
            * STEP;
        let core = if healing { 0.055 } else { 0.035 };
        smooth((distance - core - 0.5 * STEP * 2.0_f32.sqrt() - margin / self.scale) / 0.055)
    }
}

fn sample(px: &Pixels<'_>, [x, y]: [f32; 2]) -> Option<f32> {
    if x < 0.0 || y < 0.0 || x >= (px.width - 1) as f32 || y >= (px.height - 1) as f32 {
        return None;
    }
    let (xx, yy) = (x.floor() as usize, y.floor() as usize);
    let (fx, fy) = (x.fract(), y.fract());
    let at = |a, b| super::luma(px.linear(a, b));
    Some(
        (at(xx, yy) * (1.0 - fx) + at(xx + 1, yy) * fx) * (1.0 - fy)
            + (at(xx, yy + 1) * (1.0 - fx) + at(xx + 1, yy + 1) * fx) * fy,
    )
}

pub(super) fn measure(g: &Geometry, px: &Pixels<'_>) -> Edges {
    let delta = [g.mouth_centre[0] - g.nose[0], g.mouth_centre[1] - g.nose[1]];
    let mouth = [
        (delta[0] * g.u[0] + delta[1] * g.u[1]) / g.d,
        (delta[0] * g.v[0] + delta[1] * g.v[1]) / g.d,
    ];
    let corners = g.mouth.map(|point| {
        let delta = [point[0] - g.nose[0], point[1] - g.nose[1]];
        [
            (delta[0] * g.u[0] + delta[1] * g.u[1]) / g.d,
            (delta[0] * g.v[0] + delta[1] * g.v[1]) / g.d,
        ]
    });
    let lip_half_width = g.mouth_width / g.d * 0.5 + 0.12;
    let height = (((mouth[1] + 1.35 - TOP) / STEP).ceil() as usize).clamp(96, 240);
    let size = WIDTH * height;
    let mut light = vec![0.0; size];
    let mut valid = vec![false; size];
    let mut zones = vec![0_u8; size];
    for y in 0..height {
        for x in 0..WIDTH {
            let (u, v) = (LEFT + x as f32 * STEP, TOP + y as f32 * STEP);
            let i = y * WIDTH + x;
            let point = [
                g.nose[0] + g.d * (g.u[0] * u + g.v[0] * v),
                g.nose[1] + g.d * (g.u[1] * u + g.v[1] * v),
            ];
            if let Some(value) = sample(px, point) {
                light[i] = value;
                valid[i] = true;
            }
            // Search corridors locate the anatomy; only measured, sufficiently
            // long edges inside them become protected. The corridor itself is
            // not excluded, so small lesions on neighbouring skin remain editable.
            let smile_fold = corners.iter().any(|corner| {
                let side = if corner[0] < mouth[0] { -1.0 } else { 1.0 };
                super::to_segment(
                    [u, v],
                    [side * 0.28, 0.12],
                    [corner[0] + side * 0.08, corner[1]],
                ) <= 0.18
            });
            zones[i] = if (u - mouth[0]).abs() <= lip_half_width && (v - mouth[1]).abs() <= 0.24 {
                4
            } else if ((0.06..=0.62).contains(&u.abs()) && (-0.42..=0.40).contains(&v))
                || smile_fold
            {
                if u < 0.0 {
                    1
                } else {
                    2
                }
            } else if (u - mouth[0]).abs() <= 1.20 && (0.05..=1.15).contains(&(v - mouth[1])) {
                3
            } else {
                0
            };
        }
    }
    let light = aura_render::bands::blur(&light, WIDTH, height, 1);
    let mut gradients = vec![[0.0_f32; 2]; size];
    for y in 3..height - 3 {
        for x in 3..WIDTH - 3 {
            let i = y * WIDTH + x;
            if !(y - 3..=y + 3).all(|yy| (x - 3..=x + 3).all(|xx| valid[yy * WIDTH + xx])) {
                continue;
            }
            let log = |j: usize| light[j].max(1e-5).ln();
            gradients[i] = [
                (log(i + 2) - log(i - 2)) * 0.5,
                (log(i + 2 * WIDTH) - log(i - 2 * WIDTH)) * 0.5,
            ];
        }
    }
    let mut selected = vec![false; size];
    let mut strong = vec![false; size];
    for y in 5..height - 5 {
        for x in 5..WIDTH - 5 {
            let i = y * WIDTH + x;
            let gradient = gradients[i][0].hypot(gradients[i][1]);
            if zones[i] == 0 || gradient < WEAK {
                continue;
            }
            let (mut xx, mut xy, mut yy) = (0.0_f32, 0.0_f32, 0.0_f32);
            for row in y - 2..=y + 2 {
                for col in x - 2..=x + 2 {
                    let [a, b] = gradients[row * WIDTH + col];
                    xx += a * a;
                    xy += a * b;
                    yy += b * b;
                }
            }
            let coherence = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt() / (xx + yy).max(1e-8);
            selected[i] = coherence >= 0.55;
            strong[i] = gradient >= STRONG;
        }
    }
    let mut visited = vec![false; size];
    let mut distance = vec![1e6_f32; size];
    let mut measured = false;
    for start in 0..size {
        if !selected[start] || visited[start] {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        visited[start] = true;
        let mut group = Vec::new();
        let (mut x0, mut x1, mut y0, mut y1) = (WIDTH, 0, height, 0);
        let mut support = 0;
        while let Some(i) = queue.pop_front() {
            group.push(i);
            let (x, y) = (i % WIDTH, i / WIDTH);
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
            support += usize::from(strong[i]);
            for row in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for col in x.saturating_sub(1)..=(x + 1).min(WIDTH - 1) {
                    let j = row * WIDTH + col;
                    if selected[j] && !visited[j] && zones[j] == zones[start] {
                        visited[j] = true;
                        queue.push_back(j);
                    }
                }
            }
        }
        let extent = (x1 - x0).max(y1 - y0) as f32 * STEP;
        let minimum = match zones[start] {
            3 => 0.45,
            4 => 0.28,
            _ => 0.18,
        };
        if extent < minimum || support < 3 || support * 8 < group.len() {
            continue;
        }
        measured = true;
        for i in group {
            distance[i] = 0.0;
        }
    }
    // Two-pass chamfer distance: bounded work and a continuous feather around
    // supported edges, rather than isolated hard patches selected by brightness.
    for y in 0..height {
        for x in 0..WIDTH {
            let i = y * WIDTH + x;
            if x > 0 {
                distance[i] = distance[i].min(distance[i - 1] + 1.0);
            }
            if y > 0 {
                distance[i] = distance[i].min(distance[i - WIDTH] + 1.0);
                if x > 0 {
                    distance[i] = distance[i].min(distance[i - WIDTH - 1] + 2.0_f32.sqrt());
                }
                if x + 1 < WIDTH {
                    distance[i] = distance[i].min(distance[i - WIDTH + 1] + 2.0_f32.sqrt());
                }
            }
        }
    }
    for y in (0..height).rev() {
        for x in (0..WIDTH).rev() {
            let i = y * WIDTH + x;
            if x + 1 < WIDTH {
                distance[i] = distance[i].min(distance[i + 1] + 1.0);
            }
            if y + 1 < height {
                distance[i] = distance[i].min(distance[i + WIDTH] + 1.0);
                if x > 0 {
                    distance[i] = distance[i].min(distance[i + WIDTH - 1] + 2.0_f32.sqrt());
                }
                if x + 1 < WIDTH {
                    distance[i] = distance[i].min(distance[i + WIDTH + 1] + 2.0_f32.sqrt());
                }
            }
        }
    }
    Edges {
        origin: g.nose,
        u: g.u,
        v: g.v,
        scale: g.d,
        height,
        distance,
        measured,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use aura_vision::portrait::PortraitFace;

    fn face() -> PortraitFace {
        PortraitFace {
            bounds: [0.1, 0.08, 0.9, 0.88],
            landmarks: [
                [0.35, 0.3],
                [0.65, 0.3],
                [0.5, 0.45],
                [0.4, 0.6],
                [0.6, 0.6],
            ],
            confidence: 0.99,
        }
    }

    fn photograph(exposure: f32, contours: bool) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(256 * 256 * 3);
        for y in 0..256 {
            for x in 0..256 {
                let edge = 211.0 - 0.003 * (x as f32 - 128.0).powi(2);
                let mut gain = exposure * (0.95 + x as f32 / 2560.0);
                if contours {
                    if y as f32 >= edge {
                        gain *= 0.62;
                    }
                    let down = (y as f32 - 115.2) / 76.8;
                    let across = (x as f32 - 128.0) / 76.8;
                    if (-0.30..=0.30).contains(&down) {
                        let crease = 0.28 + 0.06 * (down * 5.0).cos();
                        let distance = (across.abs() - crease) / 0.014;
                        gain *= 1.0 - 0.50 * (-distance * distance * 0.5).exp();
                    }
                } else {
                    for (cx, cy) in [(104.0, 115.0), (128.0, 201.0)] {
                        if (x as f32 - cx).hypot(y as f32 - cy) < 1.8 {
                            gain *= 0.35;
                        }
                    }
                }
                rgb.extend(
                    [180.0, 125.0, 95.0].map(|c| (c * gain).round().clamp(0.0, 255.0) as u8),
                );
            }
        }
        rgb
    }

    fn expression(exposure: f32) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(256 * 256 * 3);
        for y in 0..256 {
            for x in 0..256 {
                let point = [x as f32, y as f32];
                let lip = ((point[0] - 128.0) / 27.0).hypot((point[1] - 153.6) / 7.5);
                let fold = [-1.0, 1.0]
                    .into_iter()
                    .map(|side| {
                        super::super::to_segment(
                            point,
                            [128.0 + side * 22.0, 124.0],
                            [128.0 + side * 31.0, 153.6],
                        )
                    })
                    .fold(f32::INFINITY, f32::min);
                let gain = exposure
                    * if lip < 1.0 { 0.52 } else { 1.0 }
                    * (1.0 - 0.5 * (-fold * fold / 2.0).exp());
                rgb.extend([180.0, 125.0, 95.0].map(|v| (v * gain).round() as u8));
            }
        }
        rgb
    }

    #[test]
    fn expression_contours_are_preserved_without_excluding_nearby_skin() {
        for exposure in [0.35, 0.65, 1.0] {
            let rgb = expression(exposure);
            let px = Pixels::new(&rgb, 256, 256).unwrap();
            let g = Geometry::new(&face(), &px).unwrap();
            let edges = measure(&g, &px);
            for point in [
                [104.0, 132.0],
                [152.0, 132.0],
                [128.0, 146.0],
                [128.0, 161.0],
            ] {
                assert!(
                    edges.weight(point, 0.0, true) < 0.01,
                    "expression missed at {point:?}"
                );
            }
            assert_eq!(
                edges.weight([170.0, 130.0], 0.0, true),
                1.0,
                "healthy adjacent skin excluded"
            );
        }
    }

    #[test]
    fn face_and_body_tools_keep_expression_exclusions_at_export_resolution() {
        use aura_recipe::retouch_tools::Tool;
        use std::collections::BTreeMap;
        let rgb = expression(1.0);
        let px = Pixels::new(&rgb, 256, 256).unwrap();
        let settings = super::super::Settings {
            protect_eye_area: false,
            protect_nose_detail: false,
            protect_ceremonial_marks: false,
            ..Default::default()
        };
        for tool in [Tool::FrequencyHeal, Tool::SkinUniformity] {
            for body in [false, true] {
                let mut edits = [super::super::base_edit(
                    "automatic".into(),
                    tool,
                    1.0,
                    &px,
                    [128.0, 128.0, 256.0, 256.0],
                )];
                let mut mattes = BTreeMap::new();
                if body {
                    super::super::eye_guard::protect_body_jaw(
                        &face(),
                        &px,
                        &mut edits,
                        &mut mattes,
                    )
                    .unwrap();
                } else {
                    super::super::eye_guard::protect(
                        &face(),
                        &px,
                        edits.iter_mut(),
                        &mut mattes,
                        "expression",
                        &settings,
                    )
                    .unwrap();
                }
                for side in [256_usize, 1024] {
                    let image = [0.3_f32, 0.2, 0.15].repeat(side * side);
                    let mask = aura_render::retouch_tools::selection_mask_with_mattes(
                        &image, side, side, &edits[0], &mattes,
                    );
                    for (x, y) in [(104, 132), (152, 132), (128, 146), (128, 161)] {
                        assert_eq!(
                            mask[y * side / 256 * side + x * side / 256],
                            0.0,
                            "expression exclusion lost: {tool:?}, body={body}, side={side}"
                        );
                    }
                    // Stay outside the existing conservative nostril-wing
                    // fallback as well as the new expression exclusions.
                    let cheek = mask[130 * side / 256 * side + 190 * side / 256];
                    assert!(
                        cheek > 0.9,
                        "cheek coverage lost: {tool:?}, body={body}, side={side}, alpha={cheek}"
                    );
                }
            }
        }
    }

    #[test]
    fn nasal_and_jaw_contours_are_protected_at_different_exposures() {
        for exposure in [0.35, 0.65, 1.0] {
            let rgb = photograph(exposure, true);
            let px = Pixels::new(&rgb, 256, 256).unwrap();
            let g = Geometry::new(&face(), &px).unwrap();
            let edges = measure(&g, &px);
            assert!(edges.measured);
            for y in [100.0_f32, 112.0, 125.0] {
                let down = (y - 115.2) / 76.8;
                let across = 76.8 * (0.28 + 0.06 * (down * 5.0).cos());
                for side in [-1.0, 1.0] {
                    assert!(
                        edges.weight([128.0 + side * across, y], 0.0, true) < 0.01,
                        "nose missed at {exposure}, {side}, {y}"
                    );
                }
            }
            for x in [82.0_f32, 110.0, 128.0, 151.0, 174.0] {
                let y = 211.0 - 0.003 * (x - 128.0).powi(2);
                assert!(
                    edges.weight([x, y], 0.0, false) < 0.01,
                    "chin missed at {exposure}, {x}"
                );
            }
            assert_eq!(
                edges.weight([128.0, 115.0], 0.0, true),
                1.0,
                "nose skin excluded"
            );
            assert_eq!(
                edges.weight([128.0, 180.0], 0.0, true),
                1.0,
                "chin skin excluded"
            );
        }
    }

    #[test]
    fn isolated_compact_marks_are_not_mistaken_for_anatomy() {
        let rgb = photograph(1.0, false);
        let px = Pixels::new(&rgb, 256, 256).unwrap();
        let g = Geometry::new(&face(), &px).unwrap();
        let edges = measure(&g, &px);
        assert!(
            !edges.measured,
            "small blemishes established an anatomical contour"
        );
        for point in [[104.0, 115.0], [128.0, 201.0]] {
            assert_eq!(edges.weight(point, 0.0, true), 1.0);
        }
    }

    #[test]
    fn protection_rotates_with_the_landmarks() {
        let original = photograph(1.0, true);
        let mut rotated = vec![0; original.len()];
        for y in 0..256 {
            for x in 0..256 {
                let from = (y * 256 + x) * 3;
                let to = (x * 256 + 255 - y) * 3;
                rotated[to..to + 3].copy_from_slice(&original[from..from + 3]);
            }
        }
        let mut turned = face();
        turned.landmarks = turned.landmarks.map(|[x, y]| [1.0 - y, x]);
        let [l, t, r, b] = turned.bounds;
        turned.bounds = [1.0 - b, l, 1.0 - t, r];
        let px = Pixels::new(&rotated, 256, 256).unwrap();
        let g = Geometry::new(&turned, &px).unwrap();
        let edges = measure(&g, &px);
        assert!(edges.measured);
        for point in [[128.0, 211.0], [102.0, 115.0], [154.0, 115.0]] {
            assert!(edges.weight([255.0 - point[1], point[0]], 1.0, true) < 0.01);
        }
    }
}
