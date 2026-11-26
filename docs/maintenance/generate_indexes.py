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
status[('rfc','0063')]='旧MOE1記録 / 新経済はRFC0068/ADR0236のMOE3優先、未有効化'
status[('adr','0231')]='旧MOE1判断記録 / 新経済はRFC0068/ADR0236のMOE3優先、未有効化'
status[('rfc','0064')]='旧MOE2記録 / MOE3の精度・配分・feeはRFC0068優先'
status[('adr','0232')]='旧MOE2判断記録 / MOE3はADR0236、永久LP保護を継承'
for n in ['0087','0089','0090','0091','0095','0101','0114','0124','0162','0177','0179']:
 status[('adr',n)]='MOE3経済はRFC0068/ADR0236優先 / 旧採択・実装履歴を保持'
moe2_notice='> **モデル・経済条件の最新正本。** [MOE3適用・移行](../governance/fractional-position-economics-transition.md)とRFC0068/ADR0236の最終50/20/20/5/5を適用します。同一CからMiner用POSITION購入50%、LP用POSITION購入20%、MISAKA LP20%、Panel/Gate C MISAKA5%、BFT MISAKA5%。F/P/A/Jをfloor、Dを残差、B=D+Jで70%一括購入、LP量floor(qJ/B)・Miner残差。全モデル1.0・一意市場・小数8桁・MAD2管理、通常burn5%/creator1%又は0%を維持します。同日途中の80/20/q4・W/P前段・BFT10%は置換済みです。設計採択・未native有効化。\n\n'
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
moe2_notice='> **モデル採用経路の継承。** [MAD2適用・移行](../governance/model-admission-dual-route-transition.md)のkernel自動採用/BFT YES・NOを継承します。MOE3ではgenesisを含む全モデルを同じ管理対象とし、旧GENESIS免除とforkの特別経済区分は旧profileに限定します。技術互換・個別claim検証を維持します。\n\n'+moe2_notice
status[('rfc','0017')]='旧二階層の記録 / MOE3全モデル1.0はRFC0068優先'
status[('rfc','0019')]='旧年次fork制度の記録 / MOE3は同じ追加手続'
status[('rfc','0067')]='MAD2二経路を継承 / MOE3はgenesisを含め管理対象、RFC0068優先'
status[('adr','0235')]='MAD2採用経路を継承 / origin別免除はMOE3で廃止'
status[('rfc','0053')]='旧burn停止規則 / MOE3全モデル・専用BFT ResumeはRFC0068 §2.4優先'
status[('adr','0221')]='旧追加限定burn停止 / 共通化と解除権限はADR0236優先'
status[('rfc','0068')]='ADOPTED_DESIGN_NOT_ACTIVATED / 全モデル1.0・小数POSITION・最終50/20/20/5/5'
status[('adr','0236')]='ADOPTED_DESIGN_NOT_ACTIVATED / 固定5leg・合計70%購入・取得原価比LP・creator1%又は0%'
status[('rfc','0069')]='ADOPTED_DESIGN_NOT_ACTIVATED / BMR1継続BFT5%・恒久廃止・用途別bond'
status[('adr','0237')]='ADOPTED_DESIGN_NOT_ACTIVATED / 検証bond保護・Court個別slash・廃止bountyなし'
moe2_notice='> **BFT継続報酬・恒久廃止の後続正本。** [BMR1移行](../governance/model-retirement-transition.md)とRFC0069/ADR0237を優先します。BFTは共通C分割の5%継続報酬、Panel/Gate Cは5% MISAKA。正規モデル廃止で対象LP内actual MISAKA・登録者責任bond・MINING bond・未確定mining報酬を100%burn/失効。全検証bondは不正者も一律BFTburn対象外、個人不正はCourt個別slash、実回収MISAKAの49%をPanel/Gate C合計・残差51%burn。自己slash受領と同一offence二重報酬を禁止。正当な検証報酬と他モデルを保護、用途別lot分離・重複担保/責任中用途変更禁止。BFTは没収を受け取らず廃止bounty0。旧10%と可変W/P前段は置換済みです。\n\n'+moe2_notice
status[('adr','0027')]='旧slash規則を保持 / 新個別Court49/51はRFC0069優先'
status[('adr','0032')]='旧報酬/escrow記録 / 新Panel・Gate C合計49%とburn51%はADR0237優先'
status[('rfc','0014')]='個別反証責任を継承 / 新回収分配49/51はRFC0069優先'
status[('rfc','0070')]='PROPOSED_DESIGN_NOT_ACTIVATED / MWA1モデル仕事量・FOW1報酬・EXEC credit'
status[('adr','0238')]='PROPOSED_DESIGN_NOT_ACTIVATED / 共通meter・固定target・比例原資'
existing_catalog=D/'maintenance/decision-catalog.json'
if existing_catalog.exists():
 for row in json.loads(existing_catalog.read_text()).get('records',[]):
  status.setdefault((row['kind'],row['number']),row['status'])
records=[]
for kind in ['adr','rfc']:
 files=sorted(p for p in (D/kind).glob('*.md') if re.match(r'^\d{4}-',p.name));groups=defaultdict(list)
 index='# '+kind.upper()+' 番号順索引\n\n[入口](README.md) / [トピック別](TOPICS.md) / [番号台帳](../maintenance/numbering.md)。自動生成の探索用索引です。原文の過去のStatusを現在の稼働保証へ昇格させません。\n\n'
 index+='> 新規採掘は [TB1/FOW1移行契約](../governance/palw-tb1-fow1-transition.md) を参照してください。\n\n'
 index+=moe2_notice
 index+='**画像・動画（RFC-0003）とモデル改善（RFC-0004）はmainnet後へ延期・当面無効。** [現行方針](../governance/post-mainnet-scope.md)。\n\n| 番号 | 文書 | 状態の入口 |\n| --- | --- | --- |\n'
 for p in files:
  n=p.name[:4];t=title(p);short=re.sub(r'^(?:ADR|RFC)[- ]?\d+\s*[:—-]?\s*','',t);st=status.get((kind,n),'原文と後続改定を参照');top=topic(p.name)
  records.append(dict(kind=kind,number=n,path=p.relative_to(ROOT).as_posix(),title=t,topic=top,status=st));groups[top].append((p,n,short))
  index+='| '+kind.upper()+'-'+n+' | '+ref('docs/'+kind+'/INDEX.md',p,short[:90]+('…' if len(short)>90 else ''))+' | '+st+' |\n'
 (D/kind/'INDEX.md').write_text(index)
 text='# '+kind.upper()+' トピック別索引\n\n[番号順](INDEX.md) / [入口](README.md)。分類は探索用で、採用状態・権威の順位ではありません。\n\n'
 text+='> 新規採掘は [TB1/FOW1移行契約](../governance/palw-tb1-fow1-transition.md) を参照してください。\n\n'
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
