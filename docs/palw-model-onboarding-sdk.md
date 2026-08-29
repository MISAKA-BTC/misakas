# The PALW model-onboarding SDK (`misaka-palw-sdk`)

**One interface every model class passes through.** Adding an LLM to a MISAKA network is four
agreements that must hold at once — a graph the court can walk (whose id IS the class id), an
artifact whose root the chain pins, a canonical job the class is paid per, and an engine that
executes what the graph describes. Before this crate those agreements were kept per lineage, per
consumer: two class tables of different types in `misaka_palw_base0::classes`, three per-lineage
arms in `kaspad`'s backend dispatch, two loops in the panel's registration builder, a magic switch
in the artifact loader. Every new model family meant finding all of them, and an arm forgotten in
one consumer was a class that could register but not produce, or produce but not be judged.

The SDK is the seam:

* **`PalwModelLineageV1`** (trait) — what a lineage must supply: container sniff/load, the frozen
  class table, artifact↔class pairing (shape check + root derivation), the known-weights keys, and
  chain-named `(class_id, artifact_root)` → execution backend.
* **`PalwClassSdk`** (registry) — the only door consumers use: `load_artifact`, `ledger`,
  `registration_candidate`, `preflight_admission`, `build_post_genesis_registration`, `resolve`.
  The kaspad panel, the producer's backend registry and the `palw-class` CLI all hold one of these
  and nothing lineage-specific.
* **`conformance`** — the standard battery, one call (`check_lineage_v1` / `check_sdk_v1`):
  the profile validates, every reference points backwards, both coverage gates certify (kernel ids
  AND per-node shape service), the canonical job counts, fits the worst case and the declared
  context, the court cost derives, and class ids are distinct across the whole ledger. The SDK's
  own tests run it over every built-in lineage, so a new table row is covered the moment it exists.

Consensus rules are untouched: the SDK produces the same `PalwConsensusObjectV2` objects through
the same `palw_post_genesis_registration_v1` / `verify_class_admission_v2` gates, and no ruleset
id, class id or fingerprint moves.

## Adding a new checkpoint of a known lineage (the common case)

No SDK code changes. The class is data:

1. **Freeze the geometry** beside its family's profile module in `kaspa-consensus-core`
   (`palw_qwen25_profile` / `palw_qwen36_profile` / …). A geometry that moved later would silently
   rename a class the chain already registered, so it is a named constant.
2. **Add the table row** in `misaka_palw_base0::classes` (`canonical_classes_v1` for the dense
   container, `qwen36_canonical_classes_v1` for the mmap tier). Take the next unburned rung of
   whatever axis separates siblings (the A16 family uses `n_ctx`; n_ctx 17 is burned — see the
   table's own comments).
3. **`cargo test -p misaka-palw-sdk`** — the conformance battery now covers the new row. A red
   test here is a class the court could not adjudicate; fix it before anything touches a chain.
4. **Convert the weights** with the family's converter (`qwen25-convert`, `qwen36-convert`, …).
5. **Dry-run the pairing and the gate** — no node, no keys, no coin:

   ```
   palw-class inspect   --network testnet-11 /path/to/artifact
   palw-class preflight --network testnet-11 /path/to/artifact --model-id <model-id>
   ```

   `inspect` shows which class the file pairs with, under which root, and why not the others.
   `preflight` runs the real admission gate (`verify_class_admission_v2`) against that network's
   bundle. Its genesis view is static — a live chain may hold more classes — but a REFUSED here
   is a refusal the chain would also give, before any fee is spent.
6. **Register from the node that holds the artifact**: `--palw-class-artifact <file>
   --palw-register-class <model-id>` with a bonded key. The node's registration loop goes through
   the same SDK path, reads LIVE terms, applies the known-weights rule and the sibling filters,
   and runs the admission preflight again before anything is signed or funded.

## Adding a new model family (a new lineage)

A new architecture needs real new code regardless — a profile builder in `kaspa-consensus-core`
whose kernels the adjudicator catalogs, an engine, an artifact container, a converter. The SDK's
contribution is that the INTEGRATION is one impl:

1. Implement `PalwModelLineageV1` in `misaka-palw-sdk/src/lineages/<family>.rs`, delegating to the
   family's own crates (the built-in `dense.rs` / `qwen36.rs` are the worked examples; the
   `TestLineage` in `lib.rs` is the minimal one).
2. Add it to `builtin_lineages_v1` (or compose it at a call site with
   `PalwClassSdk::with_lineage`).
3. Make `conformance::check_lineage_v1` the family's first test — `check_sdk_v1` in the crate's
   tests already runs it once the lineage is in the built-in list.

Nothing else learns the family exists: the panel's registration builder, the producer's resolve
path, the loader's magic dispatch and the CLI all iterate lineages generically. That is the
"must pass through" property — there is no second path a model can take, so the contract cannot
be partially honored.

## The two scars the candidate path encodes

* **Known weights never candidate for a new class** (`registered_weight_keys` vs the chain's
  `registered_artifact_roots`): re-registering weights the chain already has under a fresh id is
  never meaningful, and with two same-shape artifacts loaded it is exactly the 2026-08-28
  mispairing that burned the A16 n_ctx-17 seat.
* **Preflight before signature, always**: `build_post_genesis_registration` refuses to build an
  object the admission gate would refuse, so a hopeless registration never reaches the signer, the
  mempool, or the fee.

## What stays outside the SDK, deliberately

Converting weights (each family's converter binary owns its container), key custody and carrier
funding (the node's), fleet distribution of multi-GiB artifacts, and the economics a registrant
must not choose (share, target, slash value — the chain's terms). The SDK is where a model becomes
a class; it is not a wallet and not a deployment tool.
