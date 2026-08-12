//! Recorded provider responses, so CI can test the whole gateway with no network.
//!
//! Acceptance criterion 7 of section 13 is "CI contains no network access and
//! still fully tests the gateway via cassettes". That is not only a convenience:
//! a test suite that talks to a provider is a test suite that fails when someone
//! else's service has a bad afternoon, that costs money on every run, and that
//! cannot be run on a train.
//!
//! ## Matching, and why not by digest
//!
//! The obvious design keys a cassette on a hash of the request. It is also the
//! wrong one here. Every request carries base64 images, so the digest changes if
//! a single pixel of a fixture moves - and a human maintaining the cassette
//! cannot compute the new digest by reading the file. The result is a suite that
//! nobody can update, and cassettes that get deleted rather than fixed.
//!
//! So a cassette declares a **matcher** instead: a URL suffix and a list of
//! strings the request body must contain. It is readable, it is stable across
//! fixture churn, and it is precise enough - `"model":"claude-sonnet-4-5"` plus
//! `mehndi` picks exactly one recording. Matching is by cassette name in sorted
//! order, so two matchers that overlap resolve the same way on every machine.
//!
//! A cassette may also pin `request_digest`, and the strict tests do, because
//! "the request bytes are exactly these" is precisely what a wire-format
//! regression test wants to assert.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::Duration;

use aura_core::errors::cloud::{provider_error, unreachable};
use aura_core::AuraResult;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::provider::{HttpRequest, HttpResponse, Transport};

/// What a recording must see before it answers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Matcher {
    /// The request URL must end with this, e.g. `/v1/messages`.
    #[serde(default)]
    pub url_suffix: Option<String>,
    /// Every one of these must appear in the request body.
    #[serde(default)]
    pub body_contains: Vec<String>,
    /// When set, the request digest must be exactly this.
    #[serde(default)]
    pub request_digest: Option<String>,
}

impl Matcher {
    /// True when this recording answers that request.
    #[must_use]
    pub fn matches(&self, request: &HttpRequest) -> bool {
        if let Some(suffix) = &self.url_suffix {
            if !request.url.ends_with(suffix.as_str()) {
                return false;
            }
        }
        if let Some(digest) = &self.request_digest {
            if &request.digest() != digest {
                return false;
            }
        }
        if self.body_contains.is_empty() {
            return true;
        }
        let body = String::from_utf8_lossy(&request.body);
        self.body_contains
            .iter()
            .all(|needle| body.contains(needle.as_str()))
    }
}

/// One recorded exchange, as it appears on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Cassette {
    /// File-stem name. Ties the recording to the test that uses it.
    pub name: String,
    /// A sentence saying what this recording is for. Required, by review.
    #[serde(default)]
    pub note: String,
    /// Which provider's shape the body is in.
    #[serde(default)]
    pub provider: String,
    /// When to answer.
    #[serde(default)]
    pub matcher: Matcher,
    /// The status to return.
    pub status: u16,
    /// Response headers, lower-cased.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// The response body as JSON, so it is diffable in review.
    ///
    /// A recording of a non-JSON response - the HTML error page a captive portal
    /// serves, which is a real failure mode with its own error code - uses
    /// [`Cassette::body_text`] instead.
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    /// The response body as literal text, for the non-JSON recordings.
    #[serde(default)]
    pub body_text: Option<String>,
}

impl Cassette {
    /// The bytes this recording returns.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        if let Some(text) = &self.body_text {
            return text.clone().into_bytes();
        }
        match &self.body {
            Some(value) => serde_json::to_vec(value).unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// Turn the recording into the response the transport hands back.
    #[must_use]
    pub fn response(&self) -> HttpResponse {
        let mut headers = self.headers.clone();
        if !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        {
            headers.push((
                "content-type".to_string(),
                if self.body_text.is_some() {
                    "text/plain".to_string()
                } else {
                    "application/json".to_string()
                },
            ));
        }
        HttpResponse {
            status: self.status,
            headers,
            body: self.bytes(),
        }
    }
}

