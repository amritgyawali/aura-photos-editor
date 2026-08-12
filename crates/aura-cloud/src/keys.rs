//! The user's API key, and the three operating systems that will hold it.
//!
//! Two constraints shape this module, and between them they rule out the obvious
//! design.
//!
//! Every crate in this workspace is `#![forbid(unsafe_code)]`, so the platform
//! credential APIs cannot be called through FFI. And a key is the one secret in
//! the product whose disclosure costs the user money, so it cannot live in the
//! catalog, in a config file, in an environment variable or in a log.
//!
//! What is left is **command invocation**: the platform's own credential tool,
//! spawned as a child process. That works on all three targets without a line of
//! unsafe code, and it has one sharp edge, which this module is built around.
//!
//! **`argv` is world-readable.** On Windows, macOS and Linux alike, any process
//! running as the same user - and on Linux, historically, any process at all -
//! can read another process's command line. A key passed as
//! `secret-tool store ... sk-ant-xxxx` is a key published to the machine. So the
//! secret is written to the child's **stdin**, always, on every platform and on
//! every operation, and [`KeyCommand`] separates the two so a test can prove it:
//! [`KeyCommand::leaks_secret`] is asserted false for every command shape this
//! module can produce, in `tests/keys.rs`.
//!
//! The second sharp edge is that a key we cannot store securely must not be
//! stored insecurely. There is no file fallback and no environment-variable
//! fallback. When the credential store refuses, the answer is
//! `AURA-CLOUD-6012` and nothing is written anywhere.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_core::errors::cloud::keystore_failed;
use aura_core::AuraResult;
use parking_lot::Mutex;

/// The service name every credential entry is filed under.
pub const SERVICE: &str = "aura-cloud";

/// A secret that will not print itself.
///
/// `Debug` renders the length and nothing else, so a key cannot reach a log
/// through a `{:?}` on a struct that happens to contain one. There is no
/// `Display`, and `expose` is deliberately verbose to read at a call site.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretKey(String);

impl SecretKey {
    /// Take ownership of a secret, trimming the whitespace a paste brings.
    #[must_use]
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into().trim().to_string())
    }

    /// The secret itself. Every call site of this is a place to look twice.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Characters in the secret. Safe to log.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when there is nothing here.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The first four and last four characters, for "is this the key you meant?".
    ///
    /// Shown in Settings so a user with three keys can tell which one is stored,
    /// without the panel ever holding the whole secret. Keys shorter than twelve
    /// characters get no fingerprint at all rather than a revealing one.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let chars: Vec<char> = self.0.chars().collect();
        if chars.len() < 12 {
            return "****".to_string();
        }
        let head: String = chars.iter().take(4).collect();
        let tail: String = chars.iter().skip(chars.len() - 4).collect();
        format!("{head}...{tail}")
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretKey(<{} chars redacted>)", self.0.len())
    }
}

impl Drop for SecretKey {
    /// Best-effort overwrite of the buffer before it is freed.
    ///
    /// Without `unsafe` this cannot be guaranteed - the allocator may already
    /// have moved the bytes, and the compiler is entitled to elide a write to a
    /// value that is about to die. It still removes the plain text from the most
    /// common case, which is a long-lived process whose heap ends up in a crash
    /// dump, and it costs nothing.
    fn drop(&mut self) {
        let filler = "\0".repeat(self.0.len());
        self.0.replace_range(.., &filler);
    }
}

/// One command this module will run, with the secret kept out of `argv`.
///
/// The split is the whole point. `program` and `args` may be logged; `stdin`
/// never is, and `Debug` enforces that.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyCommand {
    /// The executable to spawn.
    pub program: String,
    /// Arguments. **Never contains the secret.**
    pub args: Vec<String>,
    /// What is written to the child's standard input, and closed.
    pub stdin: Option<String>,
}

impl fmt::Debug for KeyCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyCommand")
            .field("program", &self.program)
            .field("args", &self.args)
            .field(
                "stdin",
                &self.stdin.as_ref().map(|s| format!("<{} bytes>", s.len())),
            )
            .finish()
    }
}

impl KeyCommand {
    /// True when the secret appears anywhere it can be read by another process.
    ///
    /// The security test in `tests/keys.rs` asserts this is false for every
    /// command every platform shape produces, for store, load and delete.
    #[must_use]
    pub fn leaks_secret(&self, secret: &str) -> bool {
        if secret.is_empty() {
            return false;
        }
        self.program.contains(secret) || self.args.iter().any(|arg| arg.contains(secret))
    }
}

/// What a credential command returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Process exit status, or -1 when it could not be determined.
    pub status: i32,
    /// Standard output, trimmed of the trailing newline the tools all add.
    pub stdout: String,
    /// Standard error, kept for the error detail.
    pub stderr: String,
}

