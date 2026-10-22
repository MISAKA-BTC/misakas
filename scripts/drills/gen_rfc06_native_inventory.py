#!/usr/bin/env python3
"""Generate explicit field visitors. This writes source; it executes no Rust validation.
The generated match/destructure is exhaustive: field/variant additions require regeneration.
Private foreign layouts and unresolved runtime types remain explicit Unsupported visitors.
"""
import re
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / 'consensus/core/src'
OUT = CORE / 'palw_native_resource_v1/generated.rs'

def clean(s):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', s, flags=re.S)

def split(s):
    out=[]; start=0; stack=[]
    for i,c in enumerate(s):
        if c in '<([{': stack.append(c)
        elif c in '>)]}' and stack: stack.pop()
        elif c==',' and not stack: out.append(s[start:i].strip());start=i+1
    tail=s[start:].strip()
    if tail:out.append(tail)
    return out

def brace(s, i):
    opener=s[i]; closer={'{':'}','(':')','[':']'}[opener]; depth=1; j=i+1
    while depth and j<len(s):
        if s[j]==opener:depth+=1
        elif s[j]==closer:depth-=1
        j+=1
    return s[i+1:j-1], j

def stripattrs(s):
    while '#[' in s:
        m=s.index('#['); _,e=brace(s,m+1);s=s[:m]+s[e:]
    return s.strip()

# Public names with their canonical modules. A reference already qualified by the
# source takes precedence over the unique-name fallback.
defs={}; byname={}; aliases={}; alias_byname={}
CRATES=[(CORE,'crate'),(ROOT/'misaka/palw/panel/src','misaka_palw_panel'),(ROOT/'misaka/palw/kernel/src','misaka_palw_kernel'),(ROOT/'misaka/palw/gen/src','misaka_palw_gen'),(ROOT/'misaka/palw/tir/core/src','misaka_palw_tir'),(ROOT/'misaka/palw/challenge/src','misaka_palw_challenge')]
def module_of(base,crate,file):
    module='::'.join(file.relative_to(base).with_suffix('').parts)
    if crate=='misaka_palw_panel':module=''
    if module in {'lib','mod'}:module=''
    elif module.endswith('::mod'):module=module[:-5]
    return module
# A private `mod child;` re-exported by `pub use child::*;` publishes its pub items at the PARENT path (e.g. the kernel's
# `flat_quota11::{sharded,protected}`): the computed file path would name a private module the generated code cannot reach.
reexport={}
for base,crate in CRATES:
 for file in base.rglob('*.rs'):
    if 'tests' in file.parts or file.name=='generated.rs':continue
    raw=clean(file.read_text()); module=module_of(base,crate,file)
    for m in re.finditer(r'(?m)^\s*mod\s+(\w+)\s*;',raw):
        if re.search(r'(?m)^\s*pub\s+use\s+'+m[1]+r'\s*::\s*\*\s*;',raw):
            reexport[(crate,'::'.join(x for x in [module,m[1]] if x))]=module
def canonical_module(crate,module):
    while (crate,module) in reexport:module=reexport[(crate,module)]
    return module
# A private `#[path = "file.rs"] mod child;` (palw_state_v2's fold files live beside it at the crate root) whose pub items the
# parent re-exports BY NAME (`pub use child::{A, B};`): those names are reachable only at the PARENT path, never at the path
# the file's location suggests (G14: `crate::palw_panel_v3_fold_v1` is not a crate-level module).
named_reexport={}
for base,crate in CRATES:
 for file in base.rglob('*.rs'):
    if 'tests' in file.parts or file.name=='generated.rs':continue
    raw=clean(file.read_text()); module=module_of(base,crate,file)
    for m in re.finditer(r'#\[path\s*=\s*"([^"]+)"\]\s*mod\s+(\w+)\s*;',raw):
        target=(file.parent/m[1]).resolve()
        if not target.is_file() or not target.is_relative_to(base):continue
        for u in re.finditer(r'(?m)^\s*pub\s+use\s+'+m[2]+r'\s*::\s*\{([^}]*)\}\s*;',raw):
            for n in split(u[1]):
                if re.fullmatch(r'\w+',n):named_reexport[(crate,module_of(base,crate,target),n)]=module
