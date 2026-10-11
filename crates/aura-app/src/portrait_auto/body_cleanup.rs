//! Whole-mask body blemish healing, independent of a clean face/skin sample.
use super::{cover, BrushMask, Edit, MatteUse, Settings, Tool, PREFIX};

pub(super) fn spots(
    index: usize,
    matte: &MatteUse,
    size: [f32; 2],
    feature_width: f32,
    settings: &Settings,
) -> Option<Edit> {
    if settings.body_blemishes < 0.02 {
        return None;
    }
    let [l, t, r, b] = matte.matte.bounds;
    let short = size[0].min(size[1]);
    let deep = settings.deep_blemish_cleanup;
    Some(Edit {
        id: format!("{PREFIX}{index}-body-spots"),
        tool: if deep {
            Tool::FrequencyHeal
        } else {
            Tool::AutoBlemish
        },
        enabled: true,
        region: [(l + r) * 0.5, (t + b) * 0.5, (r - l) * 0.5, (b - t) * 0.5],
        source: None,
        amount: settings.body_blemishes.clamp(0.0, 1.0),
        feather: 0.6,
        radius: (feature_width * if deep { 0.009 } else { 0.03 } / short).clamp(0.0005, 0.012),
        source_scale: 1.0,
        preserve_microtexture: false,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        sensitivity: deep.then_some(settings.blemish_sensitivity),
        keep_dark_marks: deep && !settings.remove_dark_marks,
        texture: if deep { 0.65 } else { 1.0 },
        tone: if deep { 1.0 } else { 0.5 },
        warmth: 0.0,
        tint: 0.0,
        mask: Some(BrushMask {
            strokes: cover(matte.matte.bounds, size, 0.6),
        }),
        skin: None,
        selection: None,
        matte: Some(matte.id.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disconnected_neck_hands_and_legs_receive_native_healing() {
        let (w, h) = (128, 256);
        let mut alpha = vec![0; w * h];
        let mut before = [0.3_f32, 0.2, 0.15].repeat(w * h);
        for (cx, cy) in [(64, 35), (28, 125), (92, 225)] {
            for y in cy - 16..=cy + 16 {
                for x in cx - 16..=cx + 16 {
                    alpha[y * w + x] = 255;
                }
            }
            for y in cy - 2..=cy + 2 {
                for x in cx - 2..=cx + 2 {
                    for c in 0..3 {
                        before[(y * w + x) * 3 + c] *= 0.5;
                    }
                }
            }
        }
        let matte = MatteUse {
            id: "test-body".into(),
            matte: aura_vision::skin::Matte {
                bounds: [0.0, 0.0, 1.0, 1.0],
                width: w,
                height: h,
                alpha,
            },
        };
        let settings = Settings {
            deep_blemish_cleanup: true,
            body_blemishes: 1.0,
            remove_dark_marks: true,
            blemish_sensitivity: 0.85,
            ..Settings::default()
        };
        let edit = spots(0, &matte, [w as f32, h as f32], 100.0, &settings).unwrap();
        let mask = aura_recipe::retouch_tools::Matte::encode(
            matte.matte.bounds,
            w as u32,
            h as u32,
            &matte.matte.alpha,
        );
        let mattes = std::collections::BTreeMap::from([(matte.id, mask)]);
        let mut after = before.clone();
        aura_render::retouch_tools::apply_with_mattes(&mut after, w, h, &[edit], &mattes);
        for (cx, cy) in [(64, 35), (28, 125), (92, 225)] {
            assert!(
                after[(cy * w + cx) * 3] > before[(cy * w + cx) * 3] + 0.08,
                "Disconnected skin mark at {cx},{cy} was missed"
            );
        }
        for (i, a) in matte.matte.alpha.iter().enumerate() {
            if *a == 0 {
                assert_eq!(&before[i * 3..i * 3 + 3], &after[i * 3..i * 3 + 3]);
            }
        }
    }
}
