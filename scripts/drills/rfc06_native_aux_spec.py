#!/usr/bin/env python3
"""Source of truth for the native resource scanner's kernel-route aux row codecs, tables 42–106 and 112.

Both generators import this module: `gen_rfc06_native_inventory.py` roots every value type named here (so the structural
`NativeWireScanV1` impls exist) and `gen_rfc06_native_wire.py` emits one `scan_native_aux_row_v1` arm per namespace. Running it
directly writes the inventory document (`docs/design/palw/native-resource-aux-inventory-2026-10-15.md`) and cross-checks that every
`write_kernel_row(..., Some(..))` site into tables 42–112 is claimed by exactly one namespace below (it exits non-zero otherwise).

A namespace is (table, key condition, value codec):
  * `cond` is a Rust boolean over `k: &[u8]` (the full aux key) and `n: u64` (the row's byte length); it is evaluated in order,
    so every condition tests `k.len()` before it indexes `k`.
  * `value` is one of
      - a Rust type with a structural `NativeWireScanV1` impl (Borsh-exact; `[u8;64]` for `Digest`, `crate::Hash64`);
      - `raw:<cap expr>`: a row stored as raw bytes (never Borsh-framed, never decoded at restore/projection); the scanner checks the
        declared bound, accounts the bytes once more as a transient copy (the outer loop already holds stored+copy), fails above it;
      - `mirror:<generator key>`: a fold-private struct the scanner cannot name; the wire generator expands its source field list
        into a Borsh-identical nested tuple (regenerated, so a field change cannot silently diverge);
      - `any:<v1>|<v2>`: one key namespace two families write with different layouts (each family only writes its own job ids,
        but the key alone cannot tell them apart). The row is buffered (bounded), every alternative is tried and the LARGEST
        accepted shape is accounted (conservative); no alternative accepting exactly fails closed.
  * `src` lists (file regex, key-expression regex) pairs matched against the extracted write sites.

Every table below ends in an explicit fail-closed arm ("unallocated ... namespace"): a key shape no fold writes is malformed.
Tables 42 and 101 are allocated but never written: any row there fails closed.
"""
import re, subprocess, sys
from pathlib import Path

D = '[u8;64]'
H = 'crate::Hash64'
B = 'crate::palw_state_v2::PalwBondKeyV2'
# PalwBondKeyV2 = TransactionOutpoint { transaction_id: Hash64, index: u32 }: 68 Borsh bytes.
BK = 68
VD = f'Vec<{D}>'
Q = 'misaka_palw_kernel::flat_quota11'
X11 = 'crate::palw_flat_execution_v11'
S14 = 'crate::palw_flat_stage_v14'
S28 = 'crate::palw_flat_stage_v28'
P29 = 'crate::palw_native_paged_proof_v29'
CAP = 'crate::palw_flat_execution_capacity_v22'
QV30 = 'crate::palw_native_queue_v30'
T29 = 'crate::palw_native_text_stop_v29'
W37 = 'crate::palw_window_class_v37'
OBJ = 'crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1'
FRAG = f'{Q}::PROTECTED_FRAGMENT_BYTES_V11'


def L(n):
    return f'k.len()=={n}'


def P(n, b0):
    return f'k.len()=={n} && k[0]=={b0}'


def P2(n, b0, b1):
    return f'k.len()=={n} && k[0]=={b0} && k[1]=={b1}'


def PL(n, b0, last):
    return f'k.len()=={n} && k[0]=={b0} && k[{n - 1}]=={last}'


FENCES = {
    'provider': 'palw_provider_court_v1',
    'opv': 'palw_panel_free_v1 (kernel-route OPV)',
    'seal4': 'palw_model_refutation_seal_v1 (with palw_panel_free_v1)',
    'v11': 'palw_funded_execution_v11 (with palw_panel_free_v1)',
    'v27': 'palw_funded_execution_v11 + palw_native_final_v27',
    'v37': 'palw_window_class_v37 + palw_native_final_v27',
    's14': 'palw_stage_execution_v14 (with palw_panel_free_v1)',
    's28': 'palw_stage_execution_v28 + palw_native_final_v27',
    'n39': 'palw_native_execution_v39 + palw_native_final_v27 (with palw_panel_free_v1)',
    'm39': 'palw_native_mining_v39 + palw_native_execution_v39 (with palw_native_final_v27, palw_panel_free_v1)',
}

NS = []


def ns(table, cond, value, src, fence, note=''):
    NS.append(dict(table=table, cond=cond, value=value, src=src, fence=FENCES[fence], note=note))


ANY = r'.*'
V11F = r'palw_flat_execution_(fold_v11|court_fold_v11|da_fold_v11|discovery_fold_v13|gc_fold_v24|source_fold_v16|capacity_fold_v23|final_fold_v27)\.rs'
T29F = r'palw_native_text_fold_v29\.rs'
TDA = r'palw_native_text_da_fold_v29\.rs'
S14F = r'palw_flat_stage_(fold|court_fold|source_fold)_v14\.rs'
S28F = r'palw_flat_stage_fold_v28\.rs'
S28S = r'palw_flat_stage_source_fold_v28\.rs'

# ---- 43–45 provider court -----------------------------------------------------------------------------------------------------
ns(43, 'true', 'crate::palw_provider_court_v1::ProviderLeaseRowV1', [(ANY, r'palw_provider_lease_key_v1')], 'provider', 'key (ProviderSubjectV1, PalwBondKeyV2)')
ns(44, 'true', 'crate::palw_provider_court_v1::ProviderChallengeRowV1', [(ANY, r'palw_provider_challenge_key_v1')], 'provider', 'key (subject, provider, PublicUnitV1)')
ns(45, 'true', 'crate::palw_provider_court_v1::ProviderSubjectRowV1', [(ANY, r'subject\.key\(\)')], 'provider', 'key ProviderSubjectV1')
# ---- 46–50 scoped model statements ----------------------------------------------------------------------------------------------
ns(46, L(64), 'crate::palw_model_artifact_v2::ModelArtifactBindingRowV2', [(ANY, r'borsh::to_vec\(&id\)')], 'opv')
ns(47, L(BK), VD, [(ANY, r'borsh::to_vec\(signer\)')], 'opv', 'per-binder binding ids')
ns(48, L(64), H, [(ANY, r'Hash64::from_bytes\(\*class\)|borsh::to_vec\(&class\)')], 'opv', 'class → authoritative binding id')
ns(49, L(64), 'crate::palw_model_conformance_v2::ModelConformanceRowV2', [(ANY, r'borsh::to_vec\(&class\)')], 'opv')
ns(50, L(0), '(u64,u32)', [(ANY, r'Vec::new\(\)')], 'opv', 'per-block run counter')
# ---- 51–56 model vector v2 ------------------------------------------------------------------------------------------------------
ns(51, L(64), 'crate::palw_model_vector_v2::ModelVectorRowV2', [(ANY, r'borsh::to_vec\(&(id|d\.vector)\)')], 'opv')
ns(52, L(BK), VD, [(ANY, r'borsh::to_vec\(signer\)')], 'opv')
ns(53, L(64), 'crate::palw_model_vector_da_v2::ModelVectorDemandV2', [(ANY, r'borsh::to_vec\(&?(demand|id)\)')], 'opv')
ns(54, L(BK + 1), H, [(ANY, r'\(\*signer, slot\)')], 'opv', 'key (bond, slot) → demand id')
ns(55, 'true', 'crate::palw_model_vector_da_v2::ModelVectorServedV2', [(ANY, r'd\.unit\.key')], 'opv', 'key (vector, ModelVectorUnitV2)')
ns(56, L(64), 'u32', [(ANY, r'borsh::to_vec\(&(id|d\.vector)\)')], 'opv', 'open demand count')
# ---- 57–63 flat vector v3 (rows shared by the v6/v7/v9 posts) ---------------------------------------------------------------------
ns(57, L(64), 'crate::palw_flat_model_vector_v3::FlatModelVectorRowV3', [(ANY, r'borsh::to_vec\(&id\)')], 'opv')
ns(58, L(BK), VD, [(ANY, r'borsh::to_vec\(signer\)')], 'opv')
ns(59, L(64), 'crate::palw_flat_model_vector_da_v3::FlatVectorDemandV3', [(ANY, r'borsh::to_vec\(&?(demand|id)\)')], 'opv')
ns(60, L(BK + 1), H, [(ANY, r'\(\*signer, slot\)')], 'opv')
ns(61, 'true', 'crate::palw_flat_model_vector_da_v3::FlatVectorServedRowV3', [(ANY, r'd\.unit\.key')], 'opv', 'answer Vec ≤ FLAT_VECTOR_ANSWER_MAX_BYTES_V3')
ns(62, L(64), 'u32', [(ANY, r'borsh::to_vec\(&(id|d\.vector)\)')], 'opv')
ns(63, L(64), 'bool', [(ANY, r'borsh::to_vec\(&d\.vector\)')], 'opv')
# ---- 64 refutation seals --------------------------------------------------------------------------------------------------------
ns(64, L(BK + 1), 'crate::palw_model_refutation_seal_v4::ModelRefutationSealRowV4', [(ANY, r'model_refutation_seal_key_v4')], 'seal4', 'key (bond, slot)')
# ---- 65–86 flat v6/v7/v9 seals and DA -------------------------------------------------------------------------------------------
for (prot, seal, actor, count, st, ver) in [(65, 66, 67, 68, 'crate::palw_flat_model_seal_v6::FlatNativeSealRowV6', 6),
                                            (69, 70, 71, 72, 'crate::palw_flat_model_vector_v7::FlatNativeSealRowV7', 7),
                                            (78, 79, 80, 81, 'crate::palw_flat_model_vector_v9::FlatNativeSealRowV9', 9)]:
    ns(prot, L(64), 'bool', [(ANY, r'borsh::to_vec\(&id\)')], 'opv', f'v{ver} protected-vector flag')
    ns(seal, L(128), st, [(ANY, r'borsh::to_vec\(&\(id, actor\)\)')], 'opv', 'key (vector, actor)')
    ns(actor, L(BK), VD, [(ANY, r'borsh::to_vec\((signer|&row\.bond)\)')], 'opv')
    ns(count, L(0), 'u32', [(ANY, r'Vec::new\(\)')], 'opv')
