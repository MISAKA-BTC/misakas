# The OpenAI surface, v1 — conformance corpus (ADR-0096 Decision 1)

Every file here is one request against `POST /v1/chat/completions` and the verdict the surface
must give it, in the shape `{"request": {…}, "expect": {"verdict": "accepted" | "refused",
"reason_contains": "…", "ignored_fields": […]}}` (`reason_contains` is required for a refusal and
absent for an acceptance; `ignored_fields`, when present, is the exact SET of names the response's
`misaka.ignored_fields` must carry — compared without regard to order, because the order is the
order an entrance met them in and two entrances need not share it). The corpus pins what ADR-0096 Decision 1 decides: content
parts flatten and a non-text part is refused by its type and position; tool turns, `tools` and
`tool_choice` are admitted (Decision 2); `response_format` is admitted inside the JSON-Schema
subset and refused by keyword outside it (Decision 3); a temperature is refused with the fence's
name while `palw_fp_decode_rules` is dormant, an identity-valued knob is accepted, and any other
value is refused by name (Decision 4); `n ≠ 1`, `logprobs`, the legacy `functions` surface, an
unknown `stream_options` key and any field the table does not name are refused by name, never
dropped; and OpenAI's no-effect fields are accepted and listed. A refusal is tested as a
SUBSTRING of the reason so the sentence can grow without the corpus moving, and every verdict is
decided with dormant chain facts — the state of every shipped network.

The gateway runs the corpus in `misaka-palw-gateway/src/surface.rs`
(`the_conformance_corpus_passes_on_this_gateway`): every file, in name order, through
`parse_and_admit` — the same function the route calls before the queue and before the worker —
and then hashes the directory (`OPENAI_SURFACE_V1_CORPUS_SHA256`: SHA-256 over each file's name,
a NUL, its bytes, a NUL, in byte-sorted name order). The Studio mirrors the directory into its own
tree, runs the same files through its `/v1`, and pins the same digest, so the two entrances cannot
drift apart without one of the two tests going red; ADR-0096 §9 records the digest both trees
carry. The digest is the seam: a file changed in one tree and not the other is a red test, not a
silent difference in what a person's app is allowed to send.

To add a case: write `NN-<name>.json` with the next free number and a name that says what the
request exercises; put the request exactly as a stock client would send it (not a minimal one —
the SDK defaults are the point); for a refusal, quote the part of the sentence that names the
field and the rule, not the whole sentence; run `cargo test -p misaka-palw-gateway --lib
conformance_corpus`, take the new digest from the failure message, update the constant in
`surface.rs`, mirror the file into the Studio and update its constant in the same change, and
append the digest to ADR-0096 §9. A case that only passes on an armed network (a temperature on
`fp_decode_rules`, a committed format on `fp_decode_constraint`) does not belong here — the
corpus is what every shipped network answers today.

## Where the two entrances differ, by decision

Five cases carry an `expect.studio` object — the verdict the STUDIO's `/v1` gives the same
request, where ADR-0096 Decision 1's table makes the two entrances differ on purpose: a sampling
knob is the engine's on the Studio and the lane's on the gateway (cases 10 and 12 — the Studio
decides after parsing, when the engine is known, and maps or refuses per `node.sampling_policy`,
Decision 4); `response_format` is shape-checked by the Studio and handed to the engine, while the
gateway applies the lane's JSON-Schema subset (case 09); `misaka.require_committed_format` is
refused by the Studio only once the engine is known to be the lane (case 20); and a tool call's
ids are forwarded to an engine that speaks them rather than listed as no-effect fields (case 06).
Each override carries a `why`. The gateway's test ignores `expect.studio`; the Studio's test
prefers it, and where it is absent the two entrances must agree — on the verdict, and on the
refusal's substring when `expect.studio.reason_contains` is given. Everything else in this
directory is byte-identical in both trees, and the digest both pin covers the overrides too.
