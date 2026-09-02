# Crash reporting and telemetry

Both are **opt-in**, both are off in `ops/flags/flags.toml`, and neither ships a transport in this
build.

## What a crash report contains

The stack, the build, the platform, the render backend, which feature flags are off, and the last
three error codes with their runbook links.

## What it never contains

**No image content.** Not a thumbnail, not a crop, not a preview, not a histogram. Section 6.4:
"crash reporting and structured telemetry are opt-in, contain no image content".

No file paths, no couple names, no venue, no folder structure. A path is a name and a date and
often a place, which is three facts about somebody who did not agree to anything.

No identity templates, no face data, no embeddings. Phase 06's biometric store is sealed and nothing
outside `aura-people` can open it; a crash reporter that could would be the one exception, so it is
not one.

`aura-explain`'s support bundle already solves this problem for support cases: it replaces every
identifier with a handle and `Evidence` has no variant that could hold image bytes. A crash report
is the same discipline with less in it.

## What telemetry contains

The five events section 11 names, and nothing else:

| Event | Fields |
|---|---|
| `export.job` | sets, images, format, ms, verified, destination_kind |
| `delivery.upload` | provider, images, bytes, ms, resumes |
| `learn.corrections` | kind, count, mean_magnitude |
| `learn.update` | profile, expected_improvement, adopted |
| `release.update` | from_version, to_version, channel, rolled_back |

Every field is a count, a duration or a closed-set word. There is no field in that table that could
hold a name, a path or a photograph, which is what makes "no image content" a property of the shape
rather than a promise about the sender.

## Local first

Every event is written locally whether or not aggregation is on, and the diagnostics screen reads
them. A photographer who never opts in still gets the value of the measurement; what opting in adds
is that we see it too.

## Consent

Per install for crash reports and telemetry; per project for the dataset contribution, recorded in
`learn_consent` with the app version that asked — because a consent given to one release's wording
is a consent to *that wording*.

`docs/privacy.md` is the plain-language version, and it is the document a photographer reads rather
than this one.