impl CommandOutput {
    /// True when the command reported success.
    #[must_use]
    pub const fn ok(&self) -> bool {
        self.status == 0
    }
}

/// Spawning a child process, behind a port so tests never touch the real one.
pub trait CommandRunner: Send + Sync + fmt::Debug {
    /// Run one command, write `stdin`, close it, and collect the output.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6012` when the program cannot be spawned at all - which on
    /// Linux is the ordinary "libsecret-tools is not installed" case and needs a
    /// message that says so.
    fn run(&self, command: &KeyCommand) -> AuraResult<CommandOutput>;
}

/// The real one.
#[derive(Debug, Default)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, command: &KeyCommand) -> AuraResult<CommandOutput> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new(&command.program)
            .args(&command.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                keystore_failed("spawn", format!("could not run {}: {err}", command.program))
                    .with_context("program", command.program.clone())
            })?;

        if let Some(secret) = &command.stdin {
            match child.stdin.as_mut() {
                Some(pipe) => pipe.write_all(secret.as_bytes()).map_err(|err| {
                    keystore_failed(
                        "write",
                        format!("could not write to {}: {err}", command.program),
                    )
                })?,
                None => {
                    return Err(keystore_failed(
                        "write",
                        format!("{} gave no standard input", command.program),
                    ))
                }
            }
        }
        // Dropped explicitly: `security` and `secret-tool` both read until EOF,
        // and a pipe left open is a child that never exits.
        drop(child.stdin.take());

        let output = child.wait_with_output().map_err(|err| {
            keystore_failed("wait", format!("{} did not finish: {err}", command.program))
        })?;

        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        })
    }
}

/// Which platform's credential tool to build commands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// DPAPI through PowerShell, user-scoped, blob on disk.
    Windows,
    /// The login keychain through `/usr/bin/security`.
    MacOs,
    /// libsecret through `secret-tool`.
    Linux,
}

impl Platform {
    /// The platform this build is running on.
    #[must_use]
    pub const fn host() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }

    /// Stable text for the error context and the settings panel.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "dpapi",
            Self::MacOs => "keychain",
            Self::Linux => "libsecret",
        }
    }
}

/// Storing and reading the user's key.
pub trait KeyStore: Send + Sync + fmt::Debug {
    /// Write a key for one account, replacing any previous one.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6012` when the credential store refuses or is missing.
    fn store(&self, account: &str, secret: &SecretKey) -> AuraResult<()>;

    /// Read the key for one account, or `None` when there is not one.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6012` when the store exists but could not be read. A missing
    /// entry is `Ok(None)`, not an error: no key is a normal state.
    fn load(&self, account: &str) -> AuraResult<Option<SecretKey>>;

    /// Forget a key. Deleting one that is not there is success.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6012` when the store refuses.
    fn delete(&self, account: &str) -> AuraResult<()>;
}

/// The platform credential store, driven by its own command-line tool.
#[derive(Debug)]
pub struct OsKeyStore {
    platform: Platform,
    runner: Arc<dyn CommandRunner>,
    /// Where the DPAPI blob lives. Windows only; the other two platforms keep
    /// the secret inside their own store and never touch the filesystem.
    blob_dir: PathBuf,
}

impl OsKeyStore {
    /// Build a store for this machine.
    #[must_use]
    pub fn new(blob_dir: &Path) -> Self {
        Self::with_runner(Platform::host(), Arc::new(SystemRunner), blob_dir)
    }

    /// Build a store for a named platform and a chosen runner. Tests use this.
    #[must_use]
    pub fn with_runner(
        platform: Platform,
        runner: Arc<dyn CommandRunner>,
        blob_dir: &Path,
    ) -> Self {
        Self {
            platform,
            runner,
            blob_dir: blob_dir.to_path_buf(),
        }
    }

    /// Which platform this store is speaking to.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Where the Windows DPAPI blob for one account lives.
    ///
    /// The account name is hashed rather than used directly, so a provider name
    /// that ever contains a path separator cannot escape the directory.
    #[must_use]
    pub fn blob_path(&self, account: &str) -> PathBuf {
        let digest = blake3::hash(account.as_bytes()).to_hex().to_string();
        self.blob_dir.join(format!("{digest}.key"))
    }

