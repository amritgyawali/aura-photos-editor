# The update channel

Two things update independently, and the independence is the feature.

## The application

Signed installers, staged by percentage. `channel.toml` holds the rollout, `ops/release/check.sh`
gates the build, and a stage that falls below the crash-free rate **rolls back** rather than pausing
— because a paused rollout leaves the installs that already took the build on it.

An update that installs and then fails its first real use is rolled back automatically and recorded
as rejected. `AURA-REL-12002`, and it is phase 03's rule applied to the whole application: *a model
is pending until it has worked once.*

## Model packs

Verified by `aura-models`: ed25519 over `models.lock`, then sha256 per file, then the model card, in
that order, offline. Delta-updated with `AURADLT1` when a delta is smaller than the file. Installed
verify-then-rename, so a half-written model is never a model this build will load.

**A model pack is not part of the application signature**, and that is the whole point of this
directory having two halves. Section 6.4: "a model rollback must be possible without downgrading the
app". If a pack lived inside the application bundle, rolling a bad model back would take a
photographer's bug fixes with it.

## How a rollback actually happens

Three levers, in the order you would reach for them:

1. **The flag.** `ops/flags/flags.toml`, stage off. Immediate, no download, no reinstall. The stage
   stops running and its rows stay exactly as they are.
2. **The model.** Pin the previous version in `models.lock`. The application is untouched.
3. **The application.** Re-install the previous installer. Catalog migrations carry their own `DROP`
   sequence in each migration's header, so a downgrade is reversible — and every one of those
   headers says what the rollback *loses*, because a reversible migration that silently discards a
   photographer's edits is not a rollback anybody would take.

Section 14 asks for all three to exist. They do, and the ordering matters: reaching for the third
when the first would have done is how a fix becomes an outage.

## What this channel does not do

**It does not phone home from this build.** No transport ships (`ops/flags/flags.toml`'s
`[reporting].endpoint` is empty, and `scripts/check-banned.sh` refuses outbound networking outside
`aura-cloud`), so the staged rollout is a specification here rather than a running service. That is
condition C6 in the phase 30 exit report.
