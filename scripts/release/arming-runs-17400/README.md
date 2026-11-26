# Historical arming runs at DAA 17,400

The 19 JSON results and corresponding main logs record suites run on source commit
`da46131a41fe7c90ec263b0f26ab9b444e4c74a2` for the 2026-10-28 G14-only candidate.
Their JSON digests are inputs to the historical 17,400 records and the later
height-only 18,400 rebind. The recorded passes apply to that source and those
commands. Reproducing the records or obtaining `LEFTOVER 0` does not establish
that the current source or an 18,400 release candidate passed any test.

Each main log is retained byte-for-byte and its SHA-256 is recorded in the
corresponding JSON. The 18,400 generator reproducibility test checks those
hashes and compares the resulting records with the existing Rust constants.
Its success checks historical reconstruction, not candidate readiness or public
activation. A current candidate requires separately recorded tests, a source
manifest, and boundary and startup checks. Write any new runs to a separate
output directory; retain these files unchanged.
