//! Bounded, order-independent reference evidence, with separate lighting populations.
use std::collections::BTreeMap;

use aura_core::contract::style::LightingBucket;
use aura_core::AuraResult;
use aura_raw::codec::Rgb8;
use serde::{Deserialize, Serialize};

use super::super::transfer::{self, GradeStats};
use super::correspondence::SpatialEvidence;

const PER_LIGHT: usize = 8;
const SAMPLES: usize = 1024;
const MIN_SAMPLES: usize = 256;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ReferenceEvidence {
    global: GradeStats,
    lighting: BTreeMap<LightingBucket, GradeStats>,
    #[serde(default)]
    spatial: Option<SpatialEvidence>,
}

impl ReferenceEvidence {
    pub(crate) fn from_preview(preview: &[u8]) -> Option<Self> {
        if preview.len() != super::super::content::BYTES {
            return None;
        }
        let image = Rgb8 {
            width: super::super::content::EDGE,
            height: super::super::content::EDGE,
            data: preview.to_vec(),
        };
        let mut builder = EvidenceBuilder::default();
        builder.add("cached-reference", LightingBucket::Unknown, &image);
        builder.finish()
    }

    pub(crate) fn validate(&self) -> AuraResult<()> {
        if !self.global.usable()
            || self.lighting.values().any(|stats| !stats.usable())
            || self
                .spatial
                .as_ref()
                .is_some_and(|spatial| !spatial.valid())
        {
            return Err(super::super::invalid(
                "The reference colour evidence is invalid. Analyse the reference again.",
            ));
        }
        Ok(())
    }

    pub(super) fn select(&self, lighting: LightingBucket, confidence: f32) -> &GradeStats {
        if confidence >= 0.5 {
            self.lighting.get(&lighting).unwrap_or(&self.global)
        } else {
            &self.global
        }
    }

    pub(super) fn corresponding(&self, image: &Rgb8) -> Option<&SpatialEvidence> {
        self.spatial
            .as_ref()
            .filter(|spatial| spatial.corresponding(image))
    }
}

#[derive(Debug)]
struct Example {
    key: [u8; 32],
    labs: Vec<[f32; 3]>,
}

#[derive(Debug, Default)]
pub(crate) struct EvidenceBuilder {
    lighting: BTreeMap<LightingBucket, Vec<Example>>,
    single: Option<GradeStats>,
    references: usize,
    spatial: Option<SpatialEvidence>,
}

fn fold(examples: &[&Example]) -> GradeStats {
    // Equal support per photo prevents a large landscape or one small JPEG from dominating
    // a mixed reference gallery. Only real observations count; tiny inputs are not padded.
    let count = examples
        .iter()
        .map(|example| example.labs.len())
        .min()
        .unwrap_or(0);
    let labs: Vec<_> = examples
        .iter()
        .flat_map(|example| {
            (0..count)
                .filter_map(move |i| example.labs.get(i * example.labs.len() / count).copied())
        })
        .collect();
    let mut stats = transfer::measure(&labs);
    stats.palette_matching = true;
    stats
}

impl EvidenceBuilder {
    pub(crate) fn add(&mut self, key: &str, lighting: LightingBucket, image: &Rgb8) {
        let key = *blake3::hash(key.as_bytes()).as_bytes();
        let examples = self.lighting.entry(lighting).or_default();
        if examples.iter().any(|example| example.key == key)
            || (examples.len() >= PER_LIGHT && examples.last().is_some_and(|last| key >= last.key))
        {
            return;
        }
        let labs = transfer::sample_labs(image, SAMPLES);
        if labs.len() < MIN_SAMPLES {
            return;
        }
        self.references += 1;
        self.single = if self.references == 1 {
            Some(transfer::measure_image(image))
        } else {
            None
        };
        self.spatial = if self.references == 1 {
            SpatialEvidence::new(image)
        } else {
            None
        };
        examples.push(Example { key, labs });
        examples.sort_by_key(|example| example.key);
        examples.truncate(PER_LIGHT);
    }

    pub(crate) fn finish(self) -> Option<ReferenceEvidence> {
        let examples: Vec<_> = self.lighting.values().flatten().collect();
        if examples.is_empty() {
            return None;
        }
        let mut global = self.single.unwrap_or_else(|| fold(&examples));
        global.palette_matching = true;
        let lighting = self
            .lighting
            .iter()
            .filter(|(_, rows)| rows.len() >= 4)
            .map(|(&bucket, rows)| {
                let examples: Vec<_> = rows.iter().collect();
                (bucket, fold(&examples))
            })
            .collect();
        Some(ReferenceEvidence {
            global,
            lighting,
            spatial: self.spatial,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(offset: u8) -> Rgb8 {
        Rgb8 {
            width: 32,
            height: 32,
            data: (0..1024)
                .flat_map(|i| {
                    let value = 25 + (i % 100) as u8;
                    [value + offset, value + 5, value]
                })
                .collect(),
        }
    }

    #[test]
    fn evidence_is_bounded_order_independent_and_conditioned_on_supported_lighting() {
        let bucket = LightingBucket::ALL[0];
        let build = |reverse: bool| {
            let mut builder = EvidenceBuilder::default();
            for i in 0..24 {
                let index = if reverse { 23 - i } else { i };
                builder.add(&index.to_string(), bucket, &image(index));
            }
            assert_eq!(builder.lighting[&bucket].len(), PER_LIGHT);
            builder.finish().unwrap()
        };
        let (first, second) = (build(false), build(true));
        first.validate().unwrap();
        assert_eq!(first.global, second.global);
        assert_eq!(first.lighting, second.lighting);
        assert_eq!(first.global.samples, (PER_LIGHT * SAMPLES) as u32);
        let text = serde_json::to_string(&first).unwrap();
        serde_json::from_str::<ReferenceEvidence>(&text)
            .unwrap()
            .validate()
            .unwrap();
    }

    #[test]
    fn weak_lighting_falls_back_and_missing_pixels_are_never_fabricated() {
        let bucket = LightingBucket::ALL[0];
        let mut builder = EvidenceBuilder::default();
        builder.add("one", bucket, &image(10));
        builder.add("one", bucket, &image(20));
        builder.add(
            "tiny",
            bucket,
            &Rgb8 {
                width: 1,
                height: 1,
                data: vec![100; 3],
            },
        );
        let evidence = builder.finish().unwrap();
        assert!(evidence.lighting.is_empty());
        assert_eq!(evidence.global.samples, SAMPLES as u32);
        assert_eq!(evidence.select(bucket, 1.0), &evidence.global);
        let mut invalid = evidence;
        invalid.global.lightness[0] = f32::NAN;
        assert!(invalid.validate().is_err());
    }
}
