# Shipping a release

A photographer is editing a wedding on Friday for a client meeting on Sunday. An update that breaks
on their machine is not an inconvenience; it is the reason they go back to Lightroom.

Everything here exists to make that unlikely, and recoverable when it happens anyway.

---

## The checklist

`ops/release/release.toml` is the gate. `ops/release/check.sh` runs it. Eleven things have to be
green, and each one names who owns it and why it is there:

| Gate | What it catches |
|---|---|
| `fmt` | Formatting noise that hides the change underneath it. |
| `banned` | `unwrap`, `panic`, a socket outside `aura-cloud`, a key written where it can be read back. |
| `clippy` | Warnings, before they accumulate to the point nobody reads them. |
| `contracts` | A frozen contract that moved without an ADR and a re-lock. |
| `models` | An unsigned manifest, a digest that moved, a model with no card. |
| `tests` | The workspace suite. |
| `doctests` | A documentation example that stopped compiling. `--all-targets` does not cover them. |
| `phases` | All thirty phase gates, through `scripts/check-phase-gates.sh`. |
| `budgets` | Every performance budget in `perf/budgets.toml`, as assertions. |
| `ui` | The front end's own tests and its type check. |
| `ipc` | The three files that have to agree about the command surface. |

`doctests` and `phases` were added after the independent review of phases 01 to 30. The `tests`
row had claimed to include the phase gates and did not - they are `aura-cli verify` subcommands
rather than test functions - so sixteen of the thirty ran nowhere on a push, phase 30's delivery
guarantee and phase 13's unattended-operation check among them.

Four sign-offs follow, by role rather than by name, so a release is never blocked on one person
being awake.

The checklist is a file that something executes rather than a page in a wiki, for one reason: a
checklist nothing runs drifts from what actually happens, and the release under time pressure is
when the drift is discovered.

**A timing budget is a statement about hardware as well as about code.** The figures in
`perf/budgets.toml` were measured on a development machine, and a slower host fails them without
anything having regressed — on the container this was last run on, phase 14's processor-path proxy
render takes 801 ms against a 450 ms budget, because that budget assumes a GPU backend this build
does not link and a machine four times faster than the one measuring it. `AURA_PERF_HOST_SCALE` is
what a slower host sets: the budget file keeps the developer-machine numbers, the assertion stays
real on both, and a genuine regression still fails. It applies to timings only. A byte is a byte on
any machine, and a slow runner is not a reason to store more.

## Building the Windows installer

```bash
# 1. The shell, with the frontend embedded, and the licence server subscriptions renew from.
(cd ui && npm run build)
AURA_LICENCE_SERVER=https://<shop-domain>   cargo build --manifest-path ui/src-tauri/Cargo.toml --features custom-protocol   # CARGO_TARGET_DIR set
# 2. The runtime and models, once (scripts/fetch-ai-models.sh <dir>), then the installer.
AURA_SIGN_COMMAND='signtool sign /sha1 <thumbprint> /fd sha256 /tr <timestamp-url> /td sha256 %1'   scripts/build-installer.sh <dir>
```

The script checks every bundled DLL and model against its pinned SHA-256, then writes
`AURA_<version>_x64-setup.exe` under `$CARGO_TARGET_DIR/debug/bundle/nsis/`. It installs per user,
with no administrator prompt, and fetches WebView2 if the machine lacks it.

## Signing

Windows builds are Authenticode-signed; macOS builds are signed with a Developer ID and submitted
to Apple for notarisation, then stapled. The certificate has to be bought in the seller's legal name
- an OV or EV code-signing certificate from a certificate authority, or Microsoft Trusted Signing -
and **without it SmartScreen warns every customer** that the publisher is unknown.
`scripts/build-installer.sh` takes the signing command in `AURA_SIGN_COMMAND` and says plainly
when it built an unsigned installer.

`ops/sign/` and `ops/notarise/` hold the rules. **No certificate exists here yet**, so no installer
built in this repository has been signed.

## Rollout

An update does not go to everybody at once.

`stable` goes to 1 % of installs, then 5 %, then 25 %, then everyone, with a 24-hour soak at each
stage and a crash-free floor of 99.5 %. `beta` goes 25 % then 100 % with a six-hour soak.
`nightly` goes straight out and has no floor, because nobody on it is delivering a wedding.

A rollout that trips its floor **rolls back**, and does not pause. Pausing protects the installs
that have not updated yet and does nothing at all for the ones the problem is happening to, which is
exactly the wrong way round.

## Model packs update separately

A model is signed, and its signature is checked before its digest, and its digest before its card.
Downloads resume, installs are verify-then-rename, and a version that fails its first real use is
rolled back automatically — the photographer keeps the quality they had that morning.

A model pack can therefore ship without an application release, and an application release does not
force a model change on anybody mid-wedding.

## Feature flags

`ops/flags/flags.toml`. Two of them are off by default and worth knowing about:

**Generative cleanup** is off. It is the one feature that puts pixels into a photograph that were
never photographed, and a studio should be able to say "not on our deliveries" once rather than per
export.

**The learning loop** is off. It changes what AURA does to future weddings based on what you did to
past ones, and that is a thing to opt into.

Every flag is a kill switch as well as a switch: a release can turn one off remotely without
shipping a build, which is what you want at eleven on a Saturday night.

## Crash reporting

Off unless you turn it on.

When it is on, a crash sends a stack trace, the build, the operating system and what AURA was doing
— the stage, not the photograph. No filenames, no image data, no identifiers that lead back to a
wedding or a client. `ops/crash/telemetry.toml` lists every field, and the list is short enough to
read.

## Support bundles

When something goes wrong that a stack trace does not explain, AURA can produce an anonymised
support bundle: the decisions, their reasons, their confidences and the versions that made them,
with every identifier replaced by a handle.

**It contains no pixels.** Not by policy — by shape. The evidence a decision can carry is a
rectangle, a list of frame handles or a list of named parameter changes, and there is no variant of
it that could hold image bytes.

## Licensing

Keys are checked **offline**: a licence is a statement signed with the vendor's ed25519 key and the
application carries only the public half (ADR-0105), so there is no server to reach and no grace
period to run out in a hotel with bad wifi.

Trial mode limits what can be *delivered*, not what can be edited. For 14 days everything works;
after that editing keeps working and exporting finished photographs needs a key. A trial that
stopped you seeing what the product does to your photographs would be a trial that tells you
nothing.

Subscriptions are sold through Paddle and keys are issued by the shop's licence server without
anybody running a command (`shop/README.md`, ADR-0107). Issuing a key by hand - a gift, a press copy,
a support case:

```bash
cargo run -p licence-issue -- issue --key <vendor-licence.key>   --name "Customer or studio name" --email customer@example.com --id <order-ref>   [--edition pro] [--expires YYYY-MM-DD]
```

The vendor key file never enters this repository. Back it up offline: without it no new key can
be issued that existing installations accept.

## What this release has not done

No closed beta has happened, so the 99.5 % crash-free target is a floor with no measurement behind
it. Nothing has been signed or notarised. No staged rollout has run.

The machinery is here and the numbers are not, and the phase gate prints that on every run rather
than leaving it in a document nobody opens.
