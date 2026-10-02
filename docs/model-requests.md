# Requesting a model, and what happens to a request

A request is the door ADR-0096 Decision 13 puts on a path that already existed end to end and had
no place to knock: the [model request form](https://github.com/MISAKA-BTC/misakas/issues/new?template=model-request.yml)
in this repository's issue tracker. This page says what you are asking for when you file one,
which hands the request passes through, which ADR governs each hand, and what a request can and
cannot do. The short version: a request is public, it is not a bond, it buys no priority, and it
moves no consensus object.

## What you are asking for

A model on a MISAKA network is a **class**, and a class is four agreements that hold at once
([palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md)): a graph the court can walk, whose id IS the class id; an
artifact whose root the chain pins; a canonical job the class is paid per; and an engine that
executes what the graph describes. A request names the weights; everything else is made from
them by people, with tools this tree ships, and priced by the chain. Nothing in the list below is
a permission — the court grades the evidence in the transition, and the fee is the rent
(ADR-0075).

## The pipeline

| # | step | who | governed by |
|---|---|---|---|
| 1 | **Request.** The issue form asks what a class needs known up front: weights, license, architecture, parameter count, quantization, the context the request needs, which lanes, who converts, who bonds, and what machine the requester has. The Studio's Network tab has a "Request a model" button that opens the form with the machine block filled in from what it knows (RAM, accelerator, the classes it holds); any field can be prefilled by its id as a query parameter. | the requester | ADR-0096 Decision 13 |
| 2 | **Conversion to an artifact.** The family's converter (`qwen25-convert` for the dense tier, `qwen36-convert` for the hybrid tier) turns the public weights into the integer container the runtime maps, and `palw-class inspect` / `preflight` check the pairing and the real admission gate offline, before any node or coin (runbook §2–§3). A known lineage is data (a geometry constant, a table row); a new architecture is code first — a profile whose kernels the court catalogs, an engine, a container, a converter, then one `PalwModelLineageV1` impl ([palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md)). | the converter (the requester, or whoever picks the request up) | [palw-add-a-model-runbook.md](palw-add-a-model-runbook.md); ADR-0056 Decision 1 (admission is arithmetic) |
| 3 | **The class id and the artifact root.** The class id is the graph's, computed by the node (`shape_profile_id()`); the root the chain pins is the artifact's INVENTORY root — what `qwen36-run --artifact … --root-only` prints on the hybrid tier and `palw-class inspect` on both, and what `getPalwProducerFacts.artifactRoot` reports once registered. It is not the file's sha256. Rebuilding the artifact from the same weights lands on the same root, which is the only reason downloading someone else's conversion is safe: a downloader pins that root and the file's sha256 side by side, and the components manifest carries both as `artifact_root` and `sha256` ([components-manifest.md](components-manifest.md)). The `.palwmanifest` sidecar (`palw-class manifest`) records both beside the file. | the converter | ADR-0056 Decision 6 (duplicate roots are priced, not policed) |
| 4 | **Registration.** From the node that holds the artifact, with `misaka model add` (runbook §5). The node reads live terms, applies the known-weights rule (weights the chain already has never candidate for a new class) and the sibling filters, runs the admission preflight again, and only then signs and funds. Share, target and slash value are the chain's terms, never the registrant's to choose, and a class registers weightless (0‰) until a certified family covers it. A registered class starts as `Candidate` and admits nothing until an admission jury drawn from the floor's operators holds it (ADR-0147). | the registrant (a bonded key) | ADR-0056 Decision 3 (registration exposure); ADR-0075 Decision 4 (every gate reads genesis ∪ chain) |
| 5 | **Certification.** A family's drill evidence rides the chain as `FamilyCertified` (once per family per lane), and a `ClassLaneCertified` binds the class to it, weight-bearing. A drill certifies kernels, not weights: a model whose graph reaches a kernel no shipped family drills is a new architecture and needs a build whose court serves it. | anyone with a funded key; usually the registrant | ADR-0075 Decisions 1, 2, 5, 7; [palw-certify-a-new-model.md](palw-certify-a-new-model.md) |
| 6 | **The line and its owner.** A line is `(class, owner bond, name)`; every class's founding line has `line_id = class_id` and its owner is the registrant's bond — so the owner of what a request becomes is whoever bonded it. Versions are signed objects the line's developer publishes (`ModelVersionPublished`: root, parent, declared hashes, preview flag), history is kept whole, and the owner sets roles or hands the line over; positions never move with it. | the line's owner and developer | ADR-0088 Decisions 1, 2, 6, 9 |
| 7 | **The seeded pair.** A line's market opens only on a seed of at least the least seed (100,000 MSK under ADR-0090; 1,000,000 MSK once ADR-0120's `palw_model_seed_v2` is in force) locked for good — no fee, no position for the seeder, the reserve never falls under it; 500,000 whole positions on a curve whose product never falls. The seed may precede the class's approval, no buy may. ADR-0094 lets the seed accumulate over several transactions instead of one (`misaka model market open --seed`); ADR-0091 has the miner's reward buy the pair, 5 % of the escrow, and pays no holder. | a bonder with the MSK | ADR-0090 §0; ADR-0094; ADR-0120; ADR-0091 |
| 8 | **What the Studio then shows.** The Studio (a separate application, not in this repository) shows a class row with its readiness and, for a downloadable class, the artifact row from the components manifest (`url`, `sha256`, `size`, `class_id`, `artifact_root`), installed through a download manager that verifies the digest. | the Studio, from the manifest | ADR-0096 Decision 10; [components-manifest.md](components-manifest.md) |

