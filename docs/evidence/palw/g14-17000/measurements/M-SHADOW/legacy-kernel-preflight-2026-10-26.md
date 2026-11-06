# Legacy Panel / kernel Panel=1 honest comparison preflight

Status: implementation prepared; compile and execution pending. This is not a
passing measurement. It does not replace the requested full M-SHADOW matrix.

The ignored `g14_legacy_kernel_honest_shadow_measure` test registers one wide128
fixture's program and weights in both formats. It writes and maps a PALWTIR1
artifact, executes the actual legacy class draw with `TirBackendV1`, admits the
winning attempt through the normal producer path, and independently replays its
accepted material. The kernel job takes that accepted attempt's exact prompt,
generated length and greedy decoding rule. The test compares generated tokens,
then submits a native seal and reveal, waits for a post-commit Panel=1 assignment,
and exercises each route's receipt and Final transitions.

The two route encodings necessarily have separate class and claim identities.
The record preserves both identifiers, admission DAAs and actual recorded Final
DAAs. It claims equality of program, weights, prompt and generated output only;
it does not manufacture equality of differently encoded claim IDs or clocks.

Configuration is an isolated test network: early TIR, kernel and Panel=1 fences,
early anchor retention, cap 64, and the adopted 500 MSK external gain bound. Public
validation still rejects this fixture configuration. Test keys supply receipts;
per-seat honest verification costs, independent operators, Gate C and beacon
withholding are not established by this preflight. It is not a real model or a
public-node campaign. Existing fixture artifact attestation and heartbeat helpers
remain explicit test assumptions.

Build under the shared `buildslot.sh` gate with `build_measures.py --target native
--record-name native-legacy-shadow --out <artifact-directory>`. Run
`run_legacy_shadow_measure.py --build-record <artifact-directory>/native-legacy-shadow-build-record.json
--out <fresh-output-directory>` under the same gate. The runner checks the pinned
binary SHA and unchanged build source; retains command, machine metadata, load,
wall/CPU/RSS and log digests; and limits its own process group to 30 minutes.
Success is labelled `HONEST_PREFLIGHT_ONLY` (or its under-load variant).

Remaining after an honest pass: paired forged-output, fake-state, duplicate-work
and withheld-randomness cases, the verification/court cost comparison, repetitions
and quiet-load controls, and the separate Gate C acceptance scope. A failed
admission or assignment must be preserved and diagnosed rather than bypassed or
turned into a passing comparison.
