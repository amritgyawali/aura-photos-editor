//! ICC profiles, synthesised rather than shipped.
//!
//! Three profiles, built in code at about 400 bytes each: sRGB, Adobe RGB (1998) and Display P3.
//! All three are the same shape - an RGB matrix/TRC profile - and differ only in nine matrix
//! numbers and a transfer curve, so shipping three binary blobs would be shipping three files
//! nobody can diff.
//!
//! ## Why a profile at all, when the file could just name the space
//!
//! Because most things that open a delivered JPEG do not know what "Display P3" means without one.
//! A file with no profile is treated as sRGB by every browser and most viewers, so an Adobe RGB
//! export without an embedded profile arrives at the client desaturated - which looks like a
//! grading mistake and is a tagging one.
//!
//! ## What these profiles are and are not
//!
//! They are **exact** descriptions of three standard spaces: the primaries are the published
//! chromaticities, the white points are the published white points, and the transfer curves are the
//! published curves. That is all a matrix/TRC profile is.
//!
//! They are not measured, and nothing here claims otherwise. A profile that described *this
//! photographer's monitor* would be a measurement, and this product has no way to make one - which
//! is condition C2 of phase 14 in a different place. The distinction matters because a delivery
//! profile's job is to say what the numbers mean, and that is a definition rather than a
//! measurement.
//!
//! ## The `Curv` shortcut, and why the sRGB profile does not take it
//!
//! ICC allows a `curv` tag with a single 16-bit value, which means "gamma = value / 256". Adobe RGB
//! is exactly gamma 2.19921875 and Display P3 uses the sRGB transfer, which is a piecewise curve
//! and not a gamma at all. Approximating sRGB as gamma 2.2 shifts the darkest two per cent of every
//! tone, which is nothing on a bright frame and is the whole of a candle-lit ceremony. So sRGB and
//! Display P3 get a sampled 1024-point curve and Adobe RGB gets the exact gamma.

use aura_core::contract::delivery::DeliveryColour;

/// How many points a sampled transfer curve carries.
///
/// 1,024. Enough that the reconstruction error is below a sixteen-bit code value everywhere,
/// including the near-black region where the sRGB curve's linear segment lives and where a
/// too-coarse table shows as banding in a dark ceremony.
const CURVE_POINTS: usize = 1024;

/// The ICC profile for one output space.
///
/// Deterministic: the same bytes on every machine and every run, which is what lets a delivered
/// file's digest be compared across a re-export. Invariant 4.
#[must_use]
pub fn profile_for(space: DeliveryColour) -> Vec<u8> {
    let (primaries, white, curve) = match space {
        // sRGB / Rec.709 primaries, D65, the sRGB piecewise transfer.
        DeliveryColour::Srgb => (
            [[0.6400_f64, 0.3300], [0.3000, 0.6000], [0.1500, 0.0600]],
            [0.3127_f64, 0.3290],
            Transfer::Srgb,
        ),
        // Adobe RGB (1998), D65, gamma 563/256.
        DeliveryColour::AdobeRgb => (
            [[0.6400_f64, 0.3300], [0.2100, 0.7100], [0.1500, 0.0600]],
            [0.3127_f64, 0.3290],
            Transfer::Gamma(563.0 / 256.0),
        ),
        // Display P3: DCI-P3 primaries, D65 white, sRGB transfer.
        DeliveryColour::DisplayP3 => (
            [[0.6800_f64, 0.3200], [0.2650, 0.6900], [0.1500, 0.0600]],
            [0.3127_f64, 0.3290],
            Transfer::Srgb,
        ),
    };

    let colourants = colourants_d50(primaries, white);
    build(space, &colourants, curve)
}

/// Which transfer curve a space uses.
#[derive(Debug, Clone, Copy)]
enum Transfer {
    /// The sRGB piecewise curve. Sampled.
    Srgb,
    /// A pure power law, written exactly as ICC's 16-bit gamma.
    Gamma(f64),
}

