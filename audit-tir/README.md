# audit-tir — the D-F drill kit (RFC-0002 Phase F)

One salted testnet-12 drill chain on this Mac, loopback only. Usage, stages and every command are in the
header of `df.sh` (`bash audit-tir/df.sh` with no argument prints it); the layout (nodes, ports, classes) is
`lib-df.sh`; the sampler is `dfwatch.py`.

## Known drill behaviours (not production defects)

- **The old relay must connect after the flag days are scheduled past, or it is never refused**
  (drill of 2026-09-29). The `old` relay (the fleet's previous release) connected at DAA 1, when both of
  its "next" fences were 6: no refusal height was stored, so the connected-peer rejudge never fired at
  `TIR_AT`. The old node only saw UTXO-invalid blocks past the IR fence and stalled at DAA 20. Restarting
  `old` gave the handshake refusal (`WrongForkId`) and D-F4's cross passed. So restart `old`
  (`bash audit-tir/nodes.sh stop old; bash audit-tir/nodes.sh start old`) once the chain is past
  `FENCE3_AT` and before `TIR_AT`, and judge D-F4's cross on the handshake refusal. Production is not
  affected: every fleet node was restarted after DAA 1,700 by the int-8 upgrade.
- **A registrant is restarted without `--palw-register-class` once its class is on the chain**
  (`lib-df.sh` `class_on_chain`). The int-8 release's panel builds nothing for a class the chain already
  holds, never marks its registration done, and skips every later duty of its tick (readiness proofs,
  seat duties, courts) for as long as the process lives. Restarting `new0` or `new4` with the flag after
  their classes landed would take one of the seven ready seats away and hold D-F1 or the small class. The
  node fix (a dropped registration is resubmitted, a registered class is done, a registrant keeps its
  duties) is on `tir/node` after 7a9ab18da.

## The B/D/C piece (RFC-0002's evidence transport)

`docs/design/palw/tir/evidence-transport-scope.md`: a class too large for any seat to hold the accused capture
is convicted through the executor's served annexes (B), the chain's demand of a step leaf (C: past
`palw_tir_fence2`, the executor discloses or its claim defaults), and F7's bottom built from the accused's root
claim on chain (D). The piece drills all three live on the SMALL class, with its producer answering only
(`--palw-drill-answer-only`: it announces and serves its answer envelope, never its capture — a 1.5B class's
condition, on a class that runs in minutes):

    DF1=0 TIR2_AT=30 bash audit-tir/df.sh up --bin-dir <tir/node release>   # its own chain: fence2 is set at `up`
    DF1=0 TIR2_AT=30 bash audit-tir/df.sh bdc                               # once the small class is Active

- **B**: new4 lies at an undissected leaf; the seats find it through its served annexes and convict.
- **D**: new4 lies at the dissected kind; the named leaf opens F7's dissection and the challengers build its
  bottom from new4's root claim on chain.
- **C**: B's lie with `--palw-drill-refuse-leaf-evidence` (no annex served): the seats demand on chain
  (`DefaultAccusedTirStep`, keyed by the claim alone) — the root, then the first frontier node their own tree
  disputes (eight levels a session), then the leaf — taking the rounds in turn (a seat waits on an open demand of
  the unit and staggers its own); new4's node answers each; the seats read the disclosures back and convict.
- **C0**: C, and new4 stopped once a demand is on chain: its claim defaults (`ProducerWithholding`, counted by
  `dfwatch.py` as `withheld`).

`DF1=0` loads no 1.5B class (the seven holders hold the small class alone, new0 is a plain seat, no old relay).
`TIR2_AT` must be past `TIR_AT` and is fixed for the chain's life (the datadir marker's `tir2_at`). The verdict
is `$WORK_DIR/bdc.verdict`; `BDC_LEAF` / `BDC_DISSECTED_LEAF` override the leaves read from the small class's
`small-leaves.txt` / `small-close-sizes.txt`.
