# RFC-0012 owned fixture process cleanup — 2026-10-24

This is a harness lifecycle fix, not execution of the RFC-0012 campaign or a gate PASS.
K3 specification, manifest, weights and signed execution inputs remain unavailable
(user response: not created). No activation, public node, network rules or acceptance
threshold changes are made.

## Defect and scope

[MeasuredDrill.command](../../../scripts/drills/misaka-rfc12-multiprocess.py)
previously killed only the directly launched command on timeout. Its forked children
could retain log descriptors and continue work after that leader exited. Sampler
registration also happened before the cleanup `try`, so a registration exception could
leave the already launched command alive.

The [owned fixture helper](../../../scripts/drills/rfc12_owned_fixture.py) launches each
finite command in a new POSIX session and sends SIGKILL to that exact owned process
group during cleanup, even when the leader has already exited. The direct child is
reaped; registration now falls inside the protected lifetime. Each command records the
cleanup scope and outcome. Output bytes are checked again after group shutdown to
include writes between the last loop check and cleanup.

This does not contain descendants that deliberately create another session. It is not
an OS memory/disk quota, complete process-tree RSS measurement, a proof of all process
termination, a node crash/restart acceptance run, or authorization to signal any other
process group. Non-POSIX launches refuse this path before starting a fixture.

## Verification

The [real subprocess controls](../../../scripts/drills/tests/test_rfc12_owned_fixture.py)
exercise a live leader plus forked descendant, an already exited leader with a live
descendant, an unrelated live process, registration failure, timeout with retained
stdout, and finite success/nonzero exit. The fork controls require EOF on an inherited
output pipe after cleanup; killing only the leader cannot satisfy them.

Commands run from the repository root:

```text
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_owned_fixture.py' -v
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_multiprocess.py' -v
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_owned_resources.py' -v
```

Results: 6 lifecycle controls, 11 existing multiprocess oracle controls, and 4 existing
resource observation controls passed. These counts are harness controls only. Actual
multiple-node useful-work, 100k claims, pruned IBD, OS crash/cold restart, real adapters
and the 15-gate campaign remain unexecuted/UNKNOWN where evidence is absent.
