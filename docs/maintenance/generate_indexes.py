#!/usr/bin/env python3
"""Generate indexes without inferring approval, verification, or deployment."""
from pathlib import Path
from collections import defaultdict
import re,json,posixpath
D=Path(__file__).resolve().parents[1];ROOT=D.parent

def clean(t):
 t=re.sub(r'\[([^\]]*)\]\([^)]*\)',r'\1',t)
 return re.sub(r'[*`<>]','',t).replace('|',' / ').strip()
def title(p):
 return next((clean(l[2:]) for l in p.read_text().splitlines() if l.startswith('# ')),p.stem)
def topic(n):
 if 'additional-model-option-economics' in n:return 'Economics / budgets'
 if any(k in n for k in ['design-freeze','post-mainnet','defer-media','token-name','network-isolation']):return 'Governance / release scope'
 if any(k in n for k in ['mldsa','hash64','lthash','utxo','signature','sighash']):return 'PQ / identity / UTXO'
 if any(k in n for k in ['evm','prea','wallet','deposit']):return 'EVM / wallet'
 if any(k in n for k in ['model','artifact','class','kernel','tensor','calibration','context','lineage']):return 'Models / execution / onboarding'
 if any(k in n for k in ['bond','budget','reward','coinbase','fee','escrow','econom','issuance']):return 'Economics / budgets'
 if any(k in n for k in ['verification','fraud','court','slash','challenge','panel','beacon','witness','constraint']):return 'Verification / court'
 if any(k in n for k in ['rpc','sdk','cli','operator','miner','manifest','release','network','prun','ibd']):return 'Operations / interfaces / recovery'
 return 'Consensus / architecture'
def ref(src,p,label):return '['+label+']('+posixpath.relpath(p.relative_to(ROOT).as_posix(),posixpath.dirname(src))+')'
status={('rfc','0003'):'画像・動画は延期/無効。共通部分は別',('rfc','0004'):'モデル改善はmainnet後へ延期/無効',('rfc','0016'):'採用設計・測定待ち・休眠',('rfc','0017'):'二階層採用・旧審査撤回',('rfc','0018'):'Draft / 個別採択あり',('rfc','0019'):'Draft / 原文参照',('rfc','0020'):'Draft / 部分実装',('rfc','0021'):'Accepted / 文書管理',('rfc','0022'):'Accepted / 延期・無効化方針',('adr','0179'):'Accepted / 二階層',('adr','0180'):'Accepted / BFT不使用の設計',('adr','0181'):'Accepted / 原則凍結',('adr','0182'):'Accepted / 延期・無効化方針'}
status[('adr','0215')]='Accepted design / 実装・測定・有効化は別'
status[('rfc','0048')]='Accepted design / EXEC 1 BPS・court容量、未有効化'
status[('adr','0216')]='Accepted design / EXEC・1 BPS追加採択、未有効化'
status[('rfc','0049')]='Accepted design / PALW実行整合性・単独反証、未有効化'
status[('adr','0217')]='Accepted design / PALW条件付き保証の追加採択、未有効化'
status[('rfc','0050')]='Accepted design / 公開資料・モデル未保有の一人検証、未有効化'
status[('adr','0218')]='Accepted design / 一人前提・最低一件承認の追加採択、未有効化'
status[('rfc','0055')]='前回提供の設計 / 資源保護・未有効化'
status[('adr','0223')]='前回提供の判断記録 / 未有効化'
status[('rfc','0056')]='Proposed design / 全claim受付難易度・未実装未有効化'
status[('adr','0224')]='Proposed design / CAD1の判断案・未有効化'
status[('rfc','0057')]='Proposed design / Court nonce・Panel固有challenge・未有効化'
status[('adr','0225')]='Proposed design / CPN1の判断案・未有効化'
status[('rfc','0058')]='Proposed design / 実行commit・段階抽選・合意資格、未有効化'
status[('adr','0226')]='Proposed design / SWC1判断案、未有効化'
status[('rfc','0059')]='旧AN1記録 / 新規採掘はRFC61/62へ置換、未有効化'
status[('rfc','0060')]='旧MOR1記録 / receiptとコアzを分離、新規採掘はRFC62'
status[('adr','0227')]='AN1の履歴と今回の置換 / ADR229/230優先'
status[('adr','0228')]='MOR1の履歴と今回の置換 / ADR230のコアz条件優先'
status[('rfc','0061')]='ADOPTED_DESIGN_NOT_ACTIVATED / TB1用途別乱数'
status[('rfc','0062')]='ADOPTED_DESIGN_NOT_ACTIVATED / FOW1コア出力・費用適格性'
status[('adr','0229')]='ADOPTED_DESIGN_NOT_ACTIVATED / TB1選択・移行'
status[('adr','0230')]='ADOPTED_DESIGN_NOT_ACTIVATED / FOW1登録・一回会計'
status[('rfc','0063')]='旧MOE1記録 / 新規30%買付・20%永久LPはRFC0064/ADR0232優先、未有効化'
status[('adr','0231')]='旧MOE1判断記録 / 新規30%買付・20%永久LPはRFC0064/ADR0232優先、未有効化'
status[('rfc','0064')]='ADOPTED_DESIGN_NOT_ACTIVATED / MOE2整数買付・永久流動性・余剰burn'
status[('adr','0232')]='ADOPTED_DESIGN_NOT_ACTIVATED / MOE2非償還receipt・有限終端・分類制約'
for n in ['0087','0089','0091','0095','0101','0114','0124','0177','0179']:
 status[('adr',n)]='ADOPTED_DESIGN_NOT_ACTIVATED / MOE2経済条項適用・旧採択と実装履歴は別保存'
