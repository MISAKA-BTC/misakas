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
