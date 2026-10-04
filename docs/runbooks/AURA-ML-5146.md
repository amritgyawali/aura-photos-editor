# AURA-ML-5146 - A reference a look was to be measured from was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing changed. No look was stored, no photograph was edited, and the panel says what went wrong
and what to do instead.

## What actually happened

One of four things, and they are separate `LookCode`s in the report even though they share an
error code here:

* **The address would not parse.** `ReferenceOrigin::parse` refuses empty text, a handle longer
  than Instagram issues, a handle with characters it does not issue, and anything shaped like a
  file path rather than a page. That last one is the guard worth knowing about: `../../etc/passwd`
  has a dot and a slash, and without the hostname check it would have been recorded as a web
  reference. It is only ever a label - nothing dereferences it - but it is a label a photographer
  would see.
* **The folder held fewer than `MIN_REFERENCES` readable photographs.** Eight. This is a cliff and
  it is deliberate: a look measured from four photographs is not a weak claim about a page, it is a
  confident claim about four photographs, and every robust statistic underneath it degenerates into
  whatever those four happened to be.
* **The folder held nothing this build reads.** JPEG, PNG and WebP. RAW files are deliberately not
  read: a reference is somebody's *finished* work, and a RAW in a reference folder is an original
  nobody has graded, so measuring a look from one would measure the camera.
* **The route was `public_url`.** See below.

## The route this build does not have

`MediaSource::PublicUrl` refuses on every call, for two separate reasons that happen to point the
same way.

The first is a property of this repository. `scripts/check-banned.sh` fails the build on an
outbound socket anywhere outside `aura-cloud`, and `aura-cloud`'s transport is a hand-written
HTTP/1.1 client with **no TLS** - ADR-0009 waived it - so there is no route from this process to an
`https://` host at all, for any purpose. Phase 04's rule is that the gateway is the only crate that
opens a socket, and a client-gallery fetcher is not a model provider.

The second is about the platform rather than about us. Reading a page's media in bulk is something
Instagram grants through its own API to the account that owns the page. The unofficial routes -
scraping the web view, replaying a private endpoint - are against its terms, break without notice,
and would put a photographer's own account at risk to save them a folder drag.

ADR-0063 section 4 has the full argument, and `docs/match-a-look.md` says the same thing in the
product's own words.

## What to do

1. **Save the photographs you want to match into a folder.** Anything you have the right to keep:
   images you have saved, a client's mood board, your own earlier work.
2. **Or ask Instagram for your data export** and point AURA at the folder that arrives. It
   understands the export's own layout - `media/posts` and the two other shapes it has had - and
   falls back on walking the whole tree when the layout has moved again.
3. **Paste the page address anyway.** It is validated and stored beside the look, so the report
   says which page the look is from even though the files arrived another way.
4. Check the folder has at least eight photographs AURA can read, and ideally
   `USABLE_REFERENCES` - twenty-four - before the look stops being called rough.

## What this error never means

It does not mean the account does not exist. **Nothing here resolves anything**, so a handle that
does not exist parses exactly as well as one that does, and the panel never renders a tick.
