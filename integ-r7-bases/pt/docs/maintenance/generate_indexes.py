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
records=[]
for kind in ['adr','rfc']:
 files=sorted(p for p in (D/kind).glob('*.md') if re.match(r'^\d{4}-',p.name));groups=defaultdict(list)
 index='# '+kind.upper()+' 番号順索引\n\n[入口](README.md) / [トピック別](TOPICS.md) / [番号台帳](../maintenance/numbering.md)。自動生成の探索用索引です。原文の過去のStatusを現在の稼働保証へ昇格させません。\n\n'
 index+='**画像・動画（RFC-0003）とモデル改善（RFC-0004）はmainnet後へ延期・当面無効。** [現行方針](../governance/post-mainnet-scope.md)。\n\n| 番号 | 文書 | 状態の入口 |\n| --- | --- | --- |\n'
 for p in files:
  n=p.name[:4];t=title(p);short=re.sub(r'^(?:ADR|RFC)[- ]?\d+\s*[:—-]?\s*','',t);st=status.get((kind,n),'原文と後続改定を参照');top=topic(p.name)
  records.append(dict(kind=kind,number=n,path=p.relative_to(ROOT).as_posix(),title=t,topic=top,status=st));groups[top].append((p,n,short))
  index+='| '+kind.upper()+'-'+n+' | '+ref('docs/'+kind+'/INDEX.md',p,short[:90]+('…' if len(short)>90 else ''))+' | '+st+' |\n'
 (D/kind/'INDEX.md').write_text(index)
 text='# '+kind.upper()+' トピック別索引\n\n[番号順](INDEX.md) / [入口](README.md)。分類は探索用で、採用状態・権威の順位ではありません。\n\n'
 for top,items in sorted(groups.items()):
  text+='## '+top+'\n\n'+'\n'.join('- '+ref('docs/'+kind+'/TOPICS.md',p,kind.upper()+'-'+n+' — '+short[:100]) for p,n,short in items)+'\n\n'
 (D/kind/'TOPICS.md').write_text(text)
(D/'maintenance/decision-catalog.json').write_text(json.dumps({'schema_version':1,'records':records},ensure_ascii=False,indent=2)+'\n')
files=sorted(p for p in D.rglob('*.md') if p.name!='catalog.md');groups=defaultdict(list)
for p in files:
 rel=p.relative_to(D);groups[rel.parts[0] if len(rel.parts)>1 else '(root)'].append(p)
t='# 全文書カタログ\n\n通常は [総合入口](../README.md) から目的別に読んでください。以下は全Markdownの一覧です。\n\n'
for top,items in sorted(groups.items()):t+='## '+top+'\n\n'+'\n'.join('- '+ref('docs/maintenance/catalog.md',p,p.relative_to(D).as_posix()) for p in items)+'\n\n'
(D/'maintenance/catalog.md').write_text(t)
print('primary records',len(records),'markdown catalog',len(files))