## What a request cannot do

* **It is not a bond and not a registration.** No key signs anything when a request is filed;
  nothing reaches the mempool. The registration's exposure is the registrant's (ADR-0056
  Decision 3), the line is the bonder's (ADR-0088 Decision 1), and the seed is the seeder's
  (ADR-0090) — a request has none of the three.
* **It buys no priority.** The queue is the tracker, in the open. A request's age, its author, its
  reactions or its comments change nothing about what the chain prices or admits — admission is
  arithmetic (ADR-0056 Decision 1) and certification is graded evidence (ADR-0075 Decision 2).
* **It moves no consensus object and widens no row.** A row's context width is part of its class
  id, so a wider context is a different class — another rung of the ladder, registered as its own
  class — never a raised ceiling on a running chain (ADR-0092 Decision 4). A request for more
  context names the width it needs in its `context` field.

## How long it takes

Honest numbers, from what has been run:

* **Conversion** is minutes to hours on a workstation. The dense 1.5 B artifact is about 1.8 GB
  (1,799,359,436 bytes at `n_ctx` 8,192, 2,868,906,956 at 2,097,152); the hybrid QWEN36 artifact is
  34–39 GB converted from a Q4_K_M GGUF (the 2M conversion took 1,024 s), and the hybrid runtime maps
  it whole. A new lineage is not hours; it is a profile, an engine, a container and a
  converter before the first `inspect` can run.
* **Certification** is a drill per family per lane, and the drill's evidence rides the chain in
  chunks (the integer floor's free-prompt family object was 214,243 bytes against a 100,000-byte
  carrier). A family that is already certified for a lane needs only the `bind`.
* **Registration and the pair** wait on money and on a person: a bonded key with the collateral
  the live terms name, and a bonder willing to lock the least seed for good. A request
  whose `bonder` field says "undecided" is at this step until it does not.
* **A wider context** is a different class: a width the network's context ladder admits is
  registered like any other row; a width outside it is a ruleset change, which is a release, not a
  step anyone on this page can take.

## Where to watch

* **The issue.** Label `model-request`; whoever picks up a step says so on the issue, and the
  values that make the model a class — the model id, the class id, the artifact root, the
  registration transaction, the drill and bind objects — belong in its comments, with what
  produced them.
* **The chain.** `palw-class ledger --network testnet-12` prints the classes the bundle knows;
  `misaka model list` and `misaka palw registry` read the live set and each class's lifecycle; a
  synced node's `getPalwProducerFacts` reports the live set with each class's artifact root; the
  explorer's class rows read the same. `[palw-lifecycle]` lines in a node's log say what the
  transition accepted or refused, by name.
* **The Studio.** The class row appears with its readiness once the class is on the chain and the
  artifact row is in the manifest; until then the Studio shows nothing, on purpose — a class row
  is a chain fact, not a request.

Related: [ADR-0096](adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
(§1.5, Decision 13), [ADR-0088](adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md),
[ADR-0090](adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md),
[ADR-0075](adr/0075-certification-is-a-consensus-object.md),
[ADR-0056](adr/0056-palw-permissionless-class-admission-and-share-economy.md),
[palw-add-a-model-runbook.md](palw-add-a-model-runbook.md) (the operator walkthrough),
[palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md),
[palw-certify-a-new-model.md](palw-certify-a-new-model.md),
[components-manifest.md](components-manifest.md).

## A model too large for one seat (ADR-0099)

A request for a model no single seat can hold is answered by the same tool that prices any
model: write the manifest — the family and the geometry's public numbers, as
[`model-manifests/kimi-k3-stand-in.json`](model-manifests/kimi-k3-stand-in.json) does for Kimi K3 — and run

```bash
cargo run -p misaka-palw-base0 --bin palw-shard-plan -- --manifest <model.json> --measured-out <dir>
```

It prints every wall of ADR-0097 at 512 / 32,768 / 131,072 / 1,048,576 positions, the artifact's
bytes per layer, the shard plans a seat budget allows and what each shard's seat must hold and be
served, and writes the Measured Model Artifact any node recomputes (`--verify <file>`). The replay
rate is the adder's to measure (`palw-certify`, `--replay-ms`); the network's own certification
drill is what verifies it. A manifest that fits no ruleset is the case ADR-0097 Decision 5
describes — a different mint — and the document says so by the wall.

If you HOLD the converted artifact, measure it rather than a manifest (ADR-0100 Decision 3): the
geometry is read off the file, the bytes off its inventory, and the document carries the root the
class registers —

```bash
cargo run -p misaka-palw-sdk --bin palw-class -- measure --network testnet-12 --key-file <bond seed> --out <model>.measured.json <model>.palwart
```

and anyone recomputes it with `palw-class verify --network testnet-12 [--artifact <model>.palwart] <model>.measured.json`
(without the artifact, the artifact's own fields are named as needing one; with it, every field
and the signature). On a genesis A16 artifact this reproduces the genesis class id and the
registered root; an artifact of an architecture this tree has no graph for is refused by name.

