# Feature flags and kill switches

`flags.toml` is read at start-up. If it cannot be read, parsed, or if it asks for something the
build does not have, **the application halts** with `AURA-REL-12003` rather than starting on
defaults.

## Why halting is right here

Every other policy table in this product falls back or refuses per row. This one does not, because
of what it is: the kill switch for every AI stage. A build that could not read its flags and started
anyway would be a build running stages somebody had switched off — which is the exact situation the
switch exists for, and the exact situation in which nobody is watching.

## What a studio may change

Any `true` to `false`. That is the whole of it.

The loader refuses a file that turns a stage **on** that this build does not have, for the reason
every config table since phase 21 has: a studio may tighten and never widen. A flags file that could
enable a half-built stage by naming it would be a flags file that ships a feature nobody tested.

## What is off by default, and why

**`cleanup`.** Generative cleanup replaces pixels that were in the photograph with pixels that were
not. Phase 24 section 2.2 is a list of what it must never do, and its safety engine is real — but
the decision to run it at all is a studio's, taken deliberately.

**`learning`.** The learning loop changes what the product will do to the *next* wedding. Consent is
per project on top of this switch, and both have to be on: the flag is the studio's answer and the
consent is the couple's.

**`cloud.enabled`, `crash_reports`, `telemetry`.** Section 7 of phase 30 says this phase makes no
cloud call and works with the network cable unplugged. These are off so that a studio with a policy
against outbound traffic can point at a file rather than take our word for it.

## Rolling a release back

Section 14's rollback path is three things and this is the first:

1. **Feature flag off.** Immediate, no reinstall, no restart of anything but the app.
2. **Previous model version pinnable.** `models/models.lock` plus `ops/update/`.
3. **Catalog migration reversible.** Every migration's header carries its own `DROP` sequence.

A release that cannot do all three is a release that cannot be undone, and section 6.4 asks for "a
documented rollback within one release cycle".
