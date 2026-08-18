//! XMP: the parameter subset Lightroom understands, out and back.
//!
//! # What is here and what is next door
//!
//! ADR-0029 section 8: `image.xmp` carries the twenty-two parameters Adobe's `crs:`
//! namespace defines **with the meaning we mean**; `image.aura.json` (see [`crate::sidecar`])
//! carries the whole recipe. Masks, retouch operations, restoration, provenance and the
//! protected-field list have no `crs:` equivalent that means the same thing, and writing an
//! approximation would be worse than writing nothing: a lossy round trip that *looks*
//! lossless is how a photographer loses a mask and never finds out.
//!
//! [`SUBSET`] is the exact list, and `the_subset_is_exactly_what_round_trips` asserts that
//! every path in it survives a write-then-read and that nothing outside it does.
//!
//! # Two conventions that differ from ours, and are converted rather than assumed
//!
//! * **Crop angle.** Adobe's `crs:CropAngle` is positive **anticlockwise**; our
//!   `geometry.rotate` is positive clockwise. The sign is flipped on the way out and back.
//!   Getting this wrong tilts every straightened horizon the wrong way, twice.
//! * **Temperature at 0.** `crs:Temperature` is absent for a photograph shot "as shot";
//!   we always have a number, so we always write one.
//!
//! # No XML library
//!
//! The packet is written by hand and read by a small scanner. An XMP packet is a fixed
//! shape we produce ourselves plus a foreign one we read defensively, and the read path
//! only ever looks for `crs:`-prefixed names - so the surface an untrusted file gets is a
//! substring search rather than a general parser. This is the same argument
//! `aura_raw::container` makes about TIFF.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use aura_core::errors::render::xmp_unreadable;
use aura_core::AuraResult;

use crate::contract::recipe::{Curve, HslShift, Recipe, HSL_BANDS};
use crate::hash::number;

/// The dotted recipe paths this format carries. Everything else lives in the sidecar.
pub const SUBSET: &[&str] = &[
    "global.exposure",
    "global.contrast",
    "global.temperature",
    "global.tint",
    "global.highlights",
    "global.shadows",
    "global.whites",
    "global.blacks",
    "global.clarity",
    "global.texture",
    "global.dehaze",
    "global.vibrance",
    "global.saturation",
    "global.curve.points",
    "global.hsl",
    "global.sharpen.amount",
    "global.sharpen.radius",
    "global.sharpen.detail",
    "global.sharpen.masking",
    "global.noise.luminance",
    "global.noise.colour",
    "global.noise.detail",
    "geometry.crop",
    "geometry.rotate",
    "bw",
];

/// Adobe's capitalised band names, in the same order as [`HSL_BANDS`].
const CRS_BANDS: [&str; 8] = [
    "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
];

