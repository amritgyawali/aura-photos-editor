//! Conservative, reversible photo adjustments through the governed gateway.

use aura_core::{AuraError, AuraResult};
use serde::{Deserialize, Serialize};

use crate::contract::cloud::{CloudTask, ImagePart, PromptSpec, Tier, Validate};
use crate::tasks::Scored;

/// Quantised readings of the original sRGB preview, included in the cache key.
#[derive(Debug, Clone, Serialize, Hash)]
pub struct PhotoReadings {
    /// Hash of the uploaded derivative.
    pub image_hash: String,
    /// Median display luminance, 0-255.
    pub median: u8,
    /// Brightest five percent boundary, 0-255.
    pub high: u8,
    /// Darkest ten percent boundary, 0-255.
    pub low: u8,
}

impl PhotoReadings {
    /// Measure a validated RGB preview.
    ///
    /// # Errors
    /// Refuses empty or incomplete RGB buffers.
    pub fn measure(rgb: &[u8], image_hash: String) -> AuraResult<Self> {
        if rgb.is_empty() || rgb.len() % 3 != 0 {
            return Err(aura_core::errors::raw::corrupt(
                "invalid auto-edit RGB preview",
            ));
        }
        let mut luminance: Vec<u16> = rgb
            .chunks_exact(3)
            .map(|p| {
                let mut channels = p.iter().copied().map(u16::from);
                (54 * channels.next().unwrap_or(0)
                    + 183 * channels.next().unwrap_or(0)
                    + 19 * channels.next().unwrap_or(0))
                    / 256
            })
            .collect();
        luminance.sort_unstable();
        let percentile = |percent: usize| -> u8 {
            u8::try_from(
                luminance
                    .get((luminance.len() - 1) * percent / 100)
                    .copied()
                    .unwrap_or(0),
            )
            .unwrap_or(255)
        };
        Ok(Self {
            image_hash,
            median: percentile(50),
            high: percentile(95),
            low: percentile(10),
        })
    }
}

/// A bounded recipe proposal; it never contains generated pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoAdjustment {
    /// Absolute exposure adjustment in stops, relative to the original.
    pub exposure: f32,
    /// Contrast adjustment.
    pub contrast: i16,
    /// Highlight recovery.
    pub highlights: i16,
    /// Shadow lift.
    pub shadows: i16,
    /// Modest colour enhancement.
    pub vibrance: i16,
    /// Uncalibrated self-assessment, not an accuracy guarantee.
    pub confidence: f32,
    /// Plain-language explanation.
    pub reasons: Vec<String>,
}

impl Validate for PhotoAdjustment {
    fn validate(&self) -> Result<(), String> {
        if !self.exposure.is_finite()
            || !(-1.5..=1.5).contains(&self.exposure)
            || !(-20..=20).contains(&self.contrast)
            || !(-50..=0).contains(&self.highlights)
            || !(0..=40).contains(&self.shadows)
            || !(-10..=20).contains(&self.vibrance)
            || !self.confidence.is_finite()
            || !(0.0..=1.0).contains(&self.confidence)
            || self.reasons.is_empty()
            || self.reasons.len() > 5
            || self
                .reasons
                .iter()
                .any(|r| r.trim().is_empty() || r.len() > 400)
        {
            return Err("Use conservative adjustment ranges and one to five short reasons".into());
        }
        Ok(())
    }
}

impl Scored for PhotoAdjustment {
    fn confidence(&self) -> f32 {
        self.confidence
    }
    fn reasons(&self) -> &[String] {
        &self.reasons
    }
}

/// Vision task whose fallback uses actual photograph luminance.
#[derive(Debug, Clone)]
pub struct PhotoAutoEdit {
    /// A metadata-free derivative, built by the payload service.
    pub image: ImagePart,
}

impl CloudTask for PhotoAutoEdit {
    const NAME: &'static str = "photo_auto_edit";
    const VERSION: u16 = 1;
    type Input = PhotoReadings;
    type Output = PhotoAdjustment;

