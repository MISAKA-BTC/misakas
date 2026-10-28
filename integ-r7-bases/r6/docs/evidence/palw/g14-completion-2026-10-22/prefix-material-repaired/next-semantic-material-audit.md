# Authenticated semantic material closure: read-only findings

Candidate investigation only; no repository edits, build, execution, new wire, price, retention policy, or original admission success.

## Existing reusable authenticators and inputs

* `palw_fp_constraint_job_v1.rs:323-361`: actual table rendering `PalwTokenTableOpeningV1` and EOG opening. Each rendering authenticates its ID, Some/None byte body, and Merkle opening against actual table root/vocab. EOG `.verifies` alone does not validate IDs are below vocab; reuse `palw_eog_stream_v1::{new,push,finish}` which enforces count>0/count<=vocab, sorted range-valid IDs, complete keyed leaf digest. No new proof format needed.
* `palw_native_resource_v1/wire_generated.rs:293-294`: existing native Borsh scanner implementations for both opening types; generated NativeHeap implementations also exist. Standalone private body decode can reuse these without assigning a public wire tag. A decoding result alone is not original scope authentication.
* `palw_native_resource_v1/retained.rs:410`: existing classified retained decode and lease ownership pattern. Its conditional global preflight is not an unconditional bound for a new route. For a new isolated native material owner, perform actual borrowed scan/shape validation before allocation and hold raw buffer, decoded body, and outer retained array leases separately. Borrowed retained-view pattern from parameter material avoids bulk payload cloning.
* `palw_tokenizer_artifact_inventory_v1.rs:99-254`: rooted EOF certificate binds artifact hash/actual bytes/inventory ID and native completion time. Inventory hashes sequential exact-offset bounded parts and finishes only at actual full length/hash; it does not retain raw artifact bytes. Peer-decoded certificates do not authenticate native state provenance.
* `palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1`: exact flat raw domain/length/bytes authentication, reusable on borrowed bytes. This does not establish valid JSON or correct token rendering.
* `palw_court_shared_tokenizer_builder_v1.rs:157+`: actual immutable claim semantic snapshot, actual rooted inventory, completion-before-original, raw byte length, target ID, rendering and real existing Court parser/replay are available. New key proof producer also exists. All-ID cross-context proof dedup must not be assumed from a two-context equality test.
* `palw_court_shared_constraint_builder_v1.rs`: existing current-source-bound IDs/trace row, semantics/table binding, EOG and rendering proof checks, real filer authority and existing Court replay. Court `next_rendering_id` visits prefix-generated IDs, committed token and auxiliary witness/beat. All legal witness/beat choices require covering the whole actual vocab (not a single honest trajectory).
* Prefix candidate uses actual canonical StepLeaf bodies and existing prefix primitive; it does not introduce global raw/table custody.

## Exact next implementation unit

Implement a private retained semantic table/raw material owner with real authenticated contents, not a Missing wrapper:

1. Accept actual registered/immutable semantic context (table root, actual vocab, tokenizer hash and actual rooted EOF). Do not accept caller asserted Complete/profile/paid.
2. Scan standalone existing rendering/EOG Borsh bodies before allocation, reserve actual decoded capacities with existing native leases, decode and check each opening against that context. Use bounded EOG stream to authenticate all range-valid EOG IDs.
3. Require contiguous unique rendering IDs 0..vocab-1 for an explicitly all-target table inventory. Count exact authenticated absent/present bytes, leaf/opening/path bytes with checked arithmetic. This is material availability/cardinality, not proof of rendering correctness, admission or a price.
4. Authenticate raw bytes against actual rooted EOF and immutable tokenizer hash; borrow or retain the real body rather than clone it. Build existing per-target canonical parser/constraint replay on these views. Preserve actual parser Invalid/EOF/duplicate outcomes and existing source/current-authority guards.
5. Test real generated table/eog positives; missing/duplicate/out-of-vocab/wrong root/malformed EOG; Some(empty) versus None; actual raw wrong length/hash/other inventory; native reserve/grow failure and held/drop accounting; compare old primitive verdict and bytes for target cases.

This meaningfully closes actual material authentication and all-ID table inventory without a new public codec. It still does not enumerate every court round/actor/work atom or prove all-branch fees.

## Custody/closure gap and RFC32 scope

`PalwPrefixHistoryRequestV1` (`palw_prefix_history_v1.rs:83`) is exact claim/owner/seed/head/sequence/material-until. `PalwPrefixHistoryRecordV1` (:115) contains the accepted signed stream movement plus previous head and acceptance point. `PalwHistoryRecoveryV1::remaining_request_v1` (`palw_history_custody_v1.rs:660`) follows this chain. Records can contain material that was actually submitted, but the contract does not establish pre-original publication/retention of every unsubmitted rendering ID or full raw tokenizer bytes. EOF inventory likewise proves prior accepted hash/length, not current byte retention or a newly obligated supplier.

RFC0032/ADR0199 A permits isolated authenticated parameter/prompt/step/trace/ID/material and terminal builder development; it explicitly does not adopt new public wire, price or retention duties. Existing owner/session-specific stream tags must not be repurposed as owner-independent shared ingress. A global public material namespace, signed F02 supply/retention obligation and alternative relay, exact original retention window, and real funded allocation remain required before claiming canonical all-branch original admission. If current original contracts do not cover those inputs, refuse/hold rather than invent a supplier.

Pre-admission coordinator must derive immutable semantic context from real original/native parent prepare; post-admission Claim snapshot helper cannot manufacture a Claim before admission. Existing `prepare_original_v1` tail guard remains untouched. No full closure or financially paid native path claimed here.
