//! Durable reference names, groups, saved strengths, and references from edited previews.
use super::{
    asset_key, check_cancel, invalid, job, new_summary, persist_reference, AppState, AuraResult,
    Controls, Deserialize, IpcResult, OptionalExtension, PhotoId, Reference, ReferenceSummary,
    RenderLevel, RenderedData, Rgb8, ENGINE,
};

mod workflows;
pub(super) use workflows::ordered_references;
pub use workflows::{run as workflow, WorkflowInput, WorkflowSnapshot};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManageInput {
    pub reference_id: String,
    pub action: String,
    pub name: Option<String>,
    pub group: Option<String>,
    pub favorite: Option<bool>,
    pub default_controls: Option<Controls>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureInput {
    pub project_id: String,
    pub photo_id: String,
    pub name: String,
    pub cancel_id: String,
}

fn label(value: &str, limit: usize) -> AuraResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > limit || value.chars().any(char::is_control) {
        return Err(invalid(
            "Use a short reference name or group without control characters.",
        ));
    }
    Ok(value.into())
}

/// Update library metadata or remove a descriptor; source files and saved edits remain intact.
/// # Errors
/// Invalid metadata, missing reference or failed catalogue transaction.
pub fn manage(state: &AppState, input: &ManageInput) -> IpcResult<Option<ReferenceSummary>> {
    let key = asset_key(&input.reference_id)?;
    if !matches!(input.action.as_str(), "update" | "remove") {
        return Err(invalid("Unknown reference library action.").into());
    }
    let name = input.name.as_deref().map(|s| label(s, 120)).transpose()?;
    let group = input.group.as_deref().map(|s| label(s, 80)).transpose()?;
    if let Some(controls) = input.default_controls {
        controls.check()?;
    }
    let input = input.clone();
    let at = aura_catalog::rfc3339(state.clock().now_utc());
    Ok(state.catalog().writer().with(move |conn| {
        let text: Option<String> = conn
            .query_row(
                "SELECT value_json FROM setting WHERE key=?1",
                [&key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| aura_core::errors::db::statement_failed("read reference metadata", &e))?;
        if input.action == "remove" {
            conn.execute("DELETE FROM setting WHERE key=?1", [&key])
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("remove colour reference", &e)
                })?;
            return Ok(None);
        }
        let text = text.ok_or_else(|| invalid("This reference is no longer in the library."))?;
        if text.len() > 8 * 1024 * 1024 {
            return Err(invalid("The saved reference exceeds its size limit."));
        }
        let mut reference: Reference = serde_json::from_str(&text)
            .map_err(|_| invalid("The saved colour reference is unreadable."))?;
        if reference.engine != ENGINE {
            return Err(invalid(
                "Import this reference again to update its analysis.",
            ));
        }
        if let Some(name) = name {
            reference.summary.name = name;
        }
        if let Some(group) = group {
            reference.summary.group = group;
        }
        if let Some(favorite) = input.favorite {
            reference.summary.favorite = favorite;
        }
        if let Some(controls) = input.default_controls {
            reference.summary.default_controls = Some(controls);
        }
        let text = serde_json::to_string(&reference)
            .map_err(|_| invalid("Could not save reference metadata."))?;
        conn.execute(
            "UPDATE setting SET value_json=?1,updated_at=?2 WHERE key=?3",
            rusqlite::params![text, at, key],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("save reference metadata", &e))?;
        Ok(Some(reference.summary))
    })?)
}

/// Create a reusable reference from the current edited preview, including crop and retouch.
/// # Errors
/// Invalid membership/name, missing source, render/cancellation or catalogue failure.
pub fn capture(state: &AppState, input: &CaptureInput) -> IpcResult<ReferenceSummary> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let name = label(&input.name, 120)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
    job(state, &input.cancel_id, |cancel| {
        let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
        let image = crate::preview_render::render(
            state,
            photo,
            recipe,
            RenderLevel::Full,
            aura_render::OutputColour::Srgb,
            true,
        )?;
        check_cancel(cancel)?;
        let RenderedData::Eight(pixels) = image.data else {
            return Err(invalid("Reference creation requires sRGB preview output."));
        };
        let rgb = image::RgbImage::from_raw(image.width, image.height, pixels)
            .ok_or_else(|| invalid("The edited preview could not be read."))?;
        let small = image::DynamicImage::ImageRgb8(rgb)
            .thumbnail(512, 512)
            .to_rgb8();
        let mut digest = blake3::Hasher::new();
        digest.update(b"edited-preview-reference-v1");
        digest.update(&small.width().to_le_bytes());
        digest.update(&small.height().to_le_bytes());
        digest.update(small.as_raw());
        let summary = new_summary(
            digest.finalize().to_hex().to_string(),
            name,
            image.width,
            image.height,
            false,
        );
        let pixels = Rgb8 {
            width: small.width(),
            height: small.height(),
            data: small.into_raw(),
        };
        persist_reference(state, summary, &pixels, cancel)
    })
}
