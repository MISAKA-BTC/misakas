# RFC-0012: checked native canonicality reads

Status: coordinated compilation and all three actual native/store controls passed.

The checked EVM number reader already propagated `is_chain_block` failures, and the EIP-1898 account/simulation selector already distinguished a returned canonicality error from `Ok(false)`. Their native implementation still panicked inside two result-returning query functions: `validate_block_exists` unwrapped the status-store result and `is_chain_ancestor_of` called the reachability service's unwrapping convenience method.

`validate_block_exists` now maps actual status read errors to existing `ConsensusError::GeneralOwned`. Genuine missing status and explicit `StatusInvalid` retain `HeaderNotFound(hash)`. `is_chain_ancestor_of` retains the same pruning session, low/high validation and actual reachability predicate, using its existing `try_is_chain_ancestor_of` variant and mapping returned errors to `GeneralOwned`. A true ancestor stays true and a valid side branch stays false. No trait, block status, chain-selection rule or policy changes.

The new `consensus/src/consensus/canonicality_fault_tests.rs` leaf has three controls. Malformed cold status bytes reach the actual `is_chain_block`, high-side ancestor query and EVM number consumer as errors; missing/invalid status remains the existing missing-header result. Explicit storage-only valid statuses with missing/corrupt reachability bytes drive the real backend to a typed error, using distinct hashes rather than a same-hash shortcut. A separate actual native accepted sibling pair verifies a distinct selected ancestor and true side-branch denial, with the EVM number reader preserving absence for the side branch and the exact target for the selected branch.

The directly inserted status/EVM rows are store fault/reader fixtures, not claim acceptance or certification. The existing skip-PoW native helper builds the sibling controls; this establishes the query mechanism, not network security, EVM execution, physical allocator coverage or a campaign. Other convenience-method unwraps and generic caller behavior remain outside this bounded change. There is no native39 settlement mapping, model examination, new activation height or invented K3 input. UNKNOWN gates remain UNKNOWN.

Package/filter: `kaspa-consensus native_canonicality_`. Leaf formatting, production parse-through-rustfmt-to-stdout and diff checks passed. Cargo execution remains parent-coordinated.

## Observed verification

The coordinated no-run build passed in **4m26s**, with 20 captured frozen source identities at `/tmp/misaka-rfc-root-preparation-canonicality-wave-source-20261025/`. The immutable Consensus binary is `/tmp/misaka-rfc-root-preparation-canonicality-consensus-20261025`, SHA-256 `920ab1ab1e5f08e694398753de5ac19db86e1fb4d8050c0acfd1cb1d2ff34fbc`. Its `native_canonicality_` tests passed **3/3 in 0.04s**. These results cover the cold-status/reachability error paths and actual ancestor/sibling query controls described above, not other tests/packages captured in the coordinated build or an EVM campaign.
