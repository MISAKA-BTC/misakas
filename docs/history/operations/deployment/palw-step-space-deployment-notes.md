# Deploying the adjudicable step spaces (ADR-0070) — operator notes

Branch: `palw-step-space-e2e`. Fingerprints after this train: testnet-11 `923fe103…`,
devnet `65eaa6e7…` (from `d7510c7a…` / `3f13411b…`).

## What kind of change this is

`PALW_V2_TRACE_FORMAT_VERSION` 2 → 3 moves the RULESET id and therefore the handshake
fingerprint. **The genesis is untouched: this is NOT a re-genesis and NOT a wipe.** Upgraded and
un-upgraded nodes follow the same chain but refuse to peer, so the fleet partitions at the
handshake until everyone is on the new build. Deploy in one window:

1. Build the release binary from this branch on every host (kaspad release build verified).
2. Stop all nodes in one window; start them all on the new binary. Datadirs stay.
3. Producers/miners restart with their existing flags; nothing about jobs, shares or bonds moves.

The court behavior differences (what is adjudicable) never disagree about any object that existed
before the train — no court session has ever run on t11 — so there is nothing to replay.

## What becomes true after the train, and what stays false until registration

* The floor and both model tiers' backends can capture, answer rungs, and assemble closes. The
  A16 tier's captured attempt lane runs for the CORRECTED class only.
* The chain still carries the v1 model-class descriptors (`ec7bbcbf…` hybrid, `f942e268…` A16).
  Those remain producible exactly as today (their backends keep the legacy composite roots) and
  remain unprosecutable — nothing about this train changes their claims.
* Prosecutable model-tier claims begin when the corrected classes are REGISTERED and producers
  switch to them:
  * A16: `qwen25_a16_registration_v2` / catalog row `Qwen/Qwen2.5-1.5B/graph-v2` — its
    `artifact_root` is the A16 operand-INVENTORY root over the converted 1.5B artifact
    (`a16_inventory_v1`), not the artifact digest. The SDK's registration builder derives it
    (`CanonicalClassV1::artifact_root` decides; the resolve path matches the same way).
  * QWEN36: the graph-v3 row, with the qwen36 operand-inventory root for the same reason.
    Note the graph-v3 profile id may have moved with the court-capable corrections — read the
    current pinned value from `palw_qwen36_profile.rs`'s test rather than from memory, and
    re-check before any `--palw-register-class` invocation.
* Registration is the established post-genesis path (bond + fee, `--palw-register-class`), or a
  future re-mint's genesis card; producers then run `--palw-producer-class=<corrected id>`.
  Moving weight-bearing share onto the corrected rows is ADR-0054's business as usual.

## Interactions to keep in mind

* `--palw-chain-classes` (ADR-0067 Decision 5) stays a deliberate operator arm; the corrected
  classes resolve through the build's own tables without it, and through the chain arm with it.
* The drill flag `--palw-drill-tamper-leaf` now works for the model tiers too (their
  `execute_with_injected_fault` is real) — a devnet court drill against a corrected-class claim
  is the recommended smoke test after the train.
* misaka-palw-worker still needs `MISAKA_LLAMA_SRC` and is not part of any node's build.
