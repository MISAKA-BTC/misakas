# Certifying a new model on chain (ADR-0075)

A class holds weight only when a family the court has drilled end to end covers every kernel its
graph reaches (ADR-0069). Since ADR-0075 that family, and the free-prompt certification of a
class, are chain state carried by ordinary transactions. Nobody's permission is involved: the
court grades the evidence in the transition, and the transaction fee is the rent.

## What you need

* A node of this build synced to the network (`kaspad`), with a funded key file for fees.
* The model's catalog id (a row `misaka-palw-sdk` can express; `palw-class list` shows them).
* The `palw-certify` and `misaka-cli` binaries from the same build.

## Steps

```bash
# 1. Register the class. Weightless (0‰) if no certified family covers it yet; at the floor
#    share if one does — the node prices it from the chain's own certified set.
kaspad ... --palw-register-class "<model id>" --palw-producer-bond <txid>:<index> ...

# 2. Post the drill of the family that covers the model's kernels (once per family per lane).
palw-certify drill --model-id "<model id>" --lane attempt --out family-attempt.obj
misaka-cli palw submit-object --key-file <seed> --object family-attempt.obj --yes

# 3. Bind the class to that family: seated at the floor share, weight-bearing.
palw-certify bind --model-id "<model id>" --lane attempt --out class-attempt.obj
misaka-cli palw submit-object --key-file <seed> --object class-attempt.obj --yes

# 4. (Optional) The free-prompt lane, the same way.
palw-certify drill --model-id "<model id>" --lane fp --out family-fp.obj
misaka-cli palw submit-object --key-file <seed> --object family-fp.obj --yes
palw-certify bind --model-id "<model id>" --lane fp --out class-fp.obj
misaka-cli palw submit-object --key-file <seed> --object class-fp.obj --yes
```

`palw-certify inspect --object <file>` shows what a file carries and whether this build's court
grades it. `submit-object` grades a `FamilyCertified` locally before spending a fee, and refuses a
`ClassLaneCertified` whose profile does not hash to the class it names; the chain applies the same
checks, and a refused object is a dropped carrier (the block stands, the fee is gone, nothing is
recorded — the node logs it under `[palw-lifecycle]`).

## What the chain checks

| Object | Accepted when | Refused as |
|---|---|---|
| `FamilyCertified` | the court convicts every planted fault and acquits every honest run; ≤ 32 vectors; the family is not yet recorded for that lane | `CertificationRefused`, `TooManyDrillVectors`, `FamilyAlreadyCertified` |
| `ClassLaneCertified` (attempt) | the class is Active and holds no share; `profile` hashes to the class id; a chain family for the lane covers its kernels | `CertificationNeedsActiveClass`, `ClassAlreadyWeighted`, `CertificationProfileIsNotTheClass`, `NoCertifiedFamilyCovers` |
| `ClassLaneCertified` (free-prompt) | as above, and the class is not already free-prompt certified | `ClassLaneAlreadyCertified` |

## Limits, stated

* A drill certifies kernels, not weights. A model whose graph reaches a kernel no shipped family
  drills (`palw-certify drill --model-id` says so) is a new architecture and needs a build whose
  court serves it.
* There is no revocation. A misbehaving class is frozen by contradiction (`ClassFrozen`), as
  before.
* Mainnet ships PALW off; the bundle it activates is built by the same code path, so the route is
  the same there.
