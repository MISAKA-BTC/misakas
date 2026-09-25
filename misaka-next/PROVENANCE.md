# Provenance — what misaka-next was derived from

misaka-next is **not** a port of testnet-12. It takes three things from it — the specification,
test vectors, and attack regressions — and nothing else. This file pins exactly which code those
were read from, so every citation in `docs/consensus/` can be checked.

## The frozen reference

| Name | Ref | Commit |
| --- | --- | --- |
| **t12 reference** | `rcore/int-3` | `a0af3c92` (2026-09-25 08:10 JST) |

Every citation of the form `path:line` in `docs/consensus/` means that path at `a0af3c92` unless it
names another commit. This branch (`next/misaka-next`) is based on that commit, so the reference
tree sits next to this directory and can be read and built from here.

ADR-0152 (account stake, staged reserve, vested rewards) is not on `rcore/int-3`; its text is read
from `docs/adr-0152-v31-postedits` @ `9ed1adce`.

## Pending t12 deltas not in the reference (as of 2026-09-25)

These branches carry t12 consensus changes that had not yet been merged into `rcore/int-3` when the
reference was pinned. The book records them where they touch a rule, labelled *pending*.

| Branch | Tip | Ahead of int-3 | What it changes |
| --- | --- | --- | --- |
| `feat/t12-aheld-node` | `c68479db` | 23 | held-attention attribution on the node (N3/N4, tag 57), F3 reason 8 for every dissection, processor arity fix, the forger's-race forfeit restore |
| `feat/t12-activation-pool` | `2e5f370e` | 28 | R1 (no reclamation of non-admitting rows), R2 (staggered Candidate audit), the Activation Pool (tag 58), jury seed v2 |
| `feat/t12-class-verify-deadline` | `559af0cc` | 6 | class-derived verification deadlines, P-1 pruning depth = D_cap claim lattice (74,920 on t12) |
| `rcore/p2-file` | `821b8ba5` | 8 | P2-8: the node files convictions on its own evidence (commit–reveal), PanelFalseValidV2 filing |
