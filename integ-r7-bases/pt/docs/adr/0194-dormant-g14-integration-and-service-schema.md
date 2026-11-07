# ADR-0194 — 休眠 G14 統合候補と service namespace

Status: **Proposed / Review Required** (2026-10-20)。利用者は A 案の隔離候補内での実装・検証を承認済み。公開 schema 採択・pre 統合・武装・配備は未承認。

## 判断の対象

独立した候補を同じ native build で接続するため、**新しい隔離 branch の中だけで**統合と検証を進める案。`pre`、既存 source branch、公開 network、稼働中 t12 は変更しない。未知の費用・帯域・inclusion 前提や 3,120 / 10,000 を採択済みとして補わない。

- main 候補: `codex/rfc14-g14-completion` / `56f7c95a9`。service policy/book/rooted hold・recovery/custody、共有 query 集合、native/legacy node 試験、正本 RFC と利用者追加の Account Bond 文書。
- X3 候補: `codex/rfc14-x3-reorg-clock` / `0c3cb9f93`。新しい batched/de-stacked cost/clock、owner/terminal/carrier 処理、instance-local reorg 試験時計、休眠 transport 会計、Proposed filing clock V2。
- 共通祖先: `f99e86f83ec069bdd3162208226b513522d693d2`。祖先からの差分は main 165 files、X3 381 files（共有 query commit 前の観測、ログを含む）。単純な「数ファイルの merge」として扱わない。

## 実際の衝突

両 head の `consensus/core/src/palw_staged_court_state_v1.rs` を比較すると、末尾追加の Borsh enum が同じ ordinal を別用途に使う。

| Namespace | main 未採択候補 | X3 |
| --- | --- | --- |
| key tag 11 | `CourtService`（unit key） | `TirContainer(Hash64)` |
| row tag 17 | `CourtService` | `TirContainer` |

main の service format は休眠候補で production writer がない。これを理由に衝突を黙殺したり、X3 の既存 tag を service の意味へ読み替えたりしない。直接 concatenation、既存 tag の reorder、shared scanner/heap schema の片側だけの変更は禁止する。

## 選択肢

### A — 推奨: X3 namespace を保ち、service を別 version として末尾追加

新しい `codex/rfc14-g14-integration-candidate` を X3 head から作り、main の service/query 変更を選別して移植する。X3 の既存 key 0..11 / row 0..17 は保持。service の新しい候補を未使用の key 12 / row 18 として末尾に追加し、service payload version 2 と独立した profile identity にする。これは新候補内での実装・試験案で、公開 schema 採択ではない。

旧 main の未採択 service v1 snapshot を、そのまま X3/新候補の snapshot として読み込む経路を作らない。source format/chain identity を区別し、旧候補を自動 import しない。必要なら後で別仕様の明示的 offline converter を設計・承認する。新 service reader は自分の version を厳密に検査する。X3 の既存 snapshot/row/tag と production chain の互換性、source format の混同拒否を試験する。

Borsh、typed heap、allocation-free scanner、state/context/journal/carriage、deadline/hold、native writer を一緒に更新する。profile/rules/accepting context を実 state から接続し、旧 claim の処理を変えない。main 側の v1 候補は元 branch/history に残し、同じ番号を新意味へ静かに再利用しない。

### B — 別の service table/namespace へ分離

既存 staged enum を増やさず、独立した versioned service table と key domain を導入する。別 table の root、codec、journal、heap/scanner、IBD/保持を全て検証する必要がある。未使用 table ID の調査・予約と追加費用が必要で、今ここで仮 ID を置かない。

### C — 分離を維持する

統合せず、各 branch の調査・部品検証を続ける。旧 main の legacy clock の不成立を X3 全体の不可能性として扱わない。分離した green tests を同じ build の G14 完成へ合算しない。

## 隔離候補へ持ち込む範囲

A の場合も branch 全体を無条件 merge しない。main の service policy/book/context guards、hold/recovery、query universe と対応テストを X3 native API に接続する。X3 の source/cost/clock、carrier cost、signed custody、native accepting order と照合し、main の古い clock/owned 実装で上書きしない。

正本 RFC-0024/0025/0026 と Account Bond RFC-0027/ADR 文書は、元資材の bytes/digest を保って取り込む。shared index/number registry は再生成・調停する。別内容の ADR-0193（Account Bond と filing clock）は全文パスで識別し、既存主文書を勝手に改名しない。Account Bond の新規則実装・料金採択をこの統合に混ぜない。

新 branch 内でも real-network fence は休眠、record list は未承認のまま。新 profile の費用・物理供給・全合法対話 graph・hard/依存 horizon が不成立なら受付/武装を拒否し、上限・モデル範囲・identity pin を変えて成功扱いにしない。tag 230 の A/B 判断はこの ADR で代行しない。

## 統合候補の受入条件

1. X3 の既存 serialized tags、t12 identity、params/schedule IDs の保存を同じ release build で確認する。期待値を repin しない。
2. 新 version の service codec/heap/scanner/context/journal/carriage を一致させる。旧未採択 service v1 と X3 container の意味を混同しない。
3. 同じ build で native default 拒否、Final/担保/証拠 hold、回復、正当な fraud 裁定、signed custody、共有 query、実署名 relay と funded node relay/reorg を再検証する。
4. query から署名者/turn/fragment/実資源の完全な義務を導出し、original admission、block supply、owner 非依存の public ingress へ接続する。部品だけの成功で T05 としない。
5. source 別の実測・synthetic fixture・derived bound・未被覆を分けて記録する。T01–T15、R-D2-NODE、tag 230、実モデル door ≤ 3,120 の残件を省略しない。

## 承認が必要な根拠

[引き継ぎ §6](../design/palw/integration/codex-handover-court-rfc14-2026-10-20.md) は「fence の武装、統合、配備は行わず、ユーザーの承認を待つこと。」と指示している。ADR-0192 A/A の詳細仕様化の指示は統合承認ではない。この文書と source 比較までは実施し、branch を混ぜる操作はまだ行っていない。

ここで求める判断は **A を隔離候補 branch 内で実装・検証してよいか**。承認されても pre への merge、公開 schema/費用/inclusion 前提の採択、武装、配備の承認へ広げない。

## 利用者の判断 — 2026-10-20

利用者回答「A：隔離候補で実装・検証（推奨）」により A の作業を開始する。X3 の既存 tag を保持し、service は別 version / namespace とする。承認範囲は新しい専用 branch の実装・検証であり、pre 統合・武装・配備・本番規則の採択を含まない。上記の未実施・承認要求の記述は判断前の経緯である。
