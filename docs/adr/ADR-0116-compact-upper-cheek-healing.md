# ADR-0116: Compact repairs on confirmed upper-cheek skin

Status: Accepted
Date: 2026-10-13

The public paired portrait retained small marks beneath the eyes. Precise
healing inherited circular eye exclusions in both its skin selection and
its final search, in addition to the actual feature guard. Consequently,
confirmed cheek skin could never reach candidate or donor validation.

Planner `measured-features-v25` separates compact eligibility from broad
finishing. An upper-cheek extension is available only with Deep blemish cleanup
and Remove dark marks enabled. Its new selection requires skin-coloured cells
with original segmentation confidence above 224/255. Uncertain segmentation
retains the conservative compact-eye guard. Small enclosed mask holes continue
to follow the existing safe skin-fill rules.

New eligibility begins below 0.14 eye distances from each eye centre and becomes
fully eligible below 0.20. It fades out toward the nose-facing inner corner
between 0.20 and 0.28 eye distances. Rotation follows the measured eye axis.
The existing asymmetric lid guard additionally limits the extension. Complete
repair footprints, surrounding light, neutral-mark shape/context, clean donors,
residual-footprint exclusions and the final sampling-margin guard still apply.
Brow exclusions, nostril openings and the underside crease remain protected.
Broad smoothing, frequency healing and texture restoration keep their existing
orbital exclusions. Ordinary compact cleanup retains its conservative settings.

The regression exercises an actual rendered repair in previously excluded
skin, exact preservation of lid and crease pixels, rejection with uncertain
segmentation and rejection without the opt-in. Rotation tests check both
compact and broad guards, including unchanged eye and inner-corner cores.
Real-photo full-resolution native exports and visual review remain necessary.

This expands supported compact skin coverage. It does not classify every mark
as acne or establish complete removal, proprietary algorithm recovery or exact
Evoto equality. Protected anatomy, unsupported light and bounded operation
budgets can still refuse repairs; the image and recipe remain reviewable.

Native comparison found that combining the new eligibility with the primary
search changed existing component/donor choices and weakened some nose repairs.
The final implementation keeps the primary and residual searches conservative,
then checks the rendered primary result for supplemental upper-cheek repairs.
Their IDs and final feature mattes are separate. Primary parameters and ordering
remain intact; supplementary candidates cannot overlap a previous repair centre.
The group allocator reserves the original primary shares first. Supplemental
repairs share only spare capacity, so a full 1024-operation group may retain
none of them. Reports disclose proposed and retained supplemental counts.
