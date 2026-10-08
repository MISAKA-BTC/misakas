# DA16 — public artifact / claim-material transport and the provider court

Lane DA16 (`da16/transport-provider-court`). Closes two items of the single future full-activation release (no DAA-9,000 flag day):

1. **RFC-0014 §16 transport** — artifact bytes and claim material are publicly fetchable by any outsider for the liability horizon
   (manifest, providers, chunk addressing, retention, fetch/verify by root), so the onboarding artifact binding (V2 `artifact_root` ↔
   kernel `ParamCommitmentsV1` root, lane D record §5–§6 GAP 1) becomes something an outsider **confirms or refutes from the bytes**.
2. **RFC-0009 §4.2 provider court** — the objective transfer of a miner's DA responsibility to bonded providers.

Everything consensus-side is dormant behind a NEW fence (proposed name **`palw_provider_court_v1`**) that no network can arm. Amounts
are BILI (1 BILI = 10^8 sompi). Status words follow the integration matrices; nothing here was run against a live node or network.

## 0. Decisions in one table

| Question | Decision | Why |
|---|---|---|
| `misaka-model-transport` crate or extend `misaka-palw-remote` | **extend `misaka-palw-remote`** (new module `public_material`); the pure, consensus-relevant half in `kaspa-consensus-core::palw_public_material_v1` | The external transport (BEP52 / libtorrent / daemon) cannot be imported in this lane (no network) and RFC-0014 §16.2 forbids linking it into a validator anyway. `misaka-palw-remote` already owns the provider seam, HTTP/dir providers, read-back upload, availability, repair and the retention monitor; a second crate would fork the layout and the fetch policy. A later BEP52 import is one more provider behind the same verify-by-root (RFC-0014 L3) and needs no consensus change. |
| Root of trust of every byte | **the chain's own root**, never a manifest or a provider signature | artifact leaf → class `artifact_root` (`verify_artifact_opening_v1`); kernel side → bound `kernel_param_root` (`ParamCommitmentsV1::root`, `TensorOpeningV1::authenticates`); claim position → the claim's committed values (`classify_position_response_v1`); V2 held unit → the claim's `execution_root` (`palw_held_da_check_disclosure_v1`). A manifest is an index only. |
| Where the court's state lives | **the kernel route's aux tables** (journaled by delta 160, carried by tail `0xEC`, rooted in `kernel-route/v1`) | exactly lane D's onboarding precedent: reorg, replay, restart and pruned import come for free, one writer. |
| What moves DA responsibility | an explicit, irreversible, fence-gated **`DaTransferV1`** by the claim's producer, over ≥ 2 live leases | "version or fence": claims committed below the fence, and claims never transferred, stay their producer's; one failure is one party's. |
| How the kernel learns it | one consumer-derived input `KernelLedgerV1::provider_liable` (like `attested_artifacts`, injected per load from the court's rows) + one receipt `LedgerEventV1::ProviderLiableDefault` (discriminant 21) + `KernelLedgerV1::provider_lapse(claim)` | the demand/default rules stay the kernel's; only the payer of a default changes. |

## 1. Transport (item 1)

### 1.1 Units (chunk addressing) and their verification

`PublicUnitV1` names one unit; `PublicUnitAnswerV1` carries it. The same enum is the court's challenge unit (§2) and the transport's file
name, so an off-chain fetch and an on-chain answer are checked by ONE function per unit.

| Unit | Subject | Bytes | Verified against (on chain) |
|---|---|---|---|
| `ArtifactLeaf { index }` | artifact | `PalwArtifactOpeningV1` (operand + path) | the V2 class's registered `artifact_root`, at the leaf the program's closed-form layout puts it (`palw_tir_inventory_leaf_count_v1`, `palw_tir_leaf_index_v1`) |
| `KernelCommitments` | artifact | `ParamCommitmentsV1` | `root() == kernel_param_root` |
| `KernelRowNodes { param, layer, level, first, count ≤ 1,024 }` | artifact | node hashes of the instance's row tree at `level` + boundary frontier + col root | recomputes the instance's tensor commitment → `commitments.by_instance[(param, layer)]` (bounded localization: two rounds reach a row of a 2^20-row tensor) |
| `KernelRow { param, layer, row }` | artifact | `TensorOpeningV1` (row) | `authenticates(commitments.by_instance[(param, layer)])` |
| `ClaimPosition { stage, position }` | kernel claim | `PositionResponseV1` (what `Respond` carries) | the claim row's committed values (`classify_position_response_v1`, the kernel's own) |
| `HeldUnit(PalwHeldMissingV1)` (transport only, §4) | V2 held claim | `PalwHeldDisclosureV1` | `palw_held_da_check_disclosure_v1` against the claim's `execution_root` and authenticated binding |

