//! Share the bounded compact-repair allowance without starving later faces.
use aura_recipe::retouch_tools::Edit;

use super::Report;

pub(super) struct FaceRepairs {
    pub assessment: usize,
    pub proposed: usize,
    pub non_spots: usize,
    pub edits: Vec<Edit>,
}

/// Max-min allocation: small plans are satisfied, then the remaining faces share
/// capacity equally. Stable face order breaks single-slot ties. Within a face,
/// the detector's candidate priority remains unchanged.
fn allocation(proposed: impl Iterator<Item = usize>, capacity: usize) -> Vec<usize> {
    let counts: Vec<_> = proposed.collect();
    let mut slots = vec![0; counts.len()];
    let mut remaining = capacity;
    while remaining > 0 {
        let mut changed = false;
        for (slot, count) in slots.iter_mut().zip(&counts) {
            if *slot < *count {
                *slot += 1;
                remaining -= 1;
                changed = true;
                if remaining == 0 {
                    break;
                }
            }
        }
        if !changed {
            break;
        }
    }
    slots
}

pub(super) fn rebalance(faces: &mut [FaceRepairs], capacity: usize) {
    let slots = allocation(faces.iter().map(|face| face.proposed), capacity);
    // Adding faces and reserving more finishing can only reduce earlier shares.
    // Trim after each face so retained proposals never exceed the recipe budget;
    // only the newly planned face's bounded candidate list is temporarily extra.
    for (face, slots) in faces.iter_mut().zip(slots) {
        face.edits.truncate(slots);
    }
}

pub(super) fn finish(faces: Vec<FaceRepairs>, spots: &mut Vec<Edit>, report: &mut Report) {
    for face in faces {
        let retained = face.edits.len();
        report.operations -= face.proposed - retained;
        if let Some(assessment) = report.assessments.get_mut(face.assessment) {
            assessment.spots_healed = retained;
            if retained < face.proposed {
                assessment.findings.push(format!(
                    "Operation budget: retained {retained} of {} proposed spot repairs after sharing the remaining allowance across all faces; existing edits are preserved.",
                    face.proposed,
                ));
            }
            if face.non_spots == 0 && retained == 0 {
                assessment.status = "skipped".into();
                assessment.reason =
                    "The saved retouch stack has reached its operation limit.".into();
                report.retouched_faces -= 1;
            }
        }
        spots.extend(face.edits);
    }
}

#[cfg(test)]
mod tests {
    use super::allocation;

    #[test]
    fn a_dense_middle_face_does_not_starve_the_last_face() {
        // Counts from the paired three-person regression. Nineteen non-spot
        // operations have already been reserved from the 1024-slot recipe.
        assert_eq!(
            allocation([278, 581, 201].into_iter(), 1005),
            [278, 526, 201]
        );
        assert_eq!(allocation([900, 900, 24].into_iter(), 1000), [488, 488, 24]);
        assert_eq!(allocation([0, 900, 900].into_iter(), 1), [0, 1, 0]);
        assert_eq!(allocation([900, 900].into_iter(), 0), [0, 0]);
    }

    #[test]
    fn later_reservations_only_shrink_previous_allocations() {
        let counts = [900, 12, 0, 581, 201, 900, 24];
        for initial in [0_usize, 1, 17, 512, 1005, 1024] {
            let mut previous = Vec::new();
            for end in 1..=counts.len() {
                let capacity = initial.saturating_sub(end * 3);
                let next = allocation(counts.iter().take(end).copied(), capacity);
                assert!(next.iter().sum::<usize>() <= capacity);
                assert!(next
                    .iter()
                    .zip(&counts)
                    .all(|(used, proposed)| used <= proposed));
                assert!(previous
                    .iter()
                    .zip(&next)
                    .all(|(before, after)| after <= before));
                previous = next;
            }
        }
    }
}