for base,crate in [(CORE,'crate'),(ROOT/'misaka/palw/panel/src','misaka_palw_panel'),(ROOT/'misaka/palw/kernel/src','misaka_palw_kernel'),(ROOT/'misaka/palw/gen/src','misaka_palw_gen'),(ROOT/'misaka/palw/tir/core/src','misaka_palw_tir'),(ROOT/'misaka/palw/challenge/src','misaka_palw_challenge')]:
 for file in base.rglob('*.rs'):
    if 'tests' in file.parts or file.name=='generated.rs':continue
    raw=clean(file.read_text()); relative=file.relative_to(base).with_suffix('')
    module='::'.join(relative.parts)
    if crate=='misaka_palw_panel':module=''
    if module in {'lib','mod'}:module=''
    elif module.endswith('::mod'):module=module[:-5]
    for a in re.finditer(r'(?:pub(?:\([^)]*\))?\s+)?type\s+(\w+)\s*=\s*([^;]+);',raw):
        apath='::'.join(x for x in [crate,module,a.group(1)] if x)
        aliases[apath]=(a.group(2),dict(path=apath,module=module))
        alias_byname.setdefault(a.group(1),[]).append(apath)
    for m in re.finditer(r'(?:pub(?:\([^)]*\))?\s+)?(struct|enum)\s+(\w+)\s*',raw):
        kind,name=m.groups();pos=m.end()
        if pos>=len(raw) or raw[pos] not in '{(;':continue
        if raw[pos]==';':body='';end=pos+1;form='unit'
        else:body,end=brace(raw,pos);form='named' if raw[pos]=='{' else 'tuple'
        path='::'.join(x for x in [crate,canonical_module(crate,module) if m.group(0).startswith('pub ') or m.group(0).startswith('pub\t') else module,name] if x)
        if m.group(0).startswith('pub') and (crate,module,name) in named_reexport:
            path='::'.join(x for x in [crate,named_reexport[(crate,module,name)],name] if x)
        attrs=raw[max(raw.rfind('}',0,m.start()),raw.rfind(';',0,m.start()))+1:m.start()]
        item=dict(attrs=attrs,kind=kind,name=name,path=path,body=body,form=form,module=module,type_public=m.group(0).startswith('pub'),file=str(file.relative_to(ROOT)))
        defs[path]=item;byname.setdefault(name,[]).append(item)

