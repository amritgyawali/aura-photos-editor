//! A standalone Lightroom develop preset carrying the actual saved reference grade.
//! Lightroom's numeric develop controls alone cannot express AURA's palette map.
use std::fmt::Write as _;

use aura_core::PhotoId;
use aura_recipe::{reference_grade, Recipe};
use serde::{Deserialize, Serialize};

use super::{
    baseline, check_cancel, invalid, job, read, session_key, AppState, IpcResult, Session, ENGINE,
};

mod bake;
mod table;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetInput {
    pub photo_id: String,
    pub session_id: String,
    pub name: String,
    pub cancel_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetReport {
    pub xml: String,
    pub file_name: String,
    pub name: String,
    pub recipe_hash: String,
    pub table_fingerprint: String,
    pub table_edge: usize,
    /// Held-out table/evaluator distance in `OKLab` x100; not Lightroom output accuracy.
    pub mean_table_error: f32,
    pub p95_table_error: f32,
    pub max_table_error: f32,
    pub warnings: Vec<String>,
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn name(value: &str) -> aura_core::AuraResult<&str> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > 120
        || value.chars().any(|c| {
            let code = u32::from(c);
            !(0x20..=0xD7FF).contains(&code)
                && !(0xE000..=0xFFFD).contains(&code)
                && !(0x0001_0000..=0x0010_FFFF).contains(&code)
        })
    {
        return Err(invalid(
            "Give the Lightroom preset a name of 1 to 120 printable characters.",
        ));
    }
    Ok(value)
}

fn filename(value: &str) -> String {
    let clean: String = value
        .chars()
        .take(80)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("AURA-{clean}.xmp")
}

fn attr(out: &mut String, key: &str, value: &str) {
    let _ = writeln!(out, "    crs:{key}=\"{}\"", escape(value));
}

/// Explicit neutral values prevent a recipient photo's earlier creative settings
/// from leaking through a preset whose current AURA settings are neutral.
fn reset_missing(base: &str, recipe: &Recipe) -> String {
    let mut extra = String::new();
    for (key, value) in [
        ("ParametricShadows", 0),
        ("ParametricDarks", 0),
        ("ParametricLights", 0),
        ("ParametricHighlights", 0),
        ("ParametricShadowSplit", 25),
        ("ParametricMidtoneSplit", 50),
        ("ParametricHighlightSplit", 75),
        ("SplitToningShadowHue", 0),
        ("SplitToningShadowSaturation", 0),
        ("SplitToningHighlightHue", 0),
        ("SplitToningHighlightSaturation", 0),
        ("SplitToningBalance", 0),
        ("ColorGradeShadowLum", 0),
        ("ColorGradeHighlightLum", 0),
        ("ColorGradeMidtoneHue", 0),
        ("ColorGradeMidtoneSat", 0),
        ("ColorGradeMidtoneLum", 0),
        ("ColorGradeGlobalHue", 0),
        ("ColorGradeGlobalSat", 0),
        ("ColorGradeGlobalLum", 0),
        ("ColorGradeBlending", 50),
        ("ShadowTint", 0),
        ("RedHue", 0),
        ("RedSaturation", 0),
        ("GreenHue", 0),
        ("GreenSaturation", 0),
        ("BlueHue", 0),
        ("BlueSaturation", 0),
        ("GrainAmount", 0),
        ("GrainSize", recipe.global.effects.grain.size),
        ("GrainFrequency", recipe.global.effects.grain.roughness),
        ("PostCropVignetteAmount", 0),
    ] {
        if !base.contains(&format!("crs:{key}=")) {
            attr(&mut extra, key, &value.to_string());
        }
    }
    extra
}

fn document(recipe: &Recipe, title: &str, table: &table::Table) -> aura_core::AuraResult<String> {
    let mut portable = bake::portable_settings(recipe);
    // The existing writer's numeric subset is reused; no source path, sidecar
    // dependency, crop or per-image geometry belongs in a reusable preset.
    portable.engine = "AURA".into();
    let original = aura_recipe::xmp::write(&portable);
    let base = original
        .lines()
        .filter(|line| {
            !line.contains("aura:")
                && !line.contains("xmlns:aura")
                && !line.contains("crs:Crop")
                && !line.contains("crs:HasCrop")
                && !line.contains("crs:Temperature=")
                && !line.contains("crs:Tint=")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (head, body) = base
        .split_once("    >\n")
        .ok_or_else(|| invalid("Could not assemble the Lightroom preset."))?;
    let mut out = head.to_owned();
    out.push_str(&reset_missing(&base, recipe));
    let mut identity = table.fingerprint.clone();
    identity.push_str(title);
    let recipe_hash = aura_recipe::hash::recipe_hash(recipe)?;
    let uuid = format!(
        "{:x}",
        md5::compute(format!("preset:{identity}:{recipe_hash}").as_bytes())
    )
    .to_uppercase();
    let profile_uuid = format!(
        "{:x}",
        md5::compute(format!("profile:{identity}").as_bytes())
    )
    .to_uppercase();
    for (key, value) in [
        ("PresetType", "Normal"),
        ("UUID", uuid.as_str()),
        ("SupportsAmount", "False"),
        ("SupportsColor", "True"),
        ("SupportsMonochrome", "True"),
        ("SupportsHighDynamicRange", "False"),
        ("SupportsNormalDynamicRange", "True"),
        ("SupportsSceneReferred", "True"),
        ("SupportsOutputReferred", "True"),
        ("RequiresRGBTables", "False"),
        ("ShowInPresets", "True"),
        ("CameraModelRestriction", ""),
        ("HasSettings", "True"),
        ("ToneCurveName2012", "Custom"),
        ("WhiteBalance", "As Shot"),
        ("IncrementalTemperature", "0"),
        ("IncrementalTint", "0"),
    ] {
        attr(&mut out, key, value);
    }
    attr(
        &mut out,
        &format!("Table_{}", table.fingerprint),
        &table.encoded,
    );
    out.push_str("    >\n");
    let body = body.strip_suffix("<?xpacket end=\"w\"?>").unwrap_or(body);
    let (curves, ending) = body
        .split_once("  </rdf:Description>")
        .ok_or_else(|| invalid("Could not finish the Lightroom preset."))?;
    out.push_str(curves);
    for channel in ["Red", "Green", "Blue"] {
        let tag = format!("ToneCurvePV2012{channel}");
        if !base.contains(&format!("<crs:{tag}>")) {
            let _ = writeln!(out,"   <crs:{tag}><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:{tag}>");
        }
    }
    let title = escape(title);
    let _ = writeln!(out,"   <crs:Name><rdf:Alt><rdf:li xml:lang=\"x-default\">{title}</rdf:li></rdf:Alt></crs:Name>");
    out.push_str("   <crs:Group><rdf:Alt><rdf:li xml:lang=\"x-default\">AURA Matched Looks</rdf:li></rdf:Alt></crs:Group>\n");
    let _ = writeln!(out,"   <crs:Look><rdf:Description crs:Name=\"{title}\" crs:Amount=\"1\" crs:UUID=\"{profile_uuid}\" crs:SupportsAmount=\"False\" crs:SupportsColor=\"True\" crs:SupportsMonochrome=\"True\" crs:SupportsHighDynamicRange=\"False\" crs:SupportsNormalDynamicRange=\"True\" crs:SupportsSceneReferred=\"True\" crs:SupportsOutputReferred=\"True\">");
    let _ = writeln!(out,"    <crs:Parameters><rdf:Description crs:Version=\"15.0\" crs:ProcessVersion=\"11.0\" crs:ConvertToGrayscale=\"False\" crs:RGBTable=\"{}\"/></crs:Parameters>",table.fingerprint);
    out.push_str("   </rdf:Description></crs:Look>\n  </rdf:Description>");
    out.push_str(ending);
    out.push_str("<?xpacket end=\"w\"?>\n");
    Ok(out)
}

/// Generate without writing files or modifying the photograph, recipe or session.
/// # Errors
/// Refuses unapplied/stale matches, invalid names, cancellation and encoding errors.
pub fn generate(state: &AppState, input: &PresetInput) -> IpcResult<PresetReport> {
    job(state, &input.cancel_id, |cancel| {
        let title = name(&input.name)?;
        let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
        let session: Session = read(state, &session_key(&input.photo_id)?)?
            .filter(|s: &Session| {
                s.engine == ENGINE && s.id == input.session_id && s.photo_id == input.photo_id
            })
            .ok_or_else(|| {
                invalid("Apply a reference colour match before generating its Lightroom preset.")
            })?;
        let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
        let hash = aura_recipe::hash::recipe_hash(&recipe)?;
        if session.last_hash != hash || session.checked_hash.as_deref() != Some(hash.as_str()) {
            return Err(invalid("The saved edit changed or has not been checked. Match and apply this photo again before generating its preset."));
        }
        aura_recipe::schema::Validation::check(&recipe)?;
        // Zero strength can canonically remove the grade. Its checked saved
        // state is a legitimate identity look, not a missing/failed analysis.
        let grade = reference_grade::read(&recipe)?
            .unwrap_or_else(reference_grade::ReferenceGrade::identity);
        grade.validate()?;
        let colour = bake::colour_recipe(&recipe);
        let table = table::build_with(
            &mut |inputs| bake::colours(state, &colour, inputs),
            &mut || check_cancel(cancel),
        )?;
        let xml = document(&recipe, title, &table)?;
        baseline::verify_current(state, photo, &hash)?;
        check_cancel(cancel)?;
        let mut warnings = vec![
            "Lightroom uses different raw processing; identical pixels across applications are not guaranteed.".into(),
            "Local masks, face retouching and crop are not included in this colour preset.".into(),
            "Spatial highlights, shadows and detail controls use Lightroom's implementation and can differ from AURA.".into(),
        ];
        if recipe
            .extra
            .keys()
            .any(|key| key != reference_grade::KEY && key != aura_recipe::calibrated_look::KEY)
        {
            warnings.push("Other AURA-only tools are not included. The preset carries the saved reference grade and supported develop settings.".into());
        }
        if table.p95_error > 2.0 {
            warnings.push("This strong treatment has noticeable colour-table approximation error. Check the result in Lightroom.".into());
        }
        Ok(PresetReport {
            xml,
            file_name: filename(title),
            name: title.into(),
            recipe_hash: hash,
            table_fingerprint: table.fingerprint,
            table_edge: table::EDGE,
            mean_table_error: table.mean_error,
            p95_table_error: table.p95_error,
            max_table_error: table.max_error,
            warnings,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_escaped_and_files_do_not_depend_on_sidecars_or_geometry() {
        let mut recipe = aura_recipe::fixtures::neutral("test-preset", "test-camera");
        recipe.geometry.crop = [0.1, 0.2, 0.8, 0.9];
        let table =
            table::build(&reference_grade::ReferenceGrade::identity(), &mut || Ok(())).unwrap();
        let xml = document(&recipe, "Warm & <Soft> \"look\"", &table).unwrap();
        assert!(xml.contains("Warm &amp; &lt;Soft&gt; &quot;look&quot;"));
        assert!(
            !xml.contains("aura:Sidecar")
                && !xml.contains("crs:Crop")
                && !xml.contains("crs:Temperature=\"0\"")
        );
        assert!(xml.contains("crs:PresetType=\"Normal\"") && xml.contains("crs:RGBTable="));
        assert!(
            xml.contains("crs:SplitToningShadowHue=\"0\"")
                && xml.contains("<crs:ToneCurvePV2012Red>")
        );
        assert!(!xml.contains("crs:Temperature=") && !xml.contains("crs:Tint="));
        assert!(xml.contains("crs:WhiteBalance=\"As Shot\""));
        assert!(name("\0bad").is_err() && name(" ").is_err());
        assert_eq!(filename("../CON:*"), "AURA-___CON__.xmp");
    }
}
