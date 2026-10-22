# Evoto continuation: file coverage, resource formats and unresolved reconstruction

The target is the supplied `F:/evoto` installation. This continuation reuses the
earlier exact-worker investigation and adds a fresh full-file consistency pass,
resource-format classification and REA MCP section-layout observation. It does
not establish complete decompilation or matching numerical output.

REA MCP `inspect_with_objdump` inspected `F:/evoto/Evoto-worker.exe` with
`operation: section_headers`. Retained evidence
`ev_3f7a60e930868de6e68156879bfc76e697535b837a336fb0be273ea659b61f54`
reports complete, untruncated tool output, exit zero, and unchanged artifact
SHA-256 `add7588f94aed190b97076f949709ba9622aa14f046b661783135d838d3d3bb2`.
The section table includes ten sections, with both code and data mappings.
The provider retained an objdump section-flag warning. Section names and flags
alone do not identify models, protection schemes or algorithms.

The fresh file pass hashed all 1,860 regular files (4,873,227,287 bytes), with no
read gaps. All 1,854 vendor MD5 observations match; these are consistency checks,
not authenticity guarantees. The tree manifest remains
`61bf5b14dca7394f04d1e0ca576a442dae62fa7156e5c727d9aea2f395363162`.

Every one of the 621 resources carrying a `.json` extension was read, bound to its
inventory SHA-256 and offered to a strict UTF-8 JSON parser. Three parsed; 618
were opaque or non-JSON. The parser failure does not establish encryption,
compression, an undocumented encoding, or any particular protection mechanism.
The examined skin-tone and dermabrasion effect resources fall within this
unresolved resource boundary. No opaque bytes were treated as a recovered model
or numerical specification.

The current REA session reports no configured deep native provider: Ghidra has
no install directory and its Java launcher probe also fails the absolute-path
requirement; IDA and Kuna are unconfigured, and Hopper is unavailable on this
host. No unrelated tools were installed or global configuration changed. The
section inspection is a successful supported REA operation, separately from
that unavailable decompiler capability. No native analysis target was opened.

| Requested question | Status | Evidence boundary |
|---|---|---|
| Account for every regular installation file | Answered for this inventory | Complete hashes, format headers, unchanged tree digest and vendor consistency checks |
| Decode all shipped effect configurations | Partially answered | All 621 extension matches inspected; 618 opaque/non-JSON resources remain undecoded |
| Explain skin/body pipeline prerequisites | Partially answered | Earlier same-SHA bounded wrapper and disassembly evidence; selected reference, landmarks and semantic masks, not trained numerical filters |
| Recover every algorithm and learned model | Unresolved | No configured deep provider; no validated architecture, weights, training data or full numeric transform |
| Establish exact AURA/Evoto tone and per-tool perfection | Unresolved | Prior native Evoto export was trial-gated; the preserved watermarked preview has unknown display transform |
| Implement original AURA improvements and further capabilities | Separate source task | See implementation ledgers and actual AURA validation; this does not turn independent code into recovered Evoto code |

No inspected proprietary code, weights, shaders, configuration values or image
assets were incorporated into AURA. No licensing/export gate was bypassed.

[REA evidence](../../.work-checks/evoto-advanced-2026-10-22/rea-evidence.json),
[provider status](../../.work-checks/evoto-advanced-2026-10-22/provider-status.json),
[file inventory](../../.work-checks/evoto-advanced-2026-10-22/file-inventory.json),
[resource classification](../../.work-checks/evoto-advanced-2026-10-22/configuration-classification.json),
[earlier bounded skin/body evidence](AURA-Evoto-evidence-2026-10-22.md).
