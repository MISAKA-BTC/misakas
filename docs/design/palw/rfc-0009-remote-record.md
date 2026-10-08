# RFC-0009 remote client / relay / redemption — implementation record (Agent C1)

Branch `rfc9/c1-remote` (base `f9c2c4308`). Matrix: [`remaining-rfc-integration-matrix.md`](remaining-rfc-integration-matrix.md) (RFC-0009 rows).
Spec of record: [RFC-0009](../../rfc/0009-palw-remote-miner.md), [implementation spec](rfc-0009-implementation-spec.md).

Statuses (only these): **IMPLEMENTED_AND_TESTED** · **IMPLEMENTED_REFERENCE_ONLY** · **DORMANT_NOT_INTEGRATED** · **CODE_GAP** ·
**DESIGN_GAP** · **EXTERNAL_GATE_PENDING**. A unit test on a reference path is never "real node" evidence: no case below has a real-node
run in this lane, and each row says so.

Invariants kept: a relay forwards bytes a node validates itself and is never a truth authority; a DA provider is never a computation
authority; multiple RPCs agreeing is **not** a proof (every unproven remote fact is labelled `UNVERIFIED_REMOTE_STATE`); signing keys
never leave the owner's process (builders and relays see unsigned objects and signed bytes only). Frozen FP Job V4, V3 redemption,
consensus params, `misaka-palw-challenge/`, `misaka-palw-kernel/` and the heartbeat/REAL/BASE-0/EXEC roles are untouched.

## 1. Node-less model registration, detached signing (task 1)

Three steps, three possible machines; the key exists only at step 2.

| Step | Command | Holds a key? | Reads a node? | Writes |
|---|---|---|---|---|
| 1 export | `misaka model add <model> --export-bundle FILE --payer-address ADDR --rpc A --quote-rpc B,C [--owner-pubkey HEX]` | no | yes (≥ 2 nodes; `--allow-single-rpc` for a node you run) | unsigned `RegistrationBundleV1` (JSON) |
| 2 sign | `misaka model sign FILE --key-file OWNER [--payer-key-file PAYER] --expect-class C --expect-root R --expect-owner TXID:I [--max-fee-sompi N] [--owner-only \| --carrier-only]` | yes | no (`--rpc` only to read the expiry clock) | `FILE.owner-signed.json`, `FILE.signed.json` (never overwritten) |
| 3 submit | `misaka model submit SIGNED --relay N1,N2,N3 [--relay-min K] [--allow-duplicate]` | no | yes (relays) | `~/.misaka/<net>/model-add/<class16>/sent.jsonl` |

**Bundle** (`misaka.palw.registration-bundle.v1`): network name, network domain (name + genesis), ruleset id (= `Params::consensus_params_id()`),
class id, artifact root, owner bond (`txid:index`) and its public key, the borsh `ClassRegistered` object (owner signature empty), the digest the
quote names, the exact message the bond key signs and its context, the payer (address, script, the single funding input), the carrier plan (fee,
mass, change, change script, priced payload length), the quote (every cost by payer, expiry DAA, totals) and the quoting nodes
(`UNVERIFIED_REMOTE_STATE`).

**What `sign` re-derives itself** (`bundle::check_bundle_v1`; nothing is taken from the bundle's display text): the network domain and ruleset id
from its own build; the object's class id from its profile; class / root / owner against `--expect-*` (the user's, or confirmed at the prompt — never
with `--yes`); the digest the quote names; the registration message (`palw_class_registration_message_v2`) it then signs; that the owner key is the
bond's registered key; the quote's totals; the registration exposure and burn this build knows (a quote understating them is refused); the carrier's
arithmetic, that the **only output is the change back to the payer's own script**, and the fee against the signer's own cap. The payer signs only after
the owner's signature is in the object (the carrier sighash covers the payload), may be a different key than the bond's, and refuses a funding input
that is not locked to its own address.

**Tests** (all in `cargo test -p misaka-palw-remote` and `cargo test -p misaka-cli --bin misaka operator::model`):

