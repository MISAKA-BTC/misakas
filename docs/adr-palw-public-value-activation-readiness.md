# ADR: PALW public/value activation readiness — definition of done and honest gate ledger

Status: **In progress.** This ADR defines exactly what "the stage where public/value activation is
possible" means, tracks every gate, and draws the honest line between gates that can be closed by
writing code and gates that cannot. It is the definition-of-done for the goal
"finish permissionless snapshot auth, automatic 0x3b submission, the Windows/CUDA 72h soak, and the
unbond/slash rehearsal to the stage where public/value activation is possible."

## What "activation possible" means

Activation is a **separate, explicitly reviewed** change that sets `palw_algo4_accept = true` for a
**fresh Header-v4 re-genesis identity** (new suffix, ports, seeds, empty datadir). No shipped preset
enables it, and nothing in this program flips it. "Activation possible" is the state in which that flip
is *defensible*: every code gate is complete and reviewed, and every operational gate has been run on
real hardware and a real network. This ADR does not authorize the flip; it makes the remaining distance
explicit and honest. Fabricating any gate (soak evidence, review sign-off, hardware runs) would defeat
the purpose and cause real financial harm, since PALW carries bonds/slashing/escrow.

## Gate ledger

### A. Code gates (can be closed by implementation + tests + review)

| Gate | Status | Evidence / remaining |
|---|---|---|
| Automatic 0x3b response (discovery + deadline-aware submission) | **Code complete** | getPalwState `da_challenges` + `palw-da-auto-respond` (node `35c4d6c`); engine 5/5 + wire roundtrip. Live withholding soak is an operational gate (§B). |
| Permissionless snapshot auth — verification core | **Complete** | Verifier `verify_chain_derived_pruning_boundary`; the two payload derivations `palw_pruning_payload_paid_work_nullifiers` / `palw_pruning_payload_da_state_root` (proven equal to `palw_paid_work_window` / `palw_da_parent_state` **by analysis** — the pruning-anchored backward walk breaks immediately; `palw_da_parent_state` returns the stored state verbatim); and the one-call importer entry `verify_chain_derived_pruning_boundary_from_payload` (node `2b8139c`, `82d2330`, `5bf0a8a`). 20/20. |
| Permissionless snapshot auth — activation lever + carrier + gate | **Complete** | `Config::palw_permissionless_snapshot_auth` (default false; no preset sets it), `PalwChainDerivedAuthBundleV1`, fail-closed `palw_pruned_ibd_chain_derived_import_allowed`. |
| Permissionless snapshot auth — importer call-site (1c) | **Complete (fenced)** | `prepare_pruning_point_palw_snapshot_import` runs the chain-derived verify before staging on a fenced path (lever off by default + all callers pass `None`); operator/v3 paths byte-for-byte unchanged; `kaspa-consensus`/`kaspa-p2p-flows` compile, import-auth tests pass (node `1afedd6`). |
| Permissionless snapshot auth — P2P transport + auth of the bundle (1d) | **Landed, still fenced — SEC-1 open** (2026-07-27) | Transport + authentication implemented: proto tags 75-78 + v8-only flow `request_palw_chain_derived_bundle.rs`, IBD requester (`ibd/flow.rs:1002-1053`), `ConsensusApi` bundle params (`api/mod.rs:586`, `:1067`), importer branch (`processor.rs:7925-8045`), lever `ConfigBuilder` (`config/mod.rs:292`) + CLI flag (`args.rs:428`). Adversarial review: review points (a)(b)(c), install-before-verify ordering and lever-off byte-identity all **CONFIRMED-SAFE**; the Borsh hash-forgery hole found during design is closed by `palw_bind_transported_header_identity` (`palw_pruned_frontier.rs:527`). **THREE design corrections vs the ADR**, all verified in code: (i) the pruning proof contains **no** post-PP header (`pruning_proof/build.rs:180-241`), so the descendant is chosen from **local** state and the transported copy must byte-match — it is *not* "selected from the proof-validated set"; (ii) support rows need span=32,768 vs proof level-0 ≤ 2,000, so review point (a) is met by local-identity binding, not proof membership; (iii) `overlay_commitment_root` is a **body** rule, so a header-only descendant costs **one block** to forge — burial under the post-PP chain, not PoW alone, is what makes (b) safe. **STILL FENCED**: `chain_derived_import_is_wired` (`ibd/flow.rs:1240-1262`) errors whenever a bundle authenticates, so lever-on IBD always fails (fail-closed). Removal requires **SEC-1 closed + independent review (§B)**. |
| **SEC-1** — chain-derived `paid_work` row attribution is uncommitted | **Open (gated)** | `reconstruct_selected_parent_state_from_pruning_payload` folds only the deduplicated nullifier **union**; each row's `block_hash`/`block_daa_score` enter no commitment, and `prepare_pruning_point_palw_snapshot_import` does no store cross-check on them. Keeping the union byte-identical, an attacker can re-date rows inside the window at **zero work**; the victim's `palw_paid_work_window` then diverges as `anchor_daa` advances and it rejects **honest** blocks with `BadOverlayCommitment` ⇒ permanent desync. **operator-pin is unaffected** (its digest covers these bytes verbatim). Not currently exploitable: gated by the default-false lever *and* the `chain_derived_import_is_wired` fence. Write-up: ADR-0042 §SEC-1. |
| G6 gate ledger row vs measured reality | **Known discrepancy, deliberately not inflated** | ADR-0043 and the emitted JSON say the gate moved `Measurement → Bounded`, but the in-code ledger still reads `("G6", GateVerifierStatus::VerifierExists, …)` (`consensus/core/src/palw.rs:10754`) because `GateVerifierStatus` has no `Bounded` variant (`palw.rs:10692-10703`). Left as-is on purpose: inflating a gate status in the code ledger is precisely the failure mode this document exists to prevent. Adding the variant is a separate, explicit decision. |
| G6 anti-spam header-flood bound | **Bounded (code)** | ADR-0043 Amendment: the consensus-validity sibling bound was soundness-rejected; the landed fix is the (A) allocation policy (`split_exponential_with_reserve` re-tile reserve + harmonic flood-regime insertion). Single-machine re-measurement: per-header writes p99 1,037 → 16 (reachability → 2) under the 1,000-sibling flood. Remaining: multi-machine serial/concurrent flood + long-soak threshold freeze and **independent review** (external). |

