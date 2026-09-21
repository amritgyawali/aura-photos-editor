//! What a photographer chose on the setup screen, and where it is kept.
//!
//! Three facts, and they are not the same kind of fact.
//!
//! **The key is not here.** It is in the operating system's credential store,
//! where phase 04 put it, and nothing in this module can read one. What is here
//! is which provider was chosen, where it lives when the photographer is allowed
//! to say, which models they picked, and whether the first-run screen has been
//! answered. All four are settings; none is a secret.
//!
//! **It is per catalog rather than per machine.** The `setting` table has been in
//! migration 0001 since phase 01 and nothing had ever written to it. That is the
//! right home: a studio with a wedding on an external drive and a second machine
//! in the edit suite opens the same catalog on both and finds the same provider
//! already chosen, and a catalog copied to a new machine carries the choice and
//! not the key - which is exactly the split that should exist between the two.
//!
//! **A finished setup and a skipped one are different rows.** `completed` says
//! the question was answered; the provider says what the answer was. A
//! photographer who pressed "not now" is not asked again on the next launch, and
//! a build that treated the two as one would either nag somebody who has decided
//! or silently hide the screen from somebody who has not.

use aura_cloud::catalog::ModelChoice;
use aura_cloud::contract::cloud::Tier;
use aura_cloud::provider::ProviderKind;
use aura_core::errors::db::statement_failed;
use aura_core::AuraResult;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// The key the whole record is filed under in the `setting` table.
pub const SETTING_KEY: &str = "ai.setup";

/// The AI provider choice, as it is stored and as the panel reads it back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AiSetup {
    /// The provider identifier, as [`ProviderKind::as_str`] spells it.
    pub provider: String,
    /// The address, for the providers whose address is the photographer's to set.
    pub endpoint: Option<String>,
    /// The model for the cheap tier, when one was named.
    pub cheap_model: Option<String>,
    /// The model for the balanced tier, when one was named.
    pub balanced_model: Option<String>,
    /// The model for the reasoning tier, when one was named.
    pub reasoning_model: Option<String>,
    /// True once the first-run screen has been answered, either way.
    pub completed: bool,
    /// True when it was answered by declining rather than by choosing.
    ///
    /// Kept separately from `completed` so the panel can offer "you skipped this
    /// - set it up now" rather than presenting a configured-looking Anthropic
    /// that has no key behind it.
    pub skipped: bool,
}

impl Default for AiSetup {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Anthropic.as_str().to_string(),
            endpoint: None,
            cheap_model: None,
            balanced_model: None,
            reasoning_model: None,
            completed: false,
            skipped: false,
        }
    }
}

impl AiSetup {
    /// The provider this record names.
    #[must_use]
    pub fn kind(&self) -> ProviderKind {
        ProviderKind::parse(&self.provider)
    }

    /// The model overrides this record carries.
    #[must_use]
    pub fn models(&self) -> ModelChoice {
        let mut chosen = ModelChoice::default();
        chosen.set(Tier::Cheap, self.cheap_model.as_deref());
        chosen.set(Tier::Balanced, self.balanced_model.as_deref());
        chosen.set(Tier::Reasoning, self.reasoning_model.as_deref());
        chosen
    }

    /// Replace the model overrides.
    pub fn set_models(&mut self, chosen: &ModelChoice) {
        self.cheap_model = chosen.for_tier(Tier::Cheap).map(ToString::to_string);
        self.balanced_model = chosen.for_tier(Tier::Balanced).map(ToString::to_string);
        self.reasoning_model = chosen.for_tier(Tier::Reasoning).map(ToString::to_string);
    }

    /// The endpoint, with blank text treated as absent.
    #[must_use]
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
    }
}