/// A transport that answers from recordings and never opens a socket.
///
/// `Debug` is written by hand. A derived one prints every recorded body, and
/// several of them contain deliberately key-shaped strings because that is what a
/// provider's 401 body really looks like. This type is reachable from `{:?}` on
/// the whole gateway, which is what a crash reporter prints.
pub struct CassetteTransport {
    /// Keyed by name so iteration order is deterministic.
    cassettes: BTreeMap<String, Cassette>,
    /// Requests seen, so a test can assert what was asked as well as answered.
    seen: Mutex<Vec<HttpRequest>>,
    /// How many times each recording answered.
    plays: Mutex<BTreeMap<String, u32>>,
}

impl fmt::Debug for CassetteTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `finish_non_exhaustive` rather than `finish`: the recorded bodies and
        // the request log are deliberately omitted, and saying so is better than
        // implying this is everything.
        f.debug_struct("CassetteTransport")
            .field("cassettes", &self.cassettes.keys().collect::<Vec<_>>())
            .field("requests_seen", &self.seen.lock().len())
            .field("plays", &self.plays.lock().len())
            .finish_non_exhaustive()
    }
}

impl CassetteTransport {
    /// An empty transport. Every request is an unmatched-request error.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            cassettes: BTreeMap::new(),
            seen: Mutex::new(Vec::new()),
            plays: Mutex::new(BTreeMap::new()),
        }
    }

    /// Load every `*.json` in a directory.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6010` when the directory cannot be read or a file is not a
    /// cassette. A malformed cassette is a broken test fixture and must be loud.
    pub fn from_dir(dir: &Path) -> AuraResult<Self> {
        let mut transport = Self::empty();
        let entries = std::fs::read_dir(dir).map_err(|err| {
            provider_error(
                "cassette",
                0,
                format!("could not read {}: {err}", dir.display()),
            )
        })?;
        // Collected and sorted rather than used in directory order: two operating
        // systems enumerate a directory differently, and a test that picks a
        // different recording on macOS is not a test.
        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                paths.push(path);
            }
        }
        paths.sort();

        for path in paths {
            let text = std::fs::read_to_string(&path).map_err(|err| {
                provider_error(
                    "cassette",
                    0,
                    format!("could not read {}: {err}", path.display()),
                )
            })?;
            let mut cassette: Cassette = serde_json::from_str(&text).map_err(|err| {
                provider_error(
                    "cassette",
                    0,
                    format!("{} is not a cassette: {err}", path.display()),
                )
            })?;
            if cassette.name.is_empty() {
                cassette.name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("unnamed")
                    .to_string();
            }
            transport.cassettes.insert(cassette.name.clone(), cassette);
        }
        Ok(transport)
    }

    /// Add one recording, for a test that builds its own.
    #[must_use]
    pub fn with(mut self, cassette: Cassette) -> Self {
        self.cassettes.insert(cassette.name.clone(), cassette);
        self
    }

    /// How many recordings are loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cassettes.len()
    }

    /// True when nothing is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cassettes.is_empty()
    }

    /// Every request that was made, in order.
    #[must_use]
    pub fn seen(&self) -> Vec<HttpRequest> {
        self.seen.lock().clone()
    }

    /// How many times each recording answered.
    #[must_use]
    pub fn plays(&self) -> BTreeMap<String, u32> {
        self.plays.lock().clone()
    }

    /// Recordings that never answered.
    ///
    /// A cassette nobody plays is either a test that stopped running or a matcher
    /// that stopped matching, and both are worth failing a suite over.
    #[must_use]
    pub fn unplayed(&self) -> Vec<String> {
        let plays = self.plays.lock();
        self.cassettes
            .keys()
            .filter(|name| !plays.contains_key(name.as_str()))
            .cloned()
            .collect()
    }
}

impl Transport for CassetteTransport {
    fn send(&self, request: &HttpRequest, _timeout: Duration) -> AuraResult<HttpResponse> {
        self.seen.lock().push(request.clone());
        for (name, cassette) in &self.cassettes {
            if cassette.matcher.matches(request) {
                *self.plays.lock().entry(name.clone()).or_insert(0) += 1;
                tracing::debug!(target: "cloud.cassette", cassette = name.as_str(), "replayed");
                return Ok(cassette.response());
            }
        }
        Err(unreachable(
            "cassette",
            format!(
                "no recording matches {} ({} loaded); record one in tests/cloud/cassettes",
                request.url,
                self.cassettes.len()
            ),
        ))
    }

    fn name(&self) -> &'static str {
        "cassette"
    }
}

impl fmt::Display for CassetteTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cassette transport with {} recordings",
            self.cassettes.len()
        )
    }
}