primitive={'u8','u16','u32','u64','u128','usize','i8','i16','i32','i64','i128','isize','bool','f32','f64','String','Hash64','BlockHash','TransactionId','Hash','Hash32','Digest','()'}
# `PalwSharedCourtV1<T>` (G14) is a generic Arc newtype whose Borsh layout is T's: a container with hand-written visitors.
containers={'Vec','Option','Box','Arc','BTreeMap','BTreeSet','HashMap','HashSet','VecDeque','IndexMap','IndexSet','SmallVec','Cow','PalwSharedCourtV1'}
queue=['crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1','crate::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3','crate::header::Header','crate::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3','crate::palw_material_session_enrollment_v4::PalwNativeRecoverySessionQuoteV4','crate::palw_material_capacity_v1::PalwKernelMaterialBundleV1','crate::palw_material_capacity_v1::PalwSegmentedMaterialBundleV1','misaka_palw_kernel::public::PositionResponseV1','misaka_palw_kernel::seg_da::SegPartResponseV1',
       'crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4','crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialIntentV4','crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4','crate::palw_prospective_kernel_material_v4::PalwTypedKernelMaterialBundleV4',
       'crate::palw_kernel_parameter_material_v4::PalwKernelSegmentedPartV4','crate::palw_kernel_parameter_material_v4::PalwKernelParameterRowV4','crate::palw_kernel_parameter_material_v4::PalwKernelParameterOpeningV4',
       'crate::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4',
       'crate::palw_kernel_material_query_v4::PalwKernelMaterialUnitQueryV4','crate::palw_kernel_material_query_v4::PalwKernelMaterialUnitResponseV4',
       'crate::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementIntentV4',
       'crate::palw_step_refute::memory_v2::PalwGdnStepRefutationV2',
       'misaka_palw_kernel::spec::retrieval::SliceResponseV1','misaka_palw_kernel::spec::retrieval::EntryResponseV1','misaka_palw_kernel::spec::retrieval::RetrievalItemV1',
       'misaka_palw_kernel::route::KernelRouteObjectV1','crate::palw_state_v2::PalwConsensusObjectV2','crate::palw_state_v2::PalwTransitionExtrasV1',
       'crate::palw_claim_compact_v4::PalwPaidCompactContractV4','crate::palw_claim_compact_v4::PalwPaidCompactProofBindingV4','crate::palw_material_release_recovery_v2::PalwMaterialProviderAckCheckpointV2','crate::palw_material_release_recovery_v2::PalwMaterialRecoveryRowV2','crate::palw_material_capacity_v1::PalwMonetaryMaterialProviderStoreV1',
       'crate::palw_state_v2::PalwStateDeltaV2','crate::palw_native_settlement_v1::NativeDeltaEvidenceV1','crate::palw_fork_choice_commitment_v1::PalwForkChoiceDeltaV1',
       'crate::palw_material_capacity_v1::PalwHeldMaterialBundleV1',
       # Field types of the hand-written Panel containers (PermissionlessPanelStateV1/PanelCandidatesV1 are manual).
       'misaka_palw_panel::ClaimRecordV3','misaka_palw_panel::BondIdV1','misaka_palw_panel::EpochSourceSetV1','misaka_palw_panel::SeatCandidateV1','misaka_palw_kernel::state::ClassRecordV1','misaka_palw_kernel::state::PipelineClassRecordV1','misaka_palw_kernel::flat_candidate7::FlatCandidateViewV7','misaka_palw_kernel::flat_candidate9::FlatCandidateViewV9','crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3','crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1','crate::palw_panel_source_fee_v2::PalwPanelSourceFeeAuthorityV2','crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2','crate::palw_claim_challenge_v2::PalwPaidClaimLockV2','crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2','crate::palw_claim_challenge_v2::PalwPaidClaimEvidenceV3','crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3','misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2','misaka_palw_kernel::claim_paid_v2::PaidClaimJobV2','misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2','misaka_palw_challenge::approval_scope::LifetimeCounterV3','misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3','crate::palw_kernel_route_v1::KernelAssignmentV1','crate::palw_kernel_route_v1::PalwKernelChunkGroupV1','misaka_palw_kernel::receipt::PalwConstraintReceiptV1','crate::palw_onboarding_v1::ArtifactBindingRowV1','crate::palw_onboarding_v1::KernelBindingRowV1','crate::palw_onboarding_v1::ConformanceRowV1','crate::palw_onboarding_v1::ConformanceAttemptRowV1','crate::palw_conformance_evidence_v1::ConformanceEvidencePostV1','crate::palw_beacon_source_fee_v1::PalwBeaconSourceCostBoundsV1','crate::palw_panel_source_snapshot_v1::PalwPanelSourceSnapshotV1','crate::palw_state_v2::PalwClassAdmissionCarriageV2','crate::palw_state_v2::PalwStateCarriageV2','crate::palw_state_v2::PalwChainStateV2','misaka_palw_kernel::ledger::KernelLedgerV1',
       'crate::palw_panel_lifetime_v2::PalwPanelAuthorizationRowV2','crate::palw_panel_lifetime_v2::PalwPanelConsentCaptureV2','crate::palw_panel_lifetime_v2::PalwPanelLifetimeEnrollmentV2',
       'misaka_palw_kernel::panel_lifetime_v2::PanelLifetimeAuthorizationV2','misaka_palw_kernel::panel_lifetime_v2::PanelConsumerLifetimeTermsV2',
       'crate::palw_econ_contracts_v1::PalwModelVerificationTallyV1','crate::palw_econ_contracts_v1::PalwHorizonExtensionRowV1','crate::palw_econ_contracts_v1::PalwMinimumPriceRowV1',
       'crate::palw_econ_contracts_v1::PalwModelVerificationContextV1','crate::palw_econ_contracts_v1::PalwModelVerificationDeferredV1','crate::palw_econ_contracts_v1::PalwModelVerificationDecidedV1',
       'crate::palw_beacon_bootstrap_v1::PalwBootstrapRoundRowV1',
       # RFC-0017 (2026-10-15): the model eligibility rows of kernel route aux 131.
       'crate::palw_model_eligibility_v1::PalwEligibilityModelRowV1','crate::palw_model_eligibility_v1::PalwEligibilityLineageRowV1',
       'crate::palw_model_eligibility_v1::PalwEligibilityPrincipalRowV1','crate::palw_model_eligibility_v1::PalwEligibilityNetworkRowV1',
       'crate::palw_model_eligibility_v1::PalwEligibilityExamRowV1','crate::palw_model_eligibility_v1::PalwEligibilityProbationClaimRowV1',
       # RFC-0018 (2026-10-15): the SecurityCertified rows of kernel route aux 136.
       'crate::palw_security_certification_v1::PalwClaimCertificationRowV1','crate::palw_security_certification_v1::PalwCertificationClassRowV1',
       'crate::palw_security_certification_v1::PalwCertificationNetworkRowV1',
       # RFC-0006 consumer v2 (2026-10-15): the router binding rows of kernel route aux 139.
       'crate::palw_router_binding_v2::PalwRouterBindingRowV2',
       # RFC-0006 RFC06-8 (2026-10-15): the class registration origins of kernel route aux 141 (dormant fence).
       'crate::palw_class_registration_origin_v1::PalwClassRegistrationOriginRowV1',
       # RFC-0008 (2026-10-15): the once-only settlement markers of kernel route aux 140 (dormant fence).
       'crate::palw_settlement_once_v1::PalwSettlementOnceRowV1',
       # RFC-0007 §4.1 T6 (2026-10-16): the paid-claim trap rows of kernel route aux 142 (dormant fence).
       'crate::palw_paid_claim_trap_v1::PalwPaidClaimConvictionRowV1','crate::palw_paid_claim_trap_v1::PalwHeldCheckFeeRowV1',
       'crate::palw_paid_claim_trap_v1::PalwPaidTrapCommitmentRowV1',
       # G14 lane closure (2026-10-16): kernel route aux 143 (dormant).
       'crate::palw_g14_lane_closure_v1::PalwKernelSignerLockRowV1','crate::palw_g14_lane_closure_v1::PalwServedMaterialRowV1',
       # RFC08 08-B (2026-10-18): held native fees, kernel route aux 144 (dormant).
       'crate::palw_native_liability_hold_v1::PalwNativeHeldFeeRowV1',
       # G14 r6 (2026-10-22): receipt use-window pause rows, kernel route aux 145 (dormant behind Gate C).
       'crate::palw_receipt_pause_v2::PalwReceiptPauseRowV2',
       'crate::palw_native_post_final_duty_row_v1::PalwNativePostFinalDutyRowV1',
       # audit B-02 (fix-b): recorded post-Final material units, aux 144 prefix 0x03 (dormant; no production writer).
       'crate::palw_native_post_final_duty_row_v1::PalwNativePostFinalMaterialRowV1']; visited=set(); generated=[]; unresolved=[]