for (dem, slot, served, count, unav, ver) in [(73, 74, 75, 76, 77, 7), (82, 83, 84, 85, 86, 9)]:
    m = f'crate::palw_flat_model_vector_da_v{ver}'
    ns(dem, L(64), f'{m}::FlatVectorDemandV{ver}', [(ANY, r'borsh::to_vec\(&?(demand|id)\)')], 'opv')
    ns(slot, L(BK + 1), H, [(ANY, r'\(\*signer, slot\)')], 'opv')
    ns(served, 'true', f'{m}::FlatVectorServedRowV{ver}', [(ANY, r'd\.unit\.key')], 'opv', f'answer Vec ≤ FLAT_VECTOR_ANSWER_MAX_BYTES_V{ver}')
    ns(count, L(64), 'u32', [(ANY, r'borsh::to_vec\(&(id|d\.vector)\)')], 'opv')
    ns(unav, L(64), 'bool', [(ANY, r'borsh::to_vec\(&d\.vector\)')], 'opv')

# ---- 87 execution classes ----------------------------------------------------------------------------------------------------
ns(87, L(64), f'{X11}::NativeExecutionClassRowV11', [(V11F, r'key\(&c\.id\(\)\)')], 'v11')
ns(87, P2(66, 29, 0), 'mirror:crate::palw_native_text_fold_v29::ClassHeader', [(T29F, r'^k\(0, &id\)$')], 'v27', 'Text29 class header (fold-private)')
ns(87, P2(66, 29, 1), 'raw:1<<20', [(T29F, r'^k\(1, &id\)$')], 'v27', 'Text29 register body, decoded only by its fold (≤ 1 MiB)')
ns(87, P2(66, 37, 0), f'({W37}::NativeWindowClassV37,{B},u64)', [(T29F, r'key\(&\(37u8, 0u8, id\)\)')], 'v37')
ns(87, P2(66, 37, 1), f'raw:{W37}::WINDOW_CLASS_MAX_BODY_BYTES_V37', [(T29F, r'key\(&\(37u8, 1u8, id\)\)')], 'v37', 'window37 register body')
ns(87, P2(BK + 2, 37, 2), VD, [(T29F, r'catalog_key')], 'v37', 'owner catalogue key (37, 2, bond)')
# ---- 88 execution jobs -------------------------------------------------------------------------------------------------------
ns(88, L(64), f'{X11}::NativeExecutionJobRowV11', [(V11F, r'key\(&(job_id|id|row\.header\.job_id)\)')], 'v11', 'book Vec ≤ EXECUTION_STATE_MAX_BYTES_V11, prompt Vec<u32>')
ns(88, P2(66, 29, 0), 'mirror:crate::palw_native_text_fold_v29::JobHeader', [(T29F, r'^k\(0, &h\.job\)$')], 'v27', 'Text29 job header (fold-private)')
ns(88, P2(66, 29, 1), f'{T29}::NativeTextBookV29', [(T29F, r'^k\(1, &h\.job\)$')], 'v27', 'Text29 book (Borsh, ≤ TEXT_STOP_BOOK_BYTES_V29)')
ns(88, P2(66, 37, 3), f'{W37}::NativeWindowJobBindingV37', [(T29F, r'key\(&\(37u8, 3u8, job\)\)')], 'v37')
# ---- 89 requester nonce counters ---------------------------------------------------------------------------------------------
ns(89, L(64), f'{X11}::NativeRequesterNonceRowV11', [(ANY, r'(key|bytes)\(&(requester_operator|a\.requester_operator|w\.requester_operator)\)|^b\(&c\.wire\.requester_operator\)$')], 'v11', 'requester operator → counter (text11/stage14/stage28/text29)')
ns(89, P(65, 13), f'{X11}::NativeRequesterNonceRowV11', [(V11F, r'counter_key')], 'v11', 'v13 discovery counter key (13, operator)')
ns(89, P(65, 29), 'u64', [(T29F, r'nonce_key')], 'v27', 'Text29 own-ticket counter key (29, operator)')
# ---- 90 vector → job ---------------------------------------------------------------------------------------------------------
ns(90, L(64), D, [(V11F, r'key\(&trace\.vector\)')], 'v11')
# ---- 91 protected posts ------------------------------------------------------------------------------------------------------
ns(91, L(64), f'raw:{OBJ}', [(V11F, r'key\(&id\)')], 'v11', 'funded post carrier, ≤ stored_post_cap ≤ the route object bound')
ns(91, P2(66, 29, 0), f'raw:{OBJ}', [(T29F, r'^k\(0, &id\)$')], 'v27', 'Text29 compact post')
# ---- 92 quota / capacity / paged / queue / initial31 ---------------------------------------------------------------------------
ns(92, L(1) + ' && k[0]==0', f'({D},{Q}::QuotaGlobalRowV11)', [(ANY, r'^vec!\[0\]$')], 'v11', 'shared quota global (fixed framing)')
ns(92, P(65, 1), f'({D},{Q}::QuotaActorRowV11)', [(ANY, r'quota_key\(1,|bytes\(&\(1u8, owner\.actor\)\)|^actor$|^b\(&\(1u8, a\.actor\)\)$'), (S28F, r'^k$', r'a\.root\(\),a')], 'v11', 'quota actor (1 ‖ actor)')
ns(92, P(65, 2), f'({D},{Q}::QuotaSubjectSummaryV11)', [(ANY, r'quota_key\(2,|bytes\(&\(2u8,h\.id\)\)|^b\(&\(2u8, (h\.id|s\.binding\.job_id)\)\)$')], 'v11', 'quota subject (2 ‖ job)')
ns(92, P(65, 3), f'({D},{Q}::ProtectedSealSummaryV11)', [(ANY, r'quota_key\(3,')], 'v11', 'protected seal summary (3 ‖ seal)')
ns(92, P(65, 4), f'({D},{Q}::discovery13::DiscoveryRowV13)', [(ANY, r'quota_key\(4,')], 'v11', 'discovery row (4 ‖ id)')
ns(92, P(65, 16), f'({D},{Q}::source16::SourceObligationV16)', [(ANY, r'key\(&\(16u8, job\)\)|bytes\(&\(16u8,id\)\)')], 'v11', 'source obligation (16, job)')
ns(92, PL(66, 16, 1), f'{Q}::source16::SourcePlanRecordV16', [(ANY, r'\(16u8, ?(job_id|id), ?1u8\)')], 'v11', 'source plan record (16, job, 1)')
ns(92, P(66, 17), f'({D},{Q}::trace17::TraceSupplementV17)', [(V11F, r'trace_row_key')], 'v11', 'trace supplement (17, role, entitlement)')
ns(92, P2(2, 22, 0), f'({D},{CAP}::NativeCarriageGlobalV22)', [(ANY, r'global_key|\(22u8, ?0u8\)')], 'v11', 'physical carriage global (22, 0)')
ns(92, P(65, 22), f'({D},{CAP}::NativeCarriageLeaseV22)', [(ANY, r'lease_key\(job\)|key\(&\(22u8, id\)\)|^b\(&\(22u8, id\)\)$')], 'v11', 'carriage lease (22, job)')
ns(92, PL(66, 22, 1), f'{CAP}::NativeCarriagePlanV22', [(ANY, r'plan_key\(job\)')], 'v11', 'carriage plan (22, job, 1)')
ns(92, PL(66, 22, 2), f'({D},{CAP}::NativePhysicalSlotsV22)', [(ANY, r'\(22u8, ?(job|id|h\.id), ?2u8\)|slotkey')], 'v11', 'physical slotbook (22, job, 2)')
ns(92, P2(2, 30, 0), f'({D},{QV30}::NativeQueueGlobalV30)', [(ANY, r'\(30u8, ?0u8\)|queue_key')], 'v27', 'queue30 global (30, 0)')
ns(92, PL(66, 30, 1), f'({D},{QV30}::NativeQueueLeaseV30)', [(ANY, r'\(30u8, ?(job|id), ?1u8\)')], 'v27', 'queue30 lease (30, job, 1)')
ns(92, PL(66, 30, 2), f'{QV30}::NativeQueuePlanRecordV30', [(S28F, r'\(30u8,id,2u8\)')], 's28', 'queue30 plan record (30, job, 2)')
ns(92, L(129) + ' && k[0]==26', f'({D},{Q}::stage14::source26::StageSourceCallerRowV26)', [(ANY, r'\(26u8, ?row\.job, ?row\.owner\)')], 'v27', 'caller26 row (26, job, owner digest)')
ns(92, PL(66, 26, 0), f'{Q}::stage14::source26::StageSourcePlanRecordV26', [(ANY, r'\(26u8, ?id, ?0u8\)')], 'v27', 'caller26 plan record (26, job, 0)')
ns(92, PL(66, 28, 29), f'({D},crate::palw_native_reply_parts_v29::NativeReplyProgressV29)', [(S28S, r'progress_key')], 's28', 'stage28 reply progress (28, pending, 29)')
ns(92, PL(66, 29, 0), f'({D},{P29}::NativePagedJobStatusV29)', [(ANY, r'status_key|\(29u8, ?(id|h\.job|p\.status\(\)\.job), ?0u8\)'), (S28F, r'^k$', r'status\.root\(\),status')], 'v27', 'paged29 job status (29, job, 0)')
ns(92, PL(66, 29, 1), f'{P29}::NativePagedProofPlanRecordV29', [(ANY, r'\(29u8, ?id, ?1u8\)')], 'v27', 'paged29 plan record (29, job, 1)')
ns(92, PL(66, 29, 3), f'{Q}::QuotaPolicyV11', [(ANY, r'\(29u8, ?id, ?3u8\)')], 'v27', 'cached quota policy (29, job, 3)')
ns(92, PL(66, 29, 22), f'({D},{P29}::NativePagedLeaseJournalV29)', [(ANY, r'\(29u8, ?id, ?22u8\)')], 'v27', 'paged29 lease journal (29, job, 22)')
ns(92, PL(66, 29, 23), f'any:({D},crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29)|crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29',
   [(ANY, r'\(29u8, ?id, ?23u8\)')], 'v27', 'AMBIGUOUS: Text29 stores (root, record), Stage28 stores the bare record under the same key shape')
