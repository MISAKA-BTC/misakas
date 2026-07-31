# misaka-palw-bridge — node-side desktop-coordinator bridge

The server half of the palw-gateway coordinator protocol v1 (see the gateway repo,
`palw-gateway/README.md` "PALW coordinator protocol"): the daemon a
`palw-gateway --palw-coordinator http://host:26621/palw/v1` points at. It coordinates
A-commits and B replicas across REAL distinct providers and decides matches with the
node's own k=2 predicate.

## What makes it the *node-side* bridge (vs the gateway's dev loopback)

1. **The match is the real one.** Replica matching builds two
   `misaka_palw::palw::ReplicaMatchKey`s and runs
   `misaka_palw::palw_replica::run_replica_k2` — the same eight-field exact-match
   predicate (design §7.5) the consensus lane mints leaves with, using the same
   domain-separated constructors (`job_set_commitment`, `gemm_trace_root`,
   `operation_schedule_commitment`, `MIL_PALW_*` domains). The qi35-serve class maps:
   - `job_set_commitment` ← class label ‖ job_id ‖ max_new ‖ prompt ids (LE)
   - `output_commitment` ← `Hash64_k(output-domain, decoded output_root)` — an
     equality-preserving re-keying of the gateway's output-ids root, which is all the k=2
     EQUALITY predicate needs. The salted, leaf-grade commitment is a separate check:
     seam 1 verifies `output_commitment_v3(output_token_ids, job_challenge)` against the
     leased challenge (see below), which IS byte-identical to the live receipt-v3 path.
   - `canonical_gemm_trace_root` ← the engine's ROUTE root (the class's canonical
     execution-trace commitment: MoE routing decisions)
   - `operation_schedule_commitment` ← KV root ‖ recurrent-STATE root
   So two replicas match only if they agree on the output AND the execution structure.
2. **Independence is enforced at the protocol layer.** A job is never offered to its own
   submitter. There is no `allow_self_replica` here and no flag to add one.
3. **Class strictness.** Submissions and results without `runtime_roots` are rejected up
   front (the gateway captures them from the engine's `ROOTS route= kv= state=` line,
   emitted under `QI35_SERVE_ROOTS=1`).
4. **Durable, tamper-evident state.** Every mutation is one JSONL event with
   `root = Hash64_k(journal-domain, prev_root ‖ event_json)`. Boot replays and verifies;
   an altered line refuses to load; a torn final line (crash mid-append) is truncated
   away. The head root is the audit digest a future consensus seam anchors on-chain.

## Protocol (under `/palw/v1`)

```
POST /palw/v1/jobs                        JobSubmission     → {accepted:true}   (idempotent)
POST /palw/v1/verdicts                    {job_ids:[…]}     → {verdicts:[{job_id,verdict}]}
GET  /palw/v1/assignments?provider_id=X                     → {assignments:[…]} (claim-on-fetch)
POST /palw/v1/assignments/{job}/decline   {provider_id,reason} → {declined:true}
POST /palw/v1/replica-results             ReplicaResult     → {recorded:true, matched:bool}
GET  /palw/v1/status                                        → journal head/seq, phases, providers
GET  /health
```

Verdict semantics: `replica_matched` is delivered exactly once (the delivery itself is a
journal event), then `certified`. A `mismatch` is an UNRESOLVED DISPUTE between two
replicas — the consensus lane resolves disputes with sampled audits; this bridge only
surfaces them. A replica result missing roots, late, or from a non-holder never brands
the job: the claim lapses/requeues instead.

## Run

```
misaka-palw-bridge --listen 127.0.0.1:26621 --data-dir /var/lib/palw-bridge \
  [--auth-token T] [--assignment-deadline-ms 120000]
```

## Verified end-to-end

Two palw-gateway instances with two REAL qi35 35B engine processes:
prov-a answered a `palw_mint` chat turn (7.3 s) and committed output + route/kv/state
roots → this bridge offered the job only to prov-b → prov-b's own engine ran a full
replay → both keys agreed field-for-field → `run_replica_k2` = Matched → verdicts flowed
back → prov-a's turn reached `certified`, journal seq 4, and a bridge restart replayed
to the identical head root. The independent-process root agreement also demonstrates the
engine's execution commitments are invariant to prefill chunking (A used a system-prefix
snapshot; B replayed from zero).

The four seams are covered by `tests/consensus_seams.rs`, which runs the whole path with
REAL ML-DSA-87 keypairs and pinned chain facts derived from those keys — so every signature
and credential check is the production one. It asserts: an impostor key, a tampered session
signature, an expired delegation and a slashed bond are all refused; a lease binds the
prompt, the requester and the epoch, and swapping the answer under a leased challenge is
refused; a beacon-sampled chunk is proved and a tampered one is not, and a silent provider
sweeps to timeout evidence; and a mismatch escalates, draws the one unconflicted auditor
(over a higher-staked but conflicted sibling), attributes slash_b / slash_both correctly,
refuses adjudication by a disputant, leaves the dispute open when no third party exists,
and survives a restart with an identical journal head root.