# Resolve dependency types; concrete names, aliases/primitive containers are left
# to Rust's existing type system, so there is no byte-size inference from codec.
def references(t, origin):
    t=stripattrs(t)
    for token in re.findall(r'(?:[A-Za-z_]\w*::)*[A-Za-z_]\w*',t):
        if token.startswith('crate::') and not origin['path'].startswith('crate::'):token=origin['path'].split('::')[0]+'::'+token[7:]
        name=token.split('::')[-1]
        if name in primitive or name in containers or name in {'std','collections','sync','borrow','mut','static'} or name.isupper():continue
        if token.startswith(('flat_candidate7::','flat_candidate9::')):token='misaka_palw_kernel::'+token
        if token=='misaka_palw_kernel::beacon_source::BeaconContextV1':token='misaka_palw_challenge::beacon::BeaconContextV1'
        if token=='misaka_palw_tir::program_v2::Block':token='misaka_palw_tir::program::Block'
        if name=='HashContinuation':token='crate::palw_rendering_stream_v1::LeafHashContinuationV1';name='LeafHashContinuationV1'
        if token in defs: resolved=token
        elif token in aliases: resolved=token
        elif '::' not in token and len(alias_byname.get(name,[]))==1: resolved=alias_byname[name][0]
        elif len(byname.get(name,[]))==1: resolved=byname[name][0]['path']
        elif '::' not in token and origin['path'].rsplit('::',1)[0]+'::'+token in defs:resolved=origin['path'].rsplit('::',1)[0]+'::'+token
        else:
            resolved=token if '::' in token else origin['path'].rsplit('::',1)[0]+'::'+token
        if resolved=='misaka_palw_tir::program_v2::Block':resolved='misaka_palw_tir::program::Block'
        if resolved not in visited:queue.append(resolved)

