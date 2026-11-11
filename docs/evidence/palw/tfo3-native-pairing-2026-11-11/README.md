# Literal TFP1, public prompt and companion pairing — 2026-11-11

This is local implementation evidence. Public T12 quote, acceptance, Final,
activation and main push remain incomplete. TFO3 is a new unpublished transport;
no existing TFP1 or TFO2 version or signed claim is reinterpreted.

## Transport and actual transaction

TFO3 version 1 carries the literal TFO1/TFO2, exact nonempty UTF-8 prompt and
fixed signed PublicOriginal body. The existing total funded-original ceiling
covers all three. Length/version/EOF, malformed UTF-8 and recursive wrappers
are rejected. Decoding borrows each body; transaction pairing stores ranges
into its actual transaction clone. It does not make a separate copy of the
prompt or companion. Direct, FPA1 and FPR2/FPA1 views retain the actual txid and
its own funding output. Supplying a different full inner payload is refused.

The preparation adapter checks actual carrier transaction origin, exact FP
claim, actual header Source identity, payer Source declaration and the native
obligation. It hashes literal prompt bytes under the native public ruleset
and prompt codec limits before authenticating the companion. The Source
identity keeps the existing domain and canonical Borsh bytes; streaming avoids
an extra serialized-header allocation. A same-claim header artifact change,
even with a fresh txid and matching altered payer terms, must not borrow the
old companion. Prompt hashing authenticates identity, not correct tokenization
or computation.

All current production profiles still refuse TFO3 with a missing complete
public profile. The isolation validator and original coordinator prevent
stripping the companion and accepting the enclosed legacy FP. The native
fixture checks unchanged state/journal on this refusal and successful existing
TFO1 admission afterward. No public fence or numerical parameter is enabled.

## Exact Studio candidate input

The probe reads the actual framed worker request and result, checks strict EOF,
recomputes their request hash and uses the native result validator. Reconstructing
the original with the same retained horizon reproduces claim
`7645bc8bbba0e00a35ccbac77ecb0e28d489688161bd0130649f3575919a83df225e509936a52c4b0e30a415e71d50da1f38dd87a1f02ce4267bc145fc6a3711`.
The literal request is `Hello`, worker prompt IDs are `[19556]`, and the matching
raw tokenizer's existing `encode_without_specials` returns the same IDs.
The native prompt commitment validates as MerkleV1; Flat refuses. This is local
byte/identity evidence, not new inference, public admission or independent
full replay. The initial Flat-only probe failure is retained, not overwritten.

The wide public replay hardcodes Flat, but changing that is insufficient:
the actual candidate uses the legacy tiled trace scheme, not wide-Q24. The
next connection must use the existing native legacy-width deterministic
runner with the candidate's MerkleV1 prompt form. It must preserve the actual
claim and trace scheme. A rendering table is not a substitute for the raw
tokenizer. Probe dependency binaries are hashed; their source provenance was
not independently qualified, so this is a reproducible local diagnosis rather
than a final-candidate public compatibility test.

## Remaining native connection

See [complete profile integration gaps](complete-profile-gaps.md). Complete
selected/outsider/defense plans, authenticated native scope/draw, cold state and
dependencies, shared-book capacity and one atomic FP/companion/link writer are
still required. Initial source/protocol parsing and shape quotes are not a
complete calibrated decoder or physical service budget. Synthetic funding in
the signed-companion test does not prove payer consent or payment.

## Verification

Implementation commit: `8d3eb0a6e`. The core/RPC test build completed in
4m 28s; Node library `cargo check` completed in 58.48s. All **16** selected
tests passed, including the actual native Batch refusal/legacy acceptance and
ordinary funded financial transitions. Exact filters, source/binary hashes and
counts are in [summary.json](summary.json), [source snapshot](source-snapshot.json),
[build log](build.log), [Node check](consensus-check.log) and [tests](tests.log).
Intermediate pre-prompt and pre-Source-binding logs remain separate; they are
not evidence for later source. Source snapshots cover the 13 changed source
files; the subsequent legacy replay adapter is outside this verified commit.

Documentation checks retain the pre-existing 930 issue occurrences beyond
the baseline, 971 missing-file and 2 missing-heading occurrences, 7 RFC
historical failures and 89 ADR failures. The full link-issue counter is
unchanged and these evidence documents add no link error. The baseline was
not modified. These failures are not a passing documentation audit.

Exact Studio request/result frames are retained here with their hashes.
[Probe source](studio-prompt-probe.rs), [command](studio-prompt-probe-command.sh),
[evidence manifest](studio-prompt-evidence.json), [prompt result](studio-prompt-verified.log)
and [trace scheme result](studio-trace-scheme.log) retain the diagnosis.
The [replay connection notes](prompt-form-replay-notes.md) distinguish the
applicable legacy runner from the separate wide-path limitations.