    /// The command that stores a secret. The secret is only ever in `stdin`.
    #[must_use]
    pub fn store_command(&self, account: &str, secret: &SecretKey) -> KeyCommand {
        match self.platform {
            // DPAPI, user-scoped: only this Windows account can decrypt the blob.
            // `[Console]::In.ReadToEnd()` keeps the secret out of the script text
            // as well as out of argv - the script itself is an argument.
            Platform::Windows => {
                let path = self.blob_path(account);
                KeyCommand {
                    program: "powershell".to_string(),
                    args: vec![
                        "-NoProfile".to_string(),
                        "-NonInteractive".to_string(),
                        "-Command".to_string(),
                        format!(
                            "$ErrorActionPreference='Stop'; \
                             $s=[Console]::In.ReadToEnd().Trim(); \
                             $d=Split-Path -Parent '{path}'; \
                             if(-not (Test-Path $d)){{New-Item -ItemType Directory -Force $d|Out-Null}}; \
                             ConvertTo-SecureString $s -AsPlainText -Force | \
                             ConvertFrom-SecureString | \
                             Set-Content -Path '{path}' -Encoding ascii",
                            path = path.display()
                        ),
                    ],
                    stdin: Some(secret.expose().to_string()),
                }
            }
            // `security add-generic-password -w` with no value reads the password
            // from stdin, prompting twice, so the secret is written twice.
            Platform::MacOs => KeyCommand {
                program: "/usr/bin/security".to_string(),
                args: vec![
                    "add-generic-password".to_string(),
                    "-a".to_string(),
                    account.to_string(),
                    "-s".to_string(),
                    SERVICE.to_string(),
                    "-U".to_string(),
                    "-w".to_string(),
                ],
                stdin: Some(format!("{0}\n{0}\n", secret.expose())),
            },
            Platform::Linux => KeyCommand {
                program: "secret-tool".to_string(),
                args: vec![
                    "store".to_string(),
                    "--label=AURA cloud AI key".to_string(),
                    "service".to_string(),
                    SERVICE.to_string(),
                    "account".to_string(),
                    account.to_string(),
                ],
                stdin: Some(format!("{}\n", secret.expose())),
            },
        }
    }

    /// The command that reads a secret back.
    #[must_use]
    pub fn load_command(&self, account: &str) -> KeyCommand {
        match self.platform {
            Platform::Windows => {
                let path = self.blob_path(account);
                KeyCommand {
                    program: "powershell".to_string(),
                    args: vec![
                        "-NoProfile".to_string(),
                        "-NonInteractive".to_string(),
                        "-Command".to_string(),
                        format!(
                            "$ErrorActionPreference='Stop'; \
                             if(-not (Test-Path '{path}')){{exit 44}}; \
                             $sec=Get-Content -Path '{path}' -Raw | ConvertTo-SecureString; \
                             $b=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec); \
                             [Runtime.InteropServices.Marshal]::PtrToStringBSTR($b)",
                            path = path.display()
                        ),
                    ],
                    stdin: None,
                }
            }
            Platform::MacOs => KeyCommand {
                program: "/usr/bin/security".to_string(),
                args: vec![
                    "find-generic-password".to_string(),
                    "-a".to_string(),
                    account.to_string(),
                    "-s".to_string(),
                    SERVICE.to_string(),
                    "-w".to_string(),
                ],
                stdin: None,
            },
            Platform::Linux => KeyCommand {
                program: "secret-tool".to_string(),
                args: vec![
                    "lookup".to_string(),
                    "service".to_string(),
                    SERVICE.to_string(),
                    "account".to_string(),
                    account.to_string(),
                ],
                stdin: None,
            },
        }
    }

    /// The command that forgets a secret.
    #[must_use]
    pub fn delete_command(&self, account: &str) -> KeyCommand {
        match self.platform {
            Platform::Windows => {
                let path = self.blob_path(account);
                KeyCommand {
                    program: "powershell".to_string(),
                    args: vec![
                        "-NoProfile".to_string(),
                        "-NonInteractive".to_string(),
                        "-Command".to_string(),
                        format!(
                            "Remove-Item -Path '{path}' -Force -ErrorAction SilentlyContinue",
                            path = path.display()
                        ),
                    ],
                    stdin: None,
                }
            }
            Platform::MacOs => KeyCommand {
                program: "/usr/bin/security".to_string(),
                args: vec![
                    "delete-generic-password".to_string(),
                    "-a".to_string(),
                    account.to_string(),
                    "-s".to_string(),
                    SERVICE.to_string(),
                ],
                stdin: None,
            },
            Platform::Linux => KeyCommand {
                program: "secret-tool".to_string(),
                args: vec![
                    "clear".to_string(),
                    "service".to_string(),
                    SERVICE.to_string(),
                    "account".to_string(),
                    account.to_string(),
                ],
                stdin: None,
            },
        }
    }

    /// Exit codes that mean "there is no entry", not "something went wrong".
    ///
    /// `security` uses 44 for `errSecItemNotFound`, `secret-tool lookup` exits 1
    /// with empty output, and the PowerShell script above is written to exit 44
    /// so all three agree.
    const fn is_absent(&self, status: i32, stdout_empty: bool) -> bool {
        match self.platform {
            Platform::Windows | Platform::MacOs => status == 44,
            Platform::Linux => status != 0 && stdout_empty,
        }
    }
}

