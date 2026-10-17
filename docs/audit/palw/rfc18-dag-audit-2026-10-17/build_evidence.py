from pathlib import Path
import hashlib
import json

root = Path('/mnt/data/dag_audit_src/misakas-pre')
out = Path('/mnt/data/palw_dag_rfc18_audit')
archive = Path('/mnt/data/misakas-pre (1).zip')
S = [
('D01','RFC18の目標・siblingの要件・認証待ち時の進行',[
('docs/rfc/0018-dag-parallel-multi-model-consensus.md',29,41),
('docs/rfc/0018-dag-parallel-multi-model-consensus.md',98,127),
('docs/rfc/0018-dag-parallel-multi-model-consensus.md',191,219)]),
('D02','複数親・GHOSTDAG・実templateの接続',[
('consensus/src/processes/ghostdag/protocol.rs',244,271),
('consensus/src/processes/ghostdag/protocol.rs',303,387),
('consensus/src/pipeline/virtual_processor/processor.rs',5185,5291),
('consensus/src/pipeline/virtual_processor/processor/template.rs',853,866)]),
('D03','全体一名のproposer選出でなく各attemptの閾値判定',[
('consensus/core/src/palw_admission_v2.rs',1152,1173),
('consensus/core/src/palw_work_target_v1.rs',94,125)]),
('D04','BLUE/RED merged workの採用とstateへの反映',[
('consensus/src/pipeline/virtual_processor/processor/palw.rs',9860,9914),
('consensus/core/src/palw_state_v2.rs',14795,14935)]),
('D05','同一mergeset中のmodel round robinと、その範囲',[
('consensus/core/src/palw_rfc0018_v1.rs',291,345),
('docs/rfc/0018-dag-parallel-multi-model-consensus.md',100,117)]),
('D06','claimごとのPanel derivation / model-specific readiness',[
('consensus/src/pipeline/virtual_processor/processor/palw.rs',10460,10567)]),
('D07','適格REDの報酬は本人へ・escrowの控除',[
('consensus/src/processes/coinbase.rs',196,219),
('consensus/src/processes/coinbase.rs',313,354)]),
('D08','Gate Cのglobal poolと複数claim sweep・抽選予算',[
('consensus/core/src/palw_security_certification_v1.rs',373,446),
('consensus/core/src/palw_security_certification_fold_v1.rs',411,505)]),
('D09','共有safe frontierとoldest unresolvedの影響',[
('consensus/core/src/palw_state_v2.rs',15020,15091)]),
('D10','共有capacity / per-DAA escrow budget / bond制約',[
('consensus/core/src/palw_capacity_s567_v1.rs',61,88),
('consensus/core/src/palw_state_v2.rs',21682,21745),
('consensus/core/src/palw_rfc0016_fold_v1.rs',286,335)]),
('D11','DAG幅scheduleはstub、既存の2分設定',[
('consensus/core/src/palw_rfc0018_v1.rs',463,538),
('consensus/core/src/config/params.rs',228,260),
('consensus/core/src/config/constants.rs',75,85)]),
('D12','genesis固定operatorだけでないpermissionless anchorの接続',[
('consensus/core/src/palw_rfc0018_v1.rs',347,456),
('consensus/src/pipeline/virtual_processor/processor/palw.rs',7022,7054),
('consensus/src/pipeline/virtual_processor/processor/palw.rs',7124,7137)]),
('D13','認証creditゼロ時のGHOSTDAG比較、旧DNS gate退役',[
('consensus/core/src/palw_fork_choice_rule_e_v1.rs',826,865),
('consensus/src/pipeline/virtual_processor/palw_rule_e.rs',202,230),
('consensus/src/pipeline/virtual_processor/processor/dns.rs',148,164)]),
('D14','3モデルsibling試験が実際にassertする範囲',[
('consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs',1,58),
('consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs',92,193),
('consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs',216,303)]),
('D15','実プロセスdriverのPanelチェック不足',[
('scripts/drills/rfc0018_dag_drill.py',45,82),
('scripts/drills/rfc0018_dag_drill.py',301,371)]),
('D16','同梱の過去検証記録：今回再実行した結果ではない',[
('docs/audit/palw/design-reviews/consolidated-validation-2026-10-15.md',51,68)]),
('D17','一Panel serviceのreplay並列性とmain carrier局所制約',[
('kaspad/src/palw_panel.rs',1504,1544),
('kaspad/src/palw_panel.rs',3006,3022),
('kaspad/src/palw_panel.rs',3073,3089)]),
('D18','参考：並列とは別のFP receipt / Gate C未完了事項',[
('consensus/core/src/palw_fp_admission_v3.rs',171,198),
('docs/rfc/0018-dag-parallel-multi-model-consensus.md',350,360)]),
]
sha = hashlib.sha256(archive.read_bytes()).hexdigest()
text = ['# PALW / RFC18 DAG並列性 — 行番号付きソース証拠\n',
        f'- 対象: `{archive.name}`\n- SHA-256: `{sha}`\n',
        '- 前提: 関連fence有効時のコード分岐を評価。現testnetで無効という点を並列性の不成立理由にはしない。\n',
        '- 本書はZIP内ソースの抜粋であり、コメント中のPASS/FAILや測定値は今回の再実行結果ではない。\n',
        '- Rust/cargo・実node・実モデルの実行は行っていない。`rule_check_results.json`は限定したPython独立検査のみ。\n',
        '## 目次\n']
for no,title,parts in S:
    text.append(f'- {no}: {title}\n')
for no,title,parts in S:
    text.append(f'\n## {no} — {title}\n')
    for rel,start,end in parts:
        p=root/rel
        lines=p.read_text().splitlines()
        assert 1 <= start <= end <= len(lines), (rel,start,end,len(lines))
        text.append(f'\n### `{rel}:{start}–{end}`\n\n```text\n')
        text += [f'{n:>6} | {lines[n-1]}\n' for n in range(start,end+1)]
        text.append('```\n')
(out/'source_evidence.md').write_text(''.join(text))
(out/'manifest.json').write_text(json.dumps({'archive':archive.name,'archive_sha256':sha,
    'sections':[{'id':no,'title':title,'excerpts':[{'path':p,'first_line':a,'last_line':b} for p,a,b in parts]} for no,title,parts in S]},ensure_ascii=False,indent=2)+'\n')
print('Evidence written:',len(''.join(text)),'characters; sections=',len(S))