moe2_notice='> **経済条件の後続正本。** [MOE2適用・移行契約](../governance/model-option-economics-transition.md)とRFC0064/ADR0232を優先します。追加モデルの30%買付・20%永久LP・余剰Position burn、整数残差のworker配分、追加登録時の市場原子生成、coreのR全額Miner配分を採択済み・未有効化として扱います。通常AMMの5%/3%、直接移転とTB1/FOW1の正当性・乱数・採掘適格性・Court条件は維持します。旧MOE1の50%取得・全retireと過去実装は[保存原本と改訂記録](../maintenance/revisions/2026-11-20-permanent-liquidity/README.md)で追跡し、新規MOE2のfallbackにしません。\n\n'
status[('rfc','0065')]='ADOPTED_DESIGN_NOT_ACTIVATED / MAS1構造・実用性検査・BFT安全停止'
status[('adr','0233')]='ADOPTED_DESIGN_NOT_ACTIVATED / MAS1限定停止権限・UNKNOWN閉鎖'
status[('adr','0180')]='台帳BFT禁止を維持 / ADR0233のモデル安全停止を限定追加'
mas1_notice='> **モデル採掘資格の後続正本。** [MAS1適用・移行](../governance/model-admission-safety-transition.md)とRFC0065/ADR0233を優先します。構造検査・G-USE・FOW1三gate・停止状態を共通確認し、UNKNOWNの仮承認をしません。BFTは限定された安全停止であり、費用定理や台帳finalityを置換しません。\n\n'
moe2_notice=mas1_notice+moe2_notice
status[('rfc','0066')]='ADOPTED_DESIGN_NOT_ACTIVATED / QF1 Qwen3整数実行・37-vector Readout'
status[('adr','0234')]='ADOPTED_DESIGN_NOT_ACTIVATED / QF1位置・codec固定、費用・品質未認定'
qf1_notice='> **Qwen向け具体profile。** [QF1適用・移行](../governance/qwen-fow1-profile-transition.md)とRFC0066/ADR0234が36層Qwen3の整数実行・37-vector Readoutを固定します。費用定理・G-USE・native有効化は未完で、MAS1のUNKNOWN停止を維持します。\n\n'
moe2_notice=qf1_notice+moe2_notice
status[('rfc','0067')]='ADOPTED_DESIGN_NOT_ACTIVATED / MAD2自動kernel採用又はBFT YES/NO・証明提出不要'
status[('adr','0235')]='ADOPTED_DESIGN_NOT_ACTIVATED / MAD2二経路・技術互換とclaim検証を維持'
status[('rfc','0065')]='MAS1自動審査・停止 / MAD2のBFT採用はRFC0067優先'
status[('adr','0233')]='MAS1の履歴 / BFT採用の追加はADR0235優先'
status[('adr','0180')]='台帳BFT禁止を維持 / モデル採用と停止はADR0235/0233'
moe2_notice='> **モデル採用・管理の最新規則。** [MAD2適用・移行](../governance/model-admission-dual-route-transition.md)とRFC0067/ADR0235を優先します。追加モデルは自動kernel審査又は証明提出なしのBFT YES/NOで採用し、両経路ともBFT管理対象です。元genesisだけはBFTの追加・変更・停止・解除の対象外。BFTで追加できるforkモデルはgenesis同様の経済扱いですが停止対象です。技術互換・個別claim検証は維持します。旧UNKNOWN一律停止はAUTO側と旧profileの記録として読み、BFT採用へ適用しません。\n\n'+moe2_notice
existing_catalog=D/'maintenance/decision-catalog.json'
if existing_catalog.exists():
 for row in json.loads(existing_catalog.read_text()).get('records',[]):
  status.setdefault((row['kind'],row['number']),row['status'])