# A field compiled only under a `#[cfg(..)]` (palw_state_v2's `#[cfg(test)]` extras writers) keeps that attribute in the generated
# exhaustive destructure, so both the test and the non-test build name exactly the fields that exist.
def field_cfgs(item):
    out={}
    for part in split(item['body']):
        cfgs=re.findall(r'#\[\s*cfg\s*\((?:[^()]|\([^()]*\))*\)\s*\]',part)
        fm=re.match(r'(?:pub(?:\([^)]*\))?\s+)?(\w+)\s*:',stripattrs(part))
        if cfgs and fm:out[fm[1]]=' '.join(cfgs)+' '
    return out

def fields(item):
    result=[]
    for part in split(item['body']):
        part=stripattrs(part)
        m=re.match(r'(pub(?:\([^)]*\))?\s+)?(\w+)\s*:\s*(.*)',part,re.S)
        if not m:return None
        vis,name,typ=m.groups();result.append((name,typ,vis is not None and (item['path'].startswith('crate::') or '(' not in vis)))
    return result

# Kernel-route aux row codecs for tables 42–106 and 112 (scripts/drills/rfc06_native_aux_spec.py): root every value type the
# scanner names, and the declared field types of the fold-private structs it mirrors (never the private structs themselves).
import importlib.util
_spec=importlib.util.spec_from_file_location('rfc06_native_aux_spec',Path(__file__).with_name('rfc06_native_aux_spec.py'))
AUX=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(AUX)
AUX_ORIGIN=dict(path='crate::palw_native_resource_v1::wire_kernel_aux',module='palw_native_resource_v1::wire_kernel_aux')
def aux_root(value):
    for alt in (value[4:].split('|') if value.startswith('any:') else [value]):
        if alt.startswith('raw:'):continue
        for key in re.findall(r'mirror:([\w:]+)',alt):
            item=defs[key]
            for part in split(item['body']):
                if re.search(r'#\[borsh\(skip\)\]',part):continue
                fm=re.match(r'(?:pub(?:\([^)]*\))?\s+)?\w+\s*:\s*(.*)',stripattrs(part),re.S)
                aux_root(fm[1]) if fm[1].startswith('mirror:') else references(fm[1],item)
        references(re.sub(r'mirror:[\w:]+','u8',alt),AUX_ORIGIN)
