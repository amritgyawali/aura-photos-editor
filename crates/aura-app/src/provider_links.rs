//! Provider dashboards opened only after a user click. The renderer supplies an identity,
//! never a URL, command, callback or credential.
use crate::commands::IpcResult;
use aura_cloud::provider::ProviderKind;
use aura_recipe::errors::recipe_invalid;

/// Resolve a provider's published key dashboard without accepting arbitrary URLs.
/// # Errors
/// Unknown providers and providers without a key dashboard are refused.
pub fn key_page(provider: &str) -> IpcResult<&'static str> {
    let kind = ProviderKind::ALL
        .iter()
        .find(|kind| kind.as_str() == provider)
        .ok_or_else(|| recipe_invalid("provider", "Choose a listed AI provider"))?;
    let spec = aura_cloud::catalog::spec(*kind);
    let url = spec.keys_url;
    if !spec.requires_key || !url.starts_with("https://") {
        return Err(recipe_invalid("provider", "This provider has no API key dashboard").into());
    }
    Ok(url)
}

/// Open the chosen provider's key dashboard in the system browser.
/// This does not authorize AURA; saving and checking a key are separate actions.
/// # Errors
/// An invalid provider or failure to start the browser.
pub fn open_key_page(provider: &str) -> IpcResult<String> {
    let url = key_page(provider)?;
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("rundll32.exe");
        cmd.args(["url.dll,FileProtocolHandler", url]);
        cmd.creation_flags(0x0800_0000);
        cmd
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = std::process::Command::new("open");
        cmd.arg(url);
        cmd
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = {
        let mut cmd = std::process::Command::new("xdg-open");
        cmd.arg(url);
        cmd
    };
    let status = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| {
            recipe_invalid(
                "provider",
                "Could not open the browser. Use the displayed key-page address.",
            )
        })?;
    if !status.success() {
        return Err(recipe_invalid(
            "provider",
            "Could not open the browser. Use the displayed key-page address.",
        )
        .into());
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_known_dashboards_can_be_opened() {
        assert_eq!(
            super::key_page("openai").ok(),
            Some("https://platform.openai.com/api-keys")
        );
        assert!(super::key_page("anthropic").is_ok());
        for value in [
            "",
            "https://evil.example",
            "openai; calc",
            "ollama",
            "compat",
            "OpenAI",
        ] {
            assert!(super::key_page(value).is_err(), "accepted {value}");
        }
    }
}
