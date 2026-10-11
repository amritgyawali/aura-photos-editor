//! Opening a provider's own key page in the photographer's browser. ADR-0108 section 5.
//!
//! `Connect Claude` and `Connect ChatGPT` do not sign anybody in from inside AURA: neither
//! vendor offers a third-party desktop application a sign-in that yields an API key. What they
//! offer is a page where the account holder creates one. So the button opens that page in the
//! default browser, the photographer creates a key there, and pastes it into the key field,
//! which stores it in the operating system's credential store exactly as before.
//!
//! The address comes **only from the catalogue**, never from the caller, and only an `https://`
//! address is opened. A command that opened whatever string it was handed would be a way for
//! anything that can reach the IPC surface to launch an arbitrary program or page.
use std::fmt;

use crate::catalog;
use crate::provider::ProviderKind;

/// Launching the browser, behind a port so tests never open a real one.
pub trait Launcher: Send + Sync + fmt::Debug {
    /// Open `url` in the default browser and return without waiting for it.
    ///
    /// # Errors
    /// A sentence when no browser could be started.
    fn open(&self, url: &str) -> Result<(), String>;
}

/// The operating system's own "open this address" command.
#[derive(Debug, Default)]
pub struct SystemLauncher;

impl Launcher for SystemLauncher {
    fn open(&self, url: &str) -> Result<(), String> {
        use std::process::{Command, Stdio};
        // `rundll32 url.dll` rather than `cmd /c start`: no shell parses the address, so an `&`
        // in a query string cannot become a second command.
        let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
            ("rundll32", vec!["url.dll,FileProtocolHandler", url])
        } else if cfg!(target_os = "macos") {
            ("open", vec![url])
        } else {
            ("xdg-open", vec![url])
        };
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("could not start {program}: {err}"))
    }
}

/// The page where a key for this provider is created, when the catalogue has one.
#[must_use]
pub fn keys_page(provider: &str) -> Option<&'static str> {
    let kind = ProviderKind::parse(provider);
    catalog::all()
        .iter()
        .find(|spec| spec.kind == kind)
        .map(|spec| spec.keys_url)
        .filter(|url| url.starts_with("https://") && !url.contains(char::is_whitespace))
}

/// What happened when a key page was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The address, for the panel to show and offer to copy whether or not it opened.
    pub url: Option<&'static str>,
    /// True when the browser was started.
    pub opened: bool,
    /// One sentence for the panel.
    pub message: String,
}

/// Open a provider's key page through `launcher`.
#[must_use]
pub fn open_keys_page(provider: &str, launcher: &dyn Launcher) -> Opened {
    let Some(url) = keys_page(provider) else {
        return Opened {
            url: None,
            opened: false,
            message: "This provider has no sign-in page AURA knows of. Paste a key from your provider below."
                .to_string(),
        };
    };
    match launcher.open(url) {
        Ok(()) => Opened {
            url: Some(url),
            opened: true,
            message: "Your browser is open on the key page. Sign in, create a key, copy it, and paste it below."
                .to_string(),
        },
        Err(why) => Opened {
            url: Some(url),
            opened: false,
            message: format!("The browser could not be opened ({why}). Open {url} yourself, create a key, and paste it below."),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct Recorder(Mutex<Vec<String>>, bool);

    impl Launcher for Recorder {
        fn open(&self, url: &str) -> Result<(), String> {
            self.0.lock().unwrap().push(url.to_string());
            if self.1 {
                Err("no browser".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn claude_and_chatgpt_open_their_own_key_pages() {
        let launcher = Recorder::default();
        let claude = open_keys_page("anthropic", &launcher);
        assert!(claude.opened);
        assert_eq!(
            claude.url,
            Some("https://console.anthropic.com/settings/keys")
        );
        let openai = open_keys_page("openai", &launcher);
        assert_eq!(openai.url, Some("https://platform.openai.com/api-keys"));
        assert_eq!(launcher.0.lock().unwrap().len(), 2);
    }

    #[test]
    fn only_a_catalogue_address_is_ever_opened() {
        let launcher = Recorder::default();
        for asked in ["https://evil.example", "calc.exe", "", "compat"] {
            let result = open_keys_page(asked, &launcher);
            assert!(result
                .url
                .is_none_or(|u| catalog::all().iter().any(|s| s.keys_url == u)));
        }
        for url in launcher.0.lock().unwrap().iter() {
            assert!(url.starts_with("https://"));
        }
    }

    #[test]
    fn a_missing_browser_still_gives_the_address() {
        let launcher = Recorder(Mutex::default(), true);
        let result = open_keys_page("anthropic", &launcher);
        assert!(!result.opened);
        assert!(result.message.contains("console.anthropic.com"));
    }
}
