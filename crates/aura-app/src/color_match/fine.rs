//! Independent strengths within the fitted colour treatment; legacy sessions use 100%.
use aura_core::contract::style::StyleDelta;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct FineControls {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub curves: f32,
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub hue: f32,
}
impl Default for FineControls {
    fn default() -> Self {
        Self {
            exposure: 1.0,
            contrast: 1.0,
            highlights: 1.0,
            shadows: 1.0,
            whites: 1.0,
            blacks: 1.0,
            curves: 1.0,
            temperature: 1.0,
            tint: 1.0,
            vibrance: 1.0,
            saturation: 1.0,
            hue: 1.0,
        }
    }
}
impl FineControls {
    pub(super) fn values(self) -> [f32; 12] {
        [
            self.exposure,
            self.contrast,
            self.highlights,
            self.shadows,
            self.whites,
            self.blacks,
            self.curves,
            self.temperature,
            self.tint,
            self.vibrance,
            self.saturation,
            self.hue,
        ]
    }
    pub(super) fn apply(self, out: &mut StyleDelta) {
        out.exposure *= self.exposure;
        out.contrast *= self.contrast;
        out.highlights *= self.highlights;
        out.shadows *= self.shadows;
        out.whites *= self.whites;
        out.blacks *= self.blacks;
        out.curve_shift = out.curve_shift.scaled(self.curves);
        out.temperature_k *= self.temperature;
        out.tint *= self.tint;
        out.vibrance *= self.vibrance;
        out.saturation *= self.saturation;
    }
}
