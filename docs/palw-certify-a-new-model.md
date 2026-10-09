# Certifying a new model on chain (ADR-0075)

> **PALW共通前提 — 2026-10-10:** [ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


A class holds weight only when a family the court has drilled end to end covers every kernel its
graph reaches (ADR-0069). Since ADR-0075 that family, and the free-prompt certification of a
class, are chain state carried by ordinary transactions. Nobody's permission is involved: the
court grades the evidence in the transition, and the transaction fee is the rent.

## Which families this build can drill

`palw-certify drill --family <base0|qwen36|a16|a16-v5|qwen36-v6>`, or `--model-id` to let the tool
pick the family whose drilled kernel set covers the row. Two of the five exist because a class IS
its graph: `a16-v5` is the dense lineage's fused graph (ADR-0082) and `qwen36-v6` (2026-09-23) is
the hybrid lineage's fused, per-token-lift graph — the kernel set of every HELD hybrid row
(`Qwen3.6-35B-A3B/graph-v7@<n_ctx>`, ADR-0103's map over graph-v6). A held hybrid row is two
kernels outside `qwen36` (the fused attention and the by-token lift), so before `qwen36-v6` no
family covered it: it could register only weightless and never carry a free-prompt certification.

## What you need

* A node of this build synced to the network (`kaspad`), with a funded key file for fees.
* The model's catalog id (a row `misaka-palw-sdk` can express; `palw-class list` shows them).
* The `palw-certify` and `misaka` binaries (the crate is `misaka-cli`; the BINARY it builds is `misaka`) from the same build.

## Steps

**Never start a second `kaspad` with a bond that a node already runs.** Since 2026-09-25 a node's
duties have no off switch. A `kaspad` given a bond's key and outpoint runs that bond's seat duties
and its execution-lane round blocks, even when it was started only to register. Two such processes
at once sign the same round permit twice, and the chain slashes the bond
(`RoundPermitEquivocated`). Register in one of these ways:

* through the running node, with `misaka model add` (RPC only, no second process);
* by restarting that node itself with the flag, and removing the flag once the class is on the
  chain (`docs/palw-add-a-model-runbook.md` §5);
* with a bond that no process runs yet. The registration run then stays that bond's node.

```bash
# 1. Register the class. Weightless (0‰) if no certified family covers it yet; at the floor
#    share if one does — the node prices it from the chain's own certified set. THIS is the node
#    that runs the bond (see above): never a second process beside it.
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
