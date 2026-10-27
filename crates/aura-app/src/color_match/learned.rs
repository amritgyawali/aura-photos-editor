//! The Match a look panel's route to a reference's own original (ADR-0161, ADR-0180).
//!
//! When the edited reference a photographer uploads was made from a photograph in the collection
//! being matched, the edit itself can be learned from the two - pixel for pixel - instead of
//! estimated from another scene's colours. On the looks harness that left an error of 0.58 where
//! the reference alone left 3.70. The search is the reference-first route's
//! (`original::learn`), with the original rendered the way this panel measures every target: as
//! it currently looks, without any reference look on it.
//!
//! One search per reference and collection is kept, keyed by the collection's size and by the
//! originals' own edits, so it is repeated only when a photograph is added or an original is
//! edited again.
use std::path::{Path, PathBuf};

use aura_core::progress::CancelToken;
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_recipe::calibrated_look::CalibratedLook;
use serde::{Deserialize, Serialize};

use super::gallery_fit::Allowance;
use super::{original, read, store, AppState};

/// Where an uploaded reference came from on disk, by reference id.
const SOURCE: &str = "colour_reference_source_v1:";
/// The search for a reference's original in one collection.
const SEARCH: &str = "colour_reference_original_v1:";
/// Bump when the search or the learned look changes meaning.
const VERSION: u32 = 1;

/// A look learned from the photographer's own edit of a photograph in the collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LearnedLook {
    /// The learned operation at full strength.
    pub model: CalibratedLook,
    /// The originals, as a photographer reads them.
    pub label: String,
    /// How far the look may move skin and clipping: as far as the edit did on its original.
    pub allowance: Allowance,
    /// Each original and the hash of its edit when the look was learned.
    pub originals: Vec<(String, String)>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Searched {
    version: u32,
    photos: usize,
    learned: Option<LearnedLook>,
    refused: Option<String>,
}

/// Remember where an uploaded reference came from, so its original can be looked for later.
pub(super) fn remember_source(state: &AppState, id: &str, path: &Path) -> AuraResult<()> {
    super::asset_key(id)?;
    store(
        state,
        &format!("{SOURCE}{id}"),
        &path.to_string_lossy().into_owned(),
    )
}

fn collection_size(state: &AppState, project: &ProjectId) -> AuraResult<usize> {
    let key = project.to_db();
    state.catalog().read(move |conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM photo WHERE project_id = ?1",
            [key],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| usize::try_from(n).unwrap_or(0))
        .map_err(|e| aura_core::errors::db::statement_failed("reference original search", &e))
    })
}

/// The hash of an original's own edit, without any reference look on it.
fn edit_hash(state: &AppState, photo: PhotoId) -> AuraResult<String> {
    let recipe =
        original::without_reference_looks(crate::develop_commands::load_or_neutral(state, photo)?);
    aura_recipe::hash::recipe_hash(&recipe)
}

fn still_current(state: &AppState, learned: &LearnedLook) -> bool {
    learned.originals.iter().all(|(photo, hash)| {
        PhotoId::from_db(photo)
            .ok()
            .and_then(|photo| edit_hash(state, photo).ok())
            .is_some_and(|now| &now == hash)
    })
}

/// The reference's own original in `project`, and the look learned from it - or why an original
/// that was found could not be used. `(None, None)` when the reference's file is gone or nothing
/// in the collection is its original.
pub(super) fn find(
    state: &AppState,
    cancel: &CancelToken,
    reference_id: &str,
    project: &ProjectId,
) -> AuraResult<(Option<LearnedLook>, Option<String>)> {
    super::asset_key(reference_id)?;
    let Some(source) = read::<String>(state, &format!("{SOURCE}{reference_id}"))? else {
        return Ok((None, None));
    };
    let path = PathBuf::from(source);
    if !path.is_file() {
        return Ok((None, None));
    }
    let key = format!("{SEARCH}{reference_id}:{}", project.to_db());
    let photos = collection_size(state, project)?;
    if let Some(saved) = read::<Searched>(state, &key)? {
        if saved.version == VERSION
            && saved.photos == photos
            && saved
                .learned
                .as_ref()
                .is_none_or(|learned| still_current(state, learned))
        {
            return Ok((saved.learned, saved.refused));
        }
    }
    let search = match original::learn(state, project, &[path], original::Baseline::Current, cancel)
    {
        Ok(search) => search,
        Err(error) if cancel.is_cancelled() => return Err(error),
        // An unreadable reference leaves matching to the reference alone; it is not remembered,
        // so a later attempt looks again.
        Err(error) => {
            tracing::debug!(code = %error.code, "Reference original not searched");
            return Ok((None, None));
        }
    };
    let learned = match search.learned {
        Some(found) => {
            let originals = found
                .photos
                .iter()
                .map(|photo| Ok((photo.to_db(), edit_hash(state, *photo)?)))
                .collect::<AuraResult<Vec<_>>>()?;
            let mut model = found.model.clone();
            model.amount = 1.0;
            Some(LearnedLook {
                label: found.label(),
                model,
                allowance: found.allowance,
                originals,
            })
        }
        None => None,
    };
    store(
        state,
        &key,
        &Searched {
            version: VERSION,
            photos,
            learned: learned.clone(),
            refused: search.refused.clone(),
        },
    )?;
    Ok((learned, search.refused))
}
