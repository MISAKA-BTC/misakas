# RFC-0012 transaction authority consumers — dedicated tests verified

This bounded follow-up starts after the separately verified canonical receipt reader/main receipt RPC change (`6490925e4`). It connects remaining transaction-status and trace consumers to API read failures without adding a settlement mapping, mature-work fact, K3 input, model examination or campaign approval.

## Concrete production paths

Four node paths still discarded canonical receipt errors with `.ok().flatten()`: candidate diagnosis converted them to `accepted=false`; lifecycle status could select pending/unknown; transaction lookup could return null block context and take a mempool fallback; accepted replay could return unknown/null. Index/raw/body/header/snapshot errors in these same flows were also discarded before choosing fallbacks or running expensive interpretation.

The new production helpers are used by those actual paths:

- Candidate preparation resolves pending-first raw bytes, then checked locations and canonical receipt before head reconstruction and trace execution. True unknown raw remains None. A pending raw value retains its priority and avoids the unused raw-store read, but does not bypass index/receipt authority errors.
- Lifecycle status reads checked locations and canonical receipt before the unchanged accepted/pending/included/orphaned/skipped/unknown priority decision. A native failure cannot become an empty location index or a false acceptance flag.
- Transaction lookup reads checked raw bytes and canonical receipt before decoding or mempool fallback. True absent raw/receipt keeps the prior behavior. Semantic raw-transaction decode compatibility remains unchanged; a failed store read does not select that fallback.
- Accepted replay resolves checked canonical receipt, replay body, selected-parent execution header and, when that header exists, selected-parent execution snapshot before interpretation. True receipt absence remains None; missing body/state remains unavailable. An unreadable parent header cannot become the default genesis prestate. True absent parent header retains the existing genesis-prestate contract.

Native `get_evm_tx_locations`, `get_evm_raw_tx` and `get_evm_header_of` each had a single store unwrap. All three now map actual StoreError to the existing `ConsensusError::GeneralOwned`, allowing real cold-row faults to reach these checked node consumers. True missing locations still return the default empty index; true missing raw and execution headers still return None. No other native reader or wire/policy is changed here.

The existing session/trace permit is retained through actual blocking execution. Candidate preparation now precedes its costly head reconstruction; it does not execute a trace on failed or absent authority. These observations are local RPC diagnostics, not consensus credit authentication or a claim that several mutable RPC reads constitute a new settlement snapshot.

## Dedicated controls

Consensus filter `evm_transaction_native_` registers three parent tests using the actual production APIs and cold RocksDB bytes: malformed transaction locations versus genuine empty and exact positive location rows; malformed raw row versus genuine absence and exact positive raw bytes. A third control distinguishes actual cold malformed execution header from true absence and an exact positive header, so replay parent-header faults reach the API as Err rather than selecting a genesis default. Their rows are explicit read fixtures, not accepted transactions.

Node filter `transaction_authority_` registers six parent tests at the actual production helper boundaries: candidate raw/index/receipt faults with execution continuation count zero; genuine unknown, pending-first raw and exact acceptance/skip flags; status faults versus genuine empty/positive authority; transaction lookup fault refusal versus true-None fallback compatibility; replay receipt/body/header/snapshot errors and missing prerequisites before execution; and genuine absent receipt, true absent parent-header/default prestate, and exact positive replay inputs. The replay bodies and receipts are API-control inputs, not fabricated on-chain execution or maturity evidence. Nine dedicated parents are registered; there are no subprocess counts.

## Remaining boundaries

The main receipt RPC's optional base-fee/raw decode enrichment fallbacks, the broader reorg log pump contract, range/blockHash log query consumers and the unrelated native `get_evm_head_header` reader are unchanged. The actual exact-block parent-header API now reports its store error to the checked replay helper. Retained market setup in candidate tracing is a separate parent-owned change, not counted as this worker's result. Generic interpreter/metadata allocation accounting and the formal native39→RFC12 settlement mapping remain outside this scope.

This worker ran leaf formatting, production parse-to-stdout and diff checks. The parent subsequently ran the coordinated build, immutable binary capture and dedicated controls below. No independent build or runtime was started by this worker.


## Actual dedicated verification

The corrected coordinated five-package retry2 build passed in 276.60s with all 29 frozen source identities matching `/tmp/misaka-rfc-material-authority-wave-source-retry2-20261025/record.json`. Earlier build failures belong to the separate material lane and are not reported as authority runtime failures or passes. No authority Rust changed for this evidence update.

Native `evm_transaction_native_` passed 3/3 (0.04s) on `/tmp/misaka-rfc-material-authority-kaspa_consensus-retry2-20261025`, SHA-256 `dc2ff3662f652858897893c12edb0eb81695ad1dfc4b9684e2314aa86b89df08`. Log: `/tmp/misaka-rfc-material-authority-authority-native-20261025.log`.

Node `transaction_authority_` passed 6/6 (0.00s) on `/tmp/misaka-rfc-material-authority-kaspad_lib-retry2-20261025`, SHA-256 `3bffa914bec89028514fbe42ae7a012c0c4231fe3fc34ab6e1d33fa2e6602d4c`. Log: `/tmp/misaka-rfc-material-authority-authority-rpc-20261025.log`. `/tmp/misaka-rfc-material-authority-focused-results-20261025.json` retains the parent's focused execution results.

These nine parent controls establish the actual three native checked-reader contracts and the four node production-helper error/absence boundaries described above. Synthetic replay inputs are not an on-chain trace/campaign execution. Related whole EthRPC, prior receipt/canonicality and interpreter trace regressions also passed, as recorded below. Other lanes' dedicated tests are separate and are not added to this authority count.


## Related regressions verified

The same retry2 source generation passed full Node EthRPC 23/23 (0.01s), including the six new authority and three earlier main-receipt tests; these subsets must not be added again. Consensus receipt 4/4 (0.06s), canonicality 3/3 (0.03s) and EVM reader 3/3 (0.03s) passed. Existing EVM trace 14/14 (0.05s), simulation 1/1 (0.03s) and retained-market 4/4 (0.01s) also passed. These preserve the accepted/candidate interpreter and earlier retained simulation boundaries without claiming a new on-chain trace or settlement campaign.

Exact logs: `/tmp/misaka-rfc-material-authority-eth-rpc-regression-20261025.log`, `/tmp/misaka-rfc-material-authority-receipt-regression-20261025.log`, `/tmp/misaka-rfc-material-authority-canonicality-regression-20261025.log`, `/tmp/misaka-rfc-material-authority-reader-regression-20261025.log`, `/tmp/misaka-rfc-material-authority-trace-regression-20261025.log`, `/tmp/misaka-rfc-material-authority-sim-regression-20261025.log` and `/tmp/misaka-rfc-material-authority-retained-market-regression-20261025.log`. Controller record: `/tmp/misaka-rfc-material-authority-regression-results-20261025.json`. Runtime data above and these regression results refer to pinned binaries, not independently rebuilt artifacts.
