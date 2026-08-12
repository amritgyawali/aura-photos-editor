# Runbook - importing a wedding

Who this is for: support, and the engineer on call when a photographer says "my
import is stuck".

## What an import actually does

1. **Register the root.** Keyed by (project, absolute path), so re-importing the
   same folder reuses the same `root_id` instead of creating a second one.
2. **Scan.** Sorted by file name at every level, symlinks never followed, skip
   list applied (`.git`, Lightroom preview bundles, recycle bins, `@eaDir`).
   Unreadable and zero-byte files go straight to quarantine; the run continues.
3. **Skip pass.** For each path already in the catalog with a matching
   `fast_key` (`size:mtime_ms`), the file is touched as seen and never read.
   This is why a second import of an unchanged wedding hashes zero bytes.
4. **Hash.** Full-content BLAKE3, never sampled. Thread count comes from the
   storage class: 1 for a card reader, 4 for a local SSD, 2 for a network share.
5. **Pair.** Files sharing a folder and a stem become one photograph. RAW wins as
   primary; a sidecar is never primary.
6. **Read metadata.** EXIF failures are warnings, never item failures. A photo
   with no EXIF still imports with `capture_time_source = 'unknown'`; a capture
   time is never invented from a file name.
7. **Insert.** Batches of 200 in one IMMEDIATE transaction each, with the
   `import_run` counters updated in the same transaction as the rows they count.
8. **Align clocks.** Frames are grouped by body serial; the busiest body is the
   reference; every other body gets an offset estimated from a histogram of
   nearest-neighbour deltas. Below 0.6 confidence AURA refuses to guess, leaves
   the offset at zero and says which frames to compare.
9. **Mark absent.** Files under the root that this scan did not see are marked
   `is_present = 0`. Nothing is ever deleted: an unplugged drive is not a deletion.

## Common situations

| Symptom | Likely cause | Action |
|---|---|---|
| Import seems stuck at the same file count | Card reader on a shared USB hub, or antivirus scanning each file | Move to a direct port, exclude the card and catalog folders from real-time scanning |
| "The drive was disconnected" | Bus-powered reader browned out | Reconnect, press Resume; hashed files are skipped by `fast_key` |
| Second shooter's frames interleave wrongly | Offset estimated below the confidence gate | Open the camera panel, compare the two suggested frames, set the offset by hand; `set_camera_label` recomputes the timeline in one UPDATE |
| Photo count lower than file count | RAW + JPEG pairs, which is correct | Check `photo_file` count: it should match the file count |
| Re-import reports thousands of imports | `fast_key` mismatch, usually a copy tool that rewrote mtimes | Expected once; the content hash still prevents duplicate photographs |

## Diagnostics to collect

```bash
aura-cli info --catalog <path>            # schema version and row counts
```

Then, from the catalog:

```sql
SELECT state, files_discovered, files_imported, files_duplicate, files_quarantined
  FROM import_run ORDER BY started_at DESC LIMIT 5;

SELECT error_code, COUNT(*) FROM quarantine WHERE resolved_at IS NULL GROUP BY error_code;

SELECT phase, COUNT(*) FROM ingest_journal GROUP BY phase;
```

The journal answers "where did it stop" precisely; the quarantine table answers
"what did it refuse and why". Both are per-project and survive a crash.

## What never happens

- A file is never silently dropped. Everything is either imported, skipped as a
  duplicate, or in the quarantine table with a code and a runbook.
- An original file is never written to. Every open is read-only.
- A capture time is never guessed from a file name.
