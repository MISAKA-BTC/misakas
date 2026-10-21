# ADR-0196 — owner 非依存の TIR 要求と認証済み公開データ

- Status: **Proposed / Review Required** (2026-10-21)。採択・武装・配備ではない。
- Companion: [RFC-0029](../rfc/0029-owner-independent-shared-tir-query-data.md)。
- Governing decision: [ADR-0192](0192-court-service-opportunity-and-shared-pursuit.md) A/A の詳細化指示と、[RFC-0026](../rfc/0026-court-policy-master-contract.md) F02/F03。
- Candidate base: `codex/rfc14-g14-integration-candidate` / `705b27616`。隔離候補内だけの実装・検証。

## 既存 tag 245 をそのまま入口にしない理由

local `g14/door-design` の `palw_tir_designated_answers_v1` は、request key / custodian / body digest / caller-carried `at_daa` の registry である。対象 writer は `apply_tir_designated_event_v1(builder,event)` で、accepting context を受け取らず、source body・original frame・literal responder の署名を検証しない。lifecycle shape reader も当該 event に `Ok(())` を返す。予約容量・任意の外部者の開示要求・原子的 original admission まで完成したという根拠にはできない。全 preset で休眠している事実と、この不足を区別する。

この監査は old tag の意味変更や有効化を許可しない。digest/座標による重複統合と公開共有の着想を使い、署名・original source・受理時刻・全分岐予約を備えた別 version/namespace を組む。32-byte digest DTO を native `Hash64` の original scope へ黙って読み替えない。

## 比較と推奨

| 案 | 結果 |
| --- | --- |
| A — 推奨: owner 非依存の source-query 集合、独立した署名付き intent、認証済みデータ共有 | owner/relay/先着順を query identity に含めず、全合法 choice を残す。native admission / response duties / supply は別途必須 |
| B — old tag 245 の DTO を既成の native 回答として流用 | source・署名・時刻・費用の不足を閉じず、wire の意味まで変えるため採らない |
| C — owner の空席か完成済み反証だけを使う | 全 owner 共謀占有下の未完成証拠の訴追を満たさないため F03 の代替にしない |

A の隔離候補を進める。今回は in-memory の共有要求・データ処理と署名付き intent、全 source-query の資源上限成分を実装する。raw body digest だけで「回答済み」にしない。ID chunk は全列の outer-root 認証が完了するまで provisional のままにする。

## 能力と責任を混ぜない

公開 opening によって source identity を認証できることは、モデル計算の正しさ、指定 signer の応答義務履行、credit、支払いを意味しない。公開データ共有は誰でも再利用できるが、fresh adaptive response の義務を果たしたという認定には literal court signer の別の認証が必要。新しい署名方式・資金鍵の委譲・モデル単位の審査は導入しない。

`SignedSharedRequestV1` は request intent の認証だけである。state から解決した bond 鍵で検査するが、渡された query set が現在の native frame/profile であること、fee/UTXO、clock、全分岐の予約を認証する型ではない。未接続の primitive を native ingress や T05 完了と表示しない。

## 容量

全 query の数と、既存 source-unit の body/parser/hash/RAM 上限の和を、tree height と run 上限に比例する仕事で導出する。これは derived_upper の一部分であり、測定 CPU・IO・保持 byte-DAA・署名/codec/fragment・全合法対話の費用をゼロにした合計ではない。上限成分が大きいことだけで、データ再利用を含む全設計が不可能とも結論しない。

original acceptance で全許容分岐を原子的に予約できるまで本番入口を作らない。後付けの先着順 cap で共謀者に要求集合を独占させない。未導出の項は未完として残し、cap や deadline の引き上げ・モデル除外で達成にしない。

## 選択した検証結果

