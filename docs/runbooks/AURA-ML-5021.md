# AURA-ML-5021 - The project has more faces than the documented in-memory ceiling

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. Grouping takes longer and produces the same answer.

## What actually happened

The project holds more than `aura_people::api::FACE_CEILING` - 25,000 - faces. That is section 11's budget figure, and in practice it is a three-day wedding or a very large multi-shooter one.

Nothing breaks past it. The expensive part of clustering is a quadratic nearest-neighbour pass, and it is already bounded independently of project size by `aura_vision::face::cluster::MAX_SKELETON` (4,096): the skeleton is built from the highest-quality voting faces, capped, and everything else is placed by a linear nearest-centroid pass with a margin. That is section 6.2's two-pass strategy, and the cap is what makes it hold at any size.

What this code reports is honest slowness. A three-day wedding taking a minute to group people needs an explanation rather than a spinner.

## What AURA does automatically

Logs the code with the face count and the ceiling, and completes the pass. The skeleton cap means the arithmetic does not grow quadratically with the project; the linear second pass does grow, at roughly a millisecond per hundred faces.

## Operator steps

1. Let it finish, then check `GroupReport::elapsed_ms` against the face count. Linear growth is expected; quadratic growth is a bug worth reporting.
2. If quality is poor as well as slow, read `GroupReport::skeleton_faces`. At the cap the skeleton is 4,096 of the *best* faces, which is a very large sample of the tens of identities a wedding has. If the wedding genuinely has hundreds of identities that sample is thinner, and `rank_refusals` and `unassigned` are the numbers to read.
3. Splitting a multi-day event into one project per day is a legitimate answer, and it is also a change of meaning: identities never cross projects (section 2.2), so the same guest becomes two people. Say that to the photographer before recommending it.
4. Do not raise the ceiling to make the message go away. It is a report, not a limit.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The similarity-index equivalent: `docs/runbooks/AURA-ML-5016.md`
- Budgets: `perf/budgets.toml`
