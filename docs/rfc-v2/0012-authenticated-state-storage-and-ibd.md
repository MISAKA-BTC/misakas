# RFC-v2-0012 — 認証State・保存・Pruning・IBD

2026-11-11の後続採択: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)がHF取得可否の認定をburn停止・解除へ置換する。追加登録モデルだけを対象に、誰でもburnでPAUSED、登録者だけがburnでACTIVEへ戻す。genesis・年次fork指定は対象外。第三者取得証明・定期可用性認定・取得不能の多数決を要求しない。HF固定版/hash照合はoff-chainの実行資料同一性確認として残す。claim固有資料、再実行承認、不正反証と責任・原期限は維持する。PAUSEDは新規受付・成功Final・新規報酬確定を止める。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09公開資料・一人保証の追加境界

**適用順:** 本節と公開profileについての追加条項は、先行採択の同じ論点を更新する。公開profileの成功条件・atomic batchの規範はRFC20を正本とし、後段の2026-11-08／実行整合性改訂の説明は、矛盾しない資源・保存条件として継承する。旧署名・歴史付録の適用条件は変更しない。

[RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)／[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)の公開profileでは、モデル未保有Panelによる取得・全体検証・単独反証を、Nodeの全モデル取得へ置き換えない。[20](0020-verification-gated-final-and-adversarial-models.md)の最低1件承認、公開検査期間、QとInspectionPending、個別救済予約、客観的default、資料義務と原時計をnative stateとして復元する。本追記は現行本文への追加要件であり、以下の旧実装snapshotが増分処理へ移行済みであるとの宣言ではない。設計状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。cold起動・IBD・reorg・再起動でも未解決異議、取消、原時計、証拠保持を復元し、全モデル取得で補完しない。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

Nodeのcold start・IBD・再起動・reorg・過去court再検査でも[19](0019-model-free-node-and-bounded-evidence.md)のモデル非保有条件を守る。保存した認証付き局所証拠と状態履歴を使い、モデルの全取得やPanel workerの必須起動へ迂回しない。同期中の不足を過去Minerの新たな有罪理由にしない。

**優先関係:** 新profileに限り、本節・RFC16〜20・[ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md)・[公開裁定の方針v0.2](../design/palw/consensus/public-adjudication-policy-v0.2.md)を適用する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**原文収録元:** [旧0036](../rfc/0036-authenticated-palw-state-map-and-row-access.md)、[旧0037](../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md)。