ns(92, L(129) + ' && k[0]==29', f'({D},{P29}::NativePagedSealV29)', [(ANY, r'owned_key|\(29u8, ?(h\.job, paid\.id\(\)|s\.job,s\.id\(\))\)')], 'v27', 'paged29 own seal (29, job, seal)')
ns(92, P2(2, 31, 0), 'mirror:crate::palw_native_initial_fold_v31::InitialGlobalV31', [(r'palw_native_initial_fold_v31', r'key\(&0u8\)')], 'v27', 'initial31 global (31, 0) (fold-private)')
ns(92, PL(66, 31, 1), f'({D},crate::palw_native_initial_v31::NativeInitialHeaderV31)', [(r'palw_native_initial_fold_v31', r'key\(&\(h\.id, 1u8\)\)')], 'v27', 'initial31 header (31, (id, 1))')
ns(92, L(68) + ' && k[0]==31 && k[65]==2', 'raw:crate::palw_native_initial_v31::INITIAL_PART_BYTES_V31', [(r'palw_native_initial_fold_v31', r'key\(&\(id, 2u8, part\.index\)\)')], 'v27', 'initial31 part (31, (id, 2, u16))')
# Owning39 (RFC-0002 §II.14): `(39, 0, job)` (the producer source39 obligation) and `(39, job, j)` j = 0..4 (the job's original
# contract, source plan, paged plan, cached quota policy, carriage record) share the 66-byte shape and k[0]; neither k[1] nor k[65]
# separates them for every job id. One buffered `any:` arm, every layout tried, the largest accepted accounted.
ns(92, L(66) + ' && k[0]==39',
   f'any:({D},{Q}::owning39::source::SourceObligationV39)|crate::palw_native_job_v39::NativeMaximumContractV39|{Q}::owning39::SourcePlanRecordV39'
   f'|{Q}::owning39::PagedProofPlanRecordV39|{Q}::QuotaPolicyV11|({D},crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29)',
   [(r'palw_native_job_fold_v39', r'^b\(&\(39u8, (0u8, id|id, [0-4]u8)\)\)$')], 'n39',
   'AMBIGUOUS by key: owning39 source obligation (39, 0, job) vs the job\'s five original records (39, job, 0..4)')
