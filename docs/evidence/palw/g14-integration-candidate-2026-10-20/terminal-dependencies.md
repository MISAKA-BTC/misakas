# Numerical terminal reconstruction dependency audit

Status: source audit of isolated candidate `28bc2f9ce`; not a runtime measurement or adopted protocol change.

## Observed dependency boundary

`PalwTirPublicRequestV1` exposes StepNode, StepLeaf, StepRun, RowsNode, TileNode, TraceTile and Ids. `TirSharedRequestSetV1` enumerates those families. `SharedStreamTargetV1` accepts one enumerated Unit or FullIds. None is a parameter inventory opening or prompt opening target. Generated IDs must not be treated as prompt IDs.

The numerical bottom builder calls `builder_partials` (or its bounded verified counterpart) then `assemble_carriage`. `TirSource::param` reads an inventory leaf and records it in `used.params`; `TirSource::token` records prompt positions before prefill and generated-ID use afterwards. `assemble_carriage` conditionally asks the evidence store for parameter carriage, whole flat prompt or Merkle prompt tiles, and decode pin. Parameter carriage is authenticated against the registered artifact root and exact requested inventory keys. These are data identity checks, not model review or computation correctness proofs.

| Builder dependency | Current shared source coverage | Work required before claiming reconstruction |
| --- | --- | --- |
| Step preimages and range siblings | StepLeaf/StepRun material exists | Implement bounded adapter and authenticate exact range; do not assume independent openings concatenate into a canonical range proof |
| Parameter carriage | No parameter request family | Identify actual read inventory coordinates; authenticated fetch, holding horizon, bounded multiproof construction and paid reservation |
| Prompt IDs | No prompt request family | Resolve actual native prompt form; bind flat prompt or tiles to original; admit fetch/retention/reconstruction |
| Decode pin | IDs/rows/trace primitives exist | Construct the exact native pin with authenticated scheme/context; partial overlap is not an implemented adapter |
| Numerical root and ancestry | Immutable signed records exist | Preserve actual native context and terminal path; no session fabrication or automatic default |

## Consequence for independent pursuit

Persisting a first filer's incomplete terminal body in one canonical root/path slot would create an exclusion point: another filer could be forced to wait behind a non-completing body. Per-filer slots alone do not establish finite paid storage, fair inclusion or an honest completion bound. The complete-bundle codec intentionally installs neither policy.

Before implementing persistent terminal transport, close one full path: either authenticated primitive material with bounded deterministic terminal reconstruction, or independent witness streams with a demonstrated finite funding/capacity and nonblocking completion rule. The former needs the parameter/prompt dependencies above; existing shared DATA is insufficient evidence that it works. This audit selects neither a new custody policy nor a fee/timeout parameter. Existing floors, caps, deadlines, old tags and production rules remain unchanged.

## Next falsifiable implementation checks

1. Instrument only test evidence stores around unchanged native builders. Record actual dependency coordinates for every terminal in the existing public-original histories; retain the original fixture and its commitments.
2. Withhold each actually used dependency family separately. Verify explicit missing-data refusal and unchanged consensus state; never turn unavailable data into a numerical conviction.
3. Build a bounded source adapter only after its authenticated input coverage is demonstrated. Compare its full terminal bytes and local outcomes with the unchanged builder, including all sibling branches and both honest/false signed numerical histories.
4. Add full source/assembly/invalid-ingress resource and funding terms, then persistence/recovery and actual-node T05. Codec success and this source audit do not discharge these gates.

Source SHA-256 values and scope exclusions are recorded in `terminal-dependencies-source-audit.json`. No tests were added or rerun for this source-only audit; prior pass counts are not new evidence for these dependencies.

## Runtime follow-up (parent `7834659e2`)

The existing signed-history test now wraps the unchanged `TinyExecution` store with a cfg(test)-only delegating observer. Both honest and false numerical partition histories traverse the same named leaf's twelve terminals: 24 observations total, not every eligible leaf of the original. Every rebuilt terminal is byte-identical to direct construction. All 24 request parameter openings and parameter carriage, and each of those two APIs is separately withheld: 48 refusal assertions pass. These are builder refusals, not convictions or financial transitions. The surrounding consensus state root stays unchanged during construction/refusal; this read-only builder has no state writer.

Outer API coordinates are in the raw log. Delegated implementations' internal calls are not recursively traced. This fixture's terminal builders use step leaves, output openings, range siblings and parameter data. No terminal requests prompt or decode-pin data, so no corresponding negative assertion is exercised. General conditional prompt/decode dependencies in the source audit remain unresolved, not disproven. There is no new custody/funding assumption or source reconstruction adapter.

The selected public-fold suite passes 42 tests. Test count is unchanged: the existing native history test was extended. The 24 rebuilds and 48 refusal assertions are cases within that test and must not be added to the test count. See `terminal-dependencies-runtime.json` for machine-readable scope and source hashes, and `terminal-dependencies-native.log` for raw output. Earlier source-audit statements about no new runtime experiment describe that earlier checkpoint only.

Fresh t12 identity (9) and activation (5) tests also pass: 56 unique selected tests total with the 42 public-fold tests. No whole-G14 or node T05 success is inferred. Documentation check remains exit1 with the same five imported missing references and 43 baseline-known occurrences; no new authored missing reference.
