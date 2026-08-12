//! 3x3 colour matrices, chromatic adaptation and the standard primaries.
//!
//! All matrix maths is `f64`. Pixels are `f32`, but a matrix inverted in `f32`
//! loses enough precision to move a ColorChecker patch by more than the dE2000
//! tolerance we promise, so the conversion happens once, in double, at profile
//! load time.

/// A row-major 3x3 matrix.
pub type Mat3 = [[f64; 3]; 3];

/// A tristimulus triple.
pub type Vec3 = [f64; 3];

/// The identity.
pub const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// CIE XYZ of D50, the illuminant DNG colour matrices are defined against.
pub const WHITE_D50: Vec3 = [0.964_2, 1.0, 0.824_9];

/// CIE XYZ of D65, the illuminant of sRGB and Rec.2020.
pub const WHITE_D65: Vec3 = [0.950_47, 1.0, 1.088_83];

/// Linear sRGB (D65) to CIE XYZ.
pub const SRGB_TO_XYZ_D65: Mat3 = [
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192_0, 0.950_304_1],
];

/// Linear Rec.2020 (D65) to CIE XYZ. The working space of the whole product.
pub const REC2020_TO_XYZ_D65: Mat3 = [
    [0.636_958_0, 0.144_616_9, 0.168_881_0],
    [0.262_700_2, 0.677_998_1, 0.059_301_7],
    [0.000_000_0, 0.028_072_7, 1.060_985_1],
];

/// The Bradford cone response matrix, the standard basis for adaptation.
const BRADFORD: Mat3 = [
    [0.895_1, 0.266_4, -0.161_4],
    [-0.750_2, 1.713_5, 0.036_7],
    [0.038_9, -0.068_5, 1.029_6],
];

/// Multiply two matrices, `a` then `b` applied to a column vector as `b * a`.
#[must_use]
pub fn mul(b: Mat3, a: Mat3) -> Mat3 {
    let mut out = [[0.0f64; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            let mut sum = 0.0;
            for k in 0..3 {
                let left = b.get(row).and_then(|r| r.get(k)).copied().unwrap_or(0.0);
                let right = a.get(k).and_then(|r| r.get(column)).copied().unwrap_or(0.0);
                sum += left * right;
            }
            if let Some(slot) = out.get_mut(row).and_then(|r| r.get_mut(column)) {
                *slot = sum;
            }
        }
    }
    out
}

/// Apply a matrix to a triple.
#[must_use]
pub fn apply(m: Mat3, v: Vec3) -> Vec3 {
    let mut out = [0.0f64; 3];
    for row in 0..3 {
        let mut sum = 0.0;
        for k in 0..3 {
            let left = m.get(row).and_then(|r| r.get(k)).copied().unwrap_or(0.0);
            let right = v.get(k).copied().unwrap_or(0.0);
            sum += left * right;
        }
        if let Some(slot) = out.get_mut(row) {
            *slot = sum;
        }
    }
    out
}

/// Determinant, used by [`invert`] and by the sanity check on loaded profiles.
#[must_use]
pub fn determinant(m: Mat3) -> f64 {
    let at = |row: usize, column: usize| -> f64 {
        m.get(row)
            .and_then(|r| r.get(column))
            .copied()
            .unwrap_or(0.0)
    };
    at(0, 0) * (at(1, 1) * at(2, 2) - at(1, 2) * at(2, 1))
        - at(0, 1) * (at(1, 0) * at(2, 2) - at(1, 2) * at(2, 0))
        + at(0, 2) * (at(1, 0) * at(2, 1) - at(1, 1) * at(2, 0))
}

