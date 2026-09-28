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
