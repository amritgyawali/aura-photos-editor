# AURA-SEC-9005 - A merge, split, rename or role change was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence - AURA could not make that change, nothing was changed - and the panel unchanged. The detail line names which rule refused.

## What actually happened

The People panel's edit surface is the only mutation path into the biometric store, so its refusals are grouped with the store's rather than with the model errors. Six rules can refuse:

| Detail | Rule |
|---|---|
| "an identity cannot be merged with itself" | a merge needs two distinct identities |
| "the identity being merged has no faces" | a merge of an empty identity would be a delete with extra steps |
| "face ... does not belong to that identity" | a split may only move faces the identity actually holds |
| "a split has to leave at least one face behind; rename the identity instead" | a split that moves everything is a rename, and would leave an empty card in the panel |
| "that identity does not exist in this project" | a stale id, usually from a panel that was not refreshed after a regroup |
| "that identity does not exist" | the same, for a rename, role or importance change |

The fourth one is the least obvious and the most useful. A "split" that moves every face is not a split: it creates a new identity with everything in it and leaves the original as an empty card that the panel then has to hide or explain. Refusing it and pointing at rename is the right answer, and it is a real product decision rather than a validation nicety.

## What AURA does automatically

Refuses before writing anything. No faces move, no identity is created or deleted, and **no journal entry is recorded** - which matters, because a journal entry for a refused edit would be replayed onto the next regroup and would then be applied.

## Operator steps

1. Read the detail line. All six causes are self-describing and five of them are fixed by the photographer doing something slightly different.
2. For "does not exist in this project", refresh the People panel. A regroup after a model change produces new identity ids, and a panel holding the old ones will refuse every edit. This is expected after `AURA-ML-5018`.
3. For a split that wants to move everything, use rename.
4. For a merge that seems obviously right and is refused, check whether one of the two identities has already been merged away by an earlier action - the survivor keeps its id and the absorbed one is deleted.
5. If none of the six explains it, capture the diagnostics bundle. The refusals are exhaustive, so an unexplained one is a bug.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The edit surface: `crates/aura-people/src/api.rs`
- Cross-project refusal: `docs/runbooks/AURA-SEC-9004.md`
- Version drift after a model change: `docs/runbooks/AURA-ML-5018.md`
