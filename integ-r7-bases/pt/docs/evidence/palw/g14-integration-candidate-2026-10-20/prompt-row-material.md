# Authenticated prompt and two-tile row material projection

Parent `7de51563d`. ADR-0199 A isolated read-only builder view; no persistent material row, paid admission or production activation.

## Prompt boundary

The view resolves the prompt form from the retained original plan's owner-work rules. Input material cannot select another form. Missing material remains explicit missing input. A Flat list must have the original's declared prefill count and exact committed hash. A Merkle subset must fit the original's tile cardinality, have unique ascending coordinates and verify each opening against the original count/root through the existing native verifier. Whole-list reads return only checked Flat data; per-tile reads return only supplied checked Merkle material. In particular, a valid tile does not mark unprovided tiles available.

The existing GQA original fixture is unchanged. A prompt-reading cone is found through the unchanged direct builder as test oracle; its parameter read subset and already authenticated step units provision the material view. The material-backed cone must serialize identically to the direct cone, while missing prompt or changed bytes/form refuse. This is one functional cone comparison, not original-wide dependency discovery or prefetch.

Two local identity tests cover Flat and ragged multi-tile Merkle input. The Merkle case constructs a synthetic context/commitment solely to test count/path/form authentication; it is not a registered native Merkle original, node admission or a replacement of the fixed GQA fixture. A complete native Merkle-original builder/transport campaign remains required.

## Row projection boundary

The view projects a native `PalwTiledDecodePinV1` from complete authenticated IDs and the committed-token and requested-lane TraceTile capabilities. Both tile records must name the same original-source row root and row opening. Missing IDs/tiles, an absent row or an out-of-vocabulary requested lane returns None. Committed token values are not certified as correct by this projection; native adjudication remains responsible for malformed/incorrect commitments.

A new separate 4,097-word original fixture exercises a full 4,096-lane tile and a one-lane ragged tile without changing or repinning the fixed history fixture. All fixture decode rows at lane0, lane4095 and lane4096 are compared byte-for-byte with the unchanged whole-row native row-pin builder. The new store itself receives no whole rows. Missing IDs or the needed other tile must refuse. Decode-pin projection must match the existing native store. These are synthetic/verified-half registration/payment/service fixtures, not real-model cost or funded actual-node results.

## Unclosed work

Prompt/row projection fills the borrowed-view API gap; it does not provide storage, authenticated source delivery, bounded prompt wire ingress, holding contracts, paid reservations, original-wide material closure, restart/reorg, invalid-ingress or complete allocation/CPU/IO/fee calibration. Clone and builder/output ownership remain separately caller-admitted unfinished work. Construction authenticates data identity, not computation correctness, custody, standing, default, slash, credit or payment. F02/F03/T05/T01-T15/R-D2 and measured real-model door remain incomplete. No cap/deadline/tag/fee change, pre integration/push, arming or deployment follows.

The first row-pin test run exposed a wrong assumption: the existing wide fixture has 4,096 words and only one tile. That failure is preserved. The existing fixture is unchanged; a new separately constructed 4,097-word synthetic program/actual-job-context fixture supplies the required second tile. No original fixture/model was repinned and no production cap was raised. An earlier test compile also required a checked u32-to-u8 tile argument conversion for the existing event helper; that log is preserved too.

## Functional checkpoint

Fresh prompt identity tests pass 2; public-original tests pass 43 (including the newly added two-tile row-pin case and the extended prompt cone/history case); the same fresh unit executable passes 11 court/inventory and 2 boundary-range tests. Cases inside tests are not extra test counts. The synthetic Merkle identity case is not promoted to a native Merkle-original campaign. Failed initial compile and fixture-assumption runs are preserved separately; their passing subsets are not added to fresh totals. Integration and type-check results are recorded after completion.

Fresh t12 identity (9) and activation (5) checks also pass: 72 unique selected tests total (prompt2 +native43 +court11 +range2 +integration14). Documentation check remains exit1 for the same five imported missing references plus 43 baseline-known occurrences; no authored missing link is added. Type-check exits are recorded in the artifact after completion.
