# RFC-0006 router binding offline SDK continuation — 2026-10-24

## Scope

The [remaining-work ledger](../../design/palw/integration/rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md)
identified the missing SDK CLI submit command for the immutable router binding. The SDK now exposes
`signed_router_binding_v2`, and `palw-class router-binding-v2 sign` writes the existing tag-244
`MemoryRoutingV2::RouterBindingDeclared` carrier. The existing `misaka palw submit-object` wallet path
can carry that output. No carrier is broadcast by this offline command.

The operator supplies `borsh(PalwRouterBindingDeclarationV2)`, including the full registered graph and
an explicit complete ascending `(table, reader, router)` list. The implementation calls the existing
consensus shape/binding checker and signs its existing message under its existing ML-DSA-87 context.
It does not pick a router in an ambiguous graph. Network domain is derived from the selected network
and genesis, including an optional private drill salt.

```text
palw-class router-binding-v2 sign --network testnet-12 \
  --declaration /absolute/path/declaration.borsh \
  --key-file /absolute/path/registrant-seed.hex \
  --out /absolute/path/fresh-object.borsh
```

The input has a local 16 MiB tooling limit; this does not change any consensus carriage limit.
Invalid declarations are refused before loading the signing key. Output uses `create_new` and
`sync_all`; an existing file is preserved. The existing seed loader retains its file permission checks.

## Verification

The actual new SDK source modules were compiled as a standalone Rust test harness against the
matching Cargo-built consensus-core, txscript, validator-core and Borsh libraries in
`/tmp/misaka-rfc06-12-build-20261024`. All **3 tests passed** in 0.07 seconds:

- Existing native tag-244 Borsh round trip and chain signature contract; a different network,
  signature context or artifact root fails signature verification.
- Missing readers, a foreign graph identity and a router that does not precede its reader are refused.
- The real CLI writes a decodable native carrier, preserves existing output and refuses an invalid
  declaration before trying to read a missing signing key.

The actual SDK library, `palw-class` binary and all test targets also passed
`cargo check -p misaka-palw-sdk --lib --bin palw-class --tests` with debug information and
incremental compilation disabled, using the dedicated target above (2m39s including a build-lock wait).
This is metadata/type verification, separate from the 3 runtime tests.

The harness command and library digests are in `/tmp/misaka-rfc01-12-router-manifest.json`; runtime
output is in `/tmp/misaka-rfc01-12-router-test.log`. These local paths are reproducibility aids, not
public chain acceptance evidence. The fixture uses an existing Qwen graph and no model weights.

## Remaining boundaries

This is offline construction and focused verification. Native acceptance still requires the fence,
registered class/artifact identity, Active original registrant signature, immutable first binding,
aux store and admissible re-derived court resources. This change does not arm any preset, change
selected-source/V6 bootstrap refusal, or enable the unbindable Kimi card. No private campaign,
production node submission, whole-model cost measurement or all-router-consumer acceptance is claimed.