# ---- 93 policies / pursuit / consents ----------------------------------------------------------------------------------------
ns(93, L(0), f'{X11}::NativeExecutionPoliciesV11', [(ANY, r'^(Vec::new\(\))$')], 'v11', 'immutable shared native policies (text11 / stage14 / stage28)')
ns(93, P(65, 19), 'crate::palw_flat_execution_pursuit_v19::NativePublicPursuitV19', [(V11F, r'\(19u8, job\)')], 'v11')
ns(93, P(65, 23), f'{CAP}::NativeExecutionConsentV23', [(V11F, r'\(23u8, job\)')], 'v11')
ns(93, P(65, 27), f'({D},mirror:crate::palw_flat_execution_final_fold_v27::NativeFinalConsentV27)', [(V11F, r'\(27u8, job\)')], 'v27', 'Final27 consent (fold-private)')
ns(93, P2(66, 29, 0), 'mirror:crate::palw_native_text_fold_v29::Prepared', [(T29F, r'^k\(0, &id\)$')], 'v27', 'Text29 preparation (fold-private)')
# ---- 94 source facts ---------------------------------------------------------------------------------------------------------
ns(94, L(64), f'{X11}::NativeVerifiedSourceFactV11', [(V11F, r'key\(&header\.binding_id\)')], 'v11', 'proof bytes Vec')
ns(94, P(65, 16), f'{X11}::NativeVerifiedHeaderFactV16', [(V11F, r'^k$')], 'v11', 'header fact (16, binding)')
ns(94, P2(66, 29, 0), f'({D},{D},u64)', [(T29F, r'source_relation')], 'v27', 'Text29 refuted source relation')
# ---- 95–97 owner catalogues and counts ---------------------------------------------------------------------------------------
ns(95, L(BK), VD, [(V11F, r'key\(signer\)')], 'v11')
ns(95, P2(BK + 2, 29, 0), VD, [(T29F, r'k\(0, signer\)')], 'v27')
ns(96, L(BK), VD, [(V11F, r'key\((signer|&h\.requester)\)')], 'v11')
ns(96, P(BK + 1, 1), VD, [(V11F, r'\(1u8, (producer|h\.producer)\)')], 'v11')
ns(96, P2(BK + 2, 29, 0), VD, [(T29F, r'k\(0, (signer|&h\.requester)\)')], 'v27')
ns(96, P2(BK + 2, 29, 1), VD, [(T29F, r'k\(1, &(producer_key|h\.producer)\)')], 'v27')
ns(97, L(0), 'u32', [(V11F, r'^(vec!\[\]|Vec::new\(\))$')], 'v11', 'live funded job count')
# ---- 98 preparations / consents ----------------------------------------------------------------------------------------------
ns(98, L(64), f'{X11}::NativeExecutionPreparationV11', [(V11F, r'key\(&prep\)')], 'v11')
ns(98, L(BK), VD, [(V11F, r'key\(signer\)')], 'v11')
ns(98, P(BK + 1, 13), VD, [(V11F, r'\(13u8, (\*signer|producer)\)')], 'v11', 'producer consent catalogue (13, bond)')
ns(98, P(65, 13), f'({D},{X11}::NativeExecutionConsentV13)', [(V11F, r'\(13u8, id\)')], 'v11')
ns(98, P(65, 18), f'({D},{X11}::NativeExecutionConsentV18)', [(V11F, r'\(18u8, id\)')], 'v11')
ns(98, P(65, 23), f'({D},{CAP}::NativeExecutionConsentV23)', [(V11F, r'\(23u8, id\)')], 'v11')
ns(98, P(65, 27), f'({D},mirror:crate::palw_flat_execution_final_fold_v27::NativeFinalConsentV27)', [(V11F, r'\(27u8, id\)')], 'v27')
ns(98, P2(BK + 2, 29, 0), VD, [(T29F, r'k\(0, signer\)')], 'v27')
ns(98, P2(66, 29, 1), 'mirror:crate::palw_native_text_fold_v29::Prepared', [(T29F, r'k\(1, &id\)')], 'v27')
ns(98, P2(BK + 2, 29, 2), VD, [(T29F, r'k\(2, (signer|&producer_key)\)')], 'v27')
ns(98, P2(66, 29, 3), f'{T29}::NativeTextConsentV29', [(T29F, r'k\(3, &a\.root\(\)\)')], 'v27')
# ---- 99 seals / proof groups / parts ------------------------------------------------------------------------------------------
ns(99, L(64), f'{X11}::NativeExecutionSealV11', [(V11F, r'key\(&(protected_key\.root\(\)|seal_id|seal_key)\)')], 'v11')
ns(99, P(65, 12), f'{X11}::NativeExecutionProofGroupV11', [(V11F, r'group_key')], 'v11', 'own proof group (12, seal)')
ns(99, P(65, 13), f'{X11}::NativeExecutionDiscoveryV13', [(V11F, r'native_key')], 'v11', 'discovery ticket (13, ticket)')
ns(99, P(66, 15), f'raw:{FRAG}', [(V11F, r'^part_key$')], 'v11', 'protected fragment (15, seal, u8 index)')
ns(99, L(68) + ' && k[0]==28', 'raw:32<<10', [(S28S, r'reply_part_key')], 's28', 'stage28 reply part (28, pending, u8, u16)')
ns(99, L(131) + ' && k[0]==29', f'raw:{T29}::TEXT_STOP_PART_BYTES_V29', [(r'palw_native_paged_fold_v29', r'part_key\(body')], 'v27', 'paged29 part (29, job, seal, u16)')
# ---- 100 seal-actor catalogues / 101 never written ----------------------------------------------------------------------------
ns(100, P2(66, 29, 0), VD, [(T29F, r'k\(0, &(id|job)\)')], 'v27', 'Text29 discovery lifetime catalogue')
# ---- 102 demands --------------------------------------------------------------------------------------------------------------
ns(102, L(64), f'{X11}::NativeExecutionDemandV11', [(V11F, r'key\(&(demand|d\.id\(\))\)')], 'v11')
ns(102, P(65, 16), f'{X11}::NativeSourceDemandV16', [(r'palw_flat_(execution_source_fold_v16|stage_source_fold_v14)', r'pending_key')], 'v11', 'source16 pending (16, id) (also stage14)')
ns(102, P(65, 22), 'u8', [(V11F, r'\(22u8, pending\)')], 'v11', 'trace attempt counter byte')
ns(102, P(65, 28), f'{S28}::NativeStagePendingV28', [(S28S, r'pending_key')], 's28')
ns(102, P2(66, 29, 0), 'mirror:crate::palw_native_text_da_fold_v29::Demand', [(TDA, r'k\(0, &d\.id\)')], 'v27', 'Text29 public demand (fold-private)')
ns(102, P2(66, 29, 1), D, [(TDA, r'k\(1, &unit\)')], 'v27')
ns(102, P2(70, 29, 2), f'({D},{D})', [(TDA, r'k\(2, &\(job, d\.ordinal\)\)')], 'v27')
ns(102, P2(66, 29, 3), f'({D},crate::palw_native_reply_parts_v29::NativeReplyProgressV29)', [(TDA, r'k\(3, &(d\.id|p\.pending)\)')], 'v27')
# ---- 103 demand slots ---------------------------------------------------------------------------------------------------------
ns(103, L(BK + 1), D, [(V11F, r'slot_key\(signer, request\.slot\)')], 'v11', 'key (bond, slot)')
ns(103, P(BK + 2, 16), D, [(r'palw_flat_(execution_source_fold_v16|stage_source_fold_v14)', r'slot_key')], 'v11', 'key (16, bond, slot)')
ns(103, P(BK + 2, 28), D, [(S28S, r'slot_key')], 's28', 'key (28, bond, slot)')
ns(103, P2(BK + 3, 29, 0), D, [(TDA, r'k\(0, &\(\*signer')], 'v27', 'key (29, 0, (bond, slot))')
# ---- 104 served answers -------------------------------------------------------------------------------------------------------
ns(104, '(k.len()==73 || k.len()==81)', 'crate::palw_flat_model_vector_da_v9::FlatVectorServedRowV9', [(V11F, r'd\.body\.unit\.key')], 'v11', 'key (vector, FlatVectorUnitV9)')
ns(104, P(65, 16), f'raw:{OBJ}', [(r'palw_flat_(execution_source_fold_v16|stage_source_fold_v14)', r'pending_key\(.*unit_key')], 'v11', 'source16 answer body')
ns(104, P(65, 28), 'raw:128<<10', [(S28S, r'served_key')], 's28', 'stage28 assembled answer')
ns(104, P2(66, 29, 0), 'raw:128<<10', [(TDA, r'k\(0, &d\.request\.unit_id\(\)\)')], 'v27', 'Text29 assembled answer')
ns(104, P2(69, 29, 3), 'raw:32<<10', [(TDA, r'k\(3, &\(d\.id, a\.attempt')], 'v27', 'Text29 reply part (29, 3, (id, u8, u16))')
# ---- 105 pursuits / counters --------------------------------------------------------------------------------------------------
ns(105, L(64), f'{X11}::NativeExecutionPursuitV11', [(V11F, r'key\(&(id|job|d\.job_id|req\.job|d\.request\.job)\)')], 'v11')
ns(105, P(65, 13), f'{X11}::NativeExecutionDemandCountersV13', [(V11F, r'\(13u8, ')], 'v11')
ns(105, P(69, 14) + ' && n==64', D, [(r'palw_flat_execution_da_fold_v11', r'\(14u8, ')], 'v11', 'ordinal → v11 demand id')
ns(105, P(69, 14) + ' && n==65', f'(u8,{D})', [(r'palw_flat_execution_source_fold_v16', r'\(14u8, ')], 'v11', 'ordinal → (16, source16 pending id); same key shape, told apart by the fixed row length')
ns(105, P2(66, 29, 0), 'mirror:crate::palw_native_text_da_fold_v29::Counts', [(TDA, r'k\(0, ?&(job|d\.request\.job)\)')], 'v27')
# ---- 106 credits / fees / collection anchors ----------------------------------------------------------------------------------
ns(106, L(64), f'{X11}::NativeExecutionCreditV11', [(V11F, r'key\(&row\.header\.job_id\)')], 'v11')
ns(106, P(65, 19), f'{X11}::NativeExecutionFeesV19', [(V11F, r'\(19u8, ')], 'v11')
ns(106, P(65, 24), 'mirror:crate::palw_flat_execution_gc_fold_v24::NativeCollectedV24', [(V11F, r'\(24u8, job\)')], 'v11', 'GC tombstone (fold-private, serialize-only)')
ns(106, L(129) + ' && k[0]==24', f'({D},{D},u64)', [(V11F, r'\(24u8, job, id\)')], 'v11', 'retired own-seal anchor')
ns(106, P2(66, 29, 0), f'({T29}::NativeTextSettlementV29,{H},u64)', [(T29F, r'k\(0, &h\.job\)')], 'v27')
ns(106, P2(66, 29, 3), f'({D},{D},{D})', [(T29F, r'k\(3, &consent\.root\(\)\)')], 'v27')
ns(106, P2(66, 29, 4), f'any:({D},{D},{D},{D},u64)|({D},{D},{D},u64)', [(T29F, r'k\(4, &(old|id)\)')], 'v27',
   'AMBIGUOUS by key: discovery replacement (job, new, receipt, ticket, horizon) vs collected credit (book, credit, source, daa); both fixed-size')

