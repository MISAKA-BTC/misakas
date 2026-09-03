# Certifying a new model on chain (ADR-0075)

A class holds weight only when a family the court has drilled end to end covers every kernel its
graph reaches (ADR-0069). Since ADR-0075 that family, and the free-prompt certification of a
class, are chain state carried by ordinary transactions. Nobody's permission is involved: the
court grades the evidence in the transition, and the transaction fee is the rent.

## What you need

* A node of this build synced to the network (`kaspad`), with a funded key file for fees.
* The model's catalog id (a row `misaka-palw-sdk` can express; `palw-class list` shows them).
* The `palw-certify` and `misaka` binaries (the crate is `misaka-cli`; the BINARY it builds is `misaka`) from the same build.

## Steps

```bash
# 1. Register the class. Weightless (0‰) if no certified family covers it yet; at the floor
#    share if one does — the node prices it from the chain's own certified set.
kaspad ... --palw-register-class "<model id>" --palw-producer-bond <txid>:<index> ...

# 2. Post the drill of the family that covers the model's kernels (once per family per lane).
palw-certify drill --model-id "<model id>" --lane attempt --out family-attempt.obj
misaka palw submit-object --key-file <seed> --object family-attempt.obj --yes

# 3. Bind the class to that family: seated at the floor share, weight-bearing.
palw-certify bind --model-id "<model id>" --lane attempt --out class-attempt.obj
misaka palw submit-object --key-file <seed> --object class-attempt.obj --yes

# 4. (Optional) The free-prompt lane, the same way.
palw-certify drill --model-id "<model id>" --lane fp --out family-fp.obj
misaka palw submit-object --key-file <seed> --object family-fp.obj --yes
palw-certify bind --model-id "<model id>" --lane fp --out class-fp.obj
misaka palw submit-object --key-file <seed> --object class-fp.obj --yes
```

**A `FamilyCertified` does not fit one carrier, and the `--object <file>` lines above are only
half the story.** A drill's evidence is far larger than a standard transaction: the integer
floor's free-prompt family object is **214,243 bytes** against a 100,000-byte carrier. When that
happens `palw-certify` writes the pieces beside the file it was asked for —
`family-fp.obj.chunk0`, `.chunk1`, `.chunk2` — and says so, and `submit-object` must be given
each of them, in index order:

```bash
palw-certify drill --model-id "<model id>" --lane fp --out family-fp.obj
# -> wrote family-fp.obj.chunk0 .chunk1 .chunk2 — submit the chunks in order
for c in family-fp.obj.chunk*; do
  misaka palw submit-object --key-file <seed> --object "$c" --yes
done
```

The chain assembles the group and applies the object **in the block that completes it**, so the
acceptance you are waiting for appears once, after the last chunk, and not after each. Submitting
the un-chunked `family-fp.obj` is refused — it is over the carrier — and a partial group simply
never applies. `palw-certify inspect` reads a chunk as well as a whole object.

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

## Producing free-prompt claims on the certified class

Once a class's free-prompt lane is certified (genesis or on chain), `misaka-palw-gateway
--worker <binary>` turns browser prompts into commitments. Two workers ship:
`palw-a16-fp-worker` (dense tier, `MISAKA_PALW_ARTIFACT` + `MISAKA_PALW_TOKENIZER`) and
`palw-qwen36-fp-worker` (hybrid tier, `MISAKA_PALW_ARTIFACT` = `.palwq36`, `MISAKA_PALW_GGUF` =
the checkpoint whose header carries the tokenizer, optional `MISAKA_PALW_MODEL_ID` for another
graph-v3 row). Both take `MISAKA_PALW_NETWORK_ID`. The rail's `--class-id` and `--class-leaves`
name the class and its canonical job in leaves.

**Three modes, and the gateway uses the third** (ADR-0077 Decision 1). Both workers answer
`--mode v3-manifest` (print the identity and exit), `--mode v3-job` (one framed request in, one
result out — what the drills and the replay arm use) and `--mode v3-serve`, the resident loop the
gateway spawns: the artifact is mapped, digested and validated ONCE, and every later job travels
the same framed request/result pair over the persistent stream. On the hybrid tier that is the
difference between eight minutes per request and eight minutes per process. A job's four roots are
byte-identical whichever of `v3-job` and `v3-serve` produced them, which is what makes residency a
cost decision rather than a semantics one.

You do not pass `--mode` yourself: `--worker <binary>` is enough, and the gateway spawns it as
`--mode v3-serve --trace-out <outbox>/traces/...`. Run `--mode v3-manifest` by hand when you want
to see which class id, `n_ctx` and end-of-generation ids a worker will announce before you point a
gateway at it.

## Limits, stated

* A drill certifies kernels, not weights. A model whose graph reaches a kernel no shipped family
  drills (`palw-certify drill --model-id` says so) is a new architecture and needs a build whose
  court serves it.
* There is no revocation. A misbehaving class is frozen by contradiction (`ClassFrozen`), as
  before.
* Mainnet ships PALW off; the bundle it activates is built by the same code path, so the route is
  the same there.