| Case | Test |
|---|---|
| relay/builder swaps the change **recipient** → `ChangeNotPayer`; swaps the funded script → `PayerIsNotThisKey` | `bundle::tests::a_builder_that_swaps_the_change_recipient_is_refused_before_anything_is_signed` |
| inflated **fee** → `QuoteInconsistent` / `FeeAboveCap` / `CarrierArithmetic` | `…a_builder_that_inflates_the_fee_is_refused_by_the_cap_and_by_the_quote` |
| swapped **class root** / **owner** / class id (self-consistent forgery) → `RootSwapped` / `OwnerSwapped` / `ClassIsNotItsProfile`; wrong key → `OwnerKeyMismatch` | `…swaps_the_class_root_or_the_owner_is_refused_against_what_the_user_expects` |
| network, ruleset, expiry, understated exposure/burn | `…network_ruleset_expiry_and_understated_costs_are_refused` |
| stages cannot be skipped/repeated; forged owner signature refused by the payer | `…the_stages_cannot_be_skipped_or_repeated` |
| bytes altered after signing (recipient, fee, root, owner, signature bit, declared id, expiry, cap) | `…a_relay_that_alters_the_signed_bytes_is_caught_before_and_after_the_wire` |
| **resend** (same bytes) vs **duplicate registration** (other carrier) vs registry-wins | `…resend_is_idempotent_a_second_carrier_is_a_duplicate_and_the_registry_wins` |
| multi-RPC quote: terms/burn/exposure/root/network/bond/mass disagreement stops everything; balances merge worst-case; one node is not agreement | `…nodes_that_quote_different_terms_are_never_averaged` |
| accepted ≠ Active: shared lifecycle codes; later stages and `BEACON_UNAVAILABLE` / `PUBLIC_PROSECUTION_INCOMPLETE` shown `DORMANT_NOT_INTEGRATED` | `…accepted_is_not_active_the_view_names_the_shared_codes_and_infers_nothing` |
| key-free pricing = the shipped signing builder's mass/fee; detached carrier has the shipped builder's tx id / payload / outputs | `model_bundle::tests::the_key_free_price_is_the_shipped_builders_price_and_the_detached_carrier_has_its_id` |
| the CLI's `ValidatorKey` through both steps with two different keys; carrier decodes as the node's fold reads it | `…the_validator_key_signs_both_detached_steps_with_two_different_keys` |
| `model sign` end to end on files; never overwrites; refuses tampered recipient / fee cap / root / owner / bare `--yes` / wrong key and writes nothing | `…model_sign_writes_a_signed_file…`, `…model_sign_refuses_a_tampered_bundle…` |

A defect found and fixed on the way: the in-process `model add --relay` path hashed `getPalwRegistrationTerms` **including `tip_daa`** (the node's
clock), so the pre-sign gate reported "terms changed" whenever a block arrived between the quote and the signature, and no two nodes at different tips
could agree on a quote. The digest now excludes `tip_daa`.

### Status

| Case | Status |
|---|---|
| detached export / sign / submit, relay-tamper refusal, fee payer ≠ bond key, resend vs duplicate, multi-RPC agreement | IMPLEMENTED_AND_TESTED (unit + file-level CLI tests); **no real-node run** |
| lifecycle display with the shared codes; Dormant / ChallengePending / `BEACON_UNAVAILABLE` / `PUBLIC_PROSECUTION_INCOMPLETE` | IMPLEMENTED_REFERENCE_ONLY (display only; the codes are tracked `DORMANT_NOT_INTEGRATED` — no on-chain source exists) |
| registrant bond in the class-row RPC (misattribution is checked on the root only) | CODE_GAP (read-only RPC field; see §2) |
| **expiry binding** | CODE_GAP / consensus gap **G-EXPIRY** |
| **ruleset-id binding inside the signed object** | CODE_GAP / consensus gap **G-RULESET** |

### Consensus gaps for the Lead (the registration wire is NOT changed)

* **G-EXPIRY.** The owner's signature (`palw_class_registration_message_v2`) covers network domain, class, share, activation DAA, owner bond, root,
  slash value, target, pwu rule and the canonical job — **no expiry**. A carrier cannot carry one: `lock_time` is a not-before bound, and the lifecycle
  carrier's inputs use the final sequence number, so it is not even evaluated. What exists instead is client-side only: the quote's `expiry_daa` is in the
  bundle and the signed file, `sign --rpc` and `submit` refuse past it, and the only consensus-level cancel is spending the funding input elsewhere. A
  leaked signed file stays valid until then. Closing it needs a versioned object field (a signed `valid_until_daa` checked by the fold) — new wire + fence;
  proposal only.
* **G-RULESET.** The ruleset id is not inside the signature either; the network domain (name + genesis) is. The signer recomputes the ruleset id from its
  own build and compares it with the bundle's, and the builder refuses quote nodes whose `consensus_params_id` differs from its build's; neither is
  enforced by consensus. A signed field (`ruleset_id` in the registration message) would need the same versioned bump as G-EXPIRY.
* **Registrant in the class row.** `PalwClassStateV2.registrant_bond` exists in state; `RpcPalwClassRow` does not report it (addressed in §2 if done).