/// Read the stored choice, or the default when nothing has been stored.
///
/// A row that cannot be parsed is treated as absent rather than as an error. The
/// worst case is a photographer being shown the setup screen once more; the
/// alternative is a catalog whose AI panel refuses to open because a settings
/// blob from a future version has a field this build does not know.
///
/// # Errors
///
/// `AURA-DB-3006` when the catalog itself cannot be read.
pub fn read(conn: &Connection) -> AuraResult<AiSetup> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value_json FROM setting WHERE key = ?1",
            [SETTING_KEY],
            |row| row.get(0),
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(|error| statement_failed("could not read the AI setup row", &error))?;

    Ok(stored
        .as_deref()
        .and_then(|text| serde_json::from_str::<AiSetup>(text).ok())
        .unwrap_or_default())
}

/// Write the choice, replacing whatever was there.
///
/// # Errors
///
/// `AURA-DB-3006` when the row cannot be written.
pub fn write(conn: &Connection, setup: &AiSetup, now: &str) -> AuraResult<()> {
    let encoded = serde_json::to_string(setup).map_err(|error| {
        aura_core::errors::db::statement_failed(
            "could not encode the AI setup row",
            &rusqlite::Error::InvalidQuery,
        )
        .with_context("detail", error.to_string())
    })?;
    conn.execute(
        "INSERT INTO setting (key, value_json, updated_at) VALUES (?1, ?2, ?3)
           ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                          updated_at = excluded.updated_at",
        rusqlite::params![SETTING_KEY, encoded, now],
    )
    .map_err(|error| statement_failed("could not write the AI setup row", &error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory catalog");
        conn.execute(
            "CREATE TABLE setting (key TEXT NOT NULL PRIMARY KEY, value_json TEXT NOT NULL, \
             updated_at TEXT NOT NULL) STRICT",
            [],
        )
        .expect("the setting table");
        conn
    }

    #[test]
    fn nothing_stored_is_an_unanswered_setup_rather_than_an_error() {
        let conn = memory();
        let setup = read(&conn).expect("a default");
        assert!(!setup.completed);
        assert!(!setup.skipped);
        assert_eq!(setup.kind(), ProviderKind::Anthropic);
    }

    #[test]
    fn a_choice_survives_a_round_trip() {
        let conn = memory();
        let mut setup = AiSetup {
            provider: "groq".to_string(),
            endpoint: None,
            completed: true,
            ..AiSetup::default()
        };
        setup.set_models(&ModelChoice::uniform("llama-4-scout"));
        write(&conn, &setup, "2026-01-01T00:00:00Z").expect("a write");

        let read_back = read(&conn).expect("a read");
        assert_eq!(read_back, setup);
        assert_eq!(read_back.kind(), ProviderKind::Groq);
        assert_eq!(
            read_back.models().for_tier(Tier::Reasoning),
            Some("llama-4-scout")
        );
    }

    #[test]
    fn a_second_write_replaces_the_first() {
        let conn = memory();
        write(&conn, &AiSetup::default(), "2026-01-01T00:00:00Z").expect("a write");
        let second = AiSetup {
            provider: "ollama".to_string(),
            completed: true,
            ..AiSetup::default()
        };
        write(&conn, &second, "2026-01-02T00:00:00Z").expect("a second write");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM setting", [], |row| row.get(0))
            .expect("a count");
        assert_eq!(count, 1);
        assert_eq!(read(&conn).expect("a read").kind(), ProviderKind::Ollama);
    }

    #[test]
    fn an_unreadable_row_is_treated_as_no_row() {
        let conn = memory();
        conn.execute(
            "INSERT INTO setting VALUES (?1, '{not json', '2026-01-01T00:00:00Z')",
            [SETTING_KEY],
        )
        .expect("a broken row");
        assert!(!read(&conn).expect("a default").completed);
    }

    #[test]
    fn a_blank_endpoint_is_not_an_endpoint() {
        let setup = AiSetup {
            endpoint: Some("   ".to_string()),
            ..AiSetup::default()
        };
        assert_eq!(setup.endpoint(), None);
    }
}
