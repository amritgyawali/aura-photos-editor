//! Grayscale contrast views used only to find spots; they never grade output pixels.
const HIGH_CONTRAST: f32 = 4.0;
const LOW_CONTRAST: f32 = 0.25;
const VIEW_PEDESTAL: f32 = 0.08;

fn gray_view(value: f32, midpoint: f32, contrast: f32) -> f32 {
    (0.5 + (value - midpoint) * contrast).clamp(0.0, 1.0)
}

/// Measured darkness supported by original, strong-contrast and flat-contrast gray views.
/// Original contrast and local redness bound any boost, so a diagnostic view alone
/// cannot turn a clipped highlight or an arbitrary shadow into an accepted defect.
#[must_use]
pub fn darkness(background: f32, fine: f32, redness: f32, midpoint: f32, pedestal: f32) -> f32 {
    let original = (background - fine) / (background + pedestal).max(1e-6);
    if original <= 0.0 {
        return original;
    }
    let high_base = gray_view(background, midpoint, HIGH_CONTRAST);
    let high = (high_base - gray_view(fine, midpoint, HIGH_CONTRAST)) / (high_base + VIEW_PEDESTAL);
    let low_base = gray_view(background, midpoint, LOW_CONTRAST);
    let low = (low_base - gray_view(fine, midpoint, LOW_CONTRAST))
        / (low_base + VIEW_PEDESTAL)
        / LOW_CONTRAST;
    let high_support = (high - original)
        .max(0.0)
        .min(redness.max(0.0) * 4.0 + original * 0.5);
    let low_support = (low - original).max(0.0).min(original * 0.25);
    original + high_support + low_support
}
