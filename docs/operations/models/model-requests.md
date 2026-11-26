# Requesting a model, and what happens to a request

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([旧ADR-0174](../../rfc/0001-principles-and-governance.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

A request is the door 旧ADR-0096 Decision 13 puts on a path that already existed end to end and had
no place to knock: the [model request form](https://github.com/MISAKA-BTC/misakas/issues/new?template=model-request.yml)
in this repository's issue tracker. This page says what you are asking for when you file one,
which hands the request passes through, which ADR governs each hand, and what a request can and
cannot do. The short version: a request is public, it is not a bond, it buys no priority, and it
moves no consensus object.

## What you are asking for

A model on a MISAKA network is a **class**, and a class is four agreements that hold at once
(`docs/palw-model-onboarding-sdk.md`): a graph the court can walk, whose id IS the class id; an
artifact whose root the chain pins; a canonical job the class is paid per; and an engine that
executes what the graph describes. A request names the weights; everything else is made from
them by people, with tools this tree ships, and priced by the chain. Nothing in the list below is
a permission — the court grades the evidence in the transition, and the fee is the rent
(旧ADR-0075).

## The pipeline

| # | step | who | governed by |
|---|---|---|---|
| 1 | **Request.** The issue form asks what a class needs known up front: weights, license, architecture, parameter count, quantization, the context the request needs, which lanes, who converts, who bonds, and what machine the requester has. The Studio's Network tab has a "Request a model" button that opens the form with the machine block filled in from what it knows (RAM, accelerator, the classes it holds); any field can be prefilled by its id as a query parameter. | the requester | 旧ADR-0096 Decision 13 |
| 2 | **Conversion to an artifact.** The family's converter turns the public weights into the integer container the runtime maps: `qwen25-convert … --a16` for the dense tier, `qwen36-convert --url <gguf> --header … --context 512` for the hybrid tier. A known lineage is data (a geometry constant, a table row, `cargo test -p misaka-palw-sdk`); a new architecture is code first — a profile whose kernels the court catalogs, an engine, a container, a converter, then one `PalwModelLineageV1` impl. Before any node or coin: `palw-class inspect` (which class the file pairs with, under which root) and `palw-class preflight --network … --model-id …` (the SDK's `registration_candidate` / `preflight_admission` — the real admission gate, `verify_class_admission_v2`, run offline). | the converter (the requester, or whoever picks the request up) | `docs/palw-model-onboarding-sdk.md`; 旧ADR-0056 Decision 1 (admission is arithmetic) |
| 3 | **The class id and the artifact root.** The class id is the graph's, computed by the node (`shape_profile_id()`); the root the chain pins is the artifact's INVENTORY root — what `qwen36-run --artifact … --root-only` prints on the hybrid tier and `palw-class inspect` on both, and what `getPalwProducerFacts.artifactRoot` reports once registered. It is not the file's sha256. Rebuilding the artifact from the same weights lands on the same root, which is the only reason downloading someone else's conversion is safe: the Studio's `PalwClassSpec` pins that root and the file's sha256 side by side, and the components manifest carries both as `artifact_root` and `sha256` (`docs/components-manifest.md`). | the converter | 旧ADR-0056 Decision 6 (duplicate roots are priced, not policed) |
| 4 | **Registration.** From the node that holds the artifact: `misaka model add <model id>` against it, or `kaspad --palw-class-artifact <file> --palw-register-class <model id> --palw-producer-bond <txid>:<index> …` with a bonded key. **That `kaspad` must be the only process running the bond.** Since 2026-09-25 it runs the bond's duties, round blocks included, and a second process beside a live node double-signs its round permits and is slashed (`docs/palw-add-a-model-runbook.md` §5). The node reads live terms, applies the known-weights rule (weights the chain already has never candidate for a new class) and the sibling filters, runs the admission preflight again, and only then signs and funds. Entry is priced in bonded collateral by the chain's own terms — share, target, slash value are the chain's, never the registrant's to choose — and a class registers weightless (0‰) until a certified family covers it. | the registrant (a bonded key) | 旧ADR-0056 Decision 3 (registration exposure); 旧ADR-0075 Decision 4 (every gate reads genesis ∪ chain) |
| 5 | **Certification.** `palw-certify drill --model-id … --lane attempt\|fp --out …` writes the family's evidence (once per family per lane; chunked when it exceeds one carrier), `misaka palw submit-object` carries it, the transition grades it (`FamilyCertified`), then `palw-certify bind` seats the class at the floor share, weight-bearing (`ClassLaneCertified`). A drill certifies kernels, not weights: a model whose graph reaches a kernel no shipped family drills is a new architecture and needs a build whose court serves it. | anyone with a funded key; usually the registrant | 旧ADR-0075 Decisions 1, 2, 5, 7; `docs/palw-certify-a-new-model.md` |
| 6 | **Model identity, owner and market.** The registered model identity and its owner are authenticated native facts. Under MOE2 an ADDITIONAL registration atomically creates its unique market and initial inventory/reserve state; GENESIS and FORK_DESIGNATED registrations create no Option market. Transferring a Position does not transfer the model owner, management keys, model bytes or historical liabilities. | the registrant / native transition | RFC0064; ADR0179/0232 |
| 7 | **The market already exists at registration.** There is no normal registered-without-market or seed-waiting state. Optional seed/deepen adds liquidity to the existing additional-model market; it does not create mining eligibility. MOE2 allocates 30% of that model's priced/capped worker reward to Position purchase, 20% BILI plus the required portion of those purchased Positions to permanent liquidity, and the remaining 50% nominally to the versioned reward plan. Only excess purchased Positions are retired; neither internal leg adds BILI burn or an owner fee. Core models have no Option market and allocate 100% of R to the Miner; their verification service fees use separate pre-funded contracts. | the native transition / optional liquidity contributor | RFC0064; ADR0091/0232 |
| 8 | **What the Studio then shows.** A class row with its readiness — `ReadyBuiltIn` for the floor, `ArtifactPresent { verified }`, `ArtifactMissing { downloadable }`, `ArtifactMismatch` — and, for a downloadable class, the artifact row from the components manifest (`url`, `sha256`, `size`, `class_id`, `artifact_root`), installed through the download manager that verifies the digest. The offline copy in `PalwClassSpec.artifact` is pinned to the manifest's sha256 by a test. | the Studio, from the manifest | 旧ADR-0096 Decision 10; `docs/components-manifest.md` |

## What a request cannot do

* **It is not a bond and not a registration.** No key signs anything when a request is filed;
  nothing reaches the mempool. The registration's exposure is the registrant's (旧ADR-0056
  Decision 3), the line is the bonder's (旧ADR-0088 Decision 1), and the seed is the seeder's
  (旧ADR-0090) — a request has none of the three.
* **It buys no priority.** The queue is the tracker, in the open. A request's age, its author, its
  reactions or its comments change nothing about what the chain prices or admits — admission is
  arithmetic (旧ADR-0056 Decision 1) and certification is graded evidence (旧ADR-0075 Decision 2).
* **It moves no consensus object and widens no row.** The shipped rows are 512 tokens; a wider
  row is a new class at mint, never a raised ceiling on a running chain (旧ADR-0092 Decision 4), so
  a request for more context is a request for the next mint and says so in its `context` field.

## How long it takes

Honest numbers, from what has been run:

* **Conversion** is hours on a workstation. The dense 1.5 B artifact is 1.8 GB (1,795,427,276
  bytes); the hybrid QWEN36 artifact is 34 GiB converted from a Q4_K_M GGUF, and the hybrid
  runtime maps it whole, which is why the Studio prints a memory note when it is larger than the
  machine's RAM. A new lineage is not hours; it is a profile, an engine, a container and a
  converter before the first `inspect` can run.
* **Certification** is a drill per family per lane, and the drill's evidence rides the chain in
  chunks (the integer floor's free-prompt family object was 214,243 bytes against a 100,000-byte
  carrier). A family that is already certified for a lane needs only the `bind`.
* **Registration and the market** are one atomic transition for an additional model under MOE2. Registration can still require the collateral and admission conditions named by the active profile, but it does not wait for a separate market seed. A missing market after successful additional registration is a state-invariant violation.
* **A wider context** is a mint, which is the operator's decision and a flag day, not a step
  anyone on this page can take.

## Where to watch

* **The issue.** Label `model-request`; whoever picks up a step says so on the issue, and the
  values that make the model a class — the model id, the class id, the artifact root, the
  registration transaction, the drill and bind objects — belong in its comments, with what
  produced them.
* **The chain.** `palw-class ledger --network testnet-11` prints the classes the bundle knows; a
  synced node's `getPalwProducerFacts` reports the live set with each class's artifact root; the
  explorer's class rows read the same. `[palw-lifecycle]` lines in a node's log say what the
  transition accepted or refused, by name.
* **The Studio.** The class row appears with its readiness once the class is on the chain and the
  artifact row is in the manifest; until then the Studio shows nothing, on purpose — a class row
  is a chain fact, not a request.

Related: [旧ADR-0096](../../rfc/0019-clients-rpc-and-diagnostics.md)
(§1.5, Decision 13), [旧ADR-0088](../../rfc/0007-bft-model-admission.md),
[旧ADR-0090](../../rfc/0016-position-market-and-rewards.md),
[旧ADR-0075](../../rfc/0011-gate-c-and-consensus-credit.md),
[旧ADR-0056](../../rfc/0007-bft-model-admission.md),
[docs/palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md),
docs/palw-certify-a-new-model.md,
[docs/components-manifest.md](../../reference/manifests/components-manifest.md).

## A model too large for one seat (旧ADR-0099)

A request for a model no single seat can hold is answered by the same tool that prices any
model: write the manifest — the family and the geometry's public numbers, as
`docs/model-manifests/kimi-k3-stand-in.json` does for Kimi K3 — and run

```bash
cargo run -p misaka-palw-base0 --bin palw-shard-plan -- --manifest <model.json> --measured-out <dir>
```

It prints every wall of 旧ADR-0097 at 512 / 32,768 / 131,072 / 1,048,576 positions, the artifact's
bytes per layer, the shard plans a seat budget allows and what each shard's seat must hold and be
served, and writes the Measured Model Artifact any node recomputes (`--verify <file>`). The replay
rate is the adder's to measure (`palw-certify`, `--replay-ms`); the network's own certification
drill is what verifies it. A manifest that fits no ruleset is the case 旧ADR-0097 Decision 5
describes — a different mint — and the document says so by the wall.

If you HOLD the converted artifact, measure it rather than a manifest (旧ADR-0100 Decision 3): the
geometry is read off the file, the bytes off its inventory, and the document carries the root the
class registers —

```bash
cargo run -p misaka-palw-sdk --bin palw-class -- measure --network testnet-11 --key-file <bond seed> --out <model>.measured.json <model>.palwart
```

and anyone recomputes it with `palw-class verify --network testnet-11 [--artifact <model>.palwart] <model>.measured.json`
(without the artifact, the artifact's own fields are named as needing one; with it, every field
and the signature). On the shipped A16 artifact this reproduces the genesis class id and the
registered root; an artifact of an architecture this tree has no graph for is refused by name.

