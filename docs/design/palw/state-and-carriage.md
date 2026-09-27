# State, objects and carriage — design

> **Not normative.** This document explains why the rules in
> [spec/palw/02-state-objects-and-carriage.md](../../spec/palw/02-state-objects-and-carriage.md) are
> what they are.

**Decisions recorded in:** ADR-0029 (superseded), ADR-0042 D1/D5, ADR-0043, ADR-0046, ADR-0058,
ADR-0152 §6.
**Last revised:** 2026-09-27

## 1. Problem

PALW adds a large state machine to a UTXO chain. That state has to be:

- **the same on every node:** equal DAGs give equal weights;
- **reorg-safe:** every write is revertible;
- **carried without a second source of truth**, such as a declared amount beside a real output, or a
  "timeout happened" object beside the deadline index;
- **safe to extend,** by appending to encodings without renumbering what is already on chain.

## 2. The design in one paragraph

One fold (`PalwStateBookV2`) is advanced one block at a time from the objects the block accepted. Every
read about a candidate is taken on that candidate's own state. Objects are tagged Borsh values that
ride dedicated subnetwork ids. Stateless failures invalidate a block. Stateful failures only skip a
carrier, so no honest builder is punished for mempool timing. The header commits a state root whose
hash order has no cycle. Enums are append-only, because Borsh discriminants are chain bytes.

## 3. Lessons that set the rules

- **Nodes disagreed on weight** (P0-4, 2026-08-19): the weigher read node-local stores, so two nodes
  with one DAG weighed differently. Hence candidate-scoped reads (ADR-0042 D5).
- **No object could be carried** (P0-11): with no lifecycle extractor, every claim voided at
  `BindTimeout` and PALW weight stayed at zero. Hence `SUBNETWORK_ID_PALW_LIFECYCLE`.
- **A variant was renumbered** (2026-09-10): inserting an enum variant mid-enum made a `main` build
  unable to sync testnet-11 from genesis. Hence "append only" (ADR-0152 §6, `PalwModelLifecycleV1`).
- **Declared collateral:** for a time nothing locked a UTXO behind a registration's declared
  collateral, so bonds came from genesis only. The carrier-binding check closed that
  (`palw_bond_registration_binds_its_carrier_v2`).

## Source texts (archived ADR bodies)

- [ADR-0046 — PALW V2 consensus-object carriage: the registrations ride their collateral, the verdicts ride their evidence](archive/0046-palw-v2-consensus-object-carriage.md)