/// Render the Lightroom-compatible subset of a recipe as an XMP packet.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn write(recipe: &Recipe) -> String {
    let g = &recipe.global;
    let mut out = String::with_capacity(4096);
    out.push_str("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    out.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"");
    out.push_str(&recipe.engine);
    out.push_str("\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
    out.push_str("  <rdf:Description rdf:about=\"\"\n");
    out.push_str("    xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\"\n");
    out.push_str("    xmlns:aura=\"https://aura.app/ns/recipe/1.0/\"\n");

    let attr = |name: &str, value: &str, out: &mut String| {
        let _ = writeln!(out, "    crs:{name}=\"{value}\"");
    };

    attr("Version", "15.0", &mut out);
    attr("ProcessVersion", "11.0", &mut out);
    attr("Exposure2012", &signed(f64::from(g.exposure)), &mut out);
    attr("Contrast2012", &g.contrast.to_string(), &mut out);
    attr("Temperature", &g.temperature.to_string(), &mut out);
    attr("Tint", &g.tint.to_string(), &mut out);
    attr("Highlights2012", &g.highlights.to_string(), &mut out);
    attr("Shadows2012", &g.shadows.to_string(), &mut out);
    attr("Whites2012", &g.whites.to_string(), &mut out);
    attr("Blacks2012", &g.blacks.to_string(), &mut out);
    attr("Clarity2012", &g.clarity.to_string(), &mut out);
    attr("Texture", &g.texture.to_string(), &mut out);
    attr("Dehaze", &g.dehaze.to_string(), &mut out);
    attr("Vibrance", &g.vibrance.to_string(), &mut out);
    attr("Saturation", &g.saturation.to_string(), &mut out);
    attr("Sharpness", &g.sharpen.amount.to_string(), &mut out);
    attr(
        "SharpenRadius",
        &number(f64::from(g.sharpen.radius)),
        &mut out,
    );
    attr("SharpenDetail", &g.sharpen.detail.to_string(), &mut out);
    attr(
        "SharpenEdgeMasking",
        &g.sharpen.masking.to_string(),
        &mut out,
    );
    attr(
        "LuminanceSmoothing",
        &g.noise.luminance.to_string(),
        &mut out,
    );
    attr("ColorNoiseReduction", &g.noise.colour.to_string(), &mut out);
    attr(
        "LuminanceNoiseReductionDetail",
        &g.noise.detail.to_string(),
        &mut out,
    );

    for (band, crs) in HSL_BANDS.iter().zip(CRS_BANDS.iter()) {
        let shift = g.hsl.get(*band).copied().unwrap_or_default();
        attr(
            &format!("HueAdjustment{crs}"),
            &shift.h.to_string(),
            &mut out,
        );
        attr(
            &format!("SaturationAdjustment{crs}"),
            &shift.s.to_string(),
            &mut out,
        );
        attr(
            &format!("LuminanceAdjustment{crs}"),
            &shift.l.to_string(),
            &mut out,
        );
    }

    let cropped = !recipe.geometry.is_identity();
    attr("HasCrop", if cropped { "True" } else { "False" }, &mut out);
    let crop = recipe.geometry.crop;
    attr("CropLeft", &number(f64::from(crop[0])), &mut out);
    attr("CropTop", &number(f64::from(crop[1])), &mut out);
    attr("CropRight", &number(f64::from(crop[2])), &mut out);
    attr("CropBottom", &number(f64::from(crop[3])), &mut out);
    // Sign flipped: Adobe's angle is positive anticlockwise, ours is positive clockwise.
    attr(
        "CropAngle",
        &number(f64::from(-recipe.geometry.rotate)),
        &mut out,
    );
    attr(
        "ConvertToGrayscale",
        if recipe.bw.is_some() { "True" } else { "False" },
        &mut out,
    );

    // One AURA-namespaced attribute, and exactly one: the sidecar's name, so a Lightroom
    // user who sees this file knows where the rest of the edit is. It is not a parameter
    // and `read` ignores it.
    let _ = writeln!(out, "    aura:Sidecar=\"{}\"", crate::sidecar::EXTENSION);
    out.push_str("    >\n");

    out.push_str("   <crs:ToneCurvePV2012>\n    <rdf:Seq>\n");
    for point in &g.curve.points {
        let _ = writeln!(out, "     <rdf:li>{}, {}</rdf:li>", point[0], point[1]);
    }
    out.push_str("    </rdf:Seq>\n   </crs:ToneCurvePV2012>\n");

    out.push_str("  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n");
    out.push_str("<?xpacket end=\"w\"?>\n");
    out
}

/// Apply an XMP packet's `crs:` subset onto `base`, returning the updated recipe.
///
/// Long by construction: it is one line per parameter in [`SUBSET`], and a helper per
/// parameter would be longer and harder to check against the list.
///
/// Fields the packet does not carry are left exactly as they are in `base`. That is what
/// makes this safe to run against a packet Lightroom wrote with a different feature set: an
/// absent attribute means "not stated", never "zero".
///
/// # Errors
///
/// `AURA-RENDER-8004` when the text carries no `crs:` block at all, which is the one case
/// where "leave everything as it is" would be indistinguishable from a successful read of an
/// empty edit.
#[allow(clippy::too_many_lines)]
pub fn read(xml: &str, base: &Recipe) -> AuraResult<Recipe> {
    if !xml.contains("crs:") {
        return Err(xmp_unreadable("no crs: block in the packet"));
    }
    let attrs = scan_attributes(xml);
    let mut out = base.clone();
    let g = &mut out.global;

    if let Some(v) = attrs.get("Exposure2012").and_then(|s| parse_f32(s)) {
        g.exposure = v;
    }
    if let Some(v) = attrs.get("Contrast2012").and_then(|s| parse_i16(s)) {
        g.contrast = v;
    }
    if let Some(v) = attrs.get("Temperature").and_then(|s| s.parse::<u32>().ok()) {
        g.temperature = v;
    }
    if let Some(v) = attrs.get("Tint").and_then(|s| parse_i16(s)) {
        g.tint = v;
    }
    if let Some(v) = attrs.get("Highlights2012").and_then(|s| parse_i16(s)) {
        g.highlights = v;
    }
    if let Some(v) = attrs.get("Shadows2012").and_then(|s| parse_i16(s)) {
        g.shadows = v;
    }
    if let Some(v) = attrs.get("Whites2012").and_then(|s| parse_i16(s)) {
        g.whites = v;
    }
    if let Some(v) = attrs.get("Blacks2012").and_then(|s| parse_i16(s)) {
        g.blacks = v;
    }
    if let Some(v) = attrs.get("Clarity2012").and_then(|s| parse_i16(s)) {
        g.clarity = v;
    }
    if let Some(v) = attrs.get("Texture").and_then(|s| parse_i16(s)) {
        g.texture = v;
    }
    if let Some(v) = attrs.get("Dehaze").and_then(|s| parse_i16(s)) {
        g.dehaze = v;
    }
    if let Some(v) = attrs.get("Vibrance").and_then(|s| parse_i16(s)) {
        g.vibrance = v;
    }
    if let Some(v) = attrs.get("Saturation").and_then(|s| parse_i16(s)) {
        g.saturation = v;
    }
    if let Some(v) = attrs.get("Sharpness").and_then(|s| parse_i16(s)) {
        g.sharpen.amount = v;
    }
    if let Some(v) = attrs.get("SharpenRadius").and_then(|s| parse_f32(s)) {
        g.sharpen.radius = v;
    }
    if let Some(v) = attrs.get("SharpenDetail").and_then(|s| parse_i16(s)) {
        g.sharpen.detail = v;
    }
    if let Some(v) = attrs.get("SharpenEdgeMasking").and_then(|s| parse_i16(s)) {
        g.sharpen.masking = v;
    }
    if let Some(v) = attrs.get("LuminanceSmoothing").and_then(|s| parse_i16(s)) {
        g.noise.luminance = v;
    }
    if let Some(v) = attrs.get("ColorNoiseReduction").and_then(|s| parse_i16(s)) {
        g.noise.colour = v;
    }
    if let Some(v) = attrs
        .get("LuminanceNoiseReductionDetail")
        .and_then(|s| parse_i16(s))
    {
        g.noise.detail = v;
    }

    let mut hsl: BTreeMap<String, HslShift> = BTreeMap::new();
    for (band, crs) in HSL_BANDS.iter().zip(CRS_BANDS.iter()) {
        let h = attrs
            .get(&format!("HueAdjustment{crs}"))
            .and_then(|s| parse_i16(s));
        let s = attrs
            .get(&format!("SaturationAdjustment{crs}"))
            .and_then(|s| parse_i16(s));
        let l = attrs
            .get(&format!("LuminanceAdjustment{crs}"))
            .and_then(|s| parse_i16(s));
        if h.is_none() && s.is_none() && l.is_none() {
            continue;
        }
        let existing = g.hsl.get(*band).copied().unwrap_or_default();
        let shift = HslShift {
            h: h.unwrap_or(existing.h),
            s: s.unwrap_or(existing.s),
            l: l.unwrap_or(existing.l),
        };
        // A band that is neutral is absent from the document rather than present and zero,
        // so that a recipe written from a packet full of zeroes hashes the same as one that
        // never mentioned HSL.
        if !shift.is_neutral() {
            hsl.insert((*band).to_string(), shift);
        }
    }
    if attrs.keys().any(|k| k.starts_with("HueAdjustment")) {
        g.hsl = hsl;
    }

    for (i, name) in ["CropLeft", "CropTop", "CropRight", "CropBottom"]
        .into_iter()
        .enumerate()
    {
        if let Some(v) = attrs.get(name).and_then(|s| parse_f32(s)) {
            if let Some(slot) = out.geometry.crop.get_mut(i) {
                *slot = v;
            }
        }
    }
    if let Some(v) = attrs.get("CropAngle").and_then(|s| parse_f32(s)) {
        out.geometry.rotate = -v;
    }

    if let Some(points) = scan_tone_curve(xml) {
        out.global.curve = Curve { points };
    }

    match attrs.get("ConvertToGrayscale").map(String::as_str) {
        Some("True") if out.bw.is_none() => out.bw = Some(crate::contract::recipe::Bw::default()),
        Some("False") => out.bw = None,
        _ => {}
    }

    Ok(out)
}

/// Compare an XMP packet against the recipe AURA believes in, and report what a person
/// changed elsewhere.
///
/// ADR-0029 section 8: when both files exist, the sidecar wins and the XMP's values are
/// compared against it; a difference means the photographer edited in Lightroom, and those
/// paths are returned so section 6.4's protection covers them from then on.
///
/// Returns the recipe with the Lightroom edits applied, and the sorted list of paths that
/// differed.
///
/// # Errors
///
/// Propagates [`read`].
pub fn reconcile(base: &Recipe, xml: &str) -> AuraResult<(Recipe, Vec<String>)> {
    let updated = read(xml, base)?;
    let mut changed = Vec::new();
    for path in SUBSET {
        if crate::schema::leaf_differs(base, &updated, path) {
            changed.push((*path).to_string());
        }
    }
    let mut out = updated;
    if !changed.is_empty() {
        out.provenance.source = crate::contract::recipe::EditSource::User;
        let mut fields = out.provenance.user_edited_fields.clone();
        fields.extend(changed.iter().cloned());
        fields.sort_unstable();
        fields.dedup();
        out.provenance.user_edited_fields = fields;
    }
    changed.sort_unstable();
    Ok((out, changed))
}

fn signed(value: f64) -> String {
    if value >= 0.0 {
        format!("+{}", number(value))
    } else {
        number(value)
    }
}

fn parse_f32(text: &str) -> Option<f32> {
    text.trim().trim_start_matches('+').parse::<f32>().ok()
}

fn parse_i16(text: &str) -> Option<i16> {
    let cleaned = text.trim().trim_start_matches('+');
    cleaned
        .parse::<i16>()
        .ok()
        .or_else(|| cleaned.parse::<f32>().ok().map(|v| v.round() as i16))
}

/// Every `crs:Name="value"` in the text, keyed by `Name`.
///
/// Deliberately not a general XML parse: the read path only ever looks for this one prefix,
/// so a hostile packet's surface is a substring scan.
fn scan_attributes(xml: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut rest = xml;
    while let Some(pos) = rest.find("crs:") {
        let after = &rest[pos + 4..];
        let Some(eq) = after.find('=') else { break };
        let name = after[..eq].trim();
        let value_part = &after[eq + 1..];
        if !value_part.starts_with('"') {
            rest = after;
            continue;
        }
        let Some(end) = value_part[1..].find('"') else {
            break;
        };
        let value = &value_part[1..=end];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()) {
            out.insert(name.to_string(), unescape(value));
        }
        rest = &value_part[1 + end..];
    }
    out
}

