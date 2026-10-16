# AURA-RAW-2009 - An iPhone HEIC or HEIF photo, which this build cannot decode

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`: the file is an iPhone
HEIC photo, and how to bring it in as JPEG instead. The file appears in the Problems list.

## What actually happened

The decoder recognised a HEIF container (`ftyp` with a `heic`, `heix`, `mif1` or similar
brand). HEIF is only the container: the pixels inside are compressed with HEVC (H.265), and
this build has no HEVC decoder. Writing one is far beyond a safe change to a decoder that
contains no `unsafe` code, and the operating system's own decoder needs a codec Windows does
not install by default.

## What AURA does automatically

The file is set aside with this code and the import continues. Nothing is written to it.

## Operator steps

1. On the iPhone: Settings > Camera > Formats > Most Compatible makes new photos JPEG.
2. For photos already taken: export them from Photos as JPEG (on a Mac, File > Export > Export
   Photo, JPEG), or let the iPhone convert them when copying (Settings > Photos > Transfer to Mac
   or PC > Automatic).
3. Import the JPEG copies. They open, edit and export like any other photograph.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Constructor: `heif` in `crates/aura-core/src/errors/raw.rs`
- Format support: `docs/camera-support.md`
