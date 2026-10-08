//! The licence and the trial, on this machine. ADR-0105.
//!
//! A new installation can use everything for [`aura_licence::TRIAL_DAYS`] days. After that,
//! **editing keeps working and only exporting finished photographs needs a licence** - a
//! photographer's catalogue and edits are never held hostage, and the one thing a paid product
//! sells, the delivered files, is what the key unlocks.
//!
//! The key and the day the trial began are kept in `licence.json` in the application's data
//! directory, because a licence belongs to the machine rather than to one wedding's catalogue. The
//! trial's start is also written into each catalogue opened, and the earliest date anywhere wins,
//! so deleting one file does not restart the trial. It is not copy protection and does not
//! pretend to be; `aura_licence` says why.
use aura_core::contract::error::{ErrorCode, Recovery, Severity};
use aura_core::{AuraError, AuraResult};
use aura_licence::{Licence, Standing};
use serde::{Deserialize, Serialize};
use time::Date;

use crate::commands::IpcResult;
use crate::AppState;

/// No licence and the trial is over, so nothing was exported.
pub const EXPORT_NEEDS_LICENCE: ErrorCode = ErrorCode("AURA-REL-12005");
/// A pasted licence key was refused.
pub const KEY_REFUSED: ErrorCode = ErrorCode("AURA-REL-12006");

const FILE: &str = "licence.json";
const TRIAL_SETTING: &str = "trial_started_v1";

/// What is kept on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    trial_started: Option<String>,
}

/// Where this installation stands, for the Licence panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenceStatus {
    /// `licensed`, `expired`, `trial` or `trial_ended`.
    pub state: String,
    /// Whether finished photographs can be exported. Editing never depends on this.
    pub may_export: bool,
    pub name: Option<String>,
    pub email: Option<String>,
    pub edition: Option<String>,
    /// The licence's last day, for a term licence.
    pub expires: Option<String>,
    /// Trial days left, including today.
    pub days_left: Option<i64>,
    /// The trial's last day.
    pub trial_ends: Option<String>,
    /// One sentence for the panel and the title bar.
    pub message: String,
}

fn ymd(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

fn parse(text: &str) -> Option<Date> {
    Date::parse(
        text,
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .ok()
}

fn read(state: &AppState) -> Stored {
    std::fs::read(state.licence_dir().join(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write(state: &AppState, stored: &Stored) -> AuraResult<()> {
    let folder = state.licence_dir().to_path_buf();
    std::fs::create_dir_all(&folder).map_err(|e| aura_core::errors::io::from_io(&e, &folder))?;
    let path = folder.join(FILE);
    let bytes =
        serde_json::to_vec_pretty(stored).map_err(|e| refused(KEY_REFUSED, &e.to_string()))?;
    std::fs::write(&path, bytes).map_err(|e| aura_core::errors::io::from_io(&e, &path))
}

fn refused(code: ErrorCode, message: &str) -> AuraError {
    AuraError::new(
        code,
        Severity::ItemFailed,
        Recovery::AskUser,
        message,
        message,
    )
}

/// The day the trial began: the earliest recorded anywhere, or today for a new installation.
/// Recorded in both places whenever it is read.
fn trial_started(state: &AppState, stored: &mut Stored, today: Date) -> Date {
    let from_catalog: Option<Date> = state
        .catalog()
        .read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT value_json FROM setting WHERE key = ?1",
                    [TRIAL_SETTING],
                    |row| row.get::<_, String>(0),
                )
                .ok())
        })
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str::<String>(&json).ok())
        .and_then(|text| parse(&text));
    let from_file = stored.trial_started.as_deref().and_then(parse);
    let start = [from_catalog, from_file, Some(today)]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(today);
    if from_file != Some(start) {
        stored.trial_started = Some(ymd(start));
        let _ = write(state, stored);
    }
    if from_catalog != Some(start) {
        let value = format!("\"{}\"", ymd(start));
        let now = aura_catalog::rfc3339(state.clock().now_utc());
        let _ = state.catalog().writer().with(move |conn| {
            conn.execute(
                "INSERT INTO setting (key, value_json, updated_at) VALUES (?1, ?2, ?3)
                   ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                                  updated_at = excluded.updated_at",
                rusqlite::params![TRIAL_SETTING, value, now],
            )
            .map_err(|e| {
                aura_core::errors::db::statement_failed("could not record the trial", &e)
            })?;
            Ok(())
        });
    }
    start
}

