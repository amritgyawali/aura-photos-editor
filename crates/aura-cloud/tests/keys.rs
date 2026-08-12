//! The security test the key store exists for.
//!
//! One assertion matters more than the rest: **the secret is never in `argv`**,
//! on any platform, for any operation. `argv` is readable by every process
//! running as the same user, so a key passed as an argument is a key published to
//! the machine. It is checked here for all three platforms at once, because the
//! CI matrix runs each of these files on one platform and the shape of the other
//! two must still be verified.

use std::sync::Arc;

use aura_cloud::keys::{
    CommandOutput, KeyStore, MemoryKeyStore, OsKeyStore, Platform, RecordingRunner, SecretKey,
};

const SECRET: &str = "sk-ant-api03-VERYSECRET00000000000000000000000000000AA";

fn store(platform: Platform, runner: Arc<RecordingRunner>) -> OsKeyStore {
    OsKeyStore::with_runner(platform, runner, std::path::Path::new("target/test-keys"))
}

#[test]
fn no_platform_ever_puts_the_secret_in_argv() {
    let secret = SecretKey::new(SECRET);
    for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
        let store = store(platform, Arc::new(RecordingRunner::new()));
        for command in [
            store.store_command("api-key:anthropic", &secret),
            store.load_command("api-key:anthropic"),
            store.delete_command("api-key:anthropic"),
        ] {
            assert!(
                !command.leaks_secret(SECRET),
                "{:?} put the secret in argv on {}",
                command.program,
                platform.as_str()
            );
        }
    }
}

#[test]
fn the_secret_travels_on_standard_input_and_only_there() {
    let secret = SecretKey::new(SECRET);
    for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
        let store = store(platform, Arc::new(RecordingRunner::new()));
        let command = store.store_command("api-key:anthropic", &secret);
        let stdin = command.stdin.unwrap_or_default();
        assert!(
            stdin.contains(SECRET),
            "{} does not write the secret to stdin",
            platform.as_str()
        );
    }
}

#[test]
fn macos_writes_the_secret_twice_because_security_prompts_twice() {
    // `security add-generic-password -w` with no value prompts for the password
    // and then for a confirmation. A single write leaves the tool waiting.
    let store = store(Platform::MacOs, Arc::new(RecordingRunner::new()));
    let command = store.store_command("api-key:openai", &SecretKey::new(SECRET));
    let stdin = command.stdin.unwrap_or_default();
    assert_eq!(stdin.matches(SECRET).count(), 2);
}

#[test]
fn a_command_debug_rendering_hides_the_secret() {
    let store = store(Platform::Linux, Arc::new(RecordingRunner::new()));
    let command = store.store_command("api-key:google", &SecretKey::new(SECRET));
    let rendered = format!("{command:?}");
    assert!(!rendered.contains(SECRET));
    assert!(rendered.contains("bytes"), "the length is still visible");
}

#[test]
fn a_secret_key_debug_rendering_is_a_length_and_nothing_else() {
    let secret = SecretKey::new(SECRET);
    let rendered = format!("{secret:?}");
    assert!(!rendered.contains(SECRET));
    assert!(rendered.contains(&secret.len().to_string()));
}

#[test]
fn a_fingerprint_shows_the_ends_and_never_the_middle() {
    let secret = SecretKey::new(SECRET);
    let fingerprint = secret.fingerprint();
    assert_eq!(fingerprint, "sk-a...00AA");
    assert!(!SECRET.contains(&fingerprint));

    // A short key gets no fingerprint at all rather than a revealing one.
    assert_eq!(SecretKey::new("short").fingerprint(), "****");
}

#[test]
fn a_pasted_key_is_trimmed() {
    let secret = SecretKey::new(format!("  {SECRET}\n"));
    assert_eq!(secret.expose(), SECRET);
}

#[test]
fn a_missing_entry_is_not_an_error() {
    // `security` and the PowerShell script both exit 44 for "no such item".
    for platform in [Platform::Windows, Platform::MacOs] {
        let runner = Arc::new(RecordingRunner::new().replying(
            if platform == Platform::Windows {
                "-NoProfile"
            } else {
                "find-generic-password"
            },
            CommandOutput {
                status: 44,
                stdout: String::new(),
                stderr: "The specified item could not be found".to_string(),
            },
        ));
        let store = store(platform, runner);
        assert_eq!(store.load("api-key:anthropic").expect("no error"), None);
    }
}

#[test]
fn secret_tool_reports_absence_as_a_non_zero_exit_with_no_output() {
    let runner = Arc::new(RecordingRunner::new().replying(
        "lookup",
        CommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: String::new(),
        },
    ));
    let store = store(Platform::Linux, runner);
    assert_eq!(store.load("api-key:anthropic").expect("no error"), None);
}

#[test]
fn a_refusing_credential_store_is_a_coded_error_and_writes_nothing_else() {
    let runner = Arc::new(RecordingRunner::new().replying(
        "store",
        CommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: "Cannot create an item in a locked collection".to_string(),
        },
    ));
    let store = store(Platform::Linux, Arc::clone(&runner));
    let err = store
        .store("api-key:anthropic", &SecretKey::new(SECRET))
        .expect_err("a locked keyring is a refusal");
    assert_eq!(err.code.0, "AURA-CLOUD-6012");

    // Exactly one command ran, and it was the store attempt. Nothing fell back
    // to a file, an environment variable or the catalog.
    assert_eq!(runner.seen().len(), 1);
}

#[test]
fn an_empty_key_is_refused_before_a_process_is_spawned() {
    let runner = Arc::new(RecordingRunner::new());
    let store = store(Platform::Linux, Arc::clone(&runner));
    let err = store
        .store("api-key:anthropic", &SecretKey::new("   "))
        .expect_err("an empty key is not a key");
    assert_eq!(err.code.0, "AURA-CLOUD-6012");
    assert!(runner.seen().is_empty());
}

#[test]
fn the_blob_path_cannot_escape_its_directory() {
    let store = store(Platform::Windows, Arc::new(RecordingRunner::new()));
    let hostile = store.blob_path("../../../../Windows/System32/evil");
    assert_eq!(
        hostile.extension().and_then(|ext| ext.to_str()),
        Some("key")
    );
    assert_eq!(
        hostile.parent(),
        Some(std::path::Path::new("target/test-keys")),
        "the account name is hashed, so it cannot contain a separator"
    );
}

#[test]
fn the_in_memory_store_round_trips() {
    let store = MemoryKeyStore::new();
    assert_eq!(store.load("api-key:anthropic").expect("read"), None);

    store
        .store("api-key:anthropic", &SecretKey::new(SECRET))
        .expect("write");
    assert_eq!(
        store
            .load("api-key:anthropic")
            .expect("read")
            .map(|key| key.expose().to_string()),
        Some(SECRET.to_string())
    );

    store.delete("api-key:anthropic").expect("delete");
    assert_eq!(store.load("api-key:anthropic").expect("read"), None);
    // Deleting something that is not there is success.
    store.delete("api-key:anthropic").expect("idempotent");
}

#[test]
fn each_provider_files_its_key_under_its_own_account() {
    use aura_cloud::provider::ProviderKind;

    let accounts: Vec<String> = [
        ProviderKind::Anthropic,
        ProviderKind::OpenAi,
        ProviderKind::Google,
        ProviderKind::Compat,
    ]
    .iter()
    .map(|kind| kind.account())
    .collect();

    let mut unique = accounts.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        accounts.len(),
        "two providers share an account"
    );
    for account in &accounts {
        assert!(account.starts_with("api-key:"));
    }
}