# ---- 112 typed stages: stage14 (kind, value), stage28 (28, kind, value), master32 (32, kind, value) ----------------------------
for kind, value, note in [(0, f'{S14}::NativeStageSourceRowV14', 'source row'), (2, f'{S14}::NativeStageClassRowV14', 'class row'),
                          (5, f'{S14}::NativeStageJobRowV14', 'job row'), (9, f'{S14}::NativeStageAcceptanceV14', ''),
                          (10, f'{S14}::NativeStageAcceptanceV18', ''), (11, f'{S14}::NativeStagePrincipalRefundV14', ''),
                          (12, f'{S14}::NativeStageFeesV20', ''), (13, f'{Q}::stage14::StageSettlementV14', 'court outcome'),
                          (14, f'{S14}::NativeStageNegativeSourceFactV14', 'refuted relation (also written by stage28)'),
                          (16, f'{S14}::NativeStagePursuitV14', ''), (17, f'{S14}::NativeStageCollectedV14', ''),
                          (18, 'mirror:crate::palw_flat_stage_source_fold_v14::StageSourcePendingV14', 'fold-private'),
                          (19, f'{S14}::NativeStageSealV14', ''), (20, f'{X11}::NativeExecutionProofGroupV11', 'own proof group (20, seal)')]:
    srcs = [(r'palw_flat_stage_(fold|court_fold|source_fold)_v14|palw_flat_stage_fold_v28', rf'stage_key_v14\({kind}, ?&')]
    if kind == 20:
        srcs = [(r'palw_flat_stage_court_fold_v14', r'group_key')]
    ns(112, P(65, kind), value, srcs, 's14', note)
ns(112, P(66, 23), f'raw:{FRAG}', [(r'palw_flat_stage_court_fold_v14', r'^part_key$')], 's14', 'protected fragment (23, seal, u8 index)')
for kind in (1, 3, 6, 7, 8):
    ns(112, P(BK + 1, kind), VD, [(r'palw_flat_stage_fold_v14', rf'stage_key_v14\({kind}, ')], 's14', 'owner catalogue (kind, bond)')
ns(112, L(1) + ' && k[0]==4', f'{S14}::NativeStagePoliciesV14', [(r'palw_flat_stage_fold_v14', r'^key$')], 's14', 'immutable stage14 policies')
S28K = [(0, 'mirror:crate::palw_flat_stage_fold_v28::BundleHeader', 66), (1, 'Vec<u8>', 66), (2, 'mirror:crate::palw_flat_stage_fold_v28::ClassHeader', 66),
        (3, 'Vec<u8>', 66), (5, 'mirror:crate::palw_flat_stage_fold_v28::JobHeader', 66), (6, 'mirror:crate::palw_flat_stage_fold_v28::JobData', 66),
        (7, VD, BK + 2), (8, VD, BK + 2), (9, VD, BK + 2), (10, f'{S28}::NativeStageConsentV28', 66), (11, f'({D},{D},{D})', 66), (12, VD, BK + 2), (13, VD, BK + 2),
        (14, f'({Q}::stage14::bounded28::StageSettlementV28,{H},{H})', 66), (15, f'({D},{D},{D},{D})', 66),
        (16, 'mirror:crate::palw_flat_stage_source_fold_v28::Pursuit', 66), (18, 'mirror:crate::palw_flat_stage_source_fold_v28::Collection', 66),
        (19, VD, 66), (20, f'(({D},{D},{D},{D},Option<{D}>,{Q}::stage14::bounded28::StageSettlementV28),{D})', 66)]
for kind, value, n in S28K:
    ns(112, P2(n, 28, kind), value, [(r'palw_flat_stage_(source_)?fold_v28', rf'^key\({kind},')], 's28', 'stage28 (28, kind, ' + ('bond' if n == BK + 2 else 'id') + ')')
ns(112, P2(70, 28, 17), f'({D},{D},{B},u8)', [(S28S, r'key\(17,')], 's28', 'stage28 collection index (28, 17, (job, u32))')
ns(112, P2(2, 28, 4), f'{S28}::NativeStagePoliciesV28', [(S28F, r'key\(4,&\(\)\)')], 's28', 'immutable stage28 policies')
ns(112, P2(66, 32, 0), 'mirror:crate::palw_flat_stage_master_fold_v32::NativeMasterHeaderV32', [(r'palw_flat_stage_master_fold_v32', r'master_key\(0,')], 's28', 'master32 header')
ns(112, P2(66, 32, 1), 'Vec<u8>', [(r'palw_flat_stage_master_fold_v32', r'master_key\(1,')], 's28', 'master32 body')
ns(112, P2(BK + 2, 32, 2), VD, [(r'palw_flat_stage_master_fold_v32', r'master_key\(2,')], 's28', 'master32 catalogue')
M32 = r'palw_flat_stage_master_fold_v32'
ns(112, P2(66, 32, 3), 'mirror:crate::palw_flat_stage_master_fold_v32::InvocationConsentV32', [(M32, r'master_key\(3,')], 's28', 'master32 invocation consent')
ns(112, P2(66, 32, 4), 'Vec<u8>', [(M32, r'master_key\(4,')], 's28', 'master32 invocation (Borsh bytes of the invocation)')
ns(112, P2(66, 32, 5), 'mirror:crate::palw_flat_stage_master_fold_v32::InvocationActualV32', [(M32, r'master_key\(5,')], 's28', 'master32 actual invocation')
ns(112, P2(66, 32, 6), 'mirror:crate::palw_flat_stage_master_fold_v32::MasterProgressV32', [(M32, r'master_key\(6,')], 's28', 'master32 progress')
ns(112, P2(66, 32, 7), f'((u16,{D},{D},{D},{B},{D}),({D},{D},{VD},mirror:crate::palw_flat_stage_master_fold_v32::MasterProgressV32))',
   [(M32, r'master_key\(7,')], 's28', 'master32 collected tombstone')