**Binding confirmation from bytes** (`binding_check_from_bytes_v1`): fetch every leaf, rebuild the V2 root (must equal the class's), reassemble
each declared instance from its leaves, compute `ParamCommitmentsV1`, compare its root with the binding's. Equal ⇒ **CONFIRMED** (a local
verdict; the chain records only the absence of a refutation inside an *available* horizon, §3). Different ⇒ the differing instances; with
the bound `KernelCommitments` and `KernelRowNodes` → the first differing row → its `KernelRow` (from the providers' transport, or forced
on chain by a court challenge) → tag 105's existing `ArtifactMismatchProofV1::Row` (`refutation_from_bytes_v1`). The refuter never needs
the false bytes in advance: the provider that leased the pair must serve them or be slashed.

### 1.2 Manifests

* `ArtifactManifestV1 { version, network_domain, v2_class, artifact_root, kernel_param_root, leaf_count, leaf_hashes, retain_until_daa }` —
  checked against the chain BEFORE any byte: `leaf_count` is the program's closed form and `artifact_root_v1(leaf_hashes) == artifact_root`;
  then every leaf verifies alone (`artifact_leaf_v1(operand) == leaf_hashes[i]`, at the coordinates the program fixes).
* `ClaimMaterialManifestV1 { version, network_domain, claim, positions: [(stage, position, len, hash)], retain_until_daa }` — an index; every
  unit is classified against the claim row read from public rows (op 211 / `palw_kernel_route_v1`).
* `HeldMaterialManifestV1 { version, network_domain, claim, execution_root, binding, units, retain_until_daa }` — the binding is
  authenticated against the claim's `execution_root` first.

### 1.3 Providers, layout, retention

Content-addressed layout added beside `claims/` and `chunks/` (same dir and HTTP providers, same reference server, strict path parser):

```text
artifacts/<class>/<artifact root>.manifest            ArtifactManifestV1 (written LAST)
artifacts/<artifact root>/leaf/<index>.unit           PalwArtifactOpeningV1 without path (the operand; the path is rebuilt from the manifest)
artifacts/<artifact root>/kernel/<kernel root>/commitments.unit
artifacts/<artifact root>/kernel/<kernel root>/<param>-<layer|g>/row/<row>.unit
material/<claim>/manifest                             ClaimMaterialManifestV1 / HeldMaterialManifestV1 (written LAST)
material/<claim>/<unit key hex>.unit
```

Upload is read-back verified (`publish_artifact_v1`, `publish_claim_material_v1`); availability is per provider per unit
(`LOCAL_OBSERVATION`, never evidence); repair re-seeds a thin provider from verified copies; the retention monitor watches subjects until
their `retain_until_daa`, which must cover the chain horizon (artifact: the binding's liability horizon; claim: its liability end).
`palw-evidence artifact-publish | artifact-fetch | artifact-verify | material-publish | material-fetch` drive it; `artifact-fetch` doubles as a
`--palw-root-fetch-cmd` for `kaspad/src/palw_root_fetch.rs` (same argv contract: class, root, `drop_dir=`), so the node's root-fetch hook
uses the public transport without a kaspad change. Discovery stays a configured provider list (DESIGN_GAP: no on-chain discovery beyond the
lease rows, which name provider BONDS, not endpoints).

## 2. The provider court (item 2)

### 2.1 Fence

`Params::palw_provider_court_v1: Option<ForkActivation>` — the `palw_probabilistic_constraints_v1` pattern: `None` on every preset and in no
flag-day list, hashed Some-only into `consensus_params_id` and `consensus_schedule_id`, collapsed from `Some(never())`, listed in
`palw_fences_v1()`, a fork-id probe arm, and `validate_palw_provider_court_v1` REFUSES every armed height. Where in force (harness only)
the route's fence must also be in force; below it tags 150–153 are dropped by name before any slot, rent or budget (A-2 uniform: an older
build cannot decode them), and the fold refuses them as the second lock. The existing `palw_evidence_court_v1` (the pure, V2-free-prompt
twin of `palw_da_court`) is untouched; its rules are this court's spec and its pure machine is used as a differential oracle in tests.