fn standing(state: &AppState) -> Standing {
    let today = state.clock().now_utc().date();
    let mut stored = read(state);
    let start = trial_started(state, &mut stored, today);
    let licence = stored
        .key
        .as_deref()
        .and_then(|k| aura_licence::decode(k).ok());
    Standing::of(licence, start, today)
}

fn describe(standing: &Standing) -> LicenceStatus {
    let base = LicenceStatus {
        state: String::new(),
        may_export: standing.may_export(),
        name: None,
        email: None,
        edition: None,
        expires: None,
        days_left: None,
        trial_ends: None,
        message: String::new(),
    };
    let with = |l: &Licence, state: &str, message: String| LicenceStatus {
        state: state.into(),
        name: Some(l.name.clone()),
        email: Some(l.email.clone()),
        edition: Some(l.edition.clone()),
        expires: l.expires.clone(),
        message,
        ..base.clone()
    };
    match standing {
        Standing::Licensed(l) => with(
            l,
            "licensed",
            match &l.expires {
                Some(last) => format!("Licensed to {} until {last}.", l.name),
                None => format!("Licensed to {}.", l.name),
            },
        ),
        Standing::LicenceExpired(l) => with(
            l,
            "expired",
            format!(
                "Your licence ended on {}. Editing still works; renew to export again.",
                l.expires.clone().unwrap_or_default()
            ),
        ),
        Standing::Trial { days_left, ends } => LicenceStatus {
            state: "trial".into(),
            days_left: Some(*days_left),
            trial_ends: Some(ymd(*ends)),
            message: format!(
                "Free trial: {days_left} day{} left. Everything works until {}.",
                if *days_left == 1 { "" } else { "s" },
                ymd(*ends)
            ),
            ..base
        },
        Standing::TrialEnded { ended } => LicenceStatus {
            state: "trial_ended".into(),
            trial_ends: Some(ymd(*ended)),
            message: "Your free trial has ended. Editing still works; enter a licence key to export finished photographs.".into(),
            ..base
        },
    }
}

/// Where this installation stands.
///
/// # Errors
/// None in practice; a missing or unreadable licence file is a new trial.
pub fn licence_status(state: &AppState) -> IpcResult<LicenceStatus> {
    Ok(describe(&standing(state)))
}

/// Check and keep a licence key.
///
/// # Errors
/// `AURA-REL-12006` with a sentence when the key is not valid or has already ended.
pub fn activate_licence(state: &AppState, key: &str) -> IpcResult<LicenceStatus> {
    let licence = aura_licence::decode(key).map_err(|e| refused(KEY_REFUSED, &e.to_string()))?;
    let today = state.clock().now_utc().date();
    if licence.expiry().is_some_and(|last| today > last) {
        return Err(refused(
            KEY_REFUSED,
            "That licence key has already ended. Renew it, or contact support with the key.",
        )
        .into());
    }
    let mut stored = read(state);
    stored.key = Some(key.chars().filter(|c| !c.is_whitespace()).collect());
    write(state, &stored)?;
    licence_status(state)
}

/// Remove the licence from this machine, e.g. before moving it to another one.
///
/// # Errors
/// The licence file cannot be written.
pub fn deactivate_licence(state: &AppState) -> IpcResult<LicenceStatus> {
    let mut stored = read(state);
    stored.key = None;
    write(state, &stored)?;
    licence_status(state)
}

/// Refuse to export when there is no licence and the trial is over.
///
/// # Errors
/// `AURA-REL-12005`, with the sentence the panel shows.
pub fn require_export(state: &AppState) -> AuraResult<()> {
    let standing = standing(state);
    if standing.may_export() {
        Ok(())
    } else {
        let message = describe(&standing).message;
        Err(AuraError::new(
            EXPORT_NEEDS_LICENCE,
            Severity::RunBlocking,
            Recovery::AskUser,
            "export refused: no active licence",
            message,
        ))
    }
}
