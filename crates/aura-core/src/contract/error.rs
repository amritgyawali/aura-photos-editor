//! FROZEN CONTRACT. Changes require an ADR per 13-CONTRACT-FREEZE-AND-ADR.md.
//!
//! Every fallible operation in AURA returns `Result<T, AuraError>`. There is no
//! other error type crossing a module boundary. A caller can always answer
//! three questions from the error alone: how bad is it, what should I do, and
//! what do I tell the photographer.

use std::fmt;

use serde::{Deserialize, Serialize};

/// How bad is it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational. The run continues and quality is unaffected.
    Warning,
    /// The run continues with reduced capability. Must be surfaced in the report.
    Degraded,
    /// One photo could not be processed. The other 3,999 continue.
    ItemFailed,
    /// This run cannot continue, but the catalog is intact and resumable.
    RunBlocking,
    /// The application cannot continue safely. Data integrity may be at risk.
    Fatal,
}

/// What the caller should do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recovery {
    /// Transient. Retry with backoff, bounded attempts.
    Retry,
    /// Use a lower-quality or slower path that is known to work.
    Fallback,
    /// Set this item aside with its reason and keep going.
    Quarantine,
    /// Stop the run cleanly. Persist state so resume is possible.
    Halt,
    /// Only a human can decide. Present the choice, never guess.
    AskUser,
}

/// `AURA-<DOMAIN>-<NNNN>`. Ranges are allocated in 11-ERROR-TAXONOMY.md:
/// IO 1000, RAW 2000, DB 3000, GPU 4000, ML 5000, CLOUD 6000, JOB 7000,
/// EXPORT 8000, SEC 9000.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ErrorCode(pub &'static str);

/// Codes are `&'static str` because they are compile-time constants everywhere in
/// the product. Deserialising one - from the event stream, a `task` row or a
/// diagnostics bundle - therefore has to intern the text. The interner is bounded
/// in practice by the size of the registry, and repeated codes reuse one entry.
impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        Ok(Self(intern(&text)))
    }
}

fn intern(code: &str) -> &'static str {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    static INTERNED: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());

    let mut interned = match INTERNED.lock() {
        Ok(guard) => guard,
        // A poisoned interner is still a correct interner: the set is only ever
        // inserted into, so recovering keeps every previously interned code.
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(found) = interned.get(code) {
        return found;
    }
    let leaked: &'static str = Box::leak(code.to_owned().into_boxed_str());
    interned.insert(leaked);
    leaked
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl ErrorCode {
    /// Domain segment, e.g. "IO".
    #[must_use]
    pub fn domain(&self) -> &'static str {
        self.0.split('-').nth(1).unwrap_or("UNKNOWN")
    }

    /// Public runbook URL shown in the UI and in support replies.
    #[must_use]
    pub fn runbook_url(&self) -> String {
        format!("https://aura.app/e/{}", self.0)
    }
}

/// The one error type. Construct it with the helpers, never with struct
/// literal syntax outside this crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuraError {
    /// Stable machine code, registered in `errors.toml`.
    pub code: ErrorCode,
    /// Blast radius of the failure.
    pub severity: Severity,
    /// The action the caller is expected to take.
    pub recovery: Recovery,
    /// Engineer-facing. May contain paths and technical detail. Never shown raw.
    pub detail: String,
    /// Photographer-facing. One sentence, no jargon, says what to do next.
    pub user_message: String,
    /// Structured context for logs. Keys are stable; values are redacted
    /// according to the data-class rules in 24-PRIVACY-CONSENT-AND-LEGAL.md.
    pub context: Vec<(String, String)>,
    /// Chained cause, formatted. We keep a string rather than a boxed error so
    /// the type stays Clone, Send, Sync and serialisable into the event stream.
    pub cause: Option<String>,
}

impl fmt::Display for AuraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.detail)?;
        if let Some(cause) = &self.cause {
            write!(f, " (cause: {cause})")?;
        }
        Ok(())
    }
}

impl std::error::Error for AuraError {}

impl AuraError {
    /// Build an error from its five mandatory parts.
    #[must_use]
    pub fn new(
        code: ErrorCode,
        severity: Severity,
        recovery: Recovery,
        detail: impl Into<String>,
        user_message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            recovery,
            detail: detail.into(),
            user_message: user_message.into(),
            context: Vec::new(),
            cause: None,
        }
    }

    /// Attach one stable key/value pair for the log line and the support bundle.
    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.push((key.into(), value.into()));
        self
    }

    /// Record the underlying cause as formatted text.
    #[must_use]
    pub fn with_cause(mut self, cause: &dyn std::error::Error) -> Self {
        self.cause = Some(cause.to_string());
        self
    }

    /// True when the caller may retry without human input.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self.recovery, Recovery::Retry)
    }

    /// True when the run must stop.
    #[must_use]
    pub fn stops_run(&self) -> bool {
        matches!(self.severity, Severity::RunBlocking | Severity::Fatal)
    }
}

/// The result type used by every fallible AURA operation.
pub type AuraResult<T> = Result<T, AuraError>;
