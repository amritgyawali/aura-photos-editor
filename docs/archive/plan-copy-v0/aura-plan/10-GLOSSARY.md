# Glossary

Use these exact terms in code, UI and documentation. Consistent vocabulary is how a 30-phase build stays coherent.

| Term | Meaning |
| --- | --- |
| **Autonomy band** | Confidence range that determines whether a decision auto-applies, applies in Zero-Touch, is suggested, or requires review (0.98+, 0.90-0.98, 0.75-0.90, under 0.75). |
| **Anchor** | A reference frame within a scene node whose colour and exposure the other frames are normalised toward. |
| **Chapter** | A user-facing grouping of segments (Getting Ready, Ceremony, Reception). |
| **Content hash** | xxhash3-128 of file bytes; identity of an image for caching and deduplication. |
| **Coverage rule** | A hard requirement that certain moments or people appear in the final selection. |
| **Decision** | A ledger record: what was chosen, why, with what confidence, by which actor. |
| **Develop engine** | The GPU pipeline that turns a RAW plus a recipe into pixels. |
| **Duplicate set** | A group of near-identical frames classified Identical, NearIdentical or Variant. |
| **Explain My Edit** | The UI surface that renders ledger reasons for culling and editing decisions. |
| **Frame integrity** | Technical quality evidence: focus, motion, clipping, noise, eye state. |
| **Gallery Brain** | Cross-photo reasoning: consistency, anchors, skin and scene harmonisation. |
| **Hardware plan** | Probed execution-provider order and resource budgets written at first run. |
| **Keep score** | Composite culling score combining integrity, emotion, composition, story value and uniqueness. |
| **Ledger** | Append-only store of all decisions, reasons and confidences. |
| **Mask kind** | Semantic class of a mask (skin, face, hair, clothing, subject, sky, ...). |
| **Moment** | A short burst of frames capturing the same event instant. |
| **Photo Brain** | Per-image technical and aesthetic decision-making. |
| **Pipeline version** | Version stamp that invalidates caches when decode or preview algorithms change. |
| **Proxy** | Tier-2 render at 2048 px used for analysis and interactive editing. |
| **Recipe** | The non-destructive JSON description of every edit applied to an image. |
| **Reason** | Structured explanation record attached to a decision. |
| **Ritual** | A culturally specific ceremony element (varmala, saptapadi, sindoor, vows, ring exchange, nikah). |
| **Runner-up** | The best non-selected frame in a moment, retained for QC replacement. |
| **Scene node** | A node in the wedding tree that groups images for consistency solving. |
| **Segment** | A contiguous time range assigned a scene class by the story model. |
| **Skin guard** | Constraint that prevents grading from pushing skin outside a plausible locus. |
| **Story graph** | The full wedding structure: chapters, segments, nodes, moments, people. |
| **Style tree** | Scene-conditional personal profile: global, group and bucket-level deltas. |
| **Subject hierarchy** | Ranking of people by role and prominence (couple, close family, VIP, guest). |
| **Texture retention** | Measured ratio of high-frequency energy in skin after retouching versus before. |
| **Ticket** | A QC finding with diagnosis, quantified deviation, remedy and status. |
| **Wedding Brain** | Scene, story, people and moment understanding. |
| **Zero-Touch** | The mode in which every stage runs autonomously within its autonomy bands. |
