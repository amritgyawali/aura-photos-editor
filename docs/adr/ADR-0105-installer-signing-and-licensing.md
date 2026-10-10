# ADR-0105: The Windows installer, code signing, and offline licence keys

Status: accepted.
Date: 2026-10-08

## Problem

AURA could be built and run, but not sold. Three things were missing:

1. **An installer.** The application was a folder: `aura-desktop.exe`, `WebView2Loader.dll`
   (needed by the GNU-toolchain build), the ONNX Runtime and DirectML libraries from ADR-0103,
   and 480 MB of masking models. Nobody buys a folder.
2. **A signature.** `ops/sign/` described Authenticode and refused to run without a certificate,
   which is right, but there was no path from a built executable to a signed installer.
3. **A licence.** Nothing distinguished a paying customer from anybody else. Phase 30's error
   `AURA-REL-12004` assumed an online check with a grace period; no such service exists.

## Decision

### The installer

NSIS, produced by Tauri's bundler (`tauri bundle --bundles nsis`) around the executable cargo has
already built. `scripts/build-installer.sh`:

- takes the executable from `$CARGO_TARGET_DIR/debug` - the shell's optimised dev profile, which is
  what this 8 GB machine builds without running out of memory (`ui/src-tauri/Cargo.toml`), and
  what the photographer has been using;
- adds `WebView2Loader.dll`, the three runtime libraries, their licence texts (under `licences/`),
  and the four models (under `models/`, where `aura_infer::accelerated::models_dir` looks);
- **checks every one of those files against its pinned SHA-256 before bundling** and refuses on
  a mismatch. The models live on a drive that has corrupted files at rest; an installer is the
  worst place to ship a damaged model, because every customer gets the same damage;
- installs per user (`installMode: currentUser`), so no administrator prompt; WebView2 is
  bootstrapped by the installer when missing, Tauri's default.

MSI was dropped from the Windows targets: it needs WiX, adds nothing a photographer can see, and
is the format for managed fleets, which is not who buys this.

### Signing

The certificate is the one input that cannot be produced here: it has to be bought from a
certificate authority (or Microsoft's Trusted Signing) in the seller's legal name, after identity
checks. The script therefore takes a signing command rather than a key:
`AURA_SIGN_COMMAND='signtool sign /sha1 <thumbprint> /fd sha256 /tr <timestamp> /td sha256 %1'`
or the equivalent for a cloud signing service. Tauri runs it on the executable, the uninstaller
and the installer. Without it the installer is built, printed as **UNSIGNED**, and Windows
SmartScreen will warn whoever runs it - acceptable for testing, not for sale. `ops/sign/README.md`
keeps the rules: timestamped, key never on a developer machine or a CI runner.

### Licensing

**Offline, signed keys.** `crates/aura-licence`:

- A licence is a small JSON statement - order reference, licensee, email, edition, issue date,
  optional end date - signed with the vendor's ed25519 key. The key a customer receives is
  `AURA1.<payload>.<signature>` in base64url: ordinary text that survives an email or a PDF.
- The application holds only `VENDOR_PUBLIC_KEY`. Checking a key needs no network, so a
  photographer in a marquee with no signal is never locked out of a licence they paid for, and
  there is no server to keep running.
- `tools/licence-issue` signs keys with the private key, which is a file kept offline by the
  vendor. It refuses to print a key the shipped public key would not accept.

**A 14-day trial, and then only export stops.** A new installation can use everything for 14
days. After that, editing, culling, retouching and the catalogue keep working; exporting finished
photographs - the export command and the autopilot's export stage - is refused with
`AURA-REL-12005` and a sentence saying how to fix it. A photographer's work is never held
hostage, and what the licence unlocks is the thing the product is sold for: the delivered files.

**Where it is kept.** `licence.json` in AURA's data directory, because a licence belongs to the
machine, not to one wedding. The trial's start is also written into every catalogue opened, and
the earliest date anywhere wins, so deleting one file does not restart the trial. Tests keep it
beside their temporary catalogue (`AppState::with_licence_dir`) so no test can touch the
developer's own trial.

**What this is not.** It is not copy protection. Keys are not bound to a machine, and a trial
clock on the customer's own disk can be reset by somebody determined to. That is the trade every
offline licence makes: paying is the easy path, and an honest customer never meets a check that
fails them. Seat limits, machine activation and revocation need a server and are left for when
there is one; `AURA-REL-12004`, phase 30's online grace period, stays registered for that day.

## What was checked

On this machine, 2026-10-08: the script verified all seven pinned files and produced
`AURA_0.1.0_x64-setup.exe`, 467 MB (the models do not compress). A silent install (`/S /D=...`)
wrote the executable, `WebView2Loader.dll`, the three runtime libraries, `licences/` and `models/`;
the installed application opened its window and loaded `onnxruntime.dll` and `DirectML.dll` from
its own folder; it recorded the trial's start in `licence.json`; a silent uninstall removed the
folder and its entry in Windows' installed-programs list. The installer was not signed, and it has
not been run on a second machine.

## Consequences

- Release steps: build the shell, run `scripts/build-installer.sh` with `AURA_SIGN_COMMAND`, issue
  keys with `tools/licence-issue` when an order arrives. `docs/release-process.md` lists them.
- The vendor key file must be backed up somewhere other than this machine. Losing it means no new
  key can be issued that existing installations accept; leaking it means anybody can.
- Payment, invoicing and the email that carries the key are not part of AURA. Any shop that can
  run a command or a webhook on an order can call `licence-issue`.
