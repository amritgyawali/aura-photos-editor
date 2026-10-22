use super::{avg, bars, map, quantile, Diagnostic};

pub(super) fn measure(p: &[[f32; 3]], y: &[f32], w: usize, h: usize) -> Vec<Diagnostic> {
    let n = y.len();
    let mut sorted = y.to_vec();
    sorted.sort_by(f32::total_cmp);
    let median = quantile(&sorted, 0.5);
    let mut deviations: Vec<_> = y.iter().map(|v| (*v - median).abs()).collect();
    deviations.sort_by(f32::total_cmp);
    let positive: Vec<_> = sorted.iter().map(|v| f64::from(v.max(0.0))).collect();
    let sum = positive.iter().sum::<f64>();
    let gini = if sum > 1e-20 {
        (positive
            .iter()
            .enumerate()
            .map(|(i, v)| (2.0 * i as f64 + 1.0 - n as f64) * v)
            .sum::<f64>()
            / (n as f64 * sum))
            .clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let mut hue = [0.0_f64; 3];
    let mut tonal = [[0.0_f64; 3]; 8];
    let mut chroma = Vec::with_capacity(n);
    let mut excursions = [0.0_f64; 3];
    let mut headroom = [
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    ];
    let mut transition = vec![0.0; 64];
    let mut counts = [0_usize; 64];
    for (i, pixel) in p.iter().enumerate() {
        let c = [pixel[0] - pixel[1], pixel[2] - pixel[1]];
        let radius = c[0].hypot(c[1]);
        chroma.push(radius);
        let angle = c[1].atan2(c[0]);
        let t = (y[i].max(0.0).sqrt().min(1.0) * 7.0) as usize;
        for v in [&mut hue, &mut tonal[t]] {
            v[0] += f64::from(radius * angle.cos());
            v[1] += f64::from(radius * angle.sin());
            v[2] += f64::from(radius);
        }
        for channel in 0..3 {
            let value = pixel[channel];
            excursions[channel] +=
                f64::from((-value).max(0.0).powi(2) + (value - 1.0).max(0.0).powi(2));
            headroom[channel].push(value);
        }
        let x = i % w;
        let row = i / w;
        let cell = row * 8 / h * 8 + x * 8 / w;
        for j in [
            if x + 1 < w { Some(i + 1) } else { None },
            if row + 1 < h { Some(i + w) } else { None },
        ]
        .into_iter()
        .flatten()
        {
            let d = (pixel[0] - pixel[1] - (p[j][0] - p[j][1]))
                .hypot(pixel[2] - pixel[1] - (p[j][2] - p[j][1]));
            transition[cell] += d;
            counts[cell] += 1;
        }
    }
    chroma.sort_by(f32::total_cmp);
    for values in &mut headroom {
        values.sort_by(f32::total_cmp);
    }
    for i in 0..64 {
        if counts[i] > 0 {
            transition[i] /= counts[i] as f32;
        }
    }
    let hue_angle = hue[1].atan2(hue[0]);
    let hue_drift = tonal.map(|t| {
        if t[2] > 1e-10 && hue[2] > 1e-10 {
            let d = t[1].atan2(t[0]) - hue_angle;
            (d.sin().atan2(d.cos()).to_degrees()) as f32
        } else {
            0.0
        }
    });
    let positive_n = y.iter().filter(|v| **v > 0.0).count();
    let geometric = if positive_n > 0 {
        (y.iter()
            .filter(|v| **v > 0.0)
            .map(|v| f64::from(*v).ln())
            .sum::<f64>()
            / positive_n as f64)
            .exp() as f32
    } else {
        0.0
    };
    let reciprocal = y
        .iter()
        .filter(|v| **v > 0.0)
        .map(|v| 1.0 / f64::from(*v))
        .sum::<f64>();
    let harmonic = if reciprocal > 0.0 {
        (positive_n as f64 / reciprocal) as f32
    } else {
        0.0
    };
    let quantiles = [0.0, 0.01, 0.05, 0.25, 0.5, 0.75, 0.95, 0.99, 1.0]
        .map(|t| quantile(&sorted, t))
        .to_vec();
    let rolloff = [0.5, 0.9, 0.95, 0.99]
        .into_iter()
        .zip([0.9, 0.95, 0.99, 1.0])
        .map(|(a, b)| (quantile(&sorted, b) - quantile(&sorted, a)) / (b - a))
        .collect();
    let variogram = [1, 2, 4, 8]
        .into_iter()
        .map(|lag| {
            let mut energy = 0.0_f64;
            let mut count = 0;
            for row in 0..h {
                for x in 0..w.saturating_sub(lag) {
                    let a = p[row * w + x];
                    let b = p[row * w + x + lag];
                    energy += f64::from(
                        (a[0] - a[1] - (b[0] - b[1])).powi(2)
                            + (a[2] - a[1] - (b[2] - b[1])).powi(2),
                    ) * 0.5;
                    count += 1;
                }
            }
            avg(energy, count)
        })
        .collect();
    vec![
        bars(("robust-tone-quantiles", "Luminance quantile landmarks", "Interpolated observed linear luminance landmarks; no assumed target exposure."), "signal", quantiles, &["Min","1%","5%","25%","Median","75%","95%","99%","Max"], n),
        bars(("median-deviation", "Robust luminance dispersion", "Median absolute departure from median luminance, interquartile range, and central 90% span."), "signal", vec![quantile(&deviations,0.5),quantile(&sorted,0.75)-quantile(&sorted,0.25),quantile(&sorted,0.95)-quantile(&sorted,0.05)], &["MAD","IQR","90% span"], n),
        bars(("luminance-inequality", "Light energy inequality", "Gini coefficient of nonnegative photographed luminance; zero for uniform light. Scene composition changes this value."), "ratio", vec![gini], &["Gini"], n),
        bars(("hue-concentration", "Circular hue concentration", "Chroma-weighted circular resultant length and mean opponent angle. Neutral pixels contribute no weight."), "mixed: ratio / degrees", vec![if hue[2]>1e-10 {(hue[0].hypot(hue[1])/hue[2]) as f32} else {0.0},if hue[2]>1e-10 {hue_angle.to_degrees() as f32} else {0.0}], &["Concentration","Mean angle"], p.iter().filter(|v| (v[0]-v[1]).hypot(v[2]-v[1])>1e-10).count()),
        bars(("tone-hue-drift", "Hue drift through light levels", "Circular hue difference from the photo-wide mean in eight square-root luminance bins. Empty or neutral bins report zero."), "degrees", hue_drift.to_vec(), &[], n),
        bars(("highlight-headroom", "Channel highlight headroom", "One minus each channel's observed 99th percentile. Negative values indicate working signal above nominal white, not necessarily export clipping."), "signal", headroom.iter().map(|v| 1.0-quantile(v,0.99)).collect(), &["Red","Green","Blue"], n),
        bars(("chroma-tail", "Absolute chroma tails", "Opponent channel-difference radius at median, 90th, 99th and maximum quantiles."), "signal", [0.5,0.9,0.99,1.0].map(|t|quantile(&chroma,t)).to_vec(), &["Median","90%","99%","Max"], n),
        bars(("exposure-locations", "Light averaging models", "Arithmetic mean of nonnegative luminance, geometric mean and harmonic mean of strictly positive luminance. These describe distribution, not correct exposure."), "signal", vec![avg(sum,n),geometric,harmonic], &["Arithmetic","Geometric","Harmonic"], n),
        map(("local-colour-jumps", "Local colour transitions", "Mean adjacent opponent-colour distance in 8 by 8 regions. Brightness and photographed colour boundaries both affect it."), "signal", transition, n),
        bars(("highlight-rolloff", "Upper quantile spacing", "Slope of observed luminance versus population rank across upper quantile intervals. Describes highlight concentration, not sensor response."), "signal / rank", rolloff, &["50-90%","90-95%","95-99%","99-100%"], n),
        bars(("gamut-excursion-energy", "Working range excursion energy", "Root mean squared channel distance outside the nominal zero-to-one working cube; this is not an sRGB gamut test."), "signal", excursions.map(|v|avg(v,n).sqrt()).to_vec(), &["Red","Green","Blue"], n),
        bars(("colour-variogram", "Colour spatial variogram", "Half mean squared opponent-colour difference at four horizontal preview lags; distinguishes spatial colour structure from global palette spread."), "signal squared", variogram, &["1 px","2 px","4 px","8 px"], n),
    ]
}
