//! Does a per-photograph model of a retoucher's settings beat one constant profile? ADR-0104.
//!
//! Leave-one-out over MIT-Adobe FiveK pairs (`AURA_FIVEK_DIR`, `AURA_FIVEK_EXPERT`): the
//! retoucher's look is fitted per pair through the real renderer (cached beside the data), then
//! each held-out photograph is predicted from the others by one constant offset, by nearest
//! neighbours, and by ridge regression on the features `personal_style` uses. Ignored: it needs
//! the dataset and takes minutes. The numbers it printed are in ADR-0104.
#![allow(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout,
    clippy::expect_used,
    clippy::needless_range_loop,
    dead_code
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_app::edit_profiles::{self, AutoCorrection, SceneStats};
use aura_raw::codec::decode_png;
use aura_raw::colour::curve::linear_u16_to_scene;
use aura_raw::demosaic::{self, RgbF32};
use aura_raw::{DecodeLimits, PixelData};
use aura_recipe::Recipe;
use aura_render::{CpuEngine, Frame, OutputSpec, RenderLevel, RenderPurpose, RenderedData};
use aura_style::fit::{
    self,
    optimise::{Axis, Candidate},
    Budget,
};

const EDGE: u32 = 384;

struct Pair {
    name: String,
    frame: Frame,
    target: Vec<u8>,
    auto: AutoCorrection,
    stats: SceneStats,
    cast: [f32; 2],
}

fn load(dng: &Path, png: &Path, clock: &dyn aura_core::clock::Clock) -> Result<Pair, String> {
    let bytes = std::fs::read(dng).map_err(|e| e.to_string())?;
    let meta = aura_raw::read_meta(&bytes, dng).map_err(|e| e.detail)?;
    let proxy = aura_raw::proxy::tier2(&bytes, &meta, DecodeLimits::default(), clock, None, dng)
        .map_err(|e| e.detail)?;
    let linear = &proxy.pair.linear;
    let PixelData::Linear16(codes) = &linear.data else {
        return Err("no linear".into());
    };
    let working = RgbF32 {
        width: linear.width,
        height: linear.height,
        data: codes.iter().map(|c| linear_u16_to_scene(*c)).collect(),
    };
    let (w, h) = demosaic::fit_long_edge(working.width, working.height, EDGE);
    let working = demosaic::resize(&working, w, h);
    let final_png = decode_png(
        &std::fs::read(png).map_err(|e| e.to_string())?,
        DecodeLimits::default(),
    )
    .map_err(|e| e.detail)?;
    let (a, b) = (
        f64::from(w) / f64::from(h),
        f64::from(final_png.width) / f64::from(final_png.height),
    );
    if (a - b).abs() / a > 0.01 {
        return Err("cropped".into());
    }
    let target = demosaic::to_srgb8(&demosaic::resize(
        &demosaic::from_srgb8(&final_png.data, final_png.width, final_png.height),
        w,
        h,
    ));
    let preview = proxy.pair.srgb.as_srgb8().ok_or("no preview")?;
    let (mut r, mut g, mut bl) = (0.0f64, 0.0f64, 0.0f64);
    for p in working.data.chunks_exact(3) {
        r += f64::from(p[0]);
        g += f64::from(p[1]);
        bl += f64::from(p[2]);
    }
    Ok(Pair {
        name: png.file_stem().unwrap().to_string_lossy().into_owned(),
        frame: Frame::working(working.data, w, h, ""),
        target,
        auto: AutoCorrection::measure(preview).map_err(|e| e.detail)?,
        stats: SceneStats::measure(preview).map_err(|e| e.detail)?,
        cast: [
            ((r / g.max(1e-9)).ln()) as f32,
            ((bl / g.max(1e-9)).ln()) as f32,
        ],
    })
}

fn render(engine: &CpuEngine, frame: &Frame, recipe: &Recipe) -> Vec<u8> {
    match engine
        .render_frame(
            frame,
            recipe,
            RenderLevel::Screen(frame.width, frame.height),
            RenderPurpose::Analysis,
            &OutputSpec::default(),
        )
        .unwrap()
        .data
    {
        RenderedData::Eight(b) => b,
        RenderedData::Sixteen(_) => panic!(),
    }
}

fn features(p: &Pair) -> Vec<f32> {
    vec![
        p.stats.low,
        p.stats.median,
        p.stats.high,
        p.stats.warmth,
        p.stats.chroma_p90,
        p.auto.exposure,
        f32::from(p.auto.highlights),
        f32::from(p.auto.shadows),
        f32::from(p.auto.contrast),
        p.cast[0],
        p.cast[1],
    ]
}