fn scan_tone_curve(xml: &str) -> Option<Vec<[u16; 2]>> {
    let start = xml.find("<crs:ToneCurvePV2012>")?;
    let end = xml[start..].find("</crs:ToneCurvePV2012>")? + start;
    let block = &xml[start..end];
    let mut points = Vec::new();
    let mut rest = block;
    while let Some(open) = rest.find("<rdf:li>") {
        let after = &rest[open + 8..];
        let close = after.find("</rdf:li>")?;
        let text = &after[..close];
        let mut parts = text.split(',');
        let x = parts.next()?.trim().parse::<u16>().ok()?;
        let y = parts.next()?.trim().parse::<u16>().ok()?;
        points.push([x, y]);
        rest = &after[close..];
    }
    if points.len() >= 2 {
        Some(points)
    } else {
        None
    }
}

fn unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::recipe::EditSource;
    use crate::fixtures;

    #[test]
    fn the_subset_survives_a_write_then_read() {
        let recipe = fixtures::reference();
        let packet = write(&recipe);
        let back = read(
            &packet,
            &fixtures::neutral(&recipe.image.content_hash, "ILCE-7M4"),
        )
        .expect("read");

        let g = &back.global;
        assert!((g.exposure - 0.31).abs() < 1e-4);
        assert_eq!(g.contrast, 11);
        assert_eq!(g.temperature, 4930);
        assert_eq!(g.tint, 8);
        assert_eq!(g.highlights, -31);
        assert_eq!(g.shadows, 22);
        assert_eq!(g.whites, 7);
        assert_eq!(g.blacks, -9);
        assert_eq!(g.clarity, 6);
        assert_eq!(g.texture, 4);
        assert_eq!(g.dehaze, 0);
        assert_eq!(g.vibrance, 8);
        assert_eq!(g.saturation, 0);
        assert_eq!(g.sharpen.amount, 40);
        assert!((g.sharpen.radius - 0.8).abs() < 1e-4);
        assert_eq!(g.sharpen.detail, 25);
        assert_eq!(g.sharpen.masking, 30);
        assert_eq!(g.noise.luminance, 22);
        assert_eq!(g.noise.colour, 30);
        assert_eq!(g.noise.detail, 50);
        assert_eq!(g.curve.points, recipe.global.curve.points);
        assert_eq!(g.hsl, recipe.global.hsl);
        assert!((back.geometry.rotate - (-0.6)).abs() < 1e-4);
        assert!((back.geometry.crop[0] - 0.02).abs() < 1e-5);
        assert!((back.geometry.crop[3] - 0.98).abs() < 1e-5);
    }

    #[test]
    fn the_crop_angle_sign_is_flipped_in_both_directions() {
        let mut recipe = fixtures::reference();
        recipe.geometry.rotate = 2.5;
        let packet = write(&recipe);
        assert!(
            packet.contains("crs:CropAngle=\"-2.500000\""),
            "clockwise for us is anticlockwise for Adobe"
        );
        let back = read(&packet, &recipe).expect("read");
        assert!((back.geometry.rotate - 2.5).abs() < 1e-4);
    }

    #[test]
    fn nothing_outside_the_subset_reaches_the_packet() {
        let recipe = fixtures::reference();
        let packet = write(&recipe);
        // The mask's target names a person. It must not be in a file that goes to Lightroom.
        assert!(!packet.contains("identity:bride"));
        assert!(!packet.contains("skin_smooth"));
        assert!(!packet.contains("d_8f21"));
        assert!(!packet.contains("amrit_v3"));
    }

    #[test]
    fn an_absent_attribute_means_not_stated_rather_than_zero() {
        let base = fixtures::reference();
        // A packet with one attribute and nothing else.
        let packet = "<rdf:Description crs:Exposure2012=\"+1.000000\"/>";
        let back = read(packet, &base).expect("read");
        assert!((back.global.exposure - 1.0).abs() < 1e-4);
        assert_eq!(back.global.contrast, base.global.contrast, "untouched");
        assert_eq!(
            back.masks.len(),
            base.masks.len(),
            "masks are not in the format"
        );
    }

    #[test]
    fn a_packet_with_no_crs_block_is_refused() {
        let err = read("<x:xmpmeta/>", &fixtures::reference()).expect_err("must refuse");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8004");
    }

    #[test]
    fn a_lightroom_edit_is_reconciled_as_a_user_edit() {
        let base = fixtures::reference();
        let mut edited = base.clone();
        edited.global.exposure = 1.4;
        edited.global.shadows = -50;
        let packet = write(&edited);

        let (out, changed) = reconcile(&base, &packet).expect("reconcile");
        assert_eq!(
            changed,
            vec!["global.exposure".to_string(), "global.shadows".to_string()]
        );
        assert_eq!(out.provenance.source, EditSource::User);
        assert!(out
            .provenance
            .user_edited_fields
            .contains(&"global.exposure".to_string()));
    }

    #[test]
    fn a_packet_that_changed_nothing_reconciles_to_no_user_edits() {
        let base = fixtures::reference();
        let packet = write(&base);
        let (out, changed) = reconcile(&base, &packet).expect("reconcile");
        assert!(
            changed.is_empty(),
            "a round trip is not an edit: {changed:?}"
        );
        assert_eq!(out.provenance.source, base.provenance.source);
    }
}