**関連RFC:** [02 TIR・Kernel・共有Queryデータ](0002-tensor-ir-and-shared-query-data.md) / [07 Claim・EXEC・仕事量・回答会計](0007-claims-exec-and-answer-accounting.md) / [08 Claim依存・Prefix生成元・未解決責任](0008-claim-dependencies-and-liability.md) / [10 DAG合意・Gate C・Native EVM決済](0010-dag-consensus-and-native-settlement.md) / [11 Bond予算・配分・Fee同意・決済会計](0011-bond-budgets-fees-and-payments.md) / [14 Court統合契約](0014-court-master-contract.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. 状態を保存し、別Nodeで復元する

旧0036の認証付きstate map／row accessと旧0037のbounded IBD stream／世代付き保存を統合する。永続化ライブラリ、状態rootの意味、ネットワーク転送、復元完了は別の層である。

## 2. State Mapと行アクセス

存在、不在、範囲完全性、更新を認証し、table・schema・親root・branchをdomain分離する。証明サイズと検証仕事を受信前の上限に結び付ける。行APIが存在することだけで、全表処理が不要になったと主張しない。

## 3. IBD転送

受信者が途中でboundedに検査できるstreamと、実際に送信者が生成する経路を両方定義する。送信者の自己申告stream hashへの一致と、consensusが拘束するrootへの一致を区別する。

旧snapshot形式との互換、protocol negotiation、未知message、欠落・重複・順序、巨大入力、最後のroot照合を扱う。受信側だけの実装をP2P完成としない。

## 4. 世代・Pruning・責任資料

新世代の準備、検証、commit、旧世代回収を分け、中断から復旧できるようにする。claimの依存、未解決court、支払・refundの一回性、必要な原資料は、それぞれ08／11／14の保持契約に従う。

digestだけのcheckpointを、外部告発者が資料を取得できることの代わりにしない。責任が残る行や必要なbytesを保存容量目標のために無断削除しない。

## 5. 受入確認

送受信両側、虚偽state、部分stream、世代切替中断、ディスク不足、pruned IBD、再起動、reorgを確認する。30 GB等の目標は対象・負荷・保持集合付きの実測で評価し、純ライブラリの試験だけで達成を宣言しない。

---

## EXEC credit・schedule・署名記録の復元

[07](0007-claims-exec-and-answer-accounting.md)のcredit状態と[10](0010-dag-consensus-and-native-settlement.md)のschedule、
slot閉鎖、permit消費、authority、取引の受理prefixをcanonical stateへ結び付け、原子的にcommit／undoする。
有効schedule、未消費予約、取消状態、受付期限、未終了liabilityに必要な行をpruningで先に捨てない。
別NodeのIBDでも、モデル本体を取得せず、認証state・有界な局所証拠から検査できるようにする。

producerのdurable署名journalは、このchain stateのundoとは別である。
reorg、credentialの再読み込み、service再起動で同一PermitIdへの署名履歴を消さず、
同じpayloadの再送と別payloadへの再署名を区別する。
旧版／新版共存時も、一つの仕事やpermitを二重に復元・消費しない。
保存対象、上限、GC可能条件、disk failure時の停止を明示し、1 BPSを保つために記録前の署名へ省略しない。

---

## 公開profileの増分処理・期限索引・復元gate

数万modelの登録と継続claim処理では、毎blockの仕事を全登録model・全bond・全終了claimの無条件走査／全表再hashへ依存させない。更新行、参照する依存、期限到来行、抽選に必要な認証範囲と、それぞれの上限をnative契約として列挙する。claimの受付、選出母集団cut、Qへの追加／終了、資料要求・応答、InspectionPending、全体承認、失効、費用・予約解放を認証付きの増分更新へ結び付ける。索引が存在することだけで実装完了とはしない。

状態と索引は、原claim／policy／batch membership、正規化したCaseIdと共有MaterialId、申立人ごとの予約、次の手番と観測可能な提出機会、notice／hard／保持期限、残る責任を同じcanonical rootへ拘束する。索引は元行を省略・重複せず、その時点に処理すべき範囲の完全性を検査できる必要がある。期限索引の欠落やfetch失敗を「Qが空」「責任なし」「同意済み」へ変換しない。期限到来件数が一blockの予算を超える場合の有界な継続と受付backpressureも定め、後から期限を延ばして追いつく前提を置かない。

restart、reorg、pruned IBD後には、本文の原子的commit／undoに加え、各caseの取得済み範囲、供給履行と検査未完了の区別、局所証拠の再検査範囲、個別予約の使用残量、atomic batchの単一original・全leaf scope・全体hold／Refutedを復元する。shared cacheのbyte再利用で別人のcase権利を併合せず、復元で新しい要求予算・検査時計を発行しない。資料を受け取った直後に再起動した場合でも、検査完了署名や正規terminalなしにQを消さない。

正常Final、反証によるRefuted／Revoked、供給未完了によるAbort、申立人defaultによるそのcaseだけの終了を区別して再現する。最低1件承認は当該claimの全実行scopeに結び付いた選出者の承認として復元し、旧75／41のclaimや別batchの承認と混在させない。「EXEC credit・schedule・署名記録の復元」節のcredit consumerも復元後の同じ更新済みstateを読む。

| ID | 追加受入ケース | 必要な結果 |
| --- | --- | --- |
| S12-P01 | 50,000モデルと多数bondを登録し、少数行だけ変更 | 各更新・抽選・期限処理の実workを測定し、未変更全表への無条件依存がないことを確認 |
| S12-P02 | 未取得→資料到着→InspectionPendingの各境界で停止・再起動 | Q、原時計、個別予約、未完了検査を正確に復元し、早期Finalなし |
| S12-P03 | 一人の反証、他caseの棄却、同batchのcredit消費をreorgでundo／replay | case隔離、反証優先、消費の一回性を維持 |
| S12-P04 | 期限索引の欠落・重複・不完全range・未知schema | 認証／完全性検査で拒否または同期未完了。空集合へ補完しない |
| S12-P05 | atomic batchの一leaf反証・共有資料不足を別NodeのIBDで復元 | batch全体のRefuted／成功holdと個別caseの責任を一致させ、部分credit・別original吸収・予算増殖なし |
| S12-P06 | 正常Final、単独反証、不供給Abort、悪意者defaultの同一binary試験 | Nodeのモデル取得APIを禁止したまま、局所証拠と認証stateで同じ終端を再現 |

これらは有効化前の要求であり、現行state実装、50,000モデル実測、帯域・IO上限の達成を意味しない。原付録の未完了事項を黙って解消済みにしない。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0036 | 1–90 | [RFC-0036 — 認証付き PALW state map と行単位アクセス](#source-v1-0036-L00001) |
| 旧0037 | 1–89 | [RFC-0037 — IBD の有界 authenticated row stream と世代番号付き保存](#source-v1-0037-L00001) |

---

<a id="source-v1-0036-L00001"></a>

### 旧RFC-0036 / 原文1–90行

出典: [0036-authenticated-palw-state-map-and-row-access.md](../rfc/0036-authenticated-palw-state-map-and-row-access.md)。配置理由: 主題別統合。

### RFC-0036 — 認証付き PALW state map と行単位アクセス

Status: **Proposed / Review Required** (2026-10-22)。[ADR-0203](../adr/0203-authenticated-palw-state-map-and-row-access.md)。30 GB pruned node 方針（ユーザー決定 2026-10-22）の state lane が起案した。純ライブラリと dormant fence の実装だけを伴う。本番採択・pre 統合・fence の武装・配備・state root 形式の変更は承認されていない。どの network の state root・判定も変えない。

#### 1. 問題

PALW の各 collection は state root に「全行を key 順に連結した 1 本の逐次 hash」（`palw_collection_root_of_entries_v1`）として入る。1 行の存在・不在を示す最小の証明は collection 全体である（[palw_state_proof_v1.rs](../../consensus/core/src/palw_state_proof_v1.rs)、O(行数)）。したがって検証する node は全 PALW state を手元に持つ以外に方法がなく、30 GB の上限を「現在必要な state を全量保持」で超えうる。加えて state root は毎 block 全表を再 hash する（[在庫 §4 #42](../design/palw/storage/state-inventory.md)）。

必要なもの:

1. 行の **存在**（値つき）・**不在**・**範囲の完全性**・**更新（旧値 → 新 root）** を、行数に対して対数的な大きさで示す commitment。
2. table・schema version・親 state root・branch で domain 分離され、他の表・版・親・分岐の行が通らないこと。
3. 証明の大きさと検証の仕事が、受け取る前に上限で抑えられること。
4. 「取得できない」が「存在しない」に化けないこと。供給者の不在・ローカル取得失敗は sync の停止であって block の無効化ではないこと。
5. cache の追い出し・pruning・再起動が永続登録や未解決責任を失わせないこと。

暗号はハッシュ（keyed BLAKE2b）と Merkle commitment だけを使う。これは取得したデータの同一性の認証であり、計算の正しさの証明ではない（[PRINCIPLES §8](../PRINCIPLES.md)）。

#### 2. 提案 — 二分 Patricia Merkle trie と state directory

##### 2.1 鍵の符号化

生の鍵 bytes を 1 byte につき `1` + 8 bit、末尾に `0` の bit 列にする。**prefix-free**（ある鍵の符号が他の鍵の符号の真の接頭辞にならない）かつ **順序保存**（符号の bit 辞書順 = 生 bytes の辞書順、接頭辞が先）。表の型付き鍵は順序保存の bytes に写す（hash 鍵はそのまま 64 B、bond outpoint は txid 64 B ‖ index big-endian 4 B、kernel aux は table byte ‖ key）。

##### 2.2 表（table）

- 葉: `H_leaf(spec ‖ len(key) ‖ key ‖ len(value) ‖ value)`。
- 枝: `H_branch(spec ‖ prefix_len ‖ prefix(packed, 0 詰め) ‖ left ‖ right)`。枝は配下の鍵が共有する**絶対** prefix を commit し、prefix の次の bit で 0 側・1 側に分ける。形は鍵集合だけで決まる（書き込み順に依らない）。
- 空: `H_empty(spec)`。表 root: `H_table(spec ‖ count ‖ trie_root)`。
- `spec = (table_id, schema_version, max_key_len, max_value_len)` を全 node hash に入れる。domain は node 種別ごとの keyed BLAKE2b 鍵。

##### 2.3 証明

1 つの形（pruned trie を preorder で並べる: `Opaque(hash)` / `Branch(prefix)` / `Leaf(key, value)` と行数）ですべてを表す。検証者は範囲の集合を与え、次を確かめる:

- 開かれた node は親が固定した prefix（親 prefix ‖ 方向 bit）の下にある。
- **完全性**: 範囲の鍵を含みうる部分木（`subtree_may_intersect_v1`、証明者と検証者が同じ関数を使う）は `Opaque` であってはならない。
- 再計算した表 root が commit 値と一致する。

成功すれば、範囲内の任意の鍵について「値つきで存在」または「不在」が**事実**として答えられる。範囲外の鍵は `NotCovered`（未証明）であり、不在ではない。

- 存在 / 不在 = 1 点範囲。大きさ ≤ `2 · (9 · max_key_len + 1) + 1` node（hash 鍵 n 行で ≈ `2 log2 n`）。
- 範囲完全性 = 範囲の境界 2 本の路と範囲内の葉。
- **更新**: 書く鍵すべての 1 点範囲を旧 root に対して検証し、各書き込みの旧値が証明値と一致することを確かめ、pruned trie 上で挿入・削除・上書きを行って新 root を計算する（完全性により必要な node はすべて開いている。削除は兄弟の部分木を絶対 prefix のまま持ち上げる）。
- **stream**: 表全体を `[位置, end)` の chunk（各 ≤ `max_rows` 行、最後以外は ≥ 1 行）で順に読み、最後に読んだ行数が commit された count と一致することを確かめる。

##### 2.4 state level

`state_root = H_state(format_version ‖ parent_state_root ‖ branch_anchor ‖ directory_root)`。directory は同じ構造の表（`table_id = u32::MAX`）で、鍵 `table_id ‖ schema_version`（big-endian）→ 値 表 root。行の証明は header・directory の 1 点証明・表の証明の 3 つ。読み手は pin（state root と、それが chain すべき親 root・branch anchor）を持ち、別分岐・別親・別形式・別表・別版の証明を拒否する。

change set は親 header・触る表の directory 証明・表ごとの書き込みと更新証明・子の branch anchor であり、検証者は親 pin だけから子 root を計算する（表は初回の書き込みで作られ、空になっても directory から消さない）。

##### 2.5 上限

表は key / value の上限を宣言し（hash に入る。ハード上限 512 B / 1 MiB）、検証者は証明の node 数と bytes を hash の前に検査する。1 点証明・change set の上限は `AuthProofLimitsV1::for_points` で導ける。

#### 3. 行アクセス層（dormant）

- `PalwRowSourceV1`: 親 root に pin した `read_row` / `read_range`。答えは `Present` / `Absent` / `Unavailable(理由)` の 3 種で、型付き読みは unavailable を `Err` にし、決して `Ok(None)` にしない。
- `PalwRowOverlayV1` / `PalwRowChangeSetV1`: 書き込みは変更だけを持ち、旧値は pin 下の読みで固定する（読めない行への書き込みは拒否）。change set は表ごとの `RowUpdateV1` になり、そのまま 2.3 の更新証明に渡る。`inverse()` が reorg の巻き戻し。
- `PalwCachedRowSourceV1`: (root, table, key) をキーとする LRU（byte 予算）。検証済みの存在 / 不在だけを持ち、unavailable は持たない。追い出しは次の答えの取得元を変えるだけで答えは変えない。
- `PalwFullStateRowSourceV1`: 現在の全量 in-memory state への adapter。既存 fixture 8 種で行・probe・範囲・型付き読みが state の getter と同一（試験）。
- `PalwProvenRowSourceV1`: 供給者から行と証明を取り、pin で検証する pruned backend。沈黙 → `SupplierUnavailable`、虚偽 → `ProofRejected`、directory が表の不在を証明 → 全鍵 `Absent`（証明済み）。

#### 4. 採らない案

- **疎 Merkle tree（鍵の hash で配置）**: 不在・更新は示せるが、生の鍵順の範囲完全性（期限索引・prefix 範囲）を示せない。
- **整列した葉の静的平衡 Merkle tree**: 挿入・削除で葉の位置がずれ、更新が O(n) 再 hash になる。
- **Merkle treap / B-tree**: 形が書き込み履歴や優先度に依存するか、攻撃者が鍵を grind して深さを伸ばせる。Patricia trie の深さは鍵長で上から抑えられる。
- **現 root に行 index を足す**: 逐次 hash は 1 行の証明を表全体より小さくできない。
- 計算の正しさの暗号学的証明は対象外（PRINCIPLES §8）。

#### 5. 互換と有効化

- 新しい state root は**新 root 版**であり、`Params::palw_state_row_map_v1`（全 preset `None`、Some-only hash、武装は検証で拒否）の後でだけ意味を持つ。現在の root・delta・wire・fork id はバイト同一。
- 有効化には: 本 RFC と ADR の採択、旧 root からの移行規則（fence 高での一括構築と、その root を header が commit する方法）、row 証明の wire（IBD lane の stream / generation 形式と整合）、全表操作の変換（§6）、node 側 fetch / cache / 予算の統合、fence を跨ぐ drill が必要。
- 移行中も旧 root の検証は変えない。新 root を shadow で計算・比較する段階を挟める（判定に使わない）。

#### 6. 全表操作 — consensus の意味に触れるもの（別途 RFC が要る）

[在庫 §4](../design/palw/storage/state-inventory.md#4-全表操作per-block--per-decisionと分類) の 46 件のうち、R（行 lookup）と I（索引による完全範囲）は rooted 索引を足して同じ結果を返せば意味不変。次は判定や上限の意味に触れるので、本 RFC では扱わず個別に起案する:

1. **Panel draw の適格集合**（`palw_panel_v2.rs` の `palw_panel_bonds_judging_v1`、census、`draw_seats`）: 取得した勝者 1 行は「より良い候補が隠されていない」を示さない。現行規則のまま pruned 検証するには適格 bond 表の完全 stream（S）が要り、per-block の取得量は bond 数に比例する。上限を持たせるには累積 stake の認証木を O(log n) で降下する抽選への変更が要り、**抽選結果が変わる**。
2. kernel route ledger の全行 flush / 全 bond 再同期（#6〜8）、RFC-0016 の容量集計（#12）、network room / audit backlog / court pool の集計（#31, #35, #37）、reporter commitment の最大値（#24）: 増分カウンタ・最大値を rooted にする変更。結果同一性の証明と試験が条件。
3. 派生索引の全件再構築（#40）と state root の全量再 hash（#42）: 本 RFC の新 root 版と索引の rooted 化で解消する。
4. fork choice の bonds 走査（#43, #45）: 表 root を registry root として使う同値性。

#### 7. 試験（実装済み、dormant）

`consensus/core/src/palw_state_auth_map_v1/tests.rs`、`palw_state_row_access_v1/tests.rs`、`palw_state_v2/tests/row_access_v1.rs`。符号の順序保存・prefix-free（全対）、完全性判定の健全性（小宇宙で網羅）、書き込み順非依存の root、BTreeMap との一致、存在・不在・範囲・prefix 範囲、証明の大きさ上限、更新の新 root 一致、stream、state change set の子 root 一致。**拒否されなければならない敵対例**: 行の欠落（葉を hash に畳む・削除する・枝ごと畳む）、偽の不在（隣の証明・鍵の改名・行を除いた map の証明・値の改竄・空証明）、範囲の省略（短い範囲の証明を長い範囲に、2 範囲中 1 つだけ）、配置の偽装（子の入れ替え・末尾の余分・切り詰め・汚れた padding・深すぎる prefix）、上限超過（node 数・bytes・鍵長）、別表・別版・別上限、別分岐・別親・親の行・別形式・directory にない版、古い旧値・旧値の偽装・未証明の書き込み・二重書き込み・別 root、stream の chunk 飛ばし・過大主張・空 chunk・行数超過。

---

<a id="source-v1-0037-L00001"></a>

### 旧RFC-0037 / 原文1–89行

出典: [0037-ibd-bounded-row-stream-and-storage-generations.md](../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md)。配置理由: 主題別統合。

### RFC-0037 — IBD の有界 authenticated row stream と世代番号付き保存

Status: **Proposed / Review Required** (2026-10-22)。[ADR-0204](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md)。
G14 storage(pruned node ≤ 30 GB)の IBD lane。**consensus は変えない**(state root の形式・header・fork choice・抽選は不変)。
変わるのは P2P に見える転送形式だけで、新 message は登録しておらず休眠。採択・protocol version 割当・配備は未承認。
設計の詳細と現状分析は [ibd-generations.md](../design/palw/storage/ibd-generations.md)。

#### 1. 問題

pruning point の PALW state は今、carriage 全体(encoded ≤ 384 MiB、decoded ≤ 512 MiB)を spill に受け取り、
全量をメモリに decode してから header の root と照合する。chunk hash は送信者が選んだ stream root への転送完全性で、
consensus root との照合は最後の 1 回だけ。偽の state は全量の受信・decode の後でしか分からず、
受信側は「どこまでが正しいか」を途中で知る手段がない。state が大きくなるほど、受信・展開の一時領域とメモリが
管理領域の上限(IBD spill 384 MiB × 2)を圧迫する。

#### 2. 提案: preimage 先行・collection 単位で検証する row stream

root は「各 collection の 64 byte root を並べた preimage の hash」なので、root の形式を変えずに:

1. 送信者は最初に preimage(数 KiB)を送る。受信者は witness header(pruning point の selected-chain 子)の
   `palw_state_root` と照合する。**row を受け取る前に**不一致なら切断・peer 責。
2. 続いて collection ごとに `CollectionStart{label, row_count, preimage_offset}`、`Row{key, value}`×n、
   `CollectionEnd`。受信者は row を collection root の hash に逐次流しつつ capped spill へ書き、
   `CollectionEnd` で preimage[offset..offset+64] と照合。不一致はその collection で停止・peer 責。
3. offset は範囲内・重複なし、label 重複なし、宣言行数の過不足は peer 責。
4. root が record の projection にかかる collection(`gen_classes`、`tir_classes`)は全 record を送り、受信者が
   decode → projection を hash する(collection ごとの codec 表が必要。§5)。非標準 root の collection は既存の
   light-client 関数(`PalwCollectionOpeningV1::root`)で照合する。
5. 全 collection の後、受信者は spill から typed carriage を組み立て、**既存の import ゲート**
   (`restore_native_carriage_v1(expected root)` = carriage 検査・index 再構築・`assert_internal_consistency*`・全 root
   再計算)を通す。名指しされなかった collection はここで落ちる。この段を通過して初めて「検証済み」。

受信側の検証器は `consensus/core/src/palw_row_stream_v1.rs` に実装済み(wire には未接続)。

2026-10-24 の補助実装として、同 module の `palw_visit_row_spill_v1` はローカル上限
(`PalwRowSpillLimitsV1`: 総 bytes、collection 数、累積 row 数、label/key/value 長)を確保前に検査し、
既存 spill を 1 row ごとの borrowed callback で読み戻す。呼出側が保持しなければ reader は全行を累積しない。
これは framing の検査のみで、callback の途中結果は staging に留め、認証済み stream と既存の全体 import
ゲートを通過するまで昇格しない。旧全量 reader の置換、全 collection codec、typed state 組立、公開 wire、
NativeDemand による heap admission への接続は未完である。ローカル容量・allocation 拒否を peer の不正としない。

##### 2.1 何を保証し、何を保証しないか

- 保証: 受け入れた collection は header が commit した state のそれである。偽 row は当該 collection の終わりで検出され、
  何も昇格しない。spill は 1 file ≤ cap、同時 2 file。
- 保証しない: stream 単体の完全性(全 collection を名指ししたか)と全体整合性。これは第 2 段(既存ゲート)が担う。
  **root、または root に一致する stream を受け取っただけでは検証済みにしない。**
- 計算の正しさの暗号学的証明は使わない(PRINCIPLES §8)。使うのはハッシュによるデータ同一性だけ。

##### 2.2 失敗の帰責

| 事象 | 帰責 |
| --- | --- |
| preimage ≠ root、collection 不一致、行数・offset・label の違反、projection の decode 失敗 | peer(切断・別 peer で再試行) |
| spill cap・byte 予算・容量予約の拒否、local I/O | **local**(停止・再試行)。peer を罰しない、block を無効にしない |
| 供給者がいない | sync の停止。「取得できない」≠「存在しない」≠「無効」 |

#### 3. 互換性と休眠

- 新しい request/response(例: `RequestPruningPointPalwRows` / `PruningPointPalwRows`)は**新 protocol version** の
  交渉でのみ使う。両端が新 version のときだけ送る。旧 peer には従来の 106/107 carriage 転送を使い続ける。
  旧 103 leader が未知 request で接続を切る問題(`request_pruning_point_palw_state` のコメント参照)を避けるため、
  version 交渉前に新 message を送らない。
- 本 RFC の時点では message を proto に追加していない。flag(node 設定)も既定 off とし、採択まで有効化しない。
- consensus fingerprint・params id に入る値は変えない。

#### 4. 世代番号付き保存(node-local、参考)

受信した state は staging 世代に書き、第 2 段を通過した証拠からのみ `VERIFIED` marker を作り、journal の原子的
rename で active に切り替える。世代は同時に最大 2(active + previous か staging)。これは node-local で P2P にも
consensus にも見えないため本 RFC の決定対象ではないが、§2 の「何も昇格しない」の実体なので併記する
(`database/src/storage_generations/`、設計 §3.1)。

#### 5. 未決事項

1. collection ごとの codec 表(label → key/value 型、projection 関数、非標準 root)。serve 側はこれに従って row を出す。
   表の網羅性は「全 root を再計算する第 2 段」が最終的に保証するが、表の欠落は IBD 不能として現れるので
   golden test で ADR-0043 §2 の一覧と突き合わせる。
2. 第 2 段のメモリ: phase 1 は現行同様 O(state)。phase 2 は state lane の authenticated map(別 RFC)を前提に、
   全体検査を世代 DB 上の bounded-memory 集計へ移す。各 `assert_*` の分類は設計 §3.2。
3. message の大きさ・row の chunking・再開(既存 106 の resume と同様に preimage + collection index で再開可能)。
4. protocol version 番号と、既存 106/107 からの移行期間。

#### 6. 検証

- 実装済みテスト: `consensus/core/tests/ibd_row_stream_generations.rs`(stream root のバイト同一性、genesis preimage、
  途中不一致での停止と非昇格、poison、cap 超過、whole-state 拒否、検証〜切替間の crash、projection)。
- 採択後に必要: 実 PALW state の全 collection を serve → 受信 → 組立 → 既存ゲート、が現行 carriage import と同一の
  `PalwChainStateV2` を作ることの等価性テスト(t12 の pruning snapshot で)。
