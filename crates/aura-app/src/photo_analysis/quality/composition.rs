use super::{avg, bars, Diagnostic};

fn centroid(weights: &[f32], w: usize, h: usize) -> (Vec<f32>, usize) {
    let mut mass = 0.0_f64;
    let mut position = [0.0_f64; 2];
    let mut count = 0;
    for (i, value) in weights.iter().enumerate() {
        let weight = f64::from(value.max(0.0));
        mass += weight;
        position[0] += weight * ((i % w) as f64 + 0.5) / w as f64;
        position[1] += weight * ((i / w) as f64 + 0.5) / h as f64;
        if weight > 0.0 {
            count += 1;
        }
    }
    (
        if mass > 1e-12 {
            position.map(|v| (v / mass) as f32).to_vec()
        } else {
            vec![0.5; 2]
        },
        count,
    )
}

fn bright_regions(luma: &[f32], w: usize, h: usize) -> (Vec<f32>, usize) {
    let mut sorted = luma.to_vec();
    sorted.sort_by(f32::total_cmp);
    let cutoff = sorted[(sorted.len() - 1) * 9 / 10];
    // Strict positivity prevents a black frame becoming a spurious bright region.
    let mut available: Vec<_> = luma.iter().map(|v| *v >= cutoff && *v > 0.0).collect();
    let selected = available.iter().filter(|v| **v).count();
    let mut components = 0;
    let mut largest = 0;
    let mut stack = Vec::new();
    for start in 0..luma.len() {
        if !available[start] {
            continue;
        }
        components += 1;
        available[start] = false;
        stack.push(start);
        let mut size = 0;
        while let Some(i) = stack.pop() {
            size += 1;
            let x = i % w;
            let y = i / w;
            for next in [
                if x > 0 { Some(i - 1) } else { None },
                if x + 1 < w { Some(i + 1) } else { None },
                if y > 0 { Some(i - w) } else { None },
                if y + 1 < h { Some(i + w) } else { None },
            ]
            .into_iter()
            .flatten()
            {
                if available[next] {
                    available[next] = false;
                    stack.push(next);
                }
            }
        }
        largest = largest.max(size);
    }
    (
        vec![
            components as f32,
            largest as f32 / luma.len() as f32 * 100.0,
            if selected == 0 {
                0.0
            } else {
                largest as f32 / selected as f32 * 100.0
            },
        ],
        selected,
    )
}

pub(super) fn measure(luma: &[f32], w: usize, h: usize) -> Vec<Diagnostic> {
    let n = luma.len();
    let mut edge = vec![0.0; n];
    let mut quadrant = [0.0_f64; 4];
    let mut quadrant_n = [0; 4];
    let mut border = [0.0_f64; 5];
    let mut border_n = [0; 5];
    let mut radial = [0.0_f64; 8];
    let mut radial_n = [0; 8];
    let mut thirds = [0.0_f64; 4];
    let mut symmetry = [0.0_f64; 2];
    let mut total_edge = 0.0_f64;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let px = (x as f32 + 0.5) / w as f32;
            let py = (y as f32 + 0.5) / h as f32;
            let dx = luma[y * w + (x + 1).min(w - 1)] - luma[y * w + x.saturating_sub(1)];
            let dy = luma[(y + 1).min(h - 1) * w + x] - luma[y.saturating_sub(1) * w + x];
            edge[i] = dx.hypot(dy) * 0.5;
            let energy = f64::from(edge[i]);
            total_edge += energy;
            let q = usize::from(py >= 0.5) * 2 + usize::from(px >= 0.5);
            quadrant[q] += f64::from(luma[i]);
            quadrant_n[q] += 1;
            let region = if px < 0.1 {
                0
            } else if px > 0.9 {
                1
            } else if py < 0.1 {
                2
            } else if py > 0.9 {
                3
            } else {
                4
            };
            border[region] += f64::from(luma[i].max(0.0)).powi(2);
            border_n[region] += 1;
            let radius = ((px - 0.5).hypot(py - 0.5) * std::f32::consts::SQRT_2).min(0.999_999);
            let ring = (radius * 8.0) as usize;
            radial[ring] += f64::from(luma[i]);
            radial_n[ring] += 1;
            for (j, (position, line)) in [
                (px, 1.0 / 3.0),
                (px, 2.0 / 3.0),
                (py, 1.0 / 3.0),
                (py, 2.0 / 3.0),
            ]
            .into_iter()
            .enumerate()
            {
                if (position - line).abs() <= 1.0 / 24.0 {
                    thirds[j] += energy;
                }
            }
            symmetry[0] += f64::from((luma[i] - luma[y * w + (w - 1 - x)]).abs());
            symmetry[1] += f64::from((luma[i] - luma[(h - 1 - y) * w + x]).abs());
        }
    }
    let (light_position, light_n) = centroid(luma, w, h);
    let (detail_position, detail_n) = centroid(&edge, w, h);
    let (regions, region_n) = bright_regions(luma, w, h);
    vec![
        bars("light-centroid", "Centre of light", "Positive-luminance centre of mass in normalized image coordinates. A black frame has no samples and reports the geometric centre.", "frame", light_position, &["Horizontal","Vertical"], light_n),
        bars("detail-centroid", "Centre of detail", "Gradient-magnitude centre of mass in normalized image coordinates; this is not face or subject recognition.", "frame", detail_position, &["Horizontal","Vertical"], detail_n),
        bars("quadrant-light", "Quadrant light balance", "Mean signed linear luminance in each quadrant, using pixel-centre membership and actual regional sample counts.", "signal", (0..4).map(|i|avg(quadrant[i],quadrant_n[i])).collect(), &["Top left","Top right","Bottom left","Bottom right"], n),
        bars("border-light-energy", "Border light energy", "Root-mean-square positive luminance in disjoint 10% border bands and the interior. Corner pixels belong to left or right first.", "signal", (0..5).map(|i|avg(border[i],border_n[i]).max(0.0).sqrt()).collect(), &["Left","Right","Top","Bottom","Interior"], n),
        bars("thirds-edge-mass", "Thirds line detail mass", "Fraction of all gradient magnitude inside narrow bands around each thirds line. Bands can overlap; alignment is descriptive, not a composition score.", "%", thirds.map(|v|if total_edge>1e-12 {(v/total_edge*100.0) as f32} else {0.0}).to_vec(), &["Left third","Right third","Top third","Bottom third"], detail_n),
        bars("mirror-residual", "Mirror asymmetry", "Mean absolute luminance differences against horizontal and vertical reflections. Scene content determines whether symmetry is desirable.", "signal", symmetry.map(|v|avg(v,n)).to_vec(), &["Left/right","Top/bottom"], n),
        bars("bright-components", "Bright region connectivity", "Four-connected positive pixels at or above the 90th luminance percentile; ties are included. Reports region count, largest region area and its share of selected pixels.", "mixed: count / %", regions, &["Regions","Largest area %","Largest share %"], region_n),
        bars("radial-light-profile", "Radial light profile", "Mean linear luminance in eight normalized concentric rings from centre to corners. Reflectance and composition affect this; it is not lens calibration.", "signal", (0..8).map(|i|avg(radial[i],radial_n[i])).collect(), &["Centre","Ring 2","Ring 3","Ring 4","Ring 5","Ring 6","Ring 7","Corners"], n),
    ]
}