## Consensus seams (`--require-bonded`)

With a chain-facts source (`--node-rpc host:port` for a live node, or `--pinned-facts
<file>` for offline dev) and `--require-bonded`, four seams are wired to the node's own
primitives. `/palw/v1/status` always reports which source is in use and whether it is live.

**Seam 1 — beacon-bound challenge, salted commitment** (`challenge.rs`). The tree leaves
`job_challenge` as a free input (dispatch only refuses all-zero) and the v1 salted
`output_commitment` has no production caller. This implements ADR-0040 §537's derivation —
`H(network_id ‖ epoch_beacon ‖ epoch ‖ scheduler_job_id ‖ requester_credential ‖
request_commitment ‖ shape_id)` — in a bridge-local domain, over the finality-buried beacon
read from `getPalwState.activation` (refused unless `derived_mode == "healthy"`, so a
carried seed can never silently salt a commitment). It is issued as a **lease BEFORE
generation**, bound to the compiled prompt and the requesting credential: a provider cannot
regenerate and re-commit under a challenge that suits the answer it liked. The answer
commitment is `output_commitment_v3(output_token_ids, job_challenge)` — byte-identical to
the live receipt-v3 path, not a lookalike.

**Seam 2 — bonded identity** (`provider.rs`). A provider is its bond outpoint, not a
string. Registration presents `owner_public_key` + a `PalwProviderSessionAuthorizationV1`
(the node's own cold→hot delegation object) and is checked with the repo's universal
pattern: `validator_id_from_pubkey(pk) == registry owner_pubkey_hash`, bond ACTIVE at the
point of view, signature verified by `kaspa_txscript::verify_mldsa87_with_context`. Every
later request carries an ML-DSA-87 session signature over route+body in a bridge-local
context, and the bond is re-checked live — a provider slashed since registration stops
being able to submit or replicate.

**Seam 3 — DA** (`da.rs`). A canonical chat-context object (version 4) carries exactly what
an auditor needs to replay. Chunk indices come from the real
`palw_da_provider_sample_indices` (beacon-driven; nobody picks their own chunk), retention
and response windows from `PalwDaPolicyV1::STRICT_TESTNET`, and failure produces the node's
own `PalwDaTimeoutEvidenceV1`. The chunk tree is re-implemented here rather than called,
because `palw_receipt_da_commitment` accepts only versions 1/2/3 and widening that set
would change what `register_leaf_obligations` does with an on-chain leaf — a consensus
behavior change for an off-chain need. `commitment_matches_consensus_for_shared_versions`
proves byte-for-byte agreement with the node's commitment, proof and verifier across
versions 1/2/3 and multi-chunk/partial-chunk/padded shapes, so version 4 is the same
algorithm in a domain consensus has not claimed.

**Seam 4 — arbitration** (`arbitration.rs`). The tree specifies mismatch attribution and
leaves it inert (`PalwMismatchParams::INERT`, zero callers, ADR-0040 SLASH-01 未着手), and
ADR-0045 D2 puts auditor scheduling explicitly outside the consensus crate. So: escalate
with the real `PalwMismatchRecordV1::is_escalated` draw, draw an auditor with the real
`select_weighted_auditor_committee` (both disputants' credentials AND operator groups
excluded, so no party adjudicates its own dispute and no sibling does it for them), take
the auditor's reference re-run, then `attribute()` → `slash_targets()`. If no unconflicted
third party exists the dispute stays open rather than being decided by an interested party.

### What is still NOT here

Arbitration ends at signed, journaled EVIDENCE. It does not submit a slashing transaction:
`PalwMismatchVerdict` has no on-chain carrier (0x39 is reserved and undecoded; the only
wired slash paths are DA timeout 0x3c and search timeout 0x3f), and a slash is all-or-
nothing on the bond's output-0 (`econ-parameters-frozen.md` E8 — the `u64` in
`Slash(outpoint, u64)` is a DAA score, not an amount). Likewise: chat DA obligations live
in this journal, not in consensus (`register_leaf_obligations` is reachable only from an
accepted 0x32 leaf, and the on-chain 0x3b response lane refuses object versions other than
1/2); rewards and `Matured` remain chain state; the journal head root is not yet anchored
on-chain. Finally, the auditor committee is drawn from providers REGISTERED WITH THIS
BRIDGE — no RPC enumerates the chain's provider registry, so that set is a subset of it.
