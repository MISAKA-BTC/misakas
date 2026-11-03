# Equal-work pruning import construction order

The fresh candidate69 IBD panic at `model/services/reachability.rs:63` is the
plain store implementation's unchecked parent-antichain query. Its pruning
import caller is `populate_reachability_and_headers`; P2P proof validation builds
a separate temporary proof context before applying into staging. The nearby
`Built headers proof ... 1 unique` log can be the fresh current consensus's
ordinary genesis defender proof and does not establish a responder race.

PALW zero-chain-position parents and children can legitimately have equal blue
work. The import previously treated `(blue_work, hash)` as a topological order.
A child preceding a larger-hash parent then queried reachability that had not
been constructed. Sorting each expanded proof level only by blue work also let
trusted headers appended after their children be silently dropped by the
subsequent known-ancestor filter.

The apply correction preserves the old work/hash priorities among ready items,
while ordering the induced graph's parents before children. The combined
reachability graph includes parent relations from every header level. Cyclic
constraints are rejected with the existing typed import-context error before
headers are persisted. Reachability read errors in the parent-antichain query
remain errors, never false or an empty parent set.

The parent-antichain reduction also previously assumed that descending work/hash
visits descendants first. Equal work defeats that premise: adding a descendant
must remove any earlier ancestor. The symmetric reduction now derives the same
maximal antichain independently of which tied parent arrived first. GHOSTDAG's
work calculation, coloring, selected-parent tie-break and consensus fields are
unchanged; this is the construction order for the imported DAG.

Four production DB-fixture regressions exercise equal-work reverse-hash import
with a redundant parent (the panic shape and final exact parent set), dependency
from a higher parent level, trusted-appended per-level order, and typed cycle
refusal without header/reachability persistence. The fixture uses the actual
ConsensusStorage/ConsensusServices and production populate function, not a copy
of the importer. Local checks are rustfmt and git diff --check; heavy Cargo is
reserved for the parent's integration target. Suggested filter:
`cargo test -p kaspa-consensus --lib parent_order_tests`.

Proof-builder traversal ordering is a separate coordinated change owned by A;
this patch does not claim that fixing the apply side alone completes fresh IBD.
