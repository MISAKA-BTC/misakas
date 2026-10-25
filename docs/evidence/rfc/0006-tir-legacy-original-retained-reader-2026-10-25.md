# RFC-0006 legacy TIR original commitment reader

Status: coordinated retry1 build and all four dedicated controls PASS; remaining limits below apply.

The two legacy Source fallbacks in `palw_tir_public_state_v1.rs` used `staged_original_job_v1`, which decoded stored original commitment bytes into a bare value even for a native retained Source caller. This allocated the original job's public key before any exact typed reservation and left it outside the Source construction ticket. Modern full-frame readers already borrow their original and do not need this decode.

The native specialization now calls `staged_original_job_native_v4`: it scans the actual original commitment's Borsh shape without allocation, reserves its complete native request before decode, and returns an immutable `NativeRetainedV1`. Both fallback locals retain that owner through descriptor checks, Source construction and later work validation. The raw specialization preserves its old reader/error contract. Missing bytes remain None, malformed/trailing bytes are Decode, and local scanner capability/arithmetic/CPU or capacity refusal remains Resource. The first review found an overbroad Decode mapping; it was corrected using `io::ErrorKind`, without inspecting error strings.

Both fallback claim identities now stream the exact canonical Borsh bytes under the existing domain and u64 little-endian encoded-length prefix. This removes their temporary identity Vec without changing a claim ID. The global historical `fp_claim_id_v3` function and wire formats are unchanged.

Four prepared test parents (`legacy_original_owner_`) cover canonical identity parity including a 70k standalone public-key geometry, the actual native state reader at Q−1/Q with held-owner pressure and retry/drop release, true absence versus malformed/trailing bytes with rooted state unchanged, and scanner local/corrupt error classification. The storage fixture uses original bytes from an existing genuinely computed tiny TIR frame. It assumes accepted storage input and does not mine a new claim, create/fund an old ClaimBundle, or exercise a legacy Preparation/Acquisition end-to-end. Synthetic scanner errors test classification only, not hardware/CPU exhaustion. The geometry input is not an approved model or job.

This closes only the original decode/identity transient in those two native fallback consumers. Their legacy work/acquisition-plan construction still needs separate native ownership. Other raw staged-original readers, diagnostics, all-model physical costs and actual campaigns remain open. Public activation None, selected-source/V6 bootstrap refusal, dormant economics, claim-selected verifier approval and frozen model tiers are unchanged. K3 remains uncreated, native39 settlement mapping unformulated, and all 15 campaign gates UNKNOWN.

## Final verification

Four dedicated parents PASS; all related selections PASS. [Build identity, immutable executable SHA, exact results and earlier fixture failure](0006-0012-deadline-legacy-reorg-verification-2026-10-25.md). These four remain cold-reader/identity/classification controls; no legacy Source end-to-end is claimed.