for _n in AUX.NS:aux_root(_n['value'])
FUNDED_MANUAL={'crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1','crate::palw_funded_original_coordinator_v1::PalwFundedOriginalScheduleV1'}
while queue:
 path=queue.pop(0)
 if path in visited:continue
 visited.add(path)
 if path in aliases:
    alias,origin=aliases[path];references(alias,origin);continue
 item=defs.get(path)
 if item is None:
    unresolved.append(path);continue
 kind=item['kind']; form=item['form']; body=item['body']
 if path=='crate::palw_kernel_route_v1::KernelLedgerCacheV1':continue
 # Funded-original pairing (holds a whole carrier Transaction) and its test-only schedule: hand-written visitors in typed.rs.
 if path in FUNDED_MANUAL:unresolved.append(path);continue
 if kind=='struct' and form=='named':
    fs=fields(item)
    if path in {'misaka_palw_kernel::ledger::KernelLedgerV1','misaka_palw_kernel::opv::OpvStateV1'}:
        for _,typ,_ in fs:references(typ,item)
        continue
    if fs is None or (not path.startswith('crate::') and any(not public for _,_,public in fs)):
        if path=='misaka_palw_kernel::flat_quota11::stage14::source26::StageSourcePlanRecordV26':
            for _,typ,_ in fs:references(typ,item)
        # Reviewed opaque layouts have a hand-written visitor beside their owner.
        unresolved.append(path);continue
    for _,typ,_ in fs:references(typ,item)
    cfgs=field_cfgs(item)
    names=', '.join(cfgs.get(name,'')+name for name,_,_ in fs)
    generated.append((item, f'native_struct!({path}; {names});', (not item['type_public'] or any(not public for _,_,public in fs))))
 elif kind=='struct' and form=='tuple':
    ts=[stripattrs(v) for v in split(body)]
    if not path.startswith('crate::') and any(not t.startswith('pub ') for t in ts):unresolved.append(path);continue
    for t in ts:references(re.sub(r'^pub(?:\([^)]*\))?\s+', '', t),item)
    generated.append((item, f'native_tuple_struct!({path}; '+', '.join(f'f{i}' for i in range(len(ts)))+');', (not item['type_public'] or any(not t.startswith('pub ') for t in ts))))
 elif kind=='struct': generated.append((item,f'native_unit!({path});',not item['type_public']))
 elif kind=='enum':
    arms=[]
    for part in split(body):
        part=stripattrs(part)
        m=re.match(r'(\w+)\s*(.*)',part,re.S)
        if not m:continue
        variant,rest=m.groups();rest=rest.strip()
        if rest.startswith('{'):
            vb,_=brace(rest,0);vs=[]
            for field in split(vb):
                field=stripattrs(field);fm=re.match(r'(\w+)\s*:\s*(.*)',field,re.S)
                if not fm:raise ValueError((path,part))
                fn,ft=fm.groups();references(ft,item);vs.append(fn)
            arms.append(variant+' { '+', '.join(vs)+' }')
        elif rest.startswith('('):
            vb,_=brace(rest,0);types=split(vb)
            for typ in types:references(typ,item)
            arms.append(variant+'('+', '.join(f'f{i}' for i in range(len(types)))+')')
        else: arms.append(variant)
    generated.append((item,f'native_enum!({path}; '+', '.join(arms)+');',not item['type_public']))

# Opaque public types: preserve the actual type name in every refusal. Handwritten
# private-owner visitors are intentionally omitted here.
manual={'crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1','crate::palw_funded_original_coordinator_v1::PalwFundedOriginalScheduleV1','misaka_palw_kernel::flat_quota11::stage14::source26::StageSourcePlanRecordV26','misaka_palw_panel::PanelCandidatesV1','misaka_palw_panel::PermissionlessPanelStateV1','crate::palw_kernel_route_v1::KernelLedgerCacheV1','kaspa_math::Uint512','kaspa_math::Uint576','misaka_palw_kernel::ledger::KernelLedgerV1','misaka_palw_kernel::opv::OpvStateV1'}
header='//! Generated explicit native field inventory. Regenerate with\n//! scripts/drills/gen_rfc06_native_inventory.py after schema changes.\n//! Every field/variant is exhaustive; private layouts are visited beside owners.\nuse super::*;\n'
private={}; public=[]
for item,code,is_private in generated:
    if is_private:
        private.setdefault(item['module'],[]).append(code)
    else:public.append(code.replace('crate::palw_bond_budget_v1::material::','crate::palw_bond_budget_v1::').replace('crate::palw_bond_budget_v1::consumer::','crate::palw_bond_budget_v1::'))
