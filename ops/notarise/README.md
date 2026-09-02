# Notarisation

macOS only. Apple's service scans a signed build for malware and issues a ticket; `stapler` attaches
the ticket to the artefact so a machine with no network can still verify it.

## The order, and why getting it wrong wastes an hour

**Sign, then notarise, then staple.** Notarising an unsigned build is rejected; stapling before the
ticket exists silently does nothing; and signing *after* stapling invalidates the ticket. Each of
those takes a full submission round trip to discover, which on a busy day is twenty minutes each.

`ops/notarise/notarise.sh` does the three in order and refuses to skip.

## Why the ticket is stapled rather than left to Gatekeeper

Gatekeeper checks the notarisation online. A photographer installing on a venue's guest wifi, or on
a machine that has been offline for a week, gets a dialogue saying the developer cannot be verified
— which reads exactly like the dialogue for unsigned software.

A stapled ticket verifies offline. It costs one command and removes the only failure mode of this
whole mechanism that a photographer would blame on us.

## Credentials

An App Store Connect API key, held by the signing service, never in this repository and never on a
CI runner's disk. `notarytool` takes it by key id plus issuer id.

## What is notarised

The application bundle and the installer. Not the model packs — those are verified by
`aura-models`'s own ed25519 chain, which is platform-independent and is what makes a model rollback
possible without downgrading the application.
