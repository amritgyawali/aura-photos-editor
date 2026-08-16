# Third-party notices

AURA links third-party Rust crates. The licence policy is `deny.toml`, checked by
`cargo deny check` in CI lane 4: MIT, Apache-2.0 (with or without the LLVM
exception), BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib and MPL-2.0 are
allowed for every dependency. Anything else needs a scoped exception in that file
and an entry here.

## Independent JPEG Group (IJG) - `jpeg-encoder`

`jpeg-encoder` is distributed under `(MIT OR Apache-2.0) AND IJG`. The IJG half
carries one obligation, and this is it:

> This software is based in part on the work of the Independent JPEG Group.

The crate is used by `aura-raw` to write the JPEG previews and the exported
proxies. No IJG code is modified, and no claim is made that AURA is the original
software.