    fn prompt(&self, input: &Self::Input) -> PromptSpec {
        PromptSpec::new(
            "You are a conservative photo editor. Inspect the attached original photograph. Return only a JSON object with exposure (-1.5 to 1.5 stops), contrast (-20 to 20 integer), highlights (-50 to 0 integer), shadows (0 to 40 integer), vibrance (-10 to 20 integer), confidence (0 to 1), and reasons (1 to 5 short sentences). These are absolute adjustments relative to the original, not increments. Preserve intentional night scenes, silhouettes, skin tones, people and content. Do not brighten every dark scene. Avoid strong saturation. No other fields.",
            format!("Original preview luminance: median {}, dark percentile {}, bright percentile {}. Choose a natural edit and explain what you saw.", input.median, input.low, input.high),
        ).with_images(vec![self.image.clone()]).with_max_tokens(512).with_min_tier(Tier::Balanced)
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","additionalProperties":false,"required":["exposure","contrast","highlights","shadows","vibrance","confidence","reasons"],"properties":{"exposure":{"type":"number","minimum":-1.5,"maximum":1.5},"contrast":{"type":"integer","minimum":-20,"maximum":20},"highlights":{"type":"integer","minimum":-50,"maximum":0},"shadows":{"type":"integer","minimum":0,"maximum":40},"vibrance":{"type":"integer","minimum":-10,"maximum":20},"confidence":{"type":"number","minimum":0,"maximum":1},"reasons":{"type":"array","minItems":1,"maxItems":5,"items":{"type":"string","minLength":1,"maxLength":400}}}}"#
    }

    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError> {
        let sample = f32::from(input.median.max(1)) / 255.0;
        let linear = aura_raw::colour::curve::srgb_decode(sample).max(0.005);
        // Small bounds because a histogram cannot recognise intentional low/high-key scenes.
        let exposure = ((0.18 / linear).log2().clamp(-0.75, 0.75) * 100.0).round() / 100.0;
        Ok(PhotoAdjustment {
            exposure,
            contrast: if input.high.saturating_sub(input.low) < 100 { 8 } else { 0 },
            highlights: if input.high > 235 { -20 } else { 0 },
            shadows: if input.low < 35 && input.high > 100 { 12 } else { 0 },
            vibrance: 0,
            confidence: 0.35,
            reasons: vec![format!("Local histogram enhancement: median brightness {} / 255; exposure {exposure:+.2} stops. Review intentional dark or bright scenes. No vision model answered this edit.", input.median)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dark_and_bright_photos_get_opposite_bounded_adjustments() {
        let task = PhotoAutoEdit {
            image: ImagePart {
                media_type: "image/jpeg".into(),
                bytes: vec![],
                content_hash: String::new(),
                width: 1,
                height: 1,
            },
        };
        for (sample, direction) in [(50_u8, 1.0), (210, -1.0)] {
            let readings = PhotoReadings::measure(&[sample; 300], "hash".into()).expect("readings");
            let output = task.local_fallback(&readings).expect("fallback");
            assert!(output.exposure * direction > 0.0);
            assert!(output.exposure.abs() <= 0.75);
            output.validate().expect("valid");
            crate::schema::Schema::parse(task.output_schema()).expect("schema");
        }
    }
    #[test]
    fn invalid_pixels_and_unsafe_answers_are_refused() {
        assert!(PhotoReadings::measure(&[], "hash".into()).is_err());
        assert!(PhotoReadings::measure(&[1, 2], "hash".into()).is_err());
        let output = PhotoAdjustment {
            exposure: 5.0,
            contrast: 0,
            highlights: 0,
            shadows: 0,
            vibrance: 0,
            confidence: 0.5,
            reasons: vec!["too strong".into()],
        };
        assert!(output.validate().is_err());
    }
}
