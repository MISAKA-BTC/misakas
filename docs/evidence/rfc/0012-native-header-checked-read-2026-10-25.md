# RFC-0012: checked native header read

Status: coordinated retry compilation passed; both actual native API/cold-DB tests passed.

The scoped production correction in `ConsensusApi::get_header`'s existing header-store read preserves true `KeyNotFound` as `ConsensusError::HeaderNotFound(hash)` and maps any other returned `StoreError` into existing `ConsensusError::GeneralOwned`. The change replaces only the intermediate `.optional().unwrap()`. It neither changes header decoding nor invents DAA zero. The already implemented EVM simulation metadata boundary consumes this API's returned error and refuses environment construction.

The `consensus/src/consensus/native_header_fault_tests.rs` leaf has two actual native API/cold-DB controls. It distinguishes genuine absence, malformed first-authority `CompressedHeaders` bytes, and exact positive current-header rows at genuine DAA zero and a nonzero DAA. A second test proves the legacy `Headers` fallback really decodes a legal positive row, then requires a malformed first-authority row to remain an error even with another positive fallback behind it. Prefixes use the actual registry enum. The legacy serializer is a test-only exact representation of the existing private fallback wire, not a new codec or authority.

These directly inserted header records are reader fixtures, not accepted blocks, authenticated EVM execution, historical signatures or campaign inputs. The old fallback conversion's internal panic for some otherwise decoded illegal legacy shapes is outside this correction. Other callers' own unwraps, status/reachability faults and full chain corruption recovery also remain outside scope. Public policies, activation heights and UNKNOWN campaign gates remain unchanged.

Package/filter: `kaspa-consensus native_header_`. Leaf formatting, production parse-through-rustfmt-to-stdout and source diff checks passed.

The first coordinated build diagnosed test-only E0369: `Header` has no `PartialEq`, so the new whole-header `assert_eq!` does not compile. Diagnostic log: `/tmp/misaka-rfc-transition-header-three-source-wave-build-20261025.log`. That build terminated with E0369 only. Root corrected the test to compare canonical serialized full-header bytes plus explicit hash/DAA fields; production `Header` derives and the reader correction were unchanged. No runtime result is attributed to that failed build.

## Observed verification

The integrated retry-1 no-run build passed in **4m19s**. The source record is `/tmp/misaka-rfc-transition-header-three-source-wave-source-retry1-20261025/` (16 frozen referenced Rust identities). The completed Consensus binary was pinned as `/tmp/misaka-rfc-transition-header-three-source-consensus-retry1-20261025`, SHA-256 `541516fe950253969595277a8ceb3641377a40cfa532b26463b202e3352fd66f`. Its `native_header_` filter passed **2/2 in 0.04s**, recorded in `/tmp/misaka-rfc-transition-header-three-source-consensus-tests-retry1-20261025.log`. The actual malformed-primary/positive-fallback refusal, genuine missing result and exact positive hash/DAA-zero/nonzero controls all completed. These two tests are this note's executed scope; other packages/tests captured in the coordinated source wave are not additional header coverage.

A separate Core/Node cache subset build was intentionally interrupted by root with SIGINT (exit 130), controller record `/tmp/misaka-rfc-node-core-subset-build-controller-stop-20261025.json`. It is a controller stop, not a runtime test failure or an OS-crash acceptance result.
