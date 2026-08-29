# AURA-ML-5099 - The retouch preset table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No retouching at all, on any photograph, and a message saying so. Everything else in the product
works.

## What actually happened

`crates/aura-retouch/config/retouch_presets.toml` failed to load, and the refusal is
**whole-file** rather than per row. Half a preset table would retouch the ceremony against
measured strengths and the reception against nothing, and that inconsistency is invisible in the
delivered gallery - which is exactly the failure a gallery-consistent retoucher must not have.

The loader refuses:

* a missing or unparseable file;
* a preset whose texture floor is below `POLISHED_FLOOR` (0.80). Section 6.3 of PHASE-20 says
  "never below 0.80 even in Polished", and that is a claim the product makes in
  `docs/retouch.md`. A text file must not be able to retract it;
* a strength, cap or limit outside its documented range;
* a scene row naming a scene that is not in the phase 07 vocabulary;
* a row with no written reason. Every threshold in this file is a product decision, and a
  threshold nobody can explain is one nobody can defend.

## What to do

1. Reinstall, or restore the file from the release. It ships inside the binary as well - the
   embedded copy is what `PresetTable::embedded` reads, so a refusal here means the on-disk
   override is broken.
2. The error names the key and the rule it broke.
3. Do not "fix" it by lowering a texture floor. The floor is the phase's headline guarantee.