records=[]
for kind in ['adr','rfc']:
 files=sorted(p for p in (D/kind).glob('*.md') if re.match(r'^\d{4}-',p.name));groups=defaultdict(list)
 index='# '+kind.upper()+' 番号順索引\n\n[入口](README.md) / [トピック別](TOPICS.md) / [番号台帳](../maintenance/numbering.md)。自動生成の探索用索引です。原文の過去のStatusを現在の稼働保証へ昇格させません。\n\n'
 index+='> **旧番号の履歴索引と後続採択。** 現行の閲覧・更新先は ['+kind.upper()+' v2](../'+kind+'-v2/README.md)。[旧→新対応表](../'+kind+'-v2/MAPPING.md)。新規採掘の後続正本は [TB1/FOW1移行契約](../governance/palw-tb1-fow1-transition.md)。RFC61/62・ADR229/230は設計採択済み・未有効化であり、旧sourceを新規受付へfallbackしない。\n\n'
 index+=moe2_notice
 index+='**画像・動画（RFC-0003）とモデル改善（RFC-0004）はmainnet後へ延期・当面無効。** [現行方針](../governance/post-mainnet-scope.md)。\n\n| 番号 | 文書 | 状態の入口 |\n| --- | --- | --- |\n'
 for p in files:
  n=p.name[:4];t=title(p);short=re.sub(r'^(?:ADR|RFC)[- ]?\d+\s*[:—-]?\s*','',t);st=status.get((kind,n),'原文と後続改定を参照');top=topic(p.name)
  records.append(dict(kind=kind,number=n,path=p.relative_to(ROOT).as_posix(),title=t,topic=top,status=st));groups[top].append((p,n,short))
  index+='| '+kind.upper()+'-'+n+' | '+ref('docs/'+kind+'/INDEX.md',p,short[:90]+('…' if len(short)>90 else ''))+' | '+st+' |\n'
 (D/kind/'INDEX.md').write_text(index)
 text='# '+kind.upper()+' トピック別索引\n\n[番号順](INDEX.md) / [入口](README.md)。分類は探索用で、採用状態・権威の順位ではありません。\n\n'
 text+='> **旧番号の履歴索引と後続採択。** 現行の閲覧・更新先は ['+kind.upper()+' v2](../'+kind+'-v2/README.md)。[旧→新対応表](../'+kind+'-v2/MAPPING.md)。新規採掘は [TB1/FOW1移行契約](../governance/palw-tb1-fow1-transition.md) を優先する。\n\n'
 text+=moe2_notice
 for top,items in sorted(groups.items()):
  text+='## '+top+'\n\n'+'\n'.join('- '+ref('docs/'+kind+'/TOPICS.md',p,kind.upper()+'-'+n+' — '+short[:100]) for p,n,short in items)+'\n\n'
 (D/kind/'TOPICS.md').write_text(text)
(D/'maintenance/decision-catalog.json').write_text(json.dumps({'schema_version':1,'records':records},ensure_ascii=False,indent=2)+'\n')
files=sorted(p for p in D.rglob('*.md') if p.name!='catalog.md');groups=defaultdict(list)
for p in files:
 rel=p.relative_to(D);groups[rel.parts[0] if len(rel.parts)>1 else '(root)'].append(p)
t='# 全文書カタログ\n\n通常は [総合入口](../README.md) から目的別に読んでください。以下は全Markdownの一覧です。新規採掘の後続採択は [TB1/FOW1移行契約](../governance/palw-tb1-fow1-transition.md) に従います。旧記録の掲載は新profileでの有効性を意味しません。\n\n'
t+=moe2_notice
for top,items in sorted(groups.items()):t+='## '+top+'\n\n'+'\n'.join('- '+ref('docs/maintenance/catalog.md',p,p.relative_to(D).as_posix()) for p in items)+'\n\n'
(D/'maintenance/catalog.md').write_text(t.rstrip()+'\n')
print('primary records',len(records),'markdown catalog',len(files))
