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
        path='::'.join(x for x in [crate,module,name] if x)
        attrs=raw[max(raw.rfind('}',0,m.start()),raw.rfind(';',0,m.start()))+1:m.start()]
        item=dict(attrs=attrs,kind=kind,name=name,path=path,body=body,form=form,module=module,type_public=m.group(0).startswith('pub'),file=str(file.relative_to(ROOT)))
        defs[path]=item;byname.setdefault(name,[]).append(item)

primitive={'u8','u16','u32','u64','u128','usize','i8','i16','i32','i64','i128','isize','bool','f32','f64','String','Hash64','BlockHash','Hash','Hash32','Digest','()'}
containers={'Vec','Option','Box','Arc','BTreeMap','BTreeSet','HashMap','HashSet','VecDeque','IndexMap','IndexSet','SmallVec','Cow'}
queue=['crate::palw_claim_challenge_v2::PalwPaidClaimIndexV3','crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1','crate::palw_panel_source_fee_v2::PalwPanelSourceFeeAuthorityV2','crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2','crate::palw_claim_challenge_v2::PalwPaidClaimLockV2','crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2','crate::palw_claim_challenge_v2::PalwPaidClaimEvidenceV3','crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3','misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2','misaka_palw_kernel::claim_paid_v2::PaidClaimJobV2','misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2','misaka_palw_challenge::approval_scope::LifetimeCounterV3','misaka_palw_challenge::approval_scope::LifetimeReviewScopeV3','crate::palw_kernel_route_v1::KernelAssignmentV1','crate::palw_kernel_route_v1::PalwKernelChunkGroupV1','misaka_palw_kernel::receipt::PalwConstraintReceiptV1','crate::palw_onboarding_v1::ArtifactBindingRowV1','crate::palw_onboarding_v1::KernelBindingRowV1','crate::palw_onboarding_v1::ConformanceRowV1','crate::palw_onboarding_v1::ConformanceAttemptRowV1','crate::palw_conformance_evidence_v1::ConformanceEvidencePostV1','crate::palw_beacon_source_fee_v1::PalwBeaconSourceCostBoundsV1','crate::palw_panel_source_snapshot_v1::PalwPanelSourceSnapshotV1','crate::palw_state_v2::PalwClassAdmissionCarriageV2','crate::palw_state_v2::PalwStateCarriageV2','crate::palw_state_v2::PalwChainStateV2','misaka_palw_kernel::ledger::KernelLedgerV1',
       'crate::palw_panel_lifetime_v2::PalwPanelAuthorizationRowV2','crate::palw_panel_lifetime_v2::PalwPanelConsentCaptureV2','crate::palw_panel_lifetime_v2::PalwPanelLifetimeEnrollmentV2',
       'misaka_palw_kernel::panel_lifetime_v2::PanelLifetimeAuthorizationV2','misaka_palw_kernel::panel_lifetime_v2::PanelConsumerLifetimeTermsV2']; visited=set(); generated=[]; unresolved=[]

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

def fields(item):
    result=[]
    for part in split(item['body']):
        part=stripattrs(part)
        m=re.match(r'(pub(?:\([^)]*\))?\s+)?(\w+)\s*:\s*(.*)',part,re.S)
        if not m:return None
        vis,name,typ=m.groups();result.append((name,typ,vis is not None and (item['path'].startswith('crate::') or '(' not in vis)))
    return result

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
 if kind=='struct' and form=='named':
    fs=fields(item)
    if path in {'misaka_palw_kernel::ledger::KernelLedgerV1','misaka_palw_kernel::opv::OpvStateV1'}:
        for _,typ,_ in fs:references(typ,item)
        continue
    if fs is None or (not path.startswith('crate::') and any(not public for _,_,public in fs)):
        # Reviewed opaque layouts have a hand-written visitor beside their owner.
        unresolved.append(path);continue
    for _,typ,_ in fs:references(typ,item)
    names=', '.join(name for name,_,_ in fs)
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
manual={'misaka_palw_panel::PanelCandidatesV1','misaka_palw_panel::PermissionlessPanelStateV1','crate::palw_kernel_route_v1::KernelLedgerCacheV1','kaspa_math::Uint512','misaka_palw_kernel::ledger::KernelLedgerV1','misaka_palw_kernel::opv::OpvStateV1'}
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
