//! What the application is busy with right now, for the status bar every screen shows.
//!
//! A long job that says nothing looks the same as a job that has stopped. Every piece of work
//! a photographer could be waiting on - a folder run, an export, an Auto edit, a retouch, an
//! AI request - registers itself here for as long as it runs, says how far it has got and on
//! which photograph, and disappears when it ends, however it ends: the registration is a guard
//! dropped with the work. `activity_status` reads all of it at once, together with how many
//! image workers are in use, so the window can show "idle" only when nothing is running.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use serde::Serialize;

use crate::AppState;

/// One piece of work in progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItemDto {
    pub id: u64,
    /// `folder`, `export`, `edit`, `retouch`, `ai`, `import`.
    pub kind: String,
    /// What is being done, in a photographer's words.
    pub label: String,
    /// The photograph or step it is on now, when there is one.
    pub detail: String,
    pub done: u64,
    pub total: u64,
    /// How long it has been running, in seconds.
    pub seconds: u64,
}

/// Everything the application is doing, at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDto {
    pub items: Vec<ActivityItemDto>,
    /// Full-size image passes running now, and how many may run at once.
    pub image_workers_busy: usize,
    pub image_workers: usize,
}

#[derive(Debug, Clone)]
struct Entry {
    kind: &'static str,
    label: String,
    detail: String,
    done: u64,
    total: u64,
    started_ms: u64,
}

static NEXT: AtomicU64 = AtomicU64::new(1);
static ITEMS: Mutex<BTreeMap<u64, Entry>> = Mutex::new(BTreeMap::new());

/// A registration that lasts as long as the work does.
#[derive(Debug)]
#[must_use = "the activity ends as soon as this is dropped"]
pub struct Busy {
    id: u64,
}

/// Register a piece of work.
pub fn begin(state: &AppState, kind: &'static str, label: impl Into<String>) -> Busy {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    ITEMS.lock().insert(
        id,
        Entry {
            kind,
            label: label.into(),
            detail: String::new(),
            done: 0,
            total: 0,
            started_ms: state.clock().monotonic_ms(),
        },
    );
    Busy { id }
}

impl Busy {
    /// How far it has got.
    pub fn progress(&self, done: u64, total: u64) {
        if let Some(entry) = ITEMS.lock().get_mut(&self.id) {
            entry.done = done;
            entry.total = total;
        }
    }

    /// The photograph or step it is on now.
    pub fn detail(&self, detail: impl Into<String>) {
        if let Some(entry) = ITEMS.lock().get_mut(&self.id) {
            entry.detail = detail.into();
        }
    }

    /// A new description of the whole piece of work.
    pub fn label(&self, label: impl Into<String>) {
        if let Some(entry) = ITEMS.lock().get_mut(&self.id) {
            entry.label = label.into();
        }
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        ITEMS.lock().remove(&self.id);
    }
}

/// The file name of a photograph, for a status line; its id when the catalog cannot say.
#[must_use]
pub fn photo_name(state: &AppState, photo_id: &str) -> String {
    state
        .catalog()
        .read(|conn| {
            conn.query_row(
                "SELECT file_name FROM photo_file WHERE photo_id=?1
                  ORDER BY role = 'primary' DESC, file_id LIMIT 1",
                [photo_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("activity photo name", &e))
        })
        .unwrap_or_else(|_| photo_id.to_owned())
}

/// Everything in progress now, oldest first.
#[must_use]
pub fn activity_status(state: &AppState) -> ActivityDto {
    let now = state.clock().monotonic_ms();
    let mut items: Vec<ActivityItemDto> = crate::one_click_commands::active_runs(now);
    items.extend(
        state
            .running_imports()
            .into_iter()
            .map(|(done, total)| ActivityItemDto {
                id: 0,
                kind: "import".into(),
                label: "Importing photographs".into(),
                detail: format!("{done} of {total} files read"),
                done,
                total,
                seconds: 0,
            }),
    );
    items.extend(ITEMS.lock().iter().map(|(id, e)| ActivityItemDto {
        id: *id,
        kind: e.kind.to_owned(),
        label: e.label.clone(),
        detail: e.detail.clone(),
        done: e.done,
        total: e.total,
        seconds: now.saturating_sub(e.started_ms) / 1000,
    }));
    let gate = state.image_work_gate();
    ActivityDto {
        items,
        image_workers_busy: gate.busy(),
        image_workers: gate.permits(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(&dir.path().join("catalog.aura")).unwrap();
        (dir, state)
    }

    #[test]
    fn work_is_listed_while_it_runs_and_gone_when_it_ends() {
        let (_dir, state) = state();
        let busy = begin(&state, "export", "Exporting 3 photographs");
        busy.progress(1, 3);
        busy.detail("IMG_0002.jpg");
        let now = activity_status(&state);
        let item = now
            .items
            .iter()
            .find(|i| i.label == "Exporting 3 photographs")
            .unwrap();
        assert_eq!(
            (item.done, item.total, item.detail.as_str()),
            (1, 3, "IMG_0002.jpg")
        );
        assert_eq!(item.kind, "export");
        let id = item.id;
        drop(busy);
        assert!(activity_status(&state).items.iter().all(|i| i.id != id));
    }

    #[test]
    fn image_workers_are_reported() {
        let (_dir, state) = state();
        let status = activity_status(&state);
        assert!(status.image_workers >= 1);
        let held = state.image_work_gate().acquire();
        assert!(activity_status(&state).image_workers_busy >= 1);
        drop(held);
    }
}