/// The part of each axis the automatic correction already decides.
fn base(axis: Axis, auto: &AutoCorrection) -> f32 {
    match axis {
        Axis::Exposure => auto.exposure,
        Axis::Highlights => f32::from(auto.highlights),
        Axis::Shadows => f32::from(auto.shadows),
        Axis::Contrast => f32::from(auto.contrast),
        Axis::Temperature => 5500.0,
        _ => 0.0,
    }
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(f32::total_cmp);
    if v.is_empty() {
        0.0
    } else if v.len() % 2 == 1 {
        v[v.len() / 2]
    } else {
        (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0
    }
}

/// Ridge regression with an intercept on standardised features.
fn ridge(x: &[Vec<f32>], y: &[f32], lambda: f64) -> (Vec<f64>, f64, Vec<(f64, f64)>) {
    let d = x[0].len();
    let n = x.len() as f64;
    let norm: Vec<(f64, f64)> = (0..d)
        .map(|j| {
            let m = x.iter().map(|r| f64::from(r[j])).sum::<f64>() / n;
            let s = (x.iter().map(|r| (f64::from(r[j]) - m).powi(2)).sum::<f64>() / n)
                .sqrt()
                .max(1e-6);
            (m, s)
        })
        .collect();
    let z: Vec<Vec<f64>> = x
        .iter()
        .map(|r| {
            (0..d)
                .map(|j| (f64::from(r[j]) - norm[j].0) / norm[j].1)
                .collect()
        })
        .collect();
    let ym = y.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    // (Z'Z + lambda I) beta = Z'(y - ym)
    let mut a = vec![vec![0.0; d + 1]; d];
    for i in 0..d {
        for j in 0..d {
            a[i][j] =
                z.iter().map(|r| r[i] * r[j]).sum::<f64>() + if i == j { lambda } else { 0.0 };
        }
        a[i][d] = z
            .iter()
            .zip(y)
            .map(|(r, v)| r[i] * (f64::from(*v) - ym))
            .sum::<f64>();
    }
    for c in 0..d {
        let p = (c..d)
            .max_by(|i, j| a[*i][c].abs().total_cmp(&a[*j][c].abs()))
            .unwrap();
        a.swap(c, p);
        let piv = a[c][c];
        for k in c..=d {
            a[c][k] /= piv;
        }
        for r in 0..d {
            if r != c {
                let f = a[r][c];
                for k in c..=d {
                    a[r][k] -= f * a[c][k];
                }
            }
        }
    }
    ((0..d).map(|i| a[i][d]).collect(), ym, norm)
}

fn ridge_predict(model: &(Vec<f64>, f64, Vec<(f64, f64)>), f: &[f32]) -> f32 {
    let (beta, ym, norm) = model;
    (ym + beta
        .iter()
        .zip(norm)
        .zip(f)
        .map(|((b, (m, s)), v)| b * ((f64::from(*v) - m) / s))
        .sum::<f64>()) as f32
}

#[test]
#[ignore = "experiment"]
fn adaptive_versus_constant() {
    let root = PathBuf::from(std::env::var("AURA_FIVEK_DIR").unwrap());
    let expert = std::env::var("AURA_FIVEK_EXPERT").unwrap_or_else(|_| "c".into());
    let cache = root.join(format!("fits_{expert}.json"));
    let mut cached: serde_json::Map<String, serde_json::Value> = std::fs::read(&cache)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let clock = aura_core::clock::SystemClock::default();
    let engine = CpuEngine::new(
        aura_render::fixtures::StaticSource::shared(aura_render::fixtures::grey_frame(2, 2, 0.18)),
        Arc::new(aura_core::clock::SystemClock::default()),
    );
    let neutral = aura_recipe::fixtures::neutral(&"0".repeat(64), "");
    let mut finals: Vec<PathBuf> = std::fs::read_dir(root.join(format!("expert_{expert}")))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    finals.sort();
    let mut data = Vec::new();
    for png in &finals {
        let stem = png.file_stem().unwrap().to_string_lossy().into_owned();
        let Ok(pair) = load(&root.join("dng").join(format!("{stem}.dng")), png, &clock) else {
            continue;
        };
        let (cand, residual, local) = if let Some(v) = cached.get(&stem) {
            let mut c = Candidate::neutral();
            for (i, a) in Axis::ALL.iter().enumerate() {
                c.set(*a, v["values"][i].as_f64().unwrap() as f32);
            }
            (
                c,
                v["residual"].as_f64().unwrap() as f32,
                v["local"].as_bool().unwrap(),
            )
        } else {
            let r = fit::fit(
                &engine,
                &pair.frame,
                &neutral,
                &pair.target,
                Budget::default(),
            )
            .unwrap();
            cached.insert(stem.clone(), serde_json::json!({"values": r.candidate.values, "residual": r.residual_de00, "local": r.localised()}));
            std::fs::write(&cache, serde_json::to_vec(&cached).unwrap()).unwrap();
            (r.candidate, r.residual_de00, r.localised())
        };
        println!("{stem}: fit {residual:.2}");
        data.push((pair, cand, residual, local));
    }
    let n = data.len();
    let usable = |i: usize| data[i].2 <= 5.0 && !data[i].3;
    let render_de = |pair: &Pair, cand: Candidate| {
        let recipe = cand.to_params().into_recipe(&neutral);
        fit::grid_de00(
            &render(&engine, &pair.frame, &recipe),
            &pair.target,
            pair.frame.width as usize,
            pair.frame.height as usize,
        )
        .0
    };
    let lambdas = [1.0, 4.0, 16.0, 64.0];
    let mut sums = vec![0.0f32; 4 + lambdas.len()];
    for i in 0..n {
        let train: Vec<usize> = (0..n).filter(|j| *j != i && usable(*j)).collect();
        let p = &data[i].0;
        let (auto_recipe, _, _) =
            edit_profiles::build(&neutral, None, 1.0, p.auto, p.stats).unwrap();
        let auto_de = fit::grid_de00(
            &render(&engine, &p.frame, &auto_recipe),
            &p.target,
            p.frame.width as usize,
            p.frame.height as usize,
        )
        .0;
        // Constant: median residual over the training set.
        let mut constant = Candidate::neutral();
        for a in Axis::ALL {
            let m = median(
                train
                    .iter()
                    .map(|j| data[*j].1.get(a) - base(a, &data[*j].0.auto))
                    .collect(),
            );
            constant.set(a, base(a, &p.auto) + m);
        }
        // k nearest neighbours in standardised feature space.
        let fx: Vec<Vec<f32>> = train.iter().map(|j| features(&data[*j].0)).collect();
        let d = fx[0].len();
        let stdv: Vec<f32> = (0..d)
            .map(|k| {
                let m = fx.iter().map(|r| r[k]).sum::<f32>() / fx.len() as f32;
                (fx.iter().map(|r| (r[k] - m).powi(2)).sum::<f32>() / fx.len() as f32)
                    .sqrt()
                    .max(1e-6)
            })
            .collect();
        let me = features(p);
        let mut dist: Vec<(f32, usize)> = train
            .iter()
            .zip(&fx)
            .map(|(j, r)| {
                (
                    (0..d)
                        .map(|k| ((r[k] - me[k]) / stdv[k]).powi(2))
                        .sum::<f32>()
                        .sqrt(),
                    *j,
                )
            })
            .collect();
        dist.sort_by(|a, b| a.0.total_cmp(&b.0));
        let k = 7.min(dist.len());
        let mut knn = Candidate::neutral();
        for a in Axis::ALL {
            let (mut s, mut wsum) = (0.0, 0.0);
            for (dd, j) in &dist[..k] {
                let w = 1.0 / (0.5 + dd);
                s += w * (data[*j].1.get(a) - base(a, &data[*j].0.auto));
                wsum += w;
            }
            knn.set(a, base(a, &p.auto) + s / wsum);
        }
        sums[0] += auto_de;
        sums[1] += render_de(p, constant);
        sums[2] += render_de(p, knn);
        sums[3] += data[i].2;
        for (li, lambda) in lambdas.iter().enumerate() {
            let mut rid = Candidate::neutral();
            for a in Axis::ALL {
                let y: Vec<f32> = train
                    .iter()
                    .map(|j| data[*j].1.get(a) - base(a, &data[*j].0.auto))
                    .collect();
                let model = ridge(&fx, &y, *lambda * train.len() as f64 / 10.0);
                rid.set(a, base(a, &p.auto) + ridge_predict(&model, &me));
            }
            sums[4 + li] += render_de(p, rid);
        }
    }
    let nf = n as f32;
    println!(
        "pairs {n} (usable {}): auto {:.2}  constant {:.2}  knn7 {:.2}  oracle {:.2}",
        (0..n).filter(|i| usable(*i)).count(),
        sums[0] / nf,
        sums[1] / nf,
        sums[2] / nf,
        sums[3] / nf
    );
    for (li, lambda) in lambdas.iter().enumerate() {
        println!("  ridge lambda {lambda}: {:.2}", sums[4 + li] / nf);
    }
}
