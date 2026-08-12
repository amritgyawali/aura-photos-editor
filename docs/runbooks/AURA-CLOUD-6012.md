# AURA-CLOUD-6012 - The operating system credential store could not be used

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The Save button in Settings > AI Keys reports the registered sentence. The key is not saved anywhere.

## What actually happened

The platform credential command failed or is missing:

| Platform | Mechanism | Usual cause |
|---|---|---|
| Windows | `powershell` DPAPI, user-scoped | PowerShell blocked by policy |
| macOS | `/usr/bin/security` | The user dismissed the keychain prompt |
| Linux | `secret-tool` (libsecret) | `libsecret-tools` not installed, or no session keyring |

## What AURA does automatically

**Nothing is written anywhere else.** There is no fallback file, no catalog row, no environment variable. A key we cannot store securely is a key we do not store. Any previously stored key is untouched.

## Operator steps

1. On Linux, install `libsecret-tools` for `secret-tool` and confirm a session keyring is running. Headless boxes usually have neither.
2. On macOS, the keychain prompt may be behind the app window; ask the user to look for it.
3. On Windows, check whether PowerShell execution is restricted by group policy.
4. Never work around this by putting the key in an environment variable or a config file. If the machine genuinely cannot store secrets, cloud AI stays off on that machine.

## Related

- Key storage: `crates/aura-cloud/src/keys.rs`
- Policy ADR: `docs/adr/ADR-0009-cloud-ai-policy.md`