source/query/frontier 11、fresh t12/G14/activation/release 26、同じ fresh core executable の native 回帰30、計 **67 passed / 0 failed / 0 ignored**。non-test consensus check と既存生成器の一致も成功。[raw log・初期失敗・hash・対象範囲](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#owner-independent-shared-source-data-candidate)。署名は実 ML-DSA、登録・original は fixture。native の有料 ingress・全 owner 共謀下の T05・全分岐予約・実供給の完了ではない。

## Rooted data candidate

`b318ea946` 後に [RFC-0029 §7](../rfc/0029-owner-independent-shared-tir-query-data.md#7-rooted-native-data-candidate-after-b318ea946) の内部 native adapter を追加して検証する。候補 key/row 14/20・15/21・16/22 は末尾追加で、lifecycle は未割当。context は完全予約・timely notice・有料受付の証明ではない。raw body/全 IDs の再認証、actual accepting clock、journal と保持を接続し、署名済み応答義務の履行や default とは分ける。前節67件は過去 checkpoint の対象であり、この拡張の検証済み件数へ足さない。

Rooted adapter の選択結果は **69 passed / 0 failed / 0 ignored**（native31 + query11 + codec1 + identity26）。targeted1・過去の67/64等は加算しない。non-test consensus check と再生成一致も成功。[fixture/未完を含む記録](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#rooted-shared-data-native-adapter-candidate)。公開採択・本番入口・完全予約・T05/G14 完了ではない。

## Signed stream envelope candidate

[RFC-0029 §8](../rfc/0029-owner-independent-shared-tir-query-data.md#8-signed-bounded-stream-envelope-candidate) に、署名付き Begin/Part/Finish の候補を追加する。既存16,384-byte part 上限と ML-DSA-87 を維持する。枠は claim/scope/target/literal signer で固定し、異なる body 宣言で枠数を増やさない。全IDsは scope ごとに一つの対象とし、各 ID chunk ごとに全列を再提出しない。署名は identity のみで、Ready資格・元データ・応答義務の履行を代替しない。

この段階は codec/署名/checked geometry のみ。rooted prefix、現在のReady資格、EOF認証、rollback/再読込、完全予約・実供給・有料搬送は未接続。再試行・中断・再開を全合法分岐へ積算する規則も未採択。固定枠という構成要素だけで受付・費用・訴追可能性の完成としない。Proposed と有効化保留を維持する。

## Rooted signed-prefix candidate

[RFC-0029 §9](../rfc/0029-owner-independent-shared-tir-query-data.md#9-rooted-signed-prefix-candidate-after-33738ea26) は隔離候補に、元preparationのliteral Ready照合と固定stream行・bounded chunk行、実受理clock、EOF認証・原子的公開・復元時再認証を追加する。新しい候補key/rowは17/23・18/24、lifecycleは未割当。Begin/Partの再送は既存時刻を変えず、途中データは認証済みデータにしない。成功EOFも計算credit・応答義務完了・bond返還・defaultの効果を持たない。

Ready資格でactor集合を元契約に束縛しても、正直な署名者の可用性や外部1人の全訴追経路を証明したことにはならない。完全予約・費用・全合法restart/失敗分岐・実供給・T05と公開採択は未完。Proposed / Review Required・有効化保留を維持する。

小さなchunkのcodec検査で、既存shared/standard Arcのwire preflightにtyped heap側のalignment余裕が含まれていない不一致を検出。typed heap側・hard cap・wire/hashを変えず、preflightの必要予約を既存会計へ揃える。任意の上限引き上げやcapacity採択で試験を通す修正ではない。

## Explicit fresh data response candidate

[RFC-0029 §10](../rfc/0029-owner-independent-shared-tir-query-data.md#10-explicit-fresh-data-response-candidate-after-fa1104eed) に、共有データの存在とliteral signer自身の応答を区別する候補を追加する。署名は実受理queryのblock/DAAと認証済みデータに束縛し、別branchで再受理された同じquery IDへ移せない。要求を含む同じblockへの自己参照署名は受理せず、次のblockが同じDAAであることは妨げない。候補key/row19/25を末尾追加し、有料lifecycleは未割当。

データ未完成なら応答処理のresource起点は未確定とし、完成後はquery/Ready/dataの実受理DAAのmax+1から導出する。これは処理機会・応答期限・新たな合法義務の採択ではない。元契約の完全な有料予約・実供給・fresh署名者可用性・全semantic court dutyが揃うまで、データ応答receiptからdefault・credit・責任解放を導かない。Proposed / Review Requiredと有効化保留を維持する。
## Signed data transport derivation

[RFC-0029 section11](../rfc/0029-owner-independent-shared-tir-query-data.md#11-derived-signed-data-transport-components) derives the distinct data-control envelope extent from original custodian bounds, grouped source bounds, canonical fragments and actual Borsh codecs. It includes one FullIds stream per signer and per-query fresh responses. This closes a previously omitted byte/control component; it does not close CPU/IO/fees/retention, repeated ingress, preparation, semantic/court/archive or the full funded graph. No production parameter, signature obligation or public endpoint is adopted by this helper.
## Borrowed rows identity for shared reads

[RFC-0029 section12](../rfc/0029-owner-independent-shared-tir-query-data.md#12-query-local-rows-identity-without-per-read-id-ownership) removes unrelated IDs work from step-only reads and uses freshly authenticated borrowed IDs for trace rows identity. Whole-state loading and trace outer-root authentication remain required; no cached flag or current native-pointer shortcut grants that identity. FullIds ingestion still owns its decoded output. No measured saving, funded profile, public admission or production activation follows from this optimization.
## Nonexclusive numerical material

[RFC-0029 section13](../rfc/0029-owner-independent-shared-tir-query-data.md#13-nonexclusive-numerical-disclosure-frontier) adds a per-fixed-root numerical component that preserves every pinned child branch and the native interval/fold checks. It does not reuse a legacy session signature as shared authority or create rooted duties/clocks/funding. Canonical source/root/signature admission, all-leaf/all-actor reservation and terminal financial effects remain necessary before public F03. [RFC-0031](../rfc/0031-shared-court-initial-notice-boundary.md) and [ADR-0198](0198-shared-court-initial-notice-boundary.md) separately make the unresolved initial-notice equality reviewable; the numerical component does not choose it.