### 2.2 Objects (tags 150–153, ML-DSA-87 by a V2 bond over `H(network ‖ kind ‖ signer ‖ payload)`, context `misaka-palw/provider-court/object/v1`)

| Tag | Object | Signer | Effect |
|---|---|---|---|
| 150 | `ProviderLeaseV1 { subject, reserved, serve_until_daa }` | the provider | a bonded promise to serve every unit of `subject` until `serve_until_daa`; reserves `reserved ≥ 100 BILI` of the provider's FREE collateral (mirrored into V2's committed-collateral ledger and both withdrawal gates) until `serve_until`; one lease per (subject, provider) |
| 151 | `ProviderChallengeV1 { subject, provider, unit }` | any bond of another operator | one unit of one lease, on chain; reserves a 10 BILI challenge bond; deadline `now + 20 DAA` (inside the lease); one open challenge per (lease, unit); ≤ 8 open per challenger |
| 152 | `ProviderAnswerV1 { challenge, answer }` | the lease's provider | the unit, verified against the chain's root (§1.1) by the deadline ⇒ cleared (challenger: bond back minus a 1 BILI fee, burned). A wrong answer neither clears nor defaults (dropped); the clock decides |
| 153 | `DaTransferV1 { claim }` | the claim's producer | moves the claim's DA responsibility to its leases (§2.3) |

`subject` is `Artifact { v2_class, kernel_param_root }` (the pair a provider vouches for: it recomputes both roots from the bytes before
leasing) or `KernelClaim { claim }`.

### 2.3 Rules (RFC-0009 §4.2, word for word)

* **Liability moves only with a provider bond, an on-chain chunk challenge, an opening deadline and a default that slashes the provider.**
  `DaTransferV1` is accepted only: claim committed at or after the fence; not terminal, not convicted; **no open demand and no default so far**
  (a failure in flight is never re-assigned); ≥ 2 live, uncharged leases of distinct operators (none the producer's), each serving through
  the claim's liability bound; Σ lease reservations ≥ the claim's reservation (moving liability never makes withholding cheaper).
  Irreversible.
* **Default slashes the provider.** A 151 unanswered at its deadline charges THAT lease: its reservation is slashed (`slash_bond`, burned at
  release), the challenger gets its bond back and 500‰ of the slash (kernel payout queue), the rest is burned; the provider's other open
  challenges on the subject settle moot. **Each provider is charged at most once per subject.**
* **The kernel's public demand stays THE material demand.** On a transferred claim a `FileDemand` that nobody answers by its deadline is
  `ProviderLiableDefault`: the producer pays nothing; every live lease of the claim is charged (any of them could have answered with a
  `Respond` — they all failed one public, on-chain challenge); demanders share `min(pool, default_penalty)` pre-Final (post-Final the pool
  is burned whole, as the producer path does); the claim is void.
* **Common-mode outage voids, never convicts.** When no live, uncharged lease is left on a transferred claim the claim LAPSES
  (`provider_lapse`): `Unavailable { producer_defaulted: false }` before Final (no reward; the producer's reservation released, never
  slashed), the OPV fact withdrawn after Final. Never `Convicted`, never `SlashFraud`/`SlashDefault` of the miner.
* **A false root or a false computation stays the miner's.** Providers are charged only for not serving bytes that verify against the
  chain's root; the bytes they serve convict the producer through the kernel's unchanged `FileProof` (a transferred claim keeps its
  reservation until its liability ends). An artifact binding whose kernel root is false is refuted by tag 105 and slashes the BINDER; a
  provider that answers with the false tree's openings has served what it promised and is cleared.
* **Never a slash:** a single Panel's timeout or a local fetch failure (not inputs at all); a lease alone (a pre-signed promise with no
  challenge); a wrong answer before the deadline; a challenge outside the lease or of a unit the subject does not commit (refused).
* **Old vs new.** Below the fence, and for every claim never transferred, the producer's demand/default path is byte-for-byte today's.
  A possession challenge against a lease on an untransferred claim charges only the provider's own promise (a separate obligation, never
  the producer's failure).

### 2.4 Artifact availability and the binding (RFC-0014 §16.4)