impl KeyStore for OsKeyStore {
    fn store(&self, account: &str, secret: &SecretKey) -> AuraResult<()> {
        if secret.is_empty() {
            return Err(keystore_failed("store", "refusing to store an empty key"));
        }
        let command = self.store_command(account, secret);
        let output = self.runner.run(&command)?;
        if output.ok() {
            tracing::info!(
                target: "cloud.key_stored",
                account,
                platform = self.platform.as_str(),
                "API key written to the operating system credential store"
            );
            return Ok(());
        }
        Err(keystore_failed(
            "store",
            format!(
                "{} exited {}: {}",
                command.program,
                output.status,
                crate::redact::scrub(&output.stderr)
            ),
        )
        .with_context("platform", self.platform.as_str()))
    }

    fn load(&self, account: &str) -> AuraResult<Option<SecretKey>> {
        let command = self.load_command(account);
        let output = self.runner.run(&command)?;
        if self.is_absent(output.status, output.stdout.is_empty()) {
            return Ok(None);
        }
        if !output.ok() {
            return Err(keystore_failed(
                "load",
                format!(
                    "{} exited {}: {}",
                    command.program,
                    output.status,
                    crate::redact::scrub(&output.stderr)
                ),
            )
            .with_context("platform", self.platform.as_str()));
        }
        let secret = SecretKey::new(output.stdout.clone());
        if secret.is_empty() {
            return Ok(None);
        }
        Ok(Some(secret))
    }

    fn delete(&self, account: &str) -> AuraResult<()> {
        let command = self.delete_command(account);
        let output = self.runner.run(&command)?;
        if output.ok() || self.is_absent(output.status, output.stdout.is_empty()) {
            return Ok(());
        }
        Err(keystore_failed(
            "delete",
            format!(
                "{} exited {}: {}",
                command.program,
                output.status,
                crate::redact::scrub(&output.stderr)
            ),
        ))
    }
}

/// An in-process store. Tests and the offline gate use it; it never reaches disk.
///
/// `Debug` is written by hand rather than derived. A derived one prints the map,
/// and this type is reachable from `{:?}` on the whole gateway - which is what a
/// panic handler, a crash reporter and half the assertion failures in a test
/// suite print. The privacy test in `tests/privacy.rs` asserts it.
#[derive(Default)]
pub struct MemoryKeyStore {
    entries: Mutex<BTreeMap<String, String>>,
}

impl fmt::Debug for MemoryKeyStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entries = self.entries.lock();
        f.debug_struct("MemoryKeyStore")
            .field("accounts", &entries.keys().collect::<Vec<_>>())
            .field("secrets", &"<redacted>")
            .finish()
    }
}

impl MemoryKeyStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store that already holds one key.
    #[must_use]
    pub fn with(account: &str, secret: &str) -> Self {
        let store = Self::new();
        store
            .entries
            .lock()
            .insert(account.to_string(), secret.to_string());
        store
    }
}

impl KeyStore for MemoryKeyStore {
    fn store(&self, account: &str, secret: &SecretKey) -> AuraResult<()> {
        if secret.is_empty() {
            return Err(keystore_failed("store", "refusing to store an empty key"));
        }
        self.entries
            .lock()
            .insert(account.to_string(), secret.expose().to_string());
        Ok(())
    }

    fn load(&self, account: &str) -> AuraResult<Option<SecretKey>> {
        Ok(self.entries.lock().get(account).map(SecretKey::new))
    }

    fn delete(&self, account: &str) -> AuraResult<()> {
        self.entries.lock().remove(account);
        Ok(())
    }
}

/// A runner that records what it was asked to do and answers from a script.
///
/// Lives here rather than in the test file because three test files need it and
/// because the security test's whole subject is the command shapes this module
/// produces.
#[derive(Debug, Default)]
pub struct RecordingRunner {
    /// What each program should return, keyed by the first argument.
    replies: Mutex<BTreeMap<String, CommandOutput>>,
    /// Every command that was run, in order.
    seen: Mutex<Vec<KeyCommand>>,
}

impl RecordingRunner {
    /// A runner that answers success with empty output to everything.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Answer `verb` - the first argument, or `-NoProfile` on Windows - with this.
    #[must_use]
    pub fn replying(self, verb: &str, output: CommandOutput) -> Self {
        self.replies.lock().insert(verb.to_string(), output);
        self
    }

    /// Every command run so far.
    #[must_use]
    pub fn seen(&self) -> Vec<KeyCommand> {
        self.seen.lock().clone()
    }
}

impl CommandRunner for RecordingRunner {
    fn run(&self, command: &KeyCommand) -> AuraResult<CommandOutput> {
        self.seen.lock().push(command.clone());
        let verb = command.args.first().cloned().unwrap_or_default();
        Ok(self
            .replies
            .lock()
            .get(&verb)
            .cloned()
            .unwrap_or(CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }))
    }
}
