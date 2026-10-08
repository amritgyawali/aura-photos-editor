# AURA-REL-12005 - An export was refused: no active licence

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Your free trial has ended. Editing still works; enter a licence key to export finished photographs.

## What actually happened

`aura_app::licensing::require_export` found neither a valid licence nor a running trial, so the
export, the delivery or the autopilot's export stage wrote nothing. Every edit, cull and catalogue
is untouched - only writing finished files needs a licence.

## What to do

Open **Settings -> Licence** and paste the key from the receipt. A term licence that has ended
needs a renewed key. The licence lives in `licence.json` in AURA's data folder
(`%APPDATA%\AURA` on Windows); copying a catalogue to another machine does not carry it.

## Where it comes from

ADR-0105, `docs/adr/ADR-0105-installer-signing-and-licensing.md`.