ns(112, P2(66, 32, 8), f'({D},mirror:crate::palw_flat_stage_master_fold_v32::InvocationCommitJournalV32)', [(M32, r'master_key\(8,')], 's28', 'master32 producer commit journal')
ns(112, P2(69, 32, 9), 'raw:32<<10', [(M32, r'^commit_part_key\(')], 's28', 'master32 producer commit page (32, 9, job, u8, u16)')
ns(112, P2(66, 28, 32), 'mirror:crate::palw_flat_stage_master_fold_v32::NativeInvocationBindingV32', [(M32, r'^binding_key\(')], 's28', 'master32 invocation binding (28, 32, job)')

# ---- 112 owning39 (RFC-0002 §II.14): key borsh(39u8, kind, id) — every native39 fold module, then mining39 -----------------------
S39 = r'palw_native_source_fold_v39'
J39 = r'palw_native_job_fold_v39'
C39 = r'palw_native_collect_fold_v39'
PR39 = r'palw_native_protected_fold_v39'
PU39 = r'palw_native_public_fold_v39'
MT39 = r'palw_native_material_fold_v39'
MA39 = r'palw_native_master_fold_v39'
MI39 = r'palw_native_mining_fold_v39'
J = 'crate::palw_native_job_v39'
MV = 'crate::palw_native_master_v39'
NM = 'crate::palw_native_mining_v39'
OWN39 = [
    # source/class journal (palw_native_source_fold_v39.rs: `key(kind, id)`)
    (66, 0, f'({D},mirror:crate::palw_native_source_fold_v39::SourceHeaderV39)', S39, r'^key\(0, ', 'source39 header'),
    (66, 1, 'Vec<u8>', S39, r'^key\(1, ', 'source39 live body'),
    (66, 2, f'({D},mirror:crate::palw_native_source_fold_v39::ClassHeaderV39)', S39, r'^key\(2, ', 'class39 header'),
    (66, 3, 'Vec<u8>', S39, r'^key\(3, ', 'class39 live body'),
    (BK + 2, 5, 'mirror:crate::palw_native_source_fold_v39::CatalogV39', S39, r'^key\(5, ', 'retention payer catalogue (39, 5, bond)'),
    (66, 6, f'({D},mirror:crate::palw_native_source_fold_v39::UploadHeaderV39)', S39, r'^key\(6, ', 'source39 upload progress'),
    (68, 7, 'Vec<u8>', S39, r'^key\(7, ', 'source39 upload page (39, 7, (id, u16))'),
    (66, 94, f'({D},mirror:crate::palw_native_source_fold_v39::ClassHold)', S39, r'^key\(94, ', 'class retention hold'),
    (66, 95, 'u64', S39, r'^key\(95, ', 'class hold count'),
    (66, 96, B, S39, r'^key\(96, ', 'source retention payer'),
    (66, 97, B, S39, r'^key\(97, ', 'class retention payer'),
    (66, 98, 'u64', S39, r'^key\(98, ', 'active classes per source'),
    # jobs (palw_native_job_fold_v39.rs: `k(kind, id)`)
    (66, 16, f'({D},mirror:crate::palw_native_job_fold_v39::JobHeader)', J39, r'^k\(16, ', 'original funded job header'),
    (66, 17, f'{J}::NativeJobBookV39', J39, r'^k\(17, ', 'original job book'),
    (66, 18, f'({D},mirror:crate::palw_native_job_fold_v39::ConsentHeader)', J39, r'^k\(18, ', 'live producer consent header'),
    (BK + 2, 19, VD, J39, r'^k\(19, ', 'producer consent list'),
    (66, 20, f'({D},{D},{D})', J39, r'^k\(20, ', 'consumed consent tombstone (job, funding, consent)'),
    (BK + 2, 21, VD, J39, r'^k\(21, |^21$|^k\(kind, &who\)$', 'requester active jobs (the terminal loop writes 21 and 22 through `k(kind, &who)`)'),
    (BK + 2, 22, VD, J39, r'^k\(22, |^22$', 'producer active jobs'),
    (66, 23, f'({D},{J}::NativeSingleCreditV39)', J39, r'^k\(23, ', 'single credit'),
    (66, 25, f'({D},{D},{D},u64)', J39, r'^k\(25, ', 'original debit receipt (nonce root, receipt, block, daa)'),
    (66, 26, 'Vec<u8>', J39, r'^k\(26, ', 'live consent body'),
    (66, 27, f'{J}::NativeConsentV39', J39, r'^k\(27, ', 'consumed original consent'),
    (66, 28, f'{J}::NativeCommitWireV39', J39, r'^k\(28, ', 'accepted Commit post'),
    # termination / producer transport (palw_native_collect_fold_v39.rs)
    (66, 29, f'({D},mirror:crate::palw_native_collect_fold_v39::CommitJournalV39)', C39, r'^k\(COMMIT_JOURNAL, ', 'commit transport journal'),
    (69, 30, f'raw:{J}::NATIVE_COMMIT_PAGE_BYTES_V39', C39, r'^k\(COMMIT_PAGE, ', 'commit transport page (39, 30, (job, u8, u16))'),
    (66, 31, f'({D},{J}::NativeCollectedJobV39)', C39, r'^k\(COLLECTED, ', 'collection53 tombstone'),
    (66, 33, VD, C39, r'^k\(OPENED, ', 'public pending ids opened per job'),
    # paid discovery/proof (palw_native_protected_fold_v39.rs)
    (66, 36, f'({D},mirror:crate::palw_native_protected_fold_v39::Status)', PR39, r'^k\(36, ', 'public status'),
    (66, 37, f'({D},mirror:crate::palw_native_protected_fold_v39::Owner)', PR39, r'^k\(37, ', 'paid discovery/proof owner'),
    (68, 38, 'raw:32<<10', PR39, r'^k\(38, ', 'proof fragment (39, 38, (owner, u16))'),
    (66, 39, f'(({D},{D},{D},{D},{D},{D}),u64)', PR39, r'^k\(39, ', 'retired owner anchor'),
    # public acquisition (palw_native_public_fold_v39.rs)
    (66, 40, f'({D},mirror:crate::palw_native_public_fold_v39::Pending)', PU39, r'^k\(40, ', 'public pending unit'),
    (66, 41, D, PU39, r'^k\(41, ', 'unit -> pending id'),
    (66, 42, f'({D},Vec<u8>,Vec<u8>)', PU39, r'^k\(42, ', 'settled reply (root, demand, reply)'),
    (69, 43, 'raw:32<<10', PU39, r'^k\(43, ', 'answer fragment (39, 43, (pending, u8, u16))'),
    (BK + 2, 44, VD, PU39, r'^k\(44, ', 'requester pending list'),
    # requester material (palw_native_material_fold_v39.rs)
    (66, 60, f'({D},mirror:crate::palw_native_material_fold_v39::MaterialHeader)', MT39, r'^k\(60, ', 'material header'),
    (66, 61, 'misaka_palw_kernel::flat_material39::MaterialProgressV39', MT39, r'^k\(61, ', 'material progress'),
    (68, 62, f'raw:{OBJ}', MT39, r'^k\(62, ', 'material page (39, 62, (id, u16)): the carrier tail'),
    (BK + 2, 63, VD, MT39, r'^k\(63, ', 'requester material list'),
    (66, 64, f'({D},{D},u64)', MT39, r'^k\(64, ', 'collected material anchor'),
    (66, 65, f'({D},mirror:crate::palw_native_material_fold_v39::CertificateProgress)', MT39, r'^k\(65, ', 'output certificate progress'),
    (68, 66, f'raw:{OBJ}', MT39, r'^k\(66, ', 'output certificate page (39, 66, (plan, u16))'),
    (66, 67, D, MT39, r'^k\(67, ', 'certificate plan root'),
    # Masters and metadata (palw_native_master_fold_v39.rs)
    (66, 80, f'({D},{MV}::NativeMasterRecordV39)', MA39, r'^k\(80, ', 'Master record'),
    (66, 81, f'(u16,{D},u32,u16,{D})', MA39, r'^k\(81, ', 'Master body directory'),
    (66, 82, f'({D},mirror:crate::palw_native_master_fold_v39::Progress)', MA39, r'^k\(82, ', 'Master component progress'),
    (66, 83, f'({D},{MV}::NativeMasterFinalRecordV39)', MA39, r'^k\(83, ', 'Master Final'),
    (66, 84, f'({D},{MV}::NativeMasterFundedComponentV39)', MA39, r'^k\(84, ', 'job -> Master component binding'),
    (BK + 2, 85, VD, MA39, r'^k\(85, ', 'owner Master list'),
    (BK + 2, 86, 'u64', MA39, r'^k\(86, ', 'owner Master sequence'),
    (66, 87, f'({D},u64)', MA39, r'^k\(87, ', 'closed Master anchor'),
    (66, 88, f'({D},mirror:crate::palw_native_master_fold_v39::MetadataUpload)', MA39, r'^k\(88, ', 'metadata upload progress'),
    (68, 89, f'raw:{OBJ}', MA39, r'^k\(89, ', 'metadata upload page (39, 89, (id, u16))'),
    (BK + 2, 90, 'u64', MA39, r'^k\(90, ', 'owner metadata sequence'),
    (BK + 2, 91, VD, MA39, r'^k\(91, ', 'owner metadata uploads'),
    (70, 92, f'({D},u16,{D},{D})', MA39, r'^k\(92, ', 'upstream edge (39, 92, (Master, u16 stage, u16 input))'),
    (68, 93, 'raw:32<<10', MA39, r'^k\(93, ', 'Master body page (39, 93, (id, u16))'),
]
for n, kind, value, f, kr, note in OWN39:
    ns(112, P2(n, 39, kind), value, [(f, kr)], 'n39', note)