### B. Operational gates (cannot be closed by writing code — irreducibly external)

These do not have a code representation that I can complete. They are listed so "activation possible" is
honest, not so they can be checked off from a keyboard.

| Gate | Why it is external | Readiness |
|---|---|---|
| 72h Windows/CUDA endurance soak | Needs the physical RTX host powered on + 72h of real wall-clock. | Harness/launcher/runbook ready (qwen `4121131`); **host offline** (last seen ~1d). Starts when powered on. |
| Live multi-node pruning/catch-up/reorg + DA-withholding soak | Needs a real multi-node network running over time. | Rehearsal driver + runbook ready (node `fdaeac5`); execute `--live` on a testnet. |
| Independent security review | Needs a human reviewer other than the author, especially for the permissionless-auth ordering and the G6 redesign. | Pending; ADRs written to make review tractable. |
| Re-genesis ceremony | An operational decision to allocate a new network identity/seeds/datadir and flip acceptance after review. | Not started; §5 of `palw-public-value-header-v4-antispam.md` is the procedure. |

## The honest boundary

I can drive the **A** gates to completion and I will (1b → 1c → 1d, then the G6 design for review). The
**B** gates are not code and cannot be satisfied by an agent writing and testing code; they require real
hardware time, a real network, human review, and an operational launch decision. I will not fabricate
any of them, and I will not flip `palw_algo4_accept`. When the A gates are complete and reviewed and the
B gates have genuinely been run, activation is *possible* — a separate change may then flip acceptance on
a fresh re-genesis candidate. Until then this codebase stays fenced exactly as shipped.
