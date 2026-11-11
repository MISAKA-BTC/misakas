# Ordinary TFP1 native public replay adapter — 2026-11-11

Implementation commit: `22ad8228c`. This extends the [literal TFO3 pairing work](../tfo3-native-pairing-2026-11-11/README.md).
It is a local material-correspondence adapter, not an accepted public claim,
full relation approval, Final, activation or main push.

## Implementation

The remote adapter checks exact FP claim, signed companion carrier digest,
Original ID, Source/header, registered class/artifact and literal public
prompt. It uses the native-selected prompt commitment form. The registered
raw tokenizer must reproduce the token IDs. Program, output, stop condition,
Trace index and every published trace tile are checked against one independent
legacy `TirClassRunnerV1` run through the existing G14 OwnStepSource.
The run computes its own step/logits/aggregate/output roots; it does not copy
the alleged roots as its result. Public raw tiles must authenticate under the
original root and match the independent disk source's exact leaf hashes.

The existing wide replay shares only the bounded tile-checking loop. It keeps
its own execution rules and signing contract. No wide plan ID or old signed
claim changes meaning. The new adapter does not expose an approval constructor.

The replay lease includes existing native RAM/disk needs, actual retained
prompt/output vector capacities and two bounded hash pages. Shared OwnStepSource
reads reserve an exact vector capacity and preserve ordered checked disk reads,
including rejection of zero, oversized and out-of-range requests. Provider
acquisition, tokenizer/program decoding and complete profile work/IO admission
remain separately owned by the future caller; wire byte limits alone do not
qualify those resources.

## Verification

The final remote build completed in 20.10s. Two tests passed on the same
binary: the new ordinary legacy fixture (0.27s) and the existing wide signed
replay (0.80s). The ordinary fixture includes exact replay RAM, ordered/exact
hash-page capacity, final hash, range refusals, wrong prompt form/text/Source,
corrupt final tile and missing final tile. This uses a tiny local execution;
it is not replay of the full Studio model on public T12.

See [build and test](build-and-test.log), [wide regression](wide-regression.log),
[source hashes](source-snapshot.sha256) and [summary](summary.json).
The first build passed but its test failed because the adapter incorrectly
compared canonical job-context trace scheme with program logits scheme.
[That failure](initial-failure.log) is retained. The fix classifies using the
registered program and native Source; the generic context ID is unchanged.
The additional exec unit assertions were not run through a new standalone
exec test binary; the same production OwnStepSource path and boundaries were
exercised by the final remote fixture.

## Saved Studio program and Source

The [offline probe](studio-source-probe.rs) reads the bounded runtime artifact
header/program and saved capture, validates worker request/result and the full
candidate commitment, then checks exact class, prompt/output and Source roots.
It reproduces claim `7645bc8b…a3711`, class `7a2616b4…c625`, Source
`1e9ef75b…127a` and 5,154 leaves. The registered program's logits scheme is
legacy tiled; the canonical job-context trace scheme is a different generic
identifier. The earlier worker-request probe alone did not establish that
registered-program distinction.

The inventory root comes from the earlier checked binding record. The probe
does not rehash weight tensors, rerun inference, or query public T12. Its saved
capture lives under the earlier `23d6…06cde` directory: it is used solely as
content after matching class, job context, prompt/output and all committed
roots to the target commitment. That directory is not a public receipt for the
target claim. [Exact inputs and hashes](studio-source-evidence.json),
[command](studio-source-probe.sh) and [output](studio-source-probe.log) preserve
this scope.

## Remaining work

The [connection map](remaining-connections.md) records the missing actual
paired-link reader, ordinary producer/material publisher, cold artifact
assembly, legacy ExecutionRules/relation plan, full FP retention and
claim-level approval/Final integration. Complete protected service budgeting
and public quote/acceptance are also unfinished. No local equality is used as
consensus credit or an independently selected verifier's approval.