MINING39 = [
    (66, 100, f'({D},{NM}::NativeMiningModelRecordV39)', r'^k\(MINING_MODEL_KIND_V39, ', 'mining model'),
    (66, 101, D, r'^k\(MINING_MODEL_SOURCE_KIND_V39, ', 'complete source -> model'),
    (2 + 64 + BK, 102, f'({D},{NM}::NativeMiningSeatRecordV39)', r'^k\(MINING_SEAT_KIND_V39, ', 'seat (39, 102, (model, bond))'),
    (66, 103, f'({D},{NM}::NativeMiningClaimRecordV39)', r'^k\(MINING_CLAIM_KIND_V39, ', 'mining claim'),
    (66, 104, f'({D},{NM}::NativeMiningReceiptRecordV39)', r'^k\(MINING_RECEIPT_KIND_V39, ', 'mining receipt'),
    (74, 106, 'u64', r'^k\(MINING_EPOCH_KIND_V39, ', 'per-model epoch count (39, 106, (model, u64))'),
    (66, 107, f'Vec<{B}>', r'^k\(MINING_SEAT_LIST_KIND_V39, ', 'model seat list'),
    (68, 108, D, r'^k\(MINING_MASTER_USED_KIND_V39, ', 'credited Master stage (39, 108, (Master, u16)) -> claim'),
    (BK + 2, 109, 'u64', r'^k\(MINING_MODEL_SEQUENCE_KIND_V39, ', 'registrant sequence'),
    (BK + 2, 110, 'u64', r'^k\(MINING_PRODUCER_SEQUENCE_KIND_V39, ', 'producer claim sequence'),
    (2, 113, 'u64', r'^k\(MINING_MODEL_COUNT_KIND_V39, ', 'registered model count (39, 113, ())'),
    (66, 114, f'({D},Option<(u64,bool)>,u64)', r'^k\(MINING_COLLECTED_KIND_V39, ', 'collected claim tombstone'),
]
for n, kind, value, kr, note in MINING39:
    ns(112, P2(n, 39, kind), value, [(MI39, kr)], 'm39', note)

RESERVED = {42: 'reserved by the chunk lane (41–42); no fold writes it', 101: 'EXECUTION_SEAL_COUNT_TABLE_V11: allocated, never written'}
TABLES = sorted(set(n['table'] for n in NS) | set(RESERVED))

# Raw-row bounds above are byte caps only; the decoding fold enforces the exact extent.

ROOT = Path(__file__).resolve().parents[2]


def table_names():
    out = {}
    for line in subprocess.check_output(['git', 'grep', '-hE', r'const [A-Z0-9_]+: u8 = [0-9]+;', '--', 'consensus/core/src'], cwd=ROOT).decode().splitlines():
        m = re.search(r'const ([A-Z0-9_]+): u8 = ([0-9]+);', line)
        if m and 'TABLE' in m[1] and 42 <= int(m[2]) <= 112:
            out.setdefault(int(m[2]), m[1])
    return out


