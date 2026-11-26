#!/usr/bin/env python3
"""Generate current RFC20/ADR indexes without upgrading proposal or runtime status."""
from pathlib import Path
from collections import defaultdict
import json,re,posixpath,sys
sys.dont_write_bytecode=True
D=Path(__file__).resolve().parents[1];ROOT=D.parent
def title(p):
    return next((x[2:].strip() for x in p.read_text().splitlines() if x.startswith('# ')),p.stem)
def label(s):return s.replace('|',' / ').replace('`','')
records=[]
for kind in ['rfc','adr']:
    files=sorted(p for p in (D/kind).glob('*.md') if re.match(r'\d{4}-',p.name))
    out=f'# {kind.upper()} 番号順索引\n\n[入口](README.md) · [現行RFC](../rfc/README.md)\n\n'
    out+='| 番号 | 文書 | 文書上の区分 |\n| --- | --- | --- |\n'
    for p in files:
        s=p.read_text();name=title(p);n=p.name[:4]
        m=re.search(r'文書区分：\*\*([^*]+)\*\*',s)
        status=m[1] if m else '対応RFCの設計区分を参照'
        records.append({'kind':kind,'number':n,'path':'docs/'+p.relative_to(D).as_posix(),'title':name,'status':status})
        out+=f'| {kind.upper()}-{n} | [{label(name)}]({p.name}) | {status} |\n'
    (D/kind/'INDEX.md').write_text(out)
    if kind in {'rfc','adr'}:
        groups=[('原則・実行',1,5),('乱数・モデル採用',6,7),('検証・Court・資格',8,12),('仕事・合意・経済',13,17),('保存・運用・移行',18,20)]
        out=f'# {kind.upper()} 分野別索引\n\n[番号順](INDEX.md) · [RFC設計](../rfc/README.md) · [ADR判断理由](../adr/README.md)\n\n'
        for topic,a,b in groups:
            out+='## '+topic+'\n\n'
            out+='\n'.join(f'- [{label(title(p))}]({p.name})' for p in files if a<=int(p.name[:4])<=b)+'\n\n'
    (D/kind/'TOPICS.md').write_text(out)
(D/'maintenance/decision-catalog.json').write_text(json.dumps({'schema_version':1,'records':records},ensure_ascii=False,indent=2)+'\n')
out='# 文書カタログ\n\n[総合入口](../README.md) · [現行RFC](../rfc/README.md)\n\n'
groups=defaultdict(list)
for p in sorted(D.rglob('*.md')):
    if p.name=='catalog.md':continue
    rel=p.relative_to(D);groups[rel.parts[0] if len(rel.parts)>1 else '入口'].append(p)
for topic,items in sorted(groups.items()):
    out+='## '+topic+'\n\n'
    out+='\n'.join(f'- [{p.relative_to(D).as_posix()}]({posixpath.relpath(p.as_posix(),(D/"maintenance").as_posix())})' for p in items)+'\n\n'
(D/'maintenance/catalog.md').write_text(out.rstrip()+'\n')
print(json.dumps({'rfc':sum(r['kind']=='rfc' for r in records),'adr':sum(r['kind']=='adr' for r in records)},ensure_ascii=False))