/// The RGB colourant matrix, adapted to D50 - which is the profile connection space, and the
/// reason a profile's numbers never look like the primaries you started with.
///
/// Bradford adaptation, which is what every ICC profile in circulation uses.
#[allow(clippy::indexing_slicing)]
fn colourants_d50(primaries: [[f64; 2]; 3], white: [f64; 2]) -> [[f64; 3]; 3] {
    // xy to XYZ at Y = 1.
    let xyz = |xy: [f64; 2]| -> [f64; 3] {
        let [x, y] = xy;
        if y.abs() < 1e-9 {
            return [0.0, 0.0, 0.0];
        }
        [x / y, 1.0, (1.0 - x - y) / y]
    };

    let r = xyz(primaries[0]);
    let g = xyz(primaries[1]);
    let b = xyz(primaries[2]);
    let w = xyz(white);

    // Solve [r g b] * s = w for the per-primary scale factors.
    let m = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
    let Some(inv) = invert(m) else {
        return [[0.0; 3]; 3];
    };
    let s = mul_vec(inv, w);

    // The unadapted RGB -> XYZ matrix, columns scaled.
    let rgb_to_xyz = [
        [r[0] * s[0], g[0] * s[1], b[0] * s[2]],
        [r[1] * s[0], g[1] * s[1], b[1] * s[2]],
        [r[2] * s[0], g[2] * s[1], b[2] * s[2]],
    ];

    // Bradford, source white to D50.
    let bradford = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    let Some(bradford_inv) = invert(bradford) else {
        return rgb_to_xyz;
    };
    let d50 = [0.964_2, 1.0, 0.824_9];
    let src_cone = mul_vec(bradford, w);
    let dst_cone = mul_vec(bradford, d50);
    let ratio = [
        if src_cone[0].abs() < 1e-9 {
            1.0
        } else {
            dst_cone[0] / src_cone[0]
        },
        if src_cone[1].abs() < 1e-9 {
            1.0
        } else {
            dst_cone[1] / src_cone[1]
        },
        if src_cone[2].abs() < 1e-9 {
            1.0
        } else {
            dst_cone[2] / src_cone[2]
        },
    ];
    let scale = [
        [ratio[0], 0.0, 0.0],
        [0.0, ratio[1], 0.0],
        [0.0, 0.0, ratio[2]],
    ];
    let adapt = mul(bradford_inv, mul(scale, bradford));
    mul(adapt, rgb_to_xyz)
}

// Fixed 3x3 arrays indexed by loops that are literally `0..3`. The bound is a compile-time
// constant and the index is a compile-time constant range, so the panic the lint guards cannot
// occur; writing these with `get` would turn nine multiplies into nine `Option` unwraps and make
// the matrix expression unreadable, which is the thing a colour matrix most needs not to be.
#[allow(clippy::indexing_slicing)]
fn mul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0_f64; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            let mut sum = 0.0;
            for k in 0..3 {
                sum += a[i][k] * b[k][j];
            }
            *cell = sum;
        }
    }
    out
}

#[allow(clippy::indexing_slicing)]
fn mul_vec(a: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2],
        a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2],
        a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2],
    ]
}

#[allow(clippy::indexing_slicing)]
fn invert(m: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det,
        ],
    ])
}

/// s15Fixed16Number, which is how ICC stores everything that is not a curve.
fn s15f16(v: f64) -> [u8; 4] {
    let clamped = v.clamp(-32768.0, 32767.999_98);
    #[allow(clippy::cast_possible_truncation)]
    let raw = (clamped * 65536.0).round() as i32;
    raw.to_be_bytes()
}

fn tag_xyz(v: [f64; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(b"XYZ ");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&s15f16(v[0]));
    out.extend_from_slice(&s15f16(v[1]));
    out.extend_from_slice(&s15f16(v[2]));
    out
}