def write_sites():
    """Every `write_kernel_row(table, key, Some(..))` into tables 42–112, the table traced from its constant (or the local that holds it)."""
    sys.path.insert(0, str(Path(__file__).parent))
    consts = {}
    for line in subprocess.check_output(['git', 'grep', '-hE', r'const [A-Z0-9_]+: u8 = [0-9]+;', '--', 'consensus/core/src'], cwd=ROOT).decode().splitlines():
        m = re.search(r'const ([A-Z0-9_]+): u8 = ([0-9]+);', line)
        if m:
            consts.setdefault(m[1], int(m[2]))
    consts['TABLE@palw_native_initial_fold_v31.rs'] = consts['EXECUTION_QUOTA_TABLE_V11']

    def bal(s, i):
        d = 1
        while d and i < len(s):
            c = s[i]
            if c in '([{':
                d += 1
            elif c in ')]}':
                d -= 1
            i += 1
        return i

    def split(s):
        out, st, d = [], 0, 0
        for i, c in enumerate(s):
            if c in '([{':
                d += 1
            elif c in ')]}':
                d -= 1
            elif c == ',' and d == 0:
                out.append(s[st:i].strip()); st = i + 1
        if s[st:].strip():
            out.append(s[st:].strip())
        return out

    def resolve(name, short, body, text=''):
        n = name.split('::')[-1].lstrip('*')
        local = re.search(r'\bconst\s+' + re.escape(n) + r'\s*:\s*u8\s*=\s*([^;]+);', text)
        if local:
            v = local[1].strip()
            return int(v) if v.isdigit() else consts.get(v.split('::')[-1])
        alias = re.search(r'\b([A-Z][A-Z0-9_]*)\s+as\s+' + re.escape(n) + r'\b', text)
        if alias:
            return consts.get(alias[1])
        if n in consts:
            return consts[n]
        if f'{n}@{short}' in consts:
            return consts[f'{n}@{short}']
        m = list(re.finditer(r'let\s+' + re.escape(n) + r'\s*(?::\s*u8)?\s*=\s*([^;]+);', body))
        if m:
            return consts.get(m[-1][1].split('::')[-1].strip())
        return None

    files = subprocess.check_output(['git', 'grep', '-l', 'write_kernel_row', '--', 'consensus/core/src'], cwd=ROOT).decode().split()
    sites = []
    for f in files:
        if '/tests/' in f or f.endswith('tests.rs'):
            continue
        s = (ROOT / f).read_text()
        short = f.split('/')[-1]
        loops = []
        for m in re.finditer(r'for\s*\(\s*(\w+)\s*,\s*(\w+)\s*(?:,\s*(\w+)\s*)?\)\s*in\s*\[', s):
            e = bal(s, m.end())
            b0 = s.index('{', e)
            loops.append((b0, bal(s, b0 + 1), {g for g in m.groups() if g}))
        for m in re.finditer(r'write_kernel_row\(', s):
            e = bal(s, m.end())
            args = split(' '.join(s[m.end():e - 1].split()))
            if len(args) != 3:
                continue
            t, k, v = args
            if any(a <= m.start() < b and (k in names or t in names) for a, b, names in loops):
                continue  # a loop over (table, key, row) tuples: its tuples are the sites, below
            fstart = max(s.rfind('\nfn ', 0, m.start()), s.rfind(' fn ', 0, m.start()))
            table = resolve(t, short, s[fstart:m.start()], s)
            sites.append((table, short, s.count('\n', 0, m.start()) + 1, k, v))
        for m in re.finditer(r'for\s*\(\s*(\w+)\s*,\s*(\w+)\s*(?:,\s*(\w+)\s*)?\)\s*in\s*\[', s):
            e = bal(s, m.end())
            b0 = s.index('{', e)
            body = s[e:bal(s, b0 + 1)]
            wm = re.search(r'write_kernel_row\(\s*([\w:]+)\s*,', body)
            if not wm:
                continue
            ln = s.count('\n', 0, m.start()) + 1
            fixed = None if wm[1] == m[1] else resolve(wm[1], short, s[:m.start()], s)
            for tup in split(' '.join(s[m.end():e - 1].split())):
                if not tup.startswith('('):
                    continue
                parts = split(tup[1:-1])
                if fixed is not None:
                    table, k, v = fixed, parts[0], (parts[1] if len(parts) > 1 else 'None')
                else:
                    table, k = resolve(parts[0], short, s[:m.start()], s), parts[1]
                    v = parts[2] if len(parts) > 2 else 'None'
                if 'Some(' not in body and len(parts) < 3 and fixed is None:
                    v = 'None'
                sites.append((table, short, ln, k, v))
    return [x for x in sites if x[0] in TABLES and x[4] != 'None' and not x[4].startswith('None')]


def classify(sites):
    claimed = {i: [] for i in range(len(NS))}
    unclaimed = []
    for (t, f, ln, k, v) in sites:
        hits = [i for i, n in enumerate(NS) if n['table'] == t and any(
            re.search(src[0], f) and re.search(src[1], k) and (len(src) < 3 or re.search(src[2], v)) for src in n['src'])]
        if len(hits) == 1:
            claimed[hits[0]].append(f'{f}:{ln}')
        else:
            unclaimed.append((t, f, ln, k, v, hits))
    return claimed, unclaimed


def doc(claimed, nsites=0):
    names = table_names()
    out = ['# Native resource scanner: kernel-route aux row inventory (tables 42–106, 112)', '',
           '_2026-10-15. Generated by `scripts/drills/rfc06_native_aux_spec.py`; the same module drives the scanner arms '
           '(`gen_rfc06_native_wire.py`) and the type roots (`gen_rfc06_native_inventory.py`). Regenerate, never hand-edit._', '',
           'Every row the scanner meets in these tables is dispatched by its full key (`k`) and, where two layouts share one key shape, '
           'by its fixed row length (`n`). A key shape no fold writes fails closed ("unallocated … namespace"); rows exist only past '
           'their owning fence, so that refusal can only fire above it. Write sites are every '
           '`write_kernel_row(…, Some(…))` found by `git grep` (tables written through a local are traced to their constant).', '',
           '## Conventions', '',
           '- `D` = `[u8; 64]` (`Digest`), `B` = `PalwBondKeyV2` (68 Borsh bytes: a `Hash64` transaction id and a `u32` index), `H` = `Hash64`.',
           '- `raw:N`: a byte string stored verbatim (not Borsh-framed, never decoded by restore or ledger projection). The scanner '
           'checks the declared bound N, then accounts the bytes as one more transient copy (the outer loop already counts the stored '
           'row and its copy).',
           '- `mirror:T`: a fold-private struct; the generator expands its declared fields into a Borsh-identical nested tuple, so the '
           'accounting is the struct\'s own structural walk.',
           '- `any:A|B`: one key shape, two family layouts (each family writes only its own ids). The bounded row is buffered, every '
           'layout is tried, the largest accepted shape is accounted; none accepting fails closed. **Marked ambiguous.**', '',
           '## Coverage', '',
           f'- {nsites} value-writing sites into these tables, {len(NS)} namespaces; every site is claimed by exactly one namespace and '
           'every namespace by at least one site (the generator run fails otherwise).',
           '- Ambiguous key shapes (handled conservatively, see `any:`): table 92 `(29, job, 23)` (Text29 rooted carriage record vs '
           'Stage28 bare record), table 106 `(29, 4, id)` (discovery replacement vs collected credit; told apart by fixed length) '
           'and table 92 `(39, …)` 66-byte keys (owning39 source obligation `(39, 0, job)` vs the job\'s original records '
           '`(39, job, 0..4)`).',
           '- Owning39 (RFC-0002 §II.14) rows in table 112 are keyed `borsh(39u8, kind, id)`; mining39 kinds 100–114 ride their own '
           'fence. Regenerating the scanner arms needs the native wire generator to run (blocked on this branch until the shape-review '
           'pins of integ/rfc06-12-remaining reach integ).',
           '- Table 105 `(14, job, ordinal)` carries a 64-byte v11 demand id or a 65-byte `(16, source16 pending)` pair; dispatched '
           'on the exact row length.',
           '- Registry completeness is enforced by `palw_native_resource_v1::aux_registry_tests` (source-discovered tables must reach '
           'an explicit arm).', '']
    for t in TABLES:
        out.append(f'## Table {t} — `{names.get(t, "?")}`')
        out.append('')
        if t in RESERVED:
            out.append(f'Reserved: {RESERVED[t]}. Any row fails closed.')
            out.append('')
            continue
        out.append('| key condition | value codec | owning fence | write sites | note |')
        out.append('|---|---|---|---|---|')
        for i, n in enumerate(NS):
            if n['table'] != t:
                continue
            v = n['value'].replace('[u8;64]', 'D').replace('crate::palw_state_v2::PalwBondKeyV2', 'B').replace('crate::Hash64', 'H')
            v = re.sub(r'crate::|misaka_palw_kernel::', '', v).replace('|', ' \\| ')
            sites = ', '.join(claimed.get(i, [])) or '(none found)'
            out.append(f'| `{n["cond"]}` | `{v}` | {n["fence"]} | {sites} | {n["note"]} |')
        out.append('')
    return '\n'.join(out) + '\n'


if __name__ == '__main__':
    sites = write_sites()
    claimed, unclaimed = classify(sites)
    path = ROOT / 'docs/design/palw/native-resource-aux-inventory-2026-10-15.md'
    path.write_text(doc(claimed, len(sites)))
    empty = [n for i, n in enumerate(NS) if not claimed[i]]
    print(f'{len(sites)} write sites, {len(NS)} namespaces, {len(unclaimed)} unclaimed, {len(empty)} namespaces without a site')
    for u in unclaimed:
        print('UNCLAIMED', u)
    for n in empty:
        print('NO SITE', n['table'], n['cond'], n['value'])
    sys.exit(1 if unclaimed else 0)
