# RFC-0012: checked legacy header parent-level decode

Status: coordinated compilation and actual-reader regression passed.

The native `get_header` consumer now returns checked store failures, but the old
`Headers` fallback could still deserialize 256 parent levels and then panic in
its Vec-to-`CompressedParents` conversion. The conversion's existing limit is
`u8::MAX`; no accepted geometry or protocol limit is changed here.

The legacy field's deserializer now checks that existing representability limit
before the compatibility conversion. It retains the same `Vec<Vec<BlockHash>>`
field and serializer. Overlong valid-bincode input becomes a deserialization
error, which the actual native API returns as `GeneralOwned`, rather than an
empty header, DAA zero, missing header or unwinding conversion.

The new `native_header_legacy_parent_levels_reject_overflow_and_preserve_valid_wire`
test uses the real cold DB fallback and native API. It requires all 255 expanded
levels and exact stored hash/DAA to survive a valid read without rewriting the
legacy bytes. A separate fresh key contains 256 levels and must return an error
on repeated reads, without caching a truncated or empty replacement. The two
prior `native_header_` controls retain genuine absence, current positive rows,
malformed primary data and positive fallback coverage.

The coordinated Core/Consensus/private EVM Node no-run build passed in **4m26s**.
The same immutable consensus binary ran `native_header_`: **3/3 PASS, 0.04s**,
including the two existing controls and this new regression, not three new tests.
Log: `/tmp/misaka-rfc-root-preparation-canonicality-header-tests-20261025.log`.
Binary: `/tmp/misaka-rfc-root-preparation-canonicality-consensus-20261025`,
SHA-256 `920ab1ab1e5f08e694398753de5ac19db86e1fb4d8050c0acfd1cb1d2ff34fbc`.
The 20 scoped Rust identities and three binary hashes are recorded in
`/tmp/misaka-rfc-root-preparation-canonicality-wave-source-20261025/record.json`;
that source inventory does not mean every target exercised every file.

These are directly inserted reader fixtures, not accepted-chain blocks,
historical signature verification, namespace migration or a campaign. The
legacy decoder still allocates its vector before checking the count; this change
does not establish a complete decoder memory bound, validate every compressed
parent invariant, or resolve other consumers' unwraps. K3 inputs, public
activation and UNKNOWN campaign gates remain unchanged.