`artifact_availability_v1(class, kernel_root)`: **READY** with ≥ 2 live, uncharged leases of the pair from distinct operators;
**LAPSED** at the first block where none is left (recorded with its DAA). Past the fence (all gated, below it lane D's behaviour is
unchanged):

* tag 104 is accepted only when the pair is READY with every counted lease serving through `bound + 200 DAA` (the binding's whole
  refutation horizon is covered by bonded availability);
* a binding whose pair LAPSED after it was bound attests nothing (`onboarding_attested_roots_v1`) and its class is held
  `AVAILABILITY_REQUIRED` ("the bytes stopped being obtainable inside the refutation horizon"); the registrant may re-bind with fresh
  leases (a lapsed binding is not "live" for the one-binding rule).

So a binding can be attested, and its class activated, only after the bytes were publicly obtainable under bonded obligation for its whole
refutation horizon — **binding equality is outsider-checkable** (confirm from bytes; refute via 105 with a kernel row the lease forces out).

### 2.5 Rows (kernel route aux tables, Lead allocation requested)

| Table | Key | Row |
|---|---|---|
| **43** leases | `(subject, provider)` | `ProviderLeaseRowV1 { reserved, filed_daa, serve_until_daa, charged }` |
| **44** challenges | challenge id | `ProviderChallengeRowV1 { subject, provider, unit, challenger, bond, filed_daa, deadline_daa }` |
| **45** subjects | `subject` | `ProviderSubjectRowV1 { transferred_daa, lapsed_daa, charged_providers }` |

The closing tick (`tick_provider_court_v1`, before the kernel's tick) sweeps due challenges, releases expired leases, records lapses; the
kernel's `ProviderLiableDefault` receipts are settled right after the kernel tick. Reads: `ConsensusApi::palw_kernel_route_v1` and RPC op 211
already serve aux rows; a typed read (`provider_court_read_v1`) decodes them (no new RPC op).

### 2.6 INTERIM numbers (drill values, not security values)

lease reservation floor 100 BILI; challenge bond 10 BILI, cleared-challenge fee 1 BILI; response window 20 DAA (the kernel's court
deadline; inside the binding window of 40); min providers 2; ≤ 8 open challenges per challenger; challenger share 500‰. Provider cost,
redundancy, bandwidth, worst-case cold-fetch budgets (RFC-0014 §16.6) and Sybil-independence are EXTERNAL_GATE.

## 3. G14 check

One outside bonded verifier, public material only: it fetches the artifact from any provider and checks it against the chain's roots
(confirm), or forces the unit it needs on chain (151) and either receives authenticated bytes (→ 105 / `FileProof` conviction) or obtains a
correctly classified default (the PROVIDER's, or the producer's on an untransferred claim). Withholding is never fraud: every default is an
availability outcome (`Unavailable { producer_defaulted }`, a provider charge), never `Convicted`.

## 4. Held 8k material

* **V2 held class** (ADR-0103 units: prompt tiles, checkpoint state chunks, step ranges, step leaves): the transport carries and verifies
  them (`HeldUnit`, verified by `palw_held_da_check_disclosure_v1` against the claim's `execution_root`), with retention to the claim's horizon.
  The **on-chain transfer** for V2 held claims is NOT made: their default path is the V2 held DA court's (`DefaultDisputed`), which does not
  read this court; moving it is a V2 claim-fold change (GAP, stated). Until then a held claim's producer stays liable — correct, never
  double.
* **Kernel-route long context** (K2S segmented claims, multi-part positions): `ClaimPosition` addresses a position; multi-part units get a
  `part` index when K2S lands (`ClaimPositionPart`, reserved in the design; the verifier is K2S's). The provider court's transfer hooks the
  kernel's demand default generically, so a per-segment demand default is a `ProviderLiableDefault` on a transferred claim with no change here.

## 5. Coordination

* **K2S** — per-segment demands: no conflict (the hook is in the default path, not the demand shape); served bytes stay out of the ledger.
* **G14-R4** — demand-bond fate and escrow: `ProviderLiableDefault` returns the demand bonds exactly as the producer path does today; if R4
  changes the producer path's demand-bond fate, the provider path must follow it (one function, noted at the hook).
* **OPV-BOOT** — conformance evidence (tag 109) rides on chain already; its leaf refutation reads the public artifact this transport serves.
* **A2U** — tags 150–153 → `palw_provider_court_v1` in the central kind→fence table.
