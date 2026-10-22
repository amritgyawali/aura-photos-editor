//! Small analytical primitives used by the advanced colour operators.
use super::super::{dot, luma, with_luma, EPS};
use aura_raw::colour::{
    matrix::{apply, REC2020_TO_XYZ_D65, WHITE_D65},
    working_space::xyz_d65_to_rec2020,
};
pub(super) fn xyz(p: [f32; 3]) -> [f64; 3] {
    apply(REC2020_TO_XYZ_D65, p.map(f64::from))
}
pub(super) fn rgb(p: [f64; 3]) -> [f32; 3] {
    apply(xyz_d65_to_rec2020(), p).map(|v| v.max(0.0) as f32)
}
pub(super) fn lab(p: [f32; 3]) -> [f32; 3] {
    let f = |t: f64| {
        if t > (6.0_f64 / 29.0).powi(3) {
            t.cbrt()
        } else {
            t / (3.0 * (6.0_f64 / 29.0).powi(2)) + 4.0 / 29.0
        }
    };
    let t = std::array::from_fn::<_, 3, _>(|k| f(xyz(p)[k] / WHITE_D65[k]));
    [
        (116.0 * t[1] - 16.0) as f32,
        (500.0 * (t[0] - t[1])) as f32,
        (200.0 * (t[1] - t[2])) as f32,
    ]
}
pub(super) fn from_lab(p: [f32; 3]) -> [f32; 3] {
    let y = (f64::from(p[0]) + 16.0) / 116.0;
    let f = [y + f64::from(p[1]) / 500.0, y, y - f64::from(p[2]) / 200.0];
    let inverse = |v: f64| {
        if v > 6.0 / 29.0 {
            v.powi(3)
        } else {
            3.0 * (6.0_f64 / 29.0).powi(2) * (v - 4.0 / 29.0)
        }
    };
    rgb(std::array::from_fn(|k| inverse(f[k]) * WHITE_D65[k]))
}
pub(super) fn uv(p: [f32; 3]) -> [f32; 2] {
    let [x, y, z] = xyz(p);
    let d = (x + 15.0 * y + 3.0 * z).max(1e-12);
    [(4.0 * x / d) as f32, (9.0 * y / d) as f32]
}
pub(super) fn from_uv(p: [f32; 3], uv: [f32; 2]) -> [f32; 3] {
    let y = f64::from(luma(p));
    let u = f64::from(uv[0]);
    let v = f64::from(uv[1]).max(1e-5);
    with_luma(
        rgb([
            9.0 * y * u / (4.0 * v),
            y,
            y * (12.0 - 3.0 * u - 20.0 * v) / (4.0 * v),
        ]),
        luma(p),
    )
}
pub(super) fn simplex(c: [f32; 2], vertices: [[f32; 2]; 3]) -> [f32; 2] {
    let a = [
        vertices[1][0] - vertices[0][0],
        vertices[1][1] - vertices[0][1],
    ];
    let b = [
        vertices[2][0] - vertices[0][0],
        vertices[2][1] - vertices[0][1],
    ];
    let d = [c[0] - vertices[0][0], c[1] - vertices[0][1]];
    let det = a[0] * b[1] - a[1] * b[0];
    if det.abs() > EPS {
        let u = (d[0] * b[1] - d[1] * b[0]) / det;
        let v = (a[0] * d[1] - a[1] * d[0]) / det;
        if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
            return c;
        }
    }
    let mut best = vertices[0];
    let mut distance = f32::INFINITY;
    for k in 0..3 {
        let from = vertices[k];
        let to = vertices[(k + 1) % 3];
        let axis = [to[0] - from[0], to[1] - from[1]];
        let t = (dot([c[0] - from[0], c[1] - from[1]], axis) / dot(axis, axis).max(EPS))
            .clamp(0.0, 1.0);
        let q = [from[0] + t * axis[0], from[1] + t * axis[1]];
        let d = (c[0] - q[0]).powi(2) + (c[1] - q[1]).powi(2);
        if d < distance {
            distance = d;
            best = q;
        }
    }
    best
}
type Mat2 = [[f32; 2]; 2];
fn mul(a: Mat2, b: Mat2) -> Mat2 {
    std::array::from_fn(|r| std::array::from_fn(|c| a[r][0] * b[0][c] + a[r][1] * b[1][c]))
}
fn sqrt(a: Mat2) -> Mat2 {
    let determinant = (a[0][0] * a[1][1] - a[0][1] * a[1][0]).max(0.0).sqrt();
    let scale = (a[0][0] + a[1][1] + 2.0 * determinant).max(EPS).sqrt();
    [
        [(a[0][0] + determinant) / scale, a[0][1] / scale],
        [a[1][0] / scale, (a[1][1] + determinant) / scale],
    ]
}
pub(super) fn transport(mut source: Mat2, mut destination: Mat2, regularization: f32) -> Mat2 {
    for k in 0..2 {
        source[k][k] += regularization;
        destination[k][k] += regularization;
    }
    let root = sqrt(source);
    let determinant = (root[0][0] * root[1][1] - root[0][1] * root[1][0]).max(EPS * EPS);
    let inverse = [
        [root[1][1] / determinant, -root[0][1] / determinant],
        [-root[1][0] / determinant, root[0][0] / determinant],
    ];
    mul(
        mul(inverse, sqrt(mul(mul(root, destination), root))),
        inverse,
    )
    .map(|row| row.map(|v| v.clamp(-8.0, 8.0)))
}
/// KL projection at fixed total channel energy, solved by bounded exponential tilting.
pub(super) fn energy_projection(p: [f32; 3], target: f32) -> [f32; 3] {
    let total = p.iter().sum::<f32>();
    if total <= EPS {
        return p;
    }
    let coefficients = super::super::LUMA;
    let low = coefficients.into_iter().fold(f32::INFINITY, f32::min) * total;
    let high = coefficients.into_iter().fold(0.0, f32::max) * total;
    let target = target.clamp(low, high);
    let evaluate = |lambda: f32| {
        let raw = std::array::from_fn::<_, 3, _>(|k| {
            p[k].max(EPS) * ((lambda * coefficients[k]).clamp(-50.0, 50.0)).exp()
        });
        let norm = raw.iter().sum::<f32>();
        raw.map(|v| v * total / norm)
    };
    let mut left = -64.0;
    let mut right = 64.0;
    for _ in 0..24 {
        let middle = (left + right) * 0.5;
        if luma(evaluate(middle)) < target {
            left = middle;
        } else {
            right = middle;
        }
    }
    evaluate((left + right) * 0.5)
}