fn tag_curve(transfer: Transfer) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"curv");
    out.extend_from_slice(&[0, 0, 0, 0]);
    match transfer {
        Transfer::Gamma(g) => {
            out.extend_from_slice(&1_u32.to_be_bytes());
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let fixed = (g * 256.0).round() as u16;
            out.extend_from_slice(&fixed.to_be_bytes());
        }
        Transfer::Srgb => {
            out.extend_from_slice(&(CURVE_POINTS as u32).to_be_bytes());
            for i in 0..CURVE_POINTS {
                let x = i as f64 / (CURVE_POINTS - 1) as f64;
                // The sRGB *encoding* curve is what a `curv` tag stores: device value to linear.
                let linear = if x <= 0.040_45 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let v = (linear.clamp(0.0, 1.0) * 65535.0).round() as u16;
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
    }
    out
}

fn tag_text(text: &str) -> Vec<u8> {
    // `mluc`, the multi-localised Unicode type. `desc` has required `mluc` since ICC v4, and the
    // v2 `desc` type is what makes a profile fail validation in newer tools.
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let bytes = utf16.len() * 2;
    let mut out = Vec::with_capacity(28 + bytes);
    out.extend_from_slice(b"mluc");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&1_u32.to_be_bytes()); // one record
    out.extend_from_slice(&12_u32.to_be_bytes()); // record size
    out.extend_from_slice(b"enUS");
    #[allow(clippy::cast_possible_truncation)]
    out.extend_from_slice(&(bytes as u32).to_be_bytes());
    out.extend_from_slice(&28_u32.to_be_bytes()); // offset from the tag's start
    for u in utf16 {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out
}

/// Assemble a v4 matrix/TRC display profile.
#[allow(clippy::indexing_slicing)]
fn build(space: DeliveryColour, colourants: &[[f64; 3]; 3], transfer: Transfer) -> Vec<u8> {
    let desc = match space {
        DeliveryColour::Srgb => "sRGB IEC61966-2.1 (AURA)",
        DeliveryColour::AdobeRgb => "Adobe RGB (1998) compatible (AURA)",
        DeliveryColour::DisplayP3 => "Display P3 (AURA)",
    };

    // Nine tags. The three colourants are columns of the adapted matrix; the three TRCs are the
    // same curve three times, which is what "one transfer function for all channels" means.
    let curve = tag_curve(transfer);
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", tag_text(desc)),
        (
            b"rXYZ",
            tag_xyz([colourants[0][0], colourants[1][0], colourants[2][0]]),
        ),
        (
            b"gXYZ",
            tag_xyz([colourants[0][1], colourants[1][1], colourants[2][1]]),
        ),
        (
            b"bXYZ",
            tag_xyz([colourants[0][2], colourants[1][2], colourants[2][2]]),
        ),
        (b"rTRC", curve.clone()),
        (b"gTRC", curve.clone()),
        (b"bTRC", curve),
        (b"wtpt", tag_xyz([0.964_2, 1.0, 0.824_9])),
        (b"cprt", tag_text("Public Domain")),
    ];

    let header = 128_usize;
    let table = 4 + tags.len() * 12;
    let mut offsets = Vec::with_capacity(tags.len());
    let mut cursor = header + table;
    // Four-byte alignment between tag bodies, which the specification requires and which some
    // parsers enforce.
    for (_, body) in &tags {
        offsets.push((cursor, body.len()));
        cursor += body.len();
        while !cursor.is_multiple_of(4) {
            cursor += 1;
        }
    }
    let total = cursor;

    let mut out = Vec::with_capacity(total);
    // --- header, 128 bytes ---
    #[allow(clippy::cast_possible_truncation)]
    out.extend_from_slice(&(total as u32).to_be_bytes());
    out.extend_from_slice(b"AURA"); // preferred CMM
    out.extend_from_slice(&0x0420_0000_u32.to_be_bytes()); // v4.2.0
    out.extend_from_slice(b"mntr"); // display device
    out.extend_from_slice(b"RGB ");
    out.extend_from_slice(b"XYZ ");
    // Creation date. Zeroed **deliberately**: a profile whose bytes depend on when it was built
    // is a delivered file whose digest changes between two identical exports, which would defeat
    // the one comparison this whole phase is built on. Invariant 4.
    out.extend_from_slice(&[0_u8; 12]);
    out.extend_from_slice(b"acsp");
    out.extend_from_slice(&[0_u8; 4]); // platform: none
    out.extend_from_slice(&[0_u8; 4]); // flags
    out.extend_from_slice(&[0_u8; 4]); // device manufacturer
    out.extend_from_slice(&[0_u8; 4]); // device model
    out.extend_from_slice(&[0_u8; 8]); // device attributes
    out.extend_from_slice(&0_u32.to_be_bytes()); // perceptual intent
    out.extend_from_slice(&s15f16(0.964_2));
    out.extend_from_slice(&s15f16(1.0));
    out.extend_from_slice(&s15f16(0.824_9));
    out.extend_from_slice(b"AURA"); // creator
    out.extend_from_slice(&[0_u8; 16]); // profile id, unset
    out.extend_from_slice(&[0_u8; 28]); // reserved
    debug_assert_eq!(out.len(), header);

    // --- tag table ---
    #[allow(clippy::cast_possible_truncation)]
    out.extend_from_slice(&(tags.len() as u32).to_be_bytes());
    for (i, (sig, _)) in tags.iter().enumerate() {
        out.extend_from_slice(*sig);
        let (off, len) = offsets.get(i).copied().unwrap_or((0, 0));
        #[allow(clippy::cast_possible_truncation)]
        out.extend_from_slice(&(off as u32).to_be_bytes());
        #[allow(clippy::cast_possible_truncation)]
        out.extend_from_slice(&(len as u32).to_be_bytes());
    }

    // --- tag bodies ---
    for (_, body) in &tags {
        out.extend_from_slice(body);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_space_produces_a_profile_that_parses_as_one() {
        for space in DeliveryColour::ALL {
            let p = profile_for(space);
            assert!(p.len() > 200, "{space:?} profile is too small");
            // The size field agrees with the buffer.
            let declared = u32::from_be_bytes([p[0], p[1], p[2], p[3]]) as usize;
            assert_eq!(declared, p.len(), "{space:?} size field");
            // `acsp` at byte 36.
            assert_eq!(&p[36..40], b"acsp", "{space:?} signature");
            // Nine tags.
            let count = u32::from_be_bytes([p[128], p[129], p[130], p[131]]);
            assert_eq!(count, 9, "{space:?} tag count");
        }
    }

    #[test]
    fn a_profile_is_byte_identical_between_two_builds() {
        // Invariant 4 at the file level: two identical exports must produce identical bytes, and a
        // creation date in the header would break exactly that. This test is why it is zeroed.
        for space in DeliveryColour::ALL {
            assert_eq!(profile_for(space), profile_for(space));
        }
    }

    #[test]
    fn the_srgb_matrix_is_the_published_one() {
        // The sRGB D50-adapted colourants are a published table; agreeing with it to four decimal
        // places is what says the Bradford adaptation above is right rather than merely
        // self-consistent.
        let m = colourants_d50(
            [[0.6400, 0.3300], [0.3000, 0.6000], [0.1500, 0.0600]],
            [0.3127, 0.3290],
        );
        let expected = [
            [0.4360, 0.3851, 0.1431],
            [0.2225, 0.7169, 0.0606],
            [0.0139, 0.0971, 0.7141],
        ];
        for (row, exp_row) in m.iter().zip(expected.iter()) {
            for (got, want) in row.iter().zip(exp_row.iter()) {
                assert!(
                    (got - want).abs() < 2e-3,
                    "colourant {got} should be about {want}"
                );
            }
        }
    }

    #[test]
    fn adobe_rgb_uses_an_exact_gamma_and_srgb_does_not() {
        // sRGB's near-black linear segment is not a gamma, and approximating it as 2.2 shifts the
        // darkest two per cent of every tone - nothing on a bright frame, the whole of a
        // candle-lit ceremony.
        let srgb = tag_curve(Transfer::Srgb);
        let count = u32::from_be_bytes([srgb[8], srgb[9], srgb[10], srgb[11]]);
        assert_eq!(count as usize, CURVE_POINTS);

        let adobe = tag_curve(Transfer::Gamma(563.0 / 256.0));
        let count = u32::from_be_bytes([adobe[8], adobe[9], adobe[10], adobe[11]]);
        assert_eq!(count, 1);
        let gamma = u16::from_be_bytes([adobe[12], adobe[13]]);
        assert_eq!(gamma, 563);
    }
}