/// Invert a matrix.
///
/// Returns `None` when the matrix is singular, which for a colour matrix means
/// the profile is nonsense and must be refused rather than approximated.
#[must_use]
pub fn invert(m: Mat3) -> Option<Mat3> {
    let det = determinant(m);
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let at = |row: usize, column: usize| -> f64 {
        m.get(row)
            .and_then(|r| r.get(column))
            .copied()
            .unwrap_or(0.0)
    };
    let inv_det = 1.0 / det;
    let mut out = [[0.0f64; 3]; 3];
    let cofactors = [
        [
            at(1, 1) * at(2, 2) - at(1, 2) * at(2, 1),
            at(0, 2) * at(2, 1) - at(0, 1) * at(2, 2),
            at(0, 1) * at(1, 2) - at(0, 2) * at(1, 1),
        ],
        [
            at(1, 2) * at(2, 0) - at(1, 0) * at(2, 2),
            at(0, 0) * at(2, 2) - at(0, 2) * at(2, 0),
            at(0, 2) * at(1, 0) - at(0, 0) * at(1, 2),
        ],
        [
            at(1, 0) * at(2, 1) - at(1, 1) * at(2, 0),
            at(0, 1) * at(2, 0) - at(0, 0) * at(2, 1),
            at(0, 0) * at(1, 1) - at(0, 1) * at(1, 0),
        ],
    ];
    for row in 0..3 {
        for column in 0..3 {
            let value = cofactors
                .get(row)
                .and_then(|r| r.get(column))
                .copied()
                .unwrap_or(0.0)
                * inv_det;
            if let Some(slot) = out.get_mut(row).and_then(|r| r.get_mut(column)) {
                *slot = value;
            }
        }
    }
    Some(out)
}

/// A diagonal matrix from three scalars.
#[must_use]
pub fn diagonal(v: Vec3) -> Mat3 {
    [
        [v.first().copied().unwrap_or(1.0), 0.0, 0.0],
        [0.0, v.get(1).copied().unwrap_or(1.0), 0.0],
        [0.0, 0.0, v.get(2).copied().unwrap_or(1.0)],
    ]
}

/// Bradford chromatic adaptation from one white point to another.
///
/// Returns the identity when the cone responses are degenerate, which cannot
/// happen for a real illuminant and must not panic if a file claims one.
#[must_use]
pub fn bradford(from_white: Vec3, to_white: Vec3) -> Mat3 {
    let source = apply(BRADFORD, from_white);
    let destination = apply(BRADFORD, to_white);
    let ratio = [
        safe_ratio(destination.first().copied(), source.first().copied()),
        safe_ratio(destination.get(1).copied(), source.get(1).copied()),
        safe_ratio(destination.get(2).copied(), source.get(2).copied()),
    ];
    let Some(inverse) = invert(BRADFORD) else {
        return IDENTITY;
    };
    mul(inverse, mul(diagonal(ratio), BRADFORD))
}

fn safe_ratio(numerator: Option<f64>, denominator: Option<f64>) -> f64 {
    match (numerator, denominator) {
        (Some(n), Some(d)) if d.abs() > 1e-12 => n / d,
        _ => 1.0,
    }
}

/// Normalise a camera-to-XYZ matrix so that a neutral camera signal maps to the
/// destination white point exactly.
///
/// Without this step a matrix that is correct in hue leaves a global cast,
/// because the rows carry an arbitrary overall scale.
#[must_use]
pub fn normalise_to_white(camera_to_xyz: Mat3, white: Vec3) -> Mat3 {
    let neutral = apply(camera_to_xyz, [1.0, 1.0, 1.0]);
    let y = neutral.get(1).copied().unwrap_or(1.0);
    if !(y.is_finite() && y.abs() > 1e-9) {
        return camera_to_xyz;
    }
    let scaled = [
        [
            camera_to_xyz[0][0] / y,
            camera_to_xyz[0][1] / y,
            camera_to_xyz[0][2] / y,
        ],
        [
            camera_to_xyz[1][0] / y,
            camera_to_xyz[1][1] / y,
            camera_to_xyz[1][2] / y,
        ],
        [
            camera_to_xyz[2][0] / y,
            camera_to_xyz[2][1] / y,
            camera_to_xyz[2][2] / y,
        ],
    ];
    // Adapt whatever white the normalised matrix actually produces onto the
    // requested one, so the result is neutral by construction.
    let produced = apply(scaled, [1.0, 1.0, 1.0]);
    mul(bradford(produced, white), scaled)
}
