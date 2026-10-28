# ADR-0193: sequential witness filing の versioned clock

Status: **Proposed / Review Required** (2026-10-20)。採択・統合・武装・配備ではない。RFC-0026 の補助案。正本は source snapshot `7fe073ec2:docs/rfc/0026-court-policy-master-contract.md` にあり、この古い X3 branch へ RFC 一式を統合したものではない。

## 問題と根拠

`kaspad/src/palw_tir_public_filer_scheduler.rs` は staged witness の各 part を、現在の accepted `TirWitness.received.len()` の offset から作る。offset が受理で進むまで次の part は送らない。現行 clock version 1 の `tir_public_filing_daa_v1` は固定 2 inclusion margin と全 move の byte inclusion だけを加える。高い byte capacity はこの accepted-cursor の直列依存を解消しない。これはノードの current pipeline に対して filing allowance 単独では直列待ちを計上しないという問題であり、実測されたネットワーク遅延の報告ではない。実 plan は `max(collection_daa, filing_daa)` を用いるため、十分に長い collection window が追加項を覆う場合もある。全ての既存 plan の期限が不足すると主張するものではない。

## 選択肢

A（現時点の推奨）: **version 2 を明示して既存の直列 pipeline を覆う。** `terminal.envelope.maximum_moves` を保守的な carried-move 上限とし、`max(2, maximum_moves) × inclusion_margin` と既存の全 byte inclusion を加える。旧 version 1 の数式、serialization、plan 再導出は保持する。新 clock は既存の実 plan 導出が呼ぶため、collect/material/bond の各値と resource identity に反映する。既存 maximum door を越えた plan は拒否し、上限を広げない。

B: filer と mempool/native preflight を実際に package/prefetch 対応させ、全 part を各受理待ちなしで送り、canonical な順序で受理する。そこで初めてその pipeline の測定と故障条件に対応した別 profile を検証する。現行コードのまま、将来の batch を clock の前提に置かない。

## 候補実装と採択条件

A の候補は `palw_tir_frame_cost_v1` に実装。struct の serialized field を増減せず、明示した `clock.version = 2` で dispatch する。version 1 の既存 ID を新意味へ読み替えない。全 preset と pinned frame-cost records は変更しない。record list は空のまま、本番 profile の承認はない。version 1 の記録を version 2 へ使い回すことも一致検査で拒否する。

この候補の synthetic fixture を、network inputs・CPU/memory・inclusion capacity の実測や actual door ≤ 3,120 の証拠にしない。新 profile の actual carrier bytes、body parts、slowest supported node、入出力・保持・capital の各費用、署名済み依存 horizon、3,120 / 10,000 の規則、fair inclusion と供給会計、全 owner 共謀時の経路がそろうまで武装を拒否する。clock V2 だけで G14 完成とはしない。

## 範囲外・残件

owner queue の再入場/tenure、全許容分岐、局所化の起点と cone、grant-ready など各 carrier の実サイズ、全体の予約/通常 tx budget 分離、response deadline の補正、default attribution、終了理由ごとの保持/解除、restart/pruned IBD はこの修正で閉じない。実 pipeline を batch 化する最適化も残る。計算の正しさの暗号学的証明やモデル単位の審査は追加しない。