OUT.write_text((header+'\n'.join(public)+'\n'+ '\n'.join('native_unsupported!('+p+');' for p in sorted(set(unresolved)-manual))).rstrip()+'\n')
private_dir=OUT.parent/'private';private_dir.mkdir(exist_ok=True)
for old in private_dir.glob('*.rs'):
    if old.read_text().startswith('// Generated owner-local field visitor') and old.stem not in {m.replace('::','_') for m in private}:old.unlink()
for module,codes in private.items():
    filename=module.replace('::','_')+'.rs'
    (private_dir/filename).write_text('// Generated owner-local field visitor. Root/hash/codec unchanged.\n#[allow(unused_imports)]\nuse crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};\n'+'\n'.join(codes)+'\n')
print(f'wrote {OUT.relative_to(ROOT)}: {len(generated)} explicit types, {len(set(unresolved)-manual)} opaque/unresolved types')
print('Private owners: '+', '.join(private))
print('\n'.join(sorted(set(unresolved)-manual)))

ledger=defs['misaka_palw_kernel::ledger::KernelLedgerV1']; fs=fields(ledger)
hidden=[(name,typ) for name,typ,public in fs if not public]
bridge="\n// Generated read-only native inventory bridge; every ledger field is exhaustive.\nimpl KernelLedgerV1 {\n    pub fn native_inventory_hidden_v1(&self) -> ("+','.join('&'+typ for _,typ in hidden)+",) {\n        let Self { "+', '.join(name if not public else name+': _' for name,_,public in fs)+" } = self;\n        ("+', '.join(name for name,_ in hidden)+",)\n    }\n}\n"
bridge_path=ROOT/'misaka/palw/kernel/src/native_inventory_ledger_v1.rs'
bridge_path.write_text('use super::*;\n'+bridge)
publicfields=[name for name,_,public in fs if public]
visitor='impl NativeHeapV1 for misaka_palw_kernel::ledger::KernelLedgerV1 {\n fn native_heap_v1(&self,path:&str)->NativeCoverageV1 {\n  let mut sum=self.native_inventory_hidden_v1().native_heap_v1(path);\n'
visitor+=''.join('  sum=sum.add::<Self>(self.'+name+'.native_heap_v1(path),path);\n' for name in publicfields)
visitor+='  sum\n }\n}\n'
(OUT.parent/'kernel_ledger.rs').write_text('//! Generated matching public/private ledger visitor.\nuse super::*;\n'+visitor)

source_plan_path='misaka_palw_kernel::flat_quota11::stage14::source26::StageSourcePlanRecordV26'
if source_plan_path in visited:
 item=defs[source_plan_path];fs=fields(item)
 groups=[fs[i:i+6] for i in range(0,len(fs),6)]
 typ='('+','.join('('+','.join('&'+t for _,t,_ in group)+',)' for group in groups)+',)'
 body='('+','.join('('+','.join(n for n,_,_ in group)+',)' for group in groups)+',)'
 marker='// Generated source26 native inventory bridge; private fields remain private.'
 owner=ROOT/item['file'];text=owner.read_text();text=text.split(marker)[0].rstrip()
 text+='\n\n'+marker+'\nimpl StageSourcePlanRecordV26 {\n pub fn native_inventory_parts_v1(&self)->'+typ+' {\n let Self { '+','.join(n for n,_,_ in fs)+' }=self;\n '+body+'\n }\n}\n'
 owner.write_text(text)
