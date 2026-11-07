# RFC-v2-0014 — Court統合契約 — 受付・対話・裁定・資金・責任終了

編集日: 2026-10-29 / 入力: `misakas-pre(5).zip`

[15本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

**原文収録元:** [旧0002](../rfc/0002-palw-tensor-ir.md)、[旧0007](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)、[旧0008](../rfc/0008-palw-claim-backed-consensus-blocks.md)、[旧0014](../rfc/0014-panel-independent-fraud-prosecution.md)、[旧0024](../rfc/0024-court-door-3200-to-ctx-2m.md)、[旧0025](../rfc/0025-scalable-objective-court-many-models-mass-disputes.md)、[旧0026](../rfc/0026-court-policy-master-contract.md)、[旧0028](../rfc/0028-rooted-court-response-eligibility-history.md)、[旧0030](../rfc/0030-court-end-effects-and-pending-liability-archive.md)、[旧0031](../rfc/0031-shared-court-initial-notice-boundary.md)、[旧0032](../rfc/0032-shared-court-terminal-material-and-nonblocking-ingress.md)、[旧0033](../rfc/0033-d2-readiness-root-opening-availability.md)、[旧0034](../rfc/0034-archived-close-qualified-financial-discharge.md)、[旧0035](../rfc/0035-court-original-funded-contract-and-role-consent.md)。

**関連RFC:** [02 TIR・Kernel・共有Queryデータ](0002-tensor-ir-and-shared-query-data.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [07 Claim・EXEC・仕事量・回答会計](0007-claims-exec-and-answer-accounting.md) / [08 Claim依存・Prefix生成元・未解決責任](0008-claim-dependencies-and-liability.md) / [09 Remote Client・SDK・非保管型操作](0009-remote-client-and-sdk.md) / [10 DAG合意・Gate C・Native EVM決済](0010-dag-consensus-and-native-settlement.md) / [11 Bond予算・配分・Fee同意・決済会計](0011-bond-budgets-fees-and-payments.md) / [12 認証State・保存・Pruning・IBD](0012-authenticated-state-storage-and-ibd.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. Courtに関する唯一の統合入口

旧0014、0024、0025、0026、0028、0030〜0035を一つに統合する。旧0002、0007、0008等のcourt専用条項もここへ移す。旧0026の主契約を最初に読み、公開訴追、長文脈局所化、大量court、応答資格、terminal material、終了責任、実資金を順に確認する。

TIR／共有queryのデータ意味は02、通常Panel検査は06、依存伝播は08、共通money engineは11、保存・転送は12が所有する。これらをCourt内で別規則にしない。旧0029を独立RFCとして残すこともしない。

## 2. 役割と対話の全体

producer、challenger、literal signer、材料provider、支払者、受取先、公開提出者を区別し、それぞれの受付段階で要求される署名と責任を拘束する。Panelの多数決は計算不正の最終裁定ではない。

処理の見取り図は、original受理 → notice／query → 認証済みmaterialの開示 → 局所化 → terminal提出 → nodeの決定論的裁定 → 終了効果と会計 → 保持終了である。これは理解のための流れで、新しいserialized状態名やwireを追加するものではない。

## 3. 受付時に全分岐を引き受ける

合法な全分岐について、bytes、検証work、IO、保持期間、control数、材料供給、fee、原資、担保をoriginalで確認する。資金・資源の一部だけ確保して受付成功にしない。満たせない新しい責任は受付時に拒否し、受付済みの正当な訴追を後から容量不足で捨てない。

全owner枠が共謀で占有されても、適格な外部1人が既存の公開認証データを使って追及できることを要求する。誰かがqueryを所有することと、データを利用する権利を同一視しない。

## 4. 時計を一つの表で読む

`a`はoriginal claimが実際に受理されたaccepting chain blockのDAAである。

| 項目 | 原文上の位置づけ | 読み替えてはいけないもの |
| --- | --- | --- |
| 通常notice締切 `a + 3,120` | 旧0014の2026-10-19採用方針、旧0026の主契約 | 3,200の性能目標に置換しない |
| 係争hard deadline `a + 10,000` | 異議のあるclaimについての方針。起点をリセットしない | 全担保返還・全資料削除の無条件期限ではない |
| door ≤ 3,200 | 旧0024／0025の性能設計目標 | 現行受付期限の承認済み変更ではない |
| 各手番・collect_until | versionごとの境界とcanonical順序 | 全て同じ「期限」として等号を統一しない |
| 混雑延長 | 主契約のhard deadline内の予約調整 | 署名済み期限を無制限に延長しない |

最後のnotice、最遅の合法応答、証拠搬送、裁定、復旧が残り時間に収まることを検査する。未決の数値・service条件は未決として保持し、本編集で埋めない。

## 5. 無応答と不正の区別

「期限までchainに応答がない」だけでは、当人の沈黙と配送・収録妨害を区別できない。acceptedな義務、応答権限、必要資料、予約、期限、処理機会、完了条件に基づいて帰責する。

計算不正、DA／資料義務違反、challengerの反証失敗、service不足、未解決責任を別に扱う。network不調やUnavailableを算術的有罪へ変換しない。rooted duty／eligibility historyは実際の受付点と機会を認証し、現在の行から過去の履行を推定しない。

## 6. 局所化・Material・Terminal

単一位置裁定、先読み開示、先行取得、数値fragment等は、それぞれの採択・実装範囲を保持する。hashの一致だけで内部数値foldの検査を省略しない。SC64や32 KiB close等の目標を、既存形式が既に達成した値として扱わない。

initial notice、Begin／Part／Finish、terminal intent、ready rootのdigest openingを、元のsession、query、claim、署名、受理点へ結び付ける。データ完成と義務者による有効な応答を区別し、非独占提出で完了を妨害できないようにする。

## 7. Court専用の資金と同意

旧0035の実UTXO裏付けescrow、役割別同意、原子的admissionをCourtの契約として収録する。carrier fee、名目bond exposure、将来の運用費、slash財源を混同しない。旧署名やoptional tailで新しい支払・保持契約に同意したと扱わない。

feeの保留・支払・refundの共通保存則は11、courtのどの事象がその効果を起こすかは本RFCが所有する。未採択の料金や候補値を、表を統合しただけで本番値にしない。

## 8. 判決・終了・金融責任の解消

計算有罪、反証失敗、default、service失敗、archive移行等の終了理由ごとに、claim、credit、producer／signer／provider、fee、bond、資料保持への効果を表にする。終了receiptだけで全ての別責任が消えるとはしない。

archived closeが後日数学的に有効だと判明したことと、元の期限内に義務を履行したことは別である。金融責任の解消には元の受理時刻・署名・期限・対象責任の一回性を確認する。後日完成したbitmapから元期限内の完成を逆算しない。

## 9. 保持・依存・復旧

original、採択ruleset、accepted prefix、応答資格、terminal bytes、裁定、残存責任、支払のonce markerを復元できるようにする。8と12の契約へ接続し、pruningで正当な反証を不可能にしない。別claimの責任は別に残す。

## 10. このRFC全体の確認条件

旧0026のT01〜T15を中心に、正直なminer、誤ったchallenger、永久沈黙、偽署名、全owner占有、合法tx flood、最遅notice、境界−1／一致／+1、PC停止、最短依存lease、内部fold不正、多数同時court、独立claim、reorg／restart／pruning／IBD、新旧fenceを確認する。

採用済み方針、詳細仕様化・隔離実装の承認、本番採択、値の承認、試験成功、有効化は別欄で扱う。旧0030〜0035等のDraft表記を、統合したことを理由にAcceptedへ昇格させない。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の15本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0026 | 1–194 | [RFC-0026 — court の主契約:時計・帰責・責任期間・置換関係の一本化](#source-v1-0026-L00001) |
| 旧0014 | 1–746 | [RFC-0014 — Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ](#source-v1-0014-L00001) |
| 旧0014 | 899–904 | [Mission alignment amendment — 2026-10-07](#source-v1-0014-L00899) |
| 旧0014 | 914–1043 | [改定 — 2026-10-12（固定期限と有限終了、共通経済会計：提案）](#source-v1-0014-L00914) |
| 旧0028 | 1–92 | [RFC-0028 — rooted court response eligibility history](#source-v1-0028-L00001) |
| 旧0031 | 1–47 | [RFC-0031 — 共有courtのinitial notice境界](#source-v1-0031-L00001) |
| 旧0033 | 1–32 | [RFC-0033 — D2 readiness rootのdigest opening取得](#source-v1-0033-L00001) |
| 旧0024 | 1–600 | [RFC-0024 — ctx 2M・大きいモデルでも door ≤ 3,200 DAA に収める court 設計(単一位置裁定・先読み開示・先行取得)](#source-v1-0024-L00001) |
| 旧0025 | 1–705 | [RFC-0025 — モデルが増え、不正に対する court が大量発生しても処理を継続できる客観的 court](#source-v1-0025-L00001) |
| 旧0002 | 456–504 | [5.4 Court feasibility and size caps](#source-v1-0002-L00456) |
| 旧0002 | 2296–2314 | [14.7 Bounded flat Gather courts and a real-program static regression](#source-v1-0002-L02296) |
| 旧0002 | 2358–2383 | [14.10 Native segmented roots and real delivery-court evidence](#source-v1-0002-L02358) |
| 旧0002 | 2646–2802 | [14.18 Public trace courts for unregistered scoped candidates](#source-v1-0002-L02646) |
| 旧0002 | 2855–2928 | [14.22 Staged flat-profile dependency-leaf terminal court](#source-v1-0002-L02855) |
| 旧0002 | 2981–3097 | [14.24 Staged flat-v5 all-position terminal-court resource admission](#source-v1-0002-L02981) |
| 旧0002 | 3374–3409 | [§14.32 — Mature actor-owned protected proof chunks (2026-10-11)](#source-v1-0002-L03374) |
| 旧0002 | 3516–3586 | [§14.35 — Explicit mixed-court graph scope and prepaid independent replay (2026-10-11)](#source-v1-0002-L03516) |
| 旧0002 | 3877–3905 | [§14.43 — Actual checkpoint point court and parameter continuity, v9](#source-v1-0002-L03877) |
| 旧0002 | 4060–4092 | [§14.48 — Native mixed-court conformance and bounded candidate restoration, v9](#source-v1-0002-L04060) |
| 旧0002 | 4368–4420 | [14.56 Original burst residency and the new full-court contracts](#source-v1-0002-L04368) |
| 旧0007 | 508–532 | [II.8 Failure, escalation, and the exact court](#source-v1-0007-L00508) |
| 旧0008 | 154–192 | [6. 公開検証・裁定・証拠保持](#source-v1-0008-L00154) |
| 旧0032 | 1–68 | [RFC-0032 — 共有courtの終端materialと非独占提出](#source-v1-0032-L00001) |
| 旧0035 | 1–135 | [RFC-0035 — shared court originalの実原資と役割別同意](#source-v1-0035-L00001) |
| 旧0030 | 1–170 | [RFC-0030 — court終了効果と未解決責任の保存](#source-v1-0030-L00001) |
| 旧0034 | 1–99 | [RFC-0034 — archived closeの実受付と金融責任の適格解消](#source-v1-0034-L00001) |

---

<a id="source-v1-0026-L00001"></a>

### 旧RFC-0026 / 原文1–194行

出典: [0026-court-policy-master-contract.md](../rfc/0026-court-policy-master-contract.md)。配置理由: 主題別統合。

### RFC-0026 — court の主契約:時計・帰責・責任期間・置換関係の一本化

- **状態:** Draft / Review Required。2026-10-19、RFC 全件整合性監査([0026-assets/audit-report.md](../rfc/0026-assets/audit-report.md)、指摘 F01〜F11)を受けて、ユーザー指示により作成。
- **役割:** court に関する RFC-0008・0009・0014・0018・0023・0024・0025 の記述が食い違って見える箇所について、**どの値がどの時計に当たるか、どの条項がどれを置き換えるか**を 1 か所にまとめる「current court policy」。本書は新しい値を決めない。ユーザーが採用した方針(RFC-0014 2026-10-19 改定)を主契約とし、それ以外は Draft・未決として区別する。
- **非変更:** ZK・SNARK/STARK・zkVM・PCS による計算証明は使わない(PRINCIPLES §8)。モデル単位の審査はしない(PRINCIPLES §9)。オンチェーン対話の方針は撤回しない(監査の結論)。
- **有効化:** 本書の条項はどれも休眠。§9 の未決条件(とくに P0 の F02・F03)が閉じるまで、関連 fence は武装できない。

#### 1. 監査の結論(要約)

設計の骨格は一貫している。ただし「オンチェーン対話にしたので、期限まで応答が収録されなければ安全に slash できる」という保証はまだ完成していない。11 項目は稼働中のバグの発見ではなく、RFC 自身が未確定と認めている条件、複数 RFC の統合の不整合、目標と既存形式の非両立を分けたものである。

| ID | 優先度 | 本書での扱い |
|---|---|---|
| F01 | P1 | §2 の時計表で一本化 |
| F02 | P0 | §4 に帰責の規範を置く。具体的な判定規則は未決(§9) |
| F03 | P0 | §5 に要件を置く。具体的な経路は未決(§9) |
| F04 | P1 | §6 で profile ごとの 4 役割を必須化 |
| F05 | P1 | §7.1 で SC64 を現行形式の未達目標と明記 |
| F06 | P1 | §7.2 で数値 fold 検査の維持を明記 |
| F07 | P1 | §3.3 で最大延長を受付時の契約に含める |
| F08 | P1 | §4.3 で終了理由ごとの効果表を要求 |
| F09 | P2 | §2.2 で境界表を要求 |
| F10 | P1 | §8 で実装状態を区別 |
| F11 | P2 | §8 の置換表と、各 RFC からの参照注記 |

#### 2. 時計(F01・F09)

##### 2.1 主契約

`a` を original claim の受付 DAA(merged block の header ではなく、accepting chain block の DAA。RFC-0019)とする。

| 時計 | 値 | 起点 | 根拠と強さ |
|---|---|---|---|
| 通常の申立て受付の締切 | `a + 3,120` | `a` | RFC-0014 2026-10-19 改定(ユーザー採用) |
| 係争の hard deadline | `a + 10,000` | `a`(申立てからではない。再申立てで reset しない) | 同上 |
| 各手番の応答期限 | 手番ごと。hard deadline とは別の制約 | 手番の開始 | 同上 D。値は未決 |
| door ≤ 3,200 | 対象 profile の**性能目標** | `a` | RFC-0024・0025(Draft)。主契約ではない。3,120 → 3,200 は明示承認の対象 |
| 混雑延長 `E_i` | **hard deadline の内側**での予約調整に限る:`a + 3,200 + E_i ≤ a + 10,000` | `a` | 本書の整理。RFC-0025 §4.3 の `E_max` は `≤ 6,800` に読み替える。それを超える延長は別の将来方針で、採用していない |

- 10,000 を維持するので、**10,000 の内側で処理できる負荷だけを original claim の受付で引き受ける**。受け付けた後に、正当な court だけを容量不足で捨てない。例:64 KiB × 10,000 件を court 用 25,000 B/DAA で運ぶと、転送だけで 26,215 DAA かかる(RFC-0025 の仮定による算術)。この負荷は 10,000 の内側に入らないので、受付の段階で抑制する。
- `10,000 − 3,120 = 6,880` は予算の差であり、最後に受理できる位置には等号・排他的境界の余裕を別に入れる。遅い申立て・毎回合法な最遅応答・最後の証拠・収録・回復の全部が残余に収まることを、機械的に検査する(T06)。
- 3,120・10,000 は bond 全額の無条件返還時点でも、全証拠の削除時点でもない(§3.3)。

##### 2.2 境界表(F09。値は未決、形式だけ固定)

notice、root、各 round、choice、最終 fragment、`collect_until`、hard deadline、default sweep、refund / bond exit のそれぞれについて、次を表にして実装・試験で共有する:起点、単位(合意 DAA)、受理区間(閉/開)、等号、同一 block 内の処理順(canonical ordering)、reorg 後の再計算。RFC-0014 10-12 改定の `d <= deadline` は例示であり、`collect_until` は排他的(`daa < collect_until` で受理、既存 fold も `daa >= collect_until` を拒否)。両者は別の時計なので矛盾ではないが、同じ「期限」という語で置き換えない。

#### 3. 事前予約と責任期間(F01・F07)

##### 3.1 受付時の予約

original claim を報酬対象として受け付ける前に、hard deadline までに必要な検証・開示・最終裁定・保持・回復の資源(bytes、検証 work、IO、保持 byte-DAA、control 数)と、料金・担保を予約する。指定の burst で全 claim が同時に争われる場合を評価する。確率的 overbooking は、全員共謀時の最悪保証として表示しない。

##### 3.2 混雑と過負荷

予約は合意状態から決定論的に導出し、後から来た申立てで先行の予約を動かさない。入りきらない新しい経済的責任は受付で抑制する(RFC-0025 §7.3)。既存の有効な申立ての期限・権利を消して帳尻を合わせない。

##### 3.3 署名済み依存契約の内側に収める(F07)

- 新しい original claim の受付時に、**最大の係争期間(`a + 10,000`)と必要な後続責任を、すべての依存契約(EXEC 依存 worker、provider lease、kernel liability、custody pledge)が既に覆っていること**を検査する。覆わない claim は受け付けない。
- 延長のたびに被告の新しい同意を取り直す設計にしない(被告が同意を拒んで訴追を止められるため)。最大延長は最初から署名・担保・資金に含める。署名済みの元 frame は書き換えず、versioned な effective terms を導出する(RFC-0025 §6.1)。
- RFC-0008 R8-WINDOW(依存 path の最短の署名済み horizon を越えて延長しない)は維持する。本条はその horizon を受付時に hard deadline まで取らせることで両立させる。
- bond の解放は、claim、EXEC 依存、provider、post-Final の公開告発 window(RFC-0018)、Gate C、既払・未払の費用、monetary material の保持(RFC-0008:最後の実支払 + W_mat)の**最大の必要期間**を同時に満たしてから。同じ bond が負う別 claim の責任は残る。
- 旧 claim には旧 ruleset を適用する。

#### 4. 無応答の帰責(F02、P0)

##### 4.1 規範

チェーン上の「期限まで有効な応答がない」は、次の 2 つで同じ履歴になり得る。

```text
A:Miner が応答しなかった
B:Miner は有効な応答を署名・送信したが、配送や収録を妨害された
```

署名は発言の帰属を認証するが、未収録の送信や受信時刻は認証しない。したがって、**チェーン上の不在だけから A だけを処罰し B だけを救済する判定は作れない**。無応答による default は、次の 4 つが揃ったときに限る。

1. accepted な義務、応答権限(誰の鍵か)、必要 bytes / work、予約、期限、適格な応答の完了条件が、合意状態にある。
2. 「予定された正規の処理機会」が、合意状態から決定論的に計算でき、その機会が与えられた。
3. 合意で観測できない個別の通信妨害は、明示した inclusion / network 前提として扱い、その前提の外側を保証しない。
4. 前提が崩れた場合の扱い(Final 保留、担保と証拠の保持、復旧)を、無応答 default とは別の経路として定める。

- `service_ok` のようなフラグを置くだけでは解決しない。その根拠がローカル回線測定、Panel の申告、mempool の観測なら、全ノードで同じ判定にならない。
- 有効な応答が来たときだけ時計を進める案は採らない(永久沈黙で時計も止まる)。逆に、clock や heartbeat で DAA が進むこと自体は応答機会の証明ではない(RFC-0008)。
- Miner 自身の DDoS の申告だけでは延長しない(RFC-0014 E)。

##### 4.2 DDoS 防御の層

| 層 | 対策 |
|---|---|
| 受理後の有料 court 負荷 | 容量予約と事前費用(challenger bond、少額の応答 fee、敗訴側の負担) |
| 受理前の不正 codec・重複 message・接続負荷 | 有界な受信 queue、長さ検査、署名検証前後の work 上限、peer 処理の制限。**少額 fee ではこの層を防げない** |

##### 4.3 終了理由ごとの効果(F08)

各終了理由(計算不正の確定、開示・応答義務違反、申立ての棄却、Panel の放棄、service 前提の崩壊、hard deadline 到達)について、claim の credit、court の終了 / 復旧、担保、証拠、共有 frontier、pruning への効果を別の field で表にする。

- 未解決を「解決済み」に書き換えない。期限だけで有効な Final にしない(RFC-0025 §6.3)。
- RFC-0023 段階 1(共有 frontier を保ち `L_class` で上限)と段階 2(独立進行、別 fence・未完)を区別する。段階 2 が未有効なら、その独立進行を段階 1 の可用性の説明に借りない。service 障害中も `L_class` 以内という保証は要求しない(RFC-0023 自身がネットワーク停止中の前進を保証しない)。

#### 5. 外部検証者 1 人の訴追経路(F03、P0)

- 共謀者が owner 枠(既存の上限 8)をすべて取って合法的に動き続けても、外部の正直な検証者が訴追できなければならない。「外部検証者用に 1 枠残す」だけでは、その枠を Sybil が取れるので解決しない(RFC-0025 §7.1)。
- 外部検証者が**完成した直接証拠をまだ持っていない**場合が要点である。「完成済みの直接反証はいつでも受理する」だけでは、局所化に必要な開示・対話に入れない。
- 要件:original claim の受付時に、許容する敵対的分岐を含む責任容量を予約する。争点・root・範囲・手番に対する開示を共用できるようにし、**先着した申立人に claim の訴追経路を独占させない**。完成済みの直接反証の優先経路と、まだ開示を要する適格な訴追の経路を両方示す。どこまでが保証対象の受付で、どこからが過負荷拒否かを明示する。
- 申立て fee と bond だけで、この条件が満たされたとは判定しない。未証明なら `OneOutsiderCapacityIncomplete` として武装を拒否する(RFC-0025 §7.2)。

#### 6. 署名者・資料提供者の役割(F04)

RFC-0009 の「計算と署名が終われば Miner は PC を止められる」は、既存の TIR court で root claim と各 round に Miner の claim bond の鍵の署名が要ることと、そのままでは両立しない。provider は bytes を持っていても Miner の署名を作れず、第三者 relay は作成済みの署名を運べるだけで、将来の適応的な応答を新しく署名する権限はない。

- profile ごとに、**資料を保持する者・新しい応答を署名する者・投稿を中継する者・不履行の責任者**を明記する。
- 現行の署名方式を保つなら、court の義務が終わるまで利用可能な court signer を残す。PC の完全停止も保つなら、限定された応答権限の委任、または第三者が追加署名なしで提出できる認証済みの終了証拠の経路を、別仕様として採択・検証する。資金移動用の鍵を provider に渡すことを解決策にしない。
- 追加署名なしの直接証拠や認証済みの終了証拠で閉じる profile は、別に扱ってよい。

#### 7. 証拠量の目標(F05・F06)

##### 7.1 SC64(64 KiB)は現行形式の未達目標

既存の各手番の署名(ML-DSA-87、4,627 B)を残したまま全履歴を絞ると、root + 履歴対話の署名は `1 + 2R` 個になる(tile = 16)。

| 条件 | R | 署名数 | 署名だけの bytes |
|---|---:|---:|---:|
| 32K・4 分岐 | 6 | 13 | 60,151 |
| 2M・4 分岐 | 9 | 19 | **87,913**(> 65,536) |
| 32K・16 分岐(仮定) | 3 | 7 | 32,389 |
| 2M・16 分岐(仮定) | 5 | 11 | 50,897 |

監査の読み取りでは、出荷 t12 preset の分岐数は 4(`review_heldnode_arity.rs` の期待値)。2M・4 分岐では、申立て・部分値・operand・opening・最終証拠を足す前に署名だけで 64 KiB を超える。これは算術上の下限で、実 proof の測定ではない。**SC64 は現行 profile の達成値ではなく、認証方式・投稿数を含めて別 profile として検証する目標**とする。現行 profile・16 分岐候補・先読み profile を別々に名付け、分岐数・tile 幅・context・署名数・各 body・最終証拠の最大値を 1 つの codec 集計にする。最後の約 12 KB や fragment 17,408 B を総量に流用しない。

##### 7.2 数値 fold 検査は削らない

RFC-0024 本文(B)の各 round の数値 fold 検査を、現行の規範として維持する。付録 C の commitment(hash)中心の局所化は**未採用の研究・設計候補**である。親 100・子 40 と 50 がすべて正しく認証されていても `40 + 50 ≠ 100` という集約の不正は残り、最終 leaf だけを調べる方式はこれを見落とす。採用するなら、任意の内部の親子 fold 不正を局所的に反証できる証拠と、その root への認証を定義し、完結性を示してからにする。

#### 8. 実装状態と置換表(F10・F11)

##### 8.1 実装状態の区別

| 機能 | 状態 |
|---|---|
| `palw_court_capacity_v1`(court 専用 block 予算・予約枠・期限順の割当・新規 claim 受付制限) | **別 branch の実装**(`release/t12-16000` に存在。`pre` には未統合)。休眠、値は仮。新しい提出保証が実装で閉じたとは言えない |
| 履歴 tile 二分探索・1 位置化・tag 245 登録簿 | 別 branch(`g14/door-design`)、休眠、単体試験のみ |
| X3 の固定期限・open session による Final 保留 | 別 branch `e27771b96`（起点 `05fcf7b5c`）。合法な浅い reorg の予約/session rollback と復元を release node 試験で確認。関連 suite 33 passed / R-D2-NODE 2 ignored、t12 同一性 26 passed。未統合・有効化なし。詳細は [記録](../evidence/palw/g14-completion-2026-10-20/README.md) |
| F02/F03 未完による武装拒否 | 別 branch の実装 `62ac3c55f`。readiness record による迂回を拒否。release 部品・回帰試験 29 件通過、実経路・敵対的統合試験は未完、有効化なし |
| §2/§3.3 の時計・依存 horizon 導出部品 | 別 branch の実装 `62ac3c55f`。state/fold 未接続。原子的期限更新・T06/T09 完了を意味しない |
| F02/F03 予約 scheduler 部品 | 別 branch `0336b6cea`、release 部品・回帰 32 件通過。全分岐の導出、供給会計、block admission、T04/T05 は未接続・未検証。有効化なし |
| F02 供給会計部品 | 別 branch `0ea164766`、release 部品・回帰 36 件通過。同一 DAA/skip/予約 quota を検査。state/fold/codec/snapshot、期限補正と Final hold は未接続。有効化なし |
| ADR-0194 A の隔離統合候補 | `codex/rfc14-g14-integration-candidate` の `b9c6d0656` で main/X3 の選択部分を統合。選択 joint release 93件。pre未統合・公開採択なし・有効化なし。[範囲と後続記録](../evidence/palw/g14-integration-candidate-2026-10-20/README.md) |
| rooted service v2 / native duty clock | 隔離候補 `705b27616` までで key/row 12/18・13/19、native duty/prerequisite clock、journal/保持・default拒否を接続。選択64件。実供給・原子的な全分岐受付・帰責の完成ではない |
| owner非依存の rooted query/data | 隔離候補 `b980f5788`、候補key/row 14/20〜16/22。build済み、選択69件の限定敵対的fixture検証済み（全8owner占有、原データ改変拒否、undo/reload等）。有料lifecycle/Ready応答/全分岐予約/T05は未完、有効化なし。[Draft RFC-0029](../rfc/0029-owner-independent-shared-tir-query-data.md) |
| Ready照合・永続signed stream/EOF | 隔離候補 `33738ea26` 後、候補key/row17/23・18/24。build・限定敵対的fixture検証86件、non-test check成功。[source hash・生ログ・範囲](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#rooted-signed-shared-stream-native-adapter)。共有chunkのcheckpoint再コピーとArc preflight会計不一致を修正。公開lifecycle・完全予約/fee・fresh duty・実供給・T05/G14は未完、有効化なし |
| 共有データとliteral signerの明示的応答の分離 | 隔離候補 `fa1104eed` 後、候補key/row19/25。署名を受理済みquery block/DAAと認証済みデータへ束縛し、実データ前提のresource起点とreceiptを分離。選択89件・non-test check・生成物一致成功。[範囲とhash](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#explicit-fresh-data-response-native-candidate)。data replyのみで、全semantic duty/default・完全予約/fee・実供給・node T05は未完、有効化なし |
| 本書 §2〜§7 の残りの規範 | 設計だけ。F02/F03 の選択肢は [ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md)（Proposed） |

上表の旧 branch の件数は各checkpointの履歴であり、隔離候補の件数に加算しない。「設計だけ / 別 branch 実装 / 統合済み / build 済み / 敵対的試験済み / 有効化済み」を別々に記し、設計メモ・別 branch のコード・toy test を混ぜて「完了」と表示しない。

##### 8.2 置換表

| 事項 | 現行規範(稼働 t12) | 採用済みの次期方針 | 未採用・Draft |
|---|---|---|---|
| 申立て受付 | X3 固定期限(RFC-0014 10-12 改定) | `a + 3,120`(RFC-0014 10-19) | — |
| 係争の hard deadline | X3 `hard_deadline` | `a + 10,000`、異議のある claim だけ(RFC-0014 10-19) | — |
| door | `maximum_door_daa = 3,120`(validation の上限) | — | ≤ 3,200(RFC-0024・0025、要承認) |
| 混雑延長 | なし | — | `E_i`(RFC-0025)。本書 §2.1 で hard deadline の内側に限定 |
| 証拠量 | 現行形式 | — | SC64・32 KiB close(RFC-0025、§7.1 で現行形式は未達) |
| 局所化 | 各 round の数値 fold | — | hash 中心(RFC-0024 付録 C、§7.2) |
| 無応答の帰責 | 応答義務のある側が期限超過で負け(既存仕様) | 収録機会に結びつける(RFC-0014 10-19 E) | 判定規則の具体化(§4、未決) |

旧本文は履歴として残し、各 RFC から本書を参照する注記を付ける。正式な採択には、RFC-0021 と凍結規程どおり、対応する ADR と条項の置換節が要る。

#### 9. 未決(武装を拒否する条件)

| 優先度 | 未決事項 |
|---|---|
| P0 | F02:正規の処理機会の具体的な計算、合意で観測できる service 違反の定義、inclusion / network 前提の範囲 |
| P0 | F03:全 owner 枠の共謀占有下で、未完成の局所化段階から外部 1 人が訴追できる具体的な経路 |
| P1 | F04:profile ごとの court signer の可用性(または委任・追加署名不要の終了証拠) |
| P1 | F05:SC64 を採るか、現行形式に別の上限を承認するか |
| P1 | F07:依存契約が受付時に `a + 10,000` を覆うことの検査の実装 |
| P1 | F08:終了理由ごとの効果表 |
| P2 | F09:境界表の値 |

修正の順序:F01・F02・F03 → F04・F07・F08 → F05・F06 → F09・F10・F11。期限の数値や少額 fee だけを先に変えても、提出妨害による不当な timeout 処罰は解決しない。

#### 10. 統合受入試験

監査の T01〜T15([0026-assets/audit-report.md](../rfc/0026-assets/audit-report.md) §7)を、関連 fence の有効化条件とする。主なもの:正直な Miner と誤った challenger(T01)、Miner の永久沈黙(T02)、偽署名・Panel の架空の完了申告(T03)、正常 tx flood・有料 court・不正 codec flood の分離測定(T04)、全 owner 枠の共謀占有下の外部 1 人(T05)、最後の受付時刻での notice と最遅応答(T06)、境界 −1 / 一致 / +1(T07)、Miner の PC 停止後の court(T08)、最短の依存 lease と同じ bond の複数 claim(T09)、内部 fold だけの不正(T10)、1〜10,000 件の最終段だけ不正(T11)、10,000 DAA への集中(T12)、最古 claim の service 失敗と独立 claim(T13)、reorg・restart・pruning・pruned IBD(T14)、新旧 fence 境界(T15)。

fixture での `close_cone_acquit` の軽さ(`docs/measurements/2026-10-19-court-verify-cpu.md`、`release/t12-16000`)や、中央値の小ささは、これらの最悪 case・転送・収録・復旧の代わりにならない。

#### 2026-10-21 追加隔離checkpoint — original parameter footprint / archive S1

original全numerical leafのparameter関係を登録program/layout/headerから一度の解析で導出する部品と、native時限資格＋元署名/native v5によるarchive当該declaration名目責任の一回解消を追加した。84 unique selected pass、core全tests/非test consensus lib型検査exit0。全graph・役割契約・実UTXO原資・有料公開入口・実供給/default・全金融精算/pruning・実model door・T01–T15は引き続き未完である。追加部品や選択数をこの主契約全体の達成へ置換しない。[raw logと厳密な範囲](../evidence/palw/g14-completion-2026-10-21/s1-and-original-cover.md)。

---

<a id="source-v1-0014-L00001"></a>

### 旧RFC-0014 / 原文1–746行

出典: [0014-panel-independent-fraud-prosecution.md](../rfc/0014-panel-independent-fraud-prosecution.md)。配置理由: 主題別統合。

### RFC-0014 — Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


#### 2026-10-09 登録不変性との整合

[ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)を適用する。モデルの配布者は登録済みweights・tokenizer・実行仕様・canonical artifact rootを変更する権限を持たない。MISAKA Torrent採用は後続改定で廃止する。peer、配布先、同じ内容の包装・取得経路はoff-chainの任意運用であり、合意上のavailability leaseは要求しない。新しいtransport bindingが必要な場合も、同じ固定model ID/rootへの対応を再検証し、違う内容なら別モデルとして登録する。

「新しいmodel/version」は独立した新モデルの登録を意味し、同じ登録IDへのversion追加・line/head置換を意味しない。配布不能だけで当該モデルのrewardability・weight・担保責任を変更しない。客観的計算不正への裁定は別に維持する。既存Position・AMMは改善版へ移動しない。§16の配布不介入・資本配分と、この永久不変bindingをそれぞれ検証する。

**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* **Status:** Revised Draft / implementation design, 2026-10-10後続改定。§16はモデル入手への合意不介入と拘束miner元本によるモデル別coinbase配分を定義する。旧Seeder/Torrent可用性/TRDC/FPR gateは撤回。倍率式・経済優位・実装・activationは未完了で、既存ネットワーク規則は変わらない。
* **Source baseline:** MISAKA-BTC/misakas public main `43f0bcb362d37cba79414940f3fdd368d40d3377`（2026-10-07確認）。コード上の現状と本RFCの提案を区別する。
* **Activation:** 協調したネットワーク別upgrade。提案fence名 `palw_dispute_complete_v1` は未割当であり、既存Paramsのfieldではない。DAA・object tag・serialization version・新しい署名domainはここでは割り当てない。
* **Scope:** class admission、第三者の証拠取得・局所化、seat/non-seat共通filer、court開始とFinalの競合、producer/seatへの客観的処罰。
* **Related:** RFC07 [S24]、RFC08 [S25]、RFC10 [S20]、RFC11 [S21]、RFC13 [S26]、ADR0171 [S22]、ADR0172 [S23]。
* **Deferred successor:** [RFC-0015: Panel=0への移行][R15]。本RFCのpublic non-seat prosecutionが完全に成立するまで、そのmodeは有効化禁止。
* **2026-10-07追加方針:** 旧モデル取得強制の設計は[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)で撤回した。以前の文書チェックは履歴であり、新coinbase配分の完成証拠ではない。
* **2026-10-10改定:** 旧モデル取得強制の設計は[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)で撤回した。以前の文書チェックは履歴であり、新coinbase配分の完成証拠ではない。
* **2026-10-10 FPR追加:** 旧モデル取得強制の設計は[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)で撤回した。以前の文書チェックは履歴であり、新coinbase配分の完成証拠ではない。
* **Normative language:** MUST / MUST NOTは有効化後に必要な規則、SHOULDは理由を記録して変更できる実装方針。以下の新しい型・module名は設計用の仮名で、現行Rust型の存在を意味しない。

#### 0. 採用する設計

**後続改定のscope:** モデル自体の入手・公開は合意条件にしない。以下のG14/Dispute Completenessは、
verifierが正しい登録modelを保有/取得できた条件で、claim固有証拠から裁定する能力を指す。
モデル取得を目的とする要求は適格DA/courtとせず、全weights/反復rangeの強制公開へ拡張しない（§16、ADR177 D2/D7）。
閉鎖時の実効検出と経済安全性は別に未立証であり、旧無条件の全量取得保証を意味しない。

Panelは通常時の検査・receipt集約を担う。claimの真偽をPanel多数派の信頼だけで決めない。
不正を見つけた任意の適格な第三者が、Panelへの所属やgenesis operatorの協力を必要とせず、
認証された公開データから不正位置を絞り込み、有界のexact courtまたは既存の
`ExecutorRefuted`へ到達できる仕組みを作る。

**classを報酬・work weightの対象にする前に、第三者だけでこの処罰経路を完結できる
Dispute Completenessを合意上のadmission条件にする。**
ハッシュが開くこと、producer自身がcourt evidenceを作れること、成功するcourt kernelがあることは、
独立した第三者が証拠を取得できることの代わりにならない。

この条件を完全に満たした後の固定Panel撤去は[RFC-0015][R15]で別に定義する。
**Panel外の普通のpublic bondが、producerの秘密状態を使わず、public authenticated materialだけから
不正をlocalizeして客観的convictionまで進める状態に達するまで、Panel=0を有効にしてはならない。**
本RFCの文書作成・merge、operatorのみの成功、小geometryやliar instanceを使うcourtテストは、
この前提の達成ではない。Panel=0は検証の廃止ではなく、固定Panelからpermissionless verifierと
objective fraud proofへの移行であり、lifecycle・担保・Finalを含む別upgradeを要する。

目標は次の二つに分ける。

1. **検出後の保証:** 正直なverifierが、不正を示す検証可能な不一致を期限内に発見した場合、
   producerが必要なclaim固有証拠を開示すれば客観裁定へ、開示しなければ定義されたDA/default処理へ進む。
   不正claimをPanelの多数票だけで経済的Finalに逃がさない。
2. **検出の保証:** 完全な決定的検証なら、対象範囲について不正を検出する。
   ADR0171の確率的constraint検査なら、承認済みsuiteの条件付き見逃し確率を受け入れる。
   「1人オンラインなら全ての偽claimが必ず検出される」とは書かない。

「1人がいる」は、任意の小さなPC1台が無制限の全claimを裁ける意味ではない。
そのverifierは対象classの承認済み検査・局所化を実行でき、許容claim rateを処理する資源、
必要なbond/exposureとfee、期限内のネットワーク到達性を持つ必要がある。
計算資源、DA、chain inclusion、challenge randomnessへの仮定はPanel多数派の仮定と別に明示する。

#### 1. 現行コードの到達点

以下はbaselineの確認結果であり、新仕様の実装完了の宣言ではない。

| 部分 | 現行の処理 | このRFCで閉じる不足 |
| --- | --- | --- |
| license後の客観処罰 | court conviction / ExecutorRefutedがReceiptLicensedをCourtFraudとしてvoidし、producerを処罰する [S01]–[S03] | 全対応classで第三者がその証拠を取得・作成できること |
| FalseValid | PanelFalseValidV2により署名範囲内の不正を責任追及する [S04] | 新しいconstraint scopeとterminal faultの対応 |
| court開始 | challenger bond、署名、期限などを検査する。Panel承認を条件にしない [S05] | non-seatの長い局所化をFinal前に完了させる有界の予約 |
| seat replay filer | mismatchからStepArithmetic/ExecutorRefuted、または名前付きStepLeaf要求へ進む [S06] | non-seat/operatorにも同じ証拠作成能力を提供する |
| operator自動監査 | replay mismatchを検出してDA告発する。整合したgarbage/borrowed traceはrow-0開示に答え得る [S07]–[S08] | errorログとDA告発で終わらず、計算不正の証明へ接続する |
| 8k held/fused | whole-capture cap、偽foldのprefix、committed fused tile取得にgapがある [S09]–[S10] | retained-level localizationとcommitted witness開示 |
| IR/pipeline DA | TirStepNode/TirRowNode/PipelineStepNode等の階層開示がある [S11] | 利用可能なものは再利用し、held/fusedの欠けた意味だけ拡張する |
| FinalとDA | 現行non-seat DA sessionはclaimをpauseしない [S12]。Final floor/horizonのhelperは既にある [S16] | 受理された不正追及とFinalの競合を合意規則で閉じる |
| court先取りとdirect proof | 現行ExecutorRefutedはopen courtがあるとClaimUnderSessionで拒否する [S03] | 別bondのsession存在だけで客観証拠を排除しない。共有進捗と決定的なsession清算 |

現行court terminalが成立しても、ノードのRAM上限・ネットワーク上限・証拠取得不能で
filerがそこへ到達しないならDispute Completenessは満たさない。
`ServedView`で攻撃側の実行インスタンスを代役にした成功テストは、fresh verifierの証拠取得を
証明しない [S10]。

#### 2. 安全性の定義と仮定

##### 2.1 検出後の条件付き不変条件

claim cについて、以下が全て成立する場合を対象にする。

* cのclass/plan/kernelがDisputeAdmissibleで、対象の故障が承認済み検証関係に含まれる。
* 少なくとも1人の正直なverifierが開始期限までに不一致を発見し、正しいchallengeを提出する。
* chainが進行し、適格なcarrierは宣言されたinclusion bound以内に受理される。
* producerは定義されたdeadlineまでの開示義務を持ち、開示の有無を合意上判定できる。
* collision resistance、署名、commitment bindingと、適用suiteのchallenge/soundness仮定が成立する。
* planの最悪時CPU・RAM・I/O・bytes・roundsと担保条件を満たす。

このときMUST:

~~~text
accepted mismatch pursuit
    ├─ authenticated material is served
    │      └─ bounded localization → exact verdict
    │             ├─ proven claim fraud → void + producer liability
    │             └─ claim upheld / accusation dismissed → normal lifecycle
    └─ required material is not served
           └─ objectively established DA/default → defined obligation penalty

unresolved accepted pursuit → economic Final is blocked within its bounded deadline
~~~

有罪判定や適格なDA defaultに至ったclaimはFinalへ進めない。
正直なproducerへの不正なchallengeは、有界の時間と費用で棄却され、claimの時計を無期限に延長しない。

Panel全席が悪意でも、この処理にPanelの追加署名・投票を要求しない。
ただしPanel captureへの耐性は、BlockDAG合意・ネットワーク・carrier inclusionまで
攻撃者が支配した状態への耐性を意味しない。

##### 2.2 確率的検査の残余リスク

ADR0171/RFC11の通常検査は条件付きの `epsilon_check` を持つ [S21]–[S22]。
完全検証能力を持つhonest verifierがいても、そのverifierが確率的検査だけを実行する場合、
未検出の不正がFinalする確率を0とはしない。

`Pr[undetected false claim]` の算定は、全constraintを対象にした承認済みsuite、
post-commit乱数、round transcript、adaptive/grinding/abortと繰返し回数を含めて行う。
`epsilon_check <= 2^-128` はRFC11の条件付き設計目標で、現行ネットワークの実測保証ではない。
Panel多数派が署名したという事実からこの暗号学的boundを導いてはならない。

legacyの少数step samplingは完全constraint検査ではない。
ADR0098の300-token / N=299 / k=4という過去の条件では、独立したhonest checkerが1人なら
単一点不正の検出は4/299、約1.34%にすぎない。3人の約3.96%、5人の約6.51%を、
悪意seatを含む署名数から読み替えてはならない [S19]。

##### 2.3 公開性と検出責任

claim discoveryは全ての適格producerを対象にし、genesis operatorのclaimも除外しない。
seat/non-seatで参照できるroot、DA unit、terminal courtを変えてはならない。
verifierが何も提出しなかったことは、「不正なし」という積極的な証明ではない。

一人のhonest verifierが全claimを監視できるという運用仮定を置くなら、
classごとの最大claim rate、同時追及数、検査時間、通信量、必要担保を測定する。
新しいbondを大量に作る攻撃でこの資源仮定が崩れるなら、claim admission/rateに合意上の上限を設ける。
ノードのローカルqueue上限だけでネットワークの安全性を主張しない。

#### 3. Dispute Completenessをclass admissionへ追加する

##### 3.1 提案するimmutable descriptor

新形式classは、仮称 `PalwDisputePlanV1` のdigestをclass identityとclaim bindingに含める。
旧classの意味やIDを後から書き換えない。descriptorは有限で型付きの宣言データであり、
uploadされたnative code、WASM、script、任意VMを実行させない [S23]。

~~~text
PalwDisputePlanV1 (proposed)
    version, kernel_id/version, verification_suite_id/version, court_suite_id/version
    program/model/artifact/task/context/layout identities
    commitment_layout_id, locator_template_id, witness_schema_id
    input/state/weight/output/proof relation coverage
    approved localization transitions → exact terminal kernels
    claim-specific DA/retention policy + permitted unit/cumulative scope, canonical evidence encodings
    immutable model identity + optional distribution descriptor binding (section 16)
    max rounds, total bytes, carrier bytes, checker work, RAM/storage
    cold-fetch/verify/localize/disclose/court/inclusion deadline envelopes
    soundness profile, challenge_policy_id, challenge_binding_policy_id
    public commitment/source/lock/transcript reconstruction and retention schema
~~~

registrantが記載した「complete=true」や宣言soundnessを信頼しない。
合意checkerは、既存kernel templateへの適合、全relationのcoverage、全分岐のterminal reachability、
数値範囲・layoutと最悪時資源を検査する。許可されない演算・courtが必要なら
`KERNEL_EXTENSION_REQUIRED`として待つ。

`challenge_policy_id`は[RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)のimmutable policy digestである。`challenge_binding_policy_id`はそのpolicy内の承認済みsubject binding schemaを参照し、別のbeacon/seed方式を作らない。`PalwDisputePlanV1`がpublic-prosecution planを兼ねる。actual beacon・queries・transcriptは後続の認証evidenceであり、class identityを変えない。

##### 3.2 admissionで必須とする五つの性質

1. **Binding:** job/input/output/weight/state/witnessと、その位置・寸法・kernel意味が
   一意のclaimに束縛される。別jobの本物のtraceの借用では成立しない。
2. **Independent access:** producerの秘密鍵、heap、debug state、攻撃注入instanceなしで、
   rootに認証された公開materialまたはその合意上の開示を得られる。
3. **Localization:** 承認済み検査の不一致を、宣言されたbytes/work/roundsでterminalへ落とせる。
   GKR/Freivaldsのfailureから不正leafが自動的に判明するとは仮定しない。
4. **Exact terminal:** terminalは有効化済みkernelの整数・量子化・順序規則で決定的に裁定される。
   giant model全体の緊急replayを隠れた依存にしない。
5. **Withholding closure:** 必須unitを回答しない場合も、適格なon-chain要求とdeadlineから
   DA/defaultへ進める。challengerのHTTP失敗だけでproducerをslashしない。

admissionは既存の `palw_class_admission_v2.rs` / `palw_tir_admission_v1.rs` の
資源・court検査 [S14]–[S15] と結合する。単発E2E試験だけで全入力の完全性を認定しない。
承認済みtemplateの構造的根拠と、最悪形状・境界・敵対条件の独立試験が必要である。

##### 3.3 class lifecycle

提案する `DisputeAdmissible` はadmission capabilityであり、現行claim phaseではない。
不足があるclassは登録・研究用に保持できても、当該新経路で報酬またはwork weightを得ない。

~~~text
RegisteredUnrewarded
    → semantic / coverage / dispute / resource admission
    → DisputeAdmissible
    → fixed candidate commitment + RFC07 Beacon Conformance / ConformancePassed
    → model/bond allocation snapshot + production budgets (§16; no acquisition gate)
    → ordinary readiness and licensing rules
    → reward-bearing / weight-bearing claims
~~~

登録・採掘のどちらでもmodelのdownload/seedを合意上義務付けない。classの裁定能力を検証する試験は、
検証者が正しいmodelを得られた条件で実施する。claim固有証拠のscopeとFinal/court条件は維持する。

現行の「held」は計算・保持形式にも使われるため、「安全審査待ち」の意味として流用しない。
既存reward classをこの文書だけで停止しない。移行判断は§11の別upgrade規則で行う。

##### 3.4 G14に含めるpost-commit challenge再現性（2026-10-08）

新しいreward-bearing profileは、RFC07 Part VIのpolicyとRFC11 §17の三段階をbindする。
semantic/constraint/court/public-material/resource admission、Beacon Conformance、Active Eligibilityを分ける。
beacon samplingで未知演算を承認せず、ConformancePassedだけでG14や報酬資格を成立させない。

fresh non-Panel public bondは公開chain/materialから次の全経路を再現できなければならない。

```text
class / Kernel / verification / dispute / challenge policy resolve
-> canonical statement commitment and its position
-> future source eligibility / independent Final / settlement evidence
-> canonical ordered source set and branch-relative lock
-> RFC07 seed, identical queries/vectors and every interactive transcript round
-> authenticated mismatch localization -> exact court / appropriate DA default
```

producerの内部cache、非公開API、Panelのseed配布、operator署名を使わない。seed・epsilonをexporterから信頼せず再計算する。
original claim/sliceチェックの再現は元のsubject/seedを使い、`PUBLIC_PROSECUTION`で有利な別drawへ切り替えない。
追加検査はRFC07の明示的なsupplemental commitment/coverage規則に従う。有効な既存exact proofの提出に新beacon待ちを要求しない。

probabilistic mismatchはconvictionではない。statement/source/transcriptのbindingを確認し、有界の認証witnessへlocalizeし、
exact terminalから客観的convictionへ進む。未開示は適格なDA/default、honest claim/誤challengeは棄却で区別する。
同じseedを多数watcherが検査しても独立repetitionとしてepsilonを人数分掛け合わせない。

post-commit順序、qualifying future work、非循環source、grinding/retry/withholding/reorgのbounds、公開のquery/transcript再現と
exact escalationを、全許可profileについてG14 evidenceに追加する。cold verifier、restart/IBD/pruning、changed roots、偽source、
source不足、GKR後出しmessage、Final阻止・回収まで実nodeで試す。元の実装結果をこの追加gateの合格と扱わない。
source不足ではcandidate/verificationをpending/既定deadline扱いにし、heartbeat・BASE-0・取引・chainを止めない。
no BFT/DNS beacon、no seed timeout fallback、no新finality overlayを維持する。

#### 4. 階層commitmentと独立した局所化

##### 4.1 認証するtree

意味上はclaim → stage/segment → retained block → step/kernel transitionへ降りる。
入力・重み・entry/exit state・output/proof relationのrootもclaim bindingに含める。
既存bindingにあるrootは再利用し、無関係な第二のexecution rootを作らない。

IRの `TirStepNode` / `TirRowNode`、pipelineの `PipelineStepNode` / leaf開示を
使用できるfamilyでは再利用する [S11]。held foldには、そのretained levelで開示できる
認証frontier/nodeを追加する。leaf数・padding・level/index・child ordering・domain separationは
kernel/commitment versionで固定する。

##### 4.2 比較と降下

challengerは自分の正しい計算・承認済み検査と、producerがcommitした認証nodeを比較する。
不一致nodeだけを降下し、名前付きrange/leaf/primitiveへ到達する。
served foldを「正しい計算で再生成してから照合」することを、偽foldの読み出し条件にしない。

原点のinput/entry stateと区間境界のstate continuityも対象にする。
偽checkpointから両者が同じ誤りを再現することを正しさとしない。
後段の計算が正しい場合は、最初の虚偽boundaryまたはそれを生成したrelationを裁く。

locatorがproducer全traceのdense展開や、retained treeにないflat prefixの再生成を必要とする場合、
そのplanは資源admissionを通さない。root/leafの比較だけなら階層的な降下が可能でも、
独立検査・witness生成の全コストが対数になるとは主張しない。

##### 4.3 保持するmaterial

全dense traceを全verifierのRAMに保持することは要求しない。
ただしproducer/DA提供者は、宣言したretention期間、committed tile・認証boundary・court operandを
回答できなければならない。hash-only foldは必要なpreimageの可用性を代替しない。

disk-backed/streaming/chunked実装を許す。再計算でwitnessを復元する場合は、
commitした実データをdeadline内に再現できる必要がある。
攻撃側の「間違った実行方法」を知るfresh verifierによる再生成を期待しない。

#### 5. held/fusedのDA gapを閉じる

##### 5.1 retrievalとadjudicationを分離する

現行のfused `StepLeaf` は `DaUnitNeedsDissection` で拒否される [S09], [S12]。
これを単純に拒否解除して、巨大fused計算をone-stepで再実行させてはならない。

提案unit `CommittedKernelWitness` はまず「何をcommitしたか」を有界に開示する。
既存enumのtag・古いobjectの意味は保持し、追加variant/encodingは別fence以降に限定する。

~~~text
CommittedKernelWitness request (proposed)
    claim_id, dispute_plan_id, kernel_site, witness_part_index

CommittedKernelWitness answer (proposed)
    committed output tile / witness preimage
    membership opening to the claim's bound roots
    kernel coordinate, shape, layout and entry-state references
    authenticated query / state / operand references
    deterministic dissection root and bounded witness-part commitments
~~~

query/operand全体が大きい場合は、bounded partとそのroot、既存held dissectionへの参照に分ける。
単一tileの数KB〜数MBというサイズを一般保証しない。
全sessionのbytes・CPU・RAM・roundsと最大carrierを別々に検査する。

##### 5.2 回答の処理

* root/位置/jobに一致しないbytesは適格回答にならない。未回答義務は所定deadlineまで残る。
* 正しいmembershipを持つ回答はDA義務を満たし、計算の正しさはexact courtへ渡す。
* bounded one-moveで裁けるものは既存の束縛された判定 [S13] を再利用する。
* fusedでdissectionが必要なら、開示済みcommitted tileと認証operandを起点に既存held courtへ進む。
* 回答がない場合は客観的なDA/defaultを処理する。計算fraudの証明として記録しない。

commitされた不正が問題のsiteにあり、正直なchallengerがそこへ到達した場合には、
producerは「偽tileを開示してexact裁定を受ける」か「必須tileを開示せずDA/defaultになる」。
正しいtileを回答した場合は棄却する。この二択を全siteが不正だという意味に拡張しない。

##### 5.3 exact semantics

fused attentionの分解は、元kernelの丸め・saturation・整数範囲・accumulation順序を維持する。
非結合的な演算を分割したせいでhonest outputと違う値になるdissectionは禁止する。
input・weight・state opening、位置とshapeをclaim bindingまで検証する。
kernelが未対応、またはterminalが上限内に収まらないなら、そのplanはDisputeAdmissibleにしない。

#### 6. seat/non-seat共通のfraud filer

##### 6.1 仮称 palw_fraud_filer

node policyの共通engineを設け、Panel seat、genesis operator、通常bond holder、
外部watchdogが同じ証拠取得・局所化・提出builderを呼ぶ。

~~~text
Discover → ResolvePlan → Check
    → Mismatch → ReservePursuit → Localize
    → RequestCommittedWitness → JudgeExact
    → SubmitExecutorRefuted / PlayBoundCourt
    → Convicted / Dismissed / DA default / Expired
~~~

Mismatchはローカル状態、ReservePursuit以降の受理・deadlineは合意状態である。
ローカルchecker failureやresource refusalをon-chain fraudと取り違えない。

engineはpublic claim/material/dispute read、capability/resource ledger、transport/carrier builder、
既存court/conviction builderへのadapterを受け取る。
roleごとに変えてよいのはclaim discovery・優先順位・ローカルbudgetであり、
証拠の有効性・terminalへ進める権限・root情報の公開性ではない。

##### 6.2 現行moduleからの接続

1. `palw_filer_replay.rs`のclaim mismatch、名前付きleaf、capture候補、conviction doorをadapterに分離する。
2. `palw_operator_da.rs`のRefuted verdictを同じengineへ渡す。
   一貫した偽traceをrow-0 DA告発だけで処理済みにしない。
3. operator向けのproducer/signer filterを、engineの許可条件にしない。
   任意の適格producerのclaimをpublic readから追及できるようにする。
4. witness取得、binder/job認証、held/TIR/pipeline terminalのfamily adapterを共通契約へ接続する。
5. legacy DAの開示要求は独立機能として保持する。計算不正と結びつかないDA要求を、
   自動的にFalseValidのbasisへ変換しない。

fee払いcarrierの既存ObjectiveOffence提出が許される範囲は狭めない。
新しいbond/depositが必要なのは、相手とchainに開示・予約・対話資源を消費させる経路である。
direct proofとinteractive challengeのentry条件を明確に分ける。

##### 6.3 node/RPCと再起動

public readはclass/plan identity、binding/root、retention、開始/Final deadline、
未処理要求、answer参照、pending court、既存convictionを返す。
秘密のoperator allowlistを知ることを証拠取得条件にしない。

nodeはclaim/site/plan別に進捗、material digest、提出carrier、deadlineを保存し、
再起動時はcanonical chainからsessionを再照合する。
reorgで無効になったopeningやreceiptを新しいrootの証拠として再利用しない。
同じ有効proofを再送してもproducerを二重徴収しない。退出・担保開放とconvictionの順序もstate foldで固定する。

#### 7. non-seatの追及予約とFinal

##### 7.1 現行との差分

現行R-core+のnon-seat DA sessionはclaimをpauseしない [S12]。
このままでは、外部verifierが局所化中にchallenge windowが終わり得る。
本RFCは、適格な外部追及についてbounded reservationを合意状態として追加する。
これはnode policy変更だけでは達成できない。

##### 7.2 仮称 PalwDisputeReservationV1

開始条件はbond資格、deposit/exposure、claim/plan/siteのbinding、retention、
開始cutoff、session数と合計資源の上限、署名・feeを含む。
初期mismatchをまだexact proofにできないケースも対象にするため、開くこと自体を有罪認定にしない。

最初の受理でclaim単位のabsolute `dispute_hard_deadline` を固定する。
別bondの新session、再送、同unitの重複要求、席交代で時計を再開始しない。
同じsite/unitは共通回答を再利用し、answered unitへの新しい資源要求は既存同様に拒否する。
複数追及は同時数・総bytes/work・総延長に合意上のcapを設ける。

開始には次の余裕がMUST必要である。

~~~text
start + B_cold_fetch + B_check + B_localize + B_disclose
      + B_court + B_carrier + B_reorg_slack
    <= dispute_hard_deadline
~~~

**2026-10-18 ユーザー決定（窓の合成）:** `dispute_hard_deadline` の項は次で定める。`T_prepare = window_bind + window_receipt`、
`T_observe = window_challenge_at(activation)`、`T_resolve = B_disclose + B_court + Δ`、`Δ = B_cold_fetch + B_check + B_localize + B_carrier + B_reorg_slack`、
`hard = accepted_daa + T_prepare + T_observe + T_resolve`。窓は activation で `Params` から読み直し、食い違えば refuse する（`palw_x3_composition_refusal_v1`）。
Δ の各項と `c_retain` は policy の名前付き入力で、0 は未測定として拒否する。既定値として 0 を入れた実 preset は作らない。
新 claim には DA pause の返還による期限延長を与えない（`palw_x3_pause_credit_v1`）。この決定の arm は `PALW_G14_ARMING_RECORDS_V1`（空）の record で行う（ADR-0188）。

最悪round数・返答時間と最小carrier処理能力からclass/plan別のDAA envelopeを導く。
秒のbenchmarkをDAAへ変換する際は、実際のnetwork clock/inclusion仮定を明記する。
「1人がwindow最終瞬間に見つけても必ず間に合う」とはしない。

producerが適格なunitを開示しない場合は、そのunitのdeadlineでDA/defaultへ進む。
challengerが義務を果たさなければ棄却・deposit処理を行う。
各期限を満たしたhonest pursuitが、新規spamによりcourtの順番を永遠に失わないqueue/resource規則も必要である。
bounded予約は無制限pauseを導入するための例外ではない。

##### 7.3 Final条件

新経路のFinalは少なくとも次のAND条件を満たす。

~~~text
DisputeAdmissible class/plan/kernel
AND immutable model identity + network/model/producer-bond budgets satisfied (§16)
AND required positive verification evidence / coverage accepted
AND permitted claim-specific evidence / DA / court obligations satisfied
AND ordinary challenge + class verify horizon expired
AND no unresolved accepted pursuit / required court
AND plan-specific settlement and unique-work accounting satisfied
~~~

`palw_claim_final_floor_v1`などの既存helper [S16] を拡張し、
全Final writer、deadline sweep、vesting/work-rights readerが同じpredicateを見る。
UI上のFinalだけを遅らせ、reward/weightを先に確定させる実装は禁止する。

Panelの3票はReceiptLicensedへ進める条件の一部であって、客観的正しさの証明ではない。
required positive evidenceの存在を検査し、欠落をsilence=passにしない。
REAL rootのpre-Final weightとclaim-backed sliceの最大損失は改定RFC08 [S25] と整合させる。
EXEC slice自体はFinal前後ともfork-choice weight 0だが、pending reward・collateral・DA・court exposureは有界化を必要とする。
root単位の一回のFinal/精算とし、reservation/convictionで重複creditが発生しないようにする。

期限後の未検出不正について、新しい無期限rollbackを導入しない。
現行のpost-Final liabilityは保持期間・担保・適用fenceに従い、
本RFCのpre-Final予約とは別に扱う。

##### 7.4 sessionの先取りで正直なverifierを排除しない

現行ExecutorRefuted adjudicatorはopen_courts_of > 0でClaimUnderSessionを返す [S03]。
既存の単一session規則をそのまま安全性の前提にしてはならない。
悪意challengerが先にcourtを開き、本物の不正証拠を受理不能にする競合を新fenceで閉じる。

新経路では、既に組み立てられた適格なdirect objective proofは、
別bondのcourt/sessionが開いているという理由だけで拒否しない。
gateとstate foldの両方を変更し、同じclaim/root/rulesetに対する証拠を完全検証した後、
conviction・未処理session終了・担保/exposure清算・deadline解除を一つの決定的transitionとして行う。
court中の発言だけで他sessionを消すことは許さない。

予約はclaimの排他的な所有権ではない。認証された開示は全参加者が再利用でき、
別の適格なverifierも必要なprogressまたはexact proofを提出できる。
同じsiteで最初にsessionを開いたbondだけが次の証拠を提出できる仕様を禁止する。
開示済みの正しいunitを使って別siteの不正を追う場合も、claim単位のabsolute deadlineは維持する。

session cap・deposit・feeだけでは、資金無制限のSybilによる資源独占を防いだとはいえない。
interactive枠の選択、共有progress、direct-proof優先処理と総work capを定義し、
先取り・全枠占有・別site誘導の試験を必須にする。
それでも必要carrierがinclusion bound内に受理されない状況は§2の仮定外であり、
「一人いるだけで安全」という無条件保証に読み替えない。


#### 8. challengerの経済条件と処罰

* interactive開示・court・予約には、適格なpublic bondとsession deposit/exposureを要求する。
  genesis membershipやPanel seatであることを要求しない。
* 適格なfraud verdictではdepositを返し、既存reporter/court rewardの原則に従ってbountyを処理する。
* false accusation、challenger default、未認証materialだけの申告は、それぞれ定義された費用を課す。
* malformed objectをdecoderで拒否することと、受理済みsessionのdepositをburnすることを区別する。
* 代数的検査のローカルfailureだけではproducerをslashしない。
  objective exact verdict、証明された証拠/義務違反、または定義されたDA/defaultが必要である。
* producer処罰には現行conviction funnelを再利用する。reservation・escrow/rights・S2/S3等の
  applicable tierを維持し、新しい別経路で`claim.reserved`だけの処罰へ弱めない [S17]。
* bounty/徴収ledgerはnominal amountと実際のcollectedを区別し、未回収額を支払財源にしない。
* classの最大gain、同時claims/予約とlock lifetimeを結び付ける。
  proof受理前に担保が開放される、攻撃がdepositより大きな無償検査を強制する、といった経路を試験する。

DA/defaultは可用性または義務違反の処理であり、それだけで全Valid signerの計算fraudを証明しない。
court default、held verdict、execution-proving fraudのbasisは現行の区別を維持する。
producer convictionに全seatの別件処罰が完了することを要求しない。

#### 9. Valid receiptの検証範囲

現行 `PanelFalseValidV2` のscope原則 [S04] を、新constraint receiptにも適用する。

~~~text
receipt binding (new scheme; conceptual)
    claim_id, class/plan/kernel/suite version
    challenge and transcript commitment
    attested relation ids, segment/cell/boundary scope
    input/output/state roots and verification assurance
~~~

receipt scheme/versionごとのcoverageは合意checkerが導く。
registrant/seatの任意文字列「全計算を検査済み」を信頼しない。

FalseValidの裁定は、faultがそのreceiptの実際のattestationと矛盾するかを検証する。
partial seatは受け取った認証boundaryからの区間計算に責任を負うが、
別区間が生成したboundaryそのものの正しさを署名していなければ、その嘘で処罰しない。
arithmetic faultでは必要なoutput/input/siteがscope内にあることを確かめる。
whole-output/identity/constraint-proof faultには、それぞれ対応するscope規則を定義する。

確率的検査が仕様どおり実行され、許された残余確率で通過したという事実だけでは、
故意の嘘や検査義務違反を証明しない。
新schemeは「関係そのものを保証する署名」と「特定検査transcriptの正しい実行を保証する署名」を区別し、
どの客観証拠でslash可能かを明文化する。
legacyのfull/partial receiptを新しい保証へ読み替えない。

#### 10. ADR0171との接続

通常時の巨大モデル検証はADR0171のencoded/algebraic constraint方針を使う。
Panelが毎回whole inferenceまたはwhole segmentをreplayする方式へ戻さない [S21]–[S23]。

MatMul、量子化、nonlinear/range/lookup、routing、memory/state、境界と出力を
承認済みkernel relationで覆う。登録されたplanの一部relationだけを検査して全claimのboundを名乗らない。
check failureからexact courtへ移るlocalizerも同じplanのadmission対象にする。

~~~text
approved probabilistic checks
    → authenticated relation disagreement
    → bounded localization / witness disclosure
    → exact terminal verdict or defined obligation default
    → applicable conviction funnel
~~~

field mismatchだけを計算fraudとしない。未認証のwitness、非canonical整数/field変換、
faulty verifierなどを切り分ける。
証明protocolの違反を処罰する場合も、定義された客観的な違反証拠を必要とする。

VM fallback、TEEの正しさへの信頼、BFT運営委員会の裁定、trusted committee beaconを追加しない。
未知のrelationはversioned kernelのreview/activationを待つ [S23]。
Panel選択乱数とalgebraic proof challenge乱数は別domainで、RFC10の公開性だけから
後者のunpredictabilityを推論しない。

#### 11. wire・合意移行

node-only変更で既存証拠の提出能力を共有する段階と、
新DA unit・admission・reservation/Final条件を変える合意段階を分ける。

合意段階ではMUST:

1. descriptor・class/claim identity、request/answer、receipt、courtのversionと署名domainを固定する。
2. 新unitのcanonical encoding、field bounds、chunk/root bindingと最大object/transaction bytesを定義する。
3. state carriage/root、delta apply/revert、IBD、pruning、RPC/SDK/manifest/fingerprintへ反映する。
4. claimのbind/admission時点のrulesetを固定し、in-flight旧claimは旧規則で完了させる。
5. 未対応nodeのunknown-version successを禁止し、協調upgrade前にshadow検証する。
6. 既存classの新descriptorは新identityとして再admitする。
   現行にgapがあるclassの新規報酬claimを止める必要がある場合は、別途公開した移行policyとfenceで実施する。
7. post-Final liability/担保保持の長さをhard deadlineと整合させ、途中でreleaseして逃げられないことを確認する。
8. §16の固定model identity、distinct miner元本、配分snapshotとnetwork/model/bond予算を
   state/root/carriageへ含める。claim固有courtの許可unit/累積scope、Finalと責任解放を規範化し、
   同じ認証済みobjectから同じ結果を得ることを確認する。model取得・peer状態は合意入力にしない。

RFC10のoperator-anchor撤去はこのRFCから暗黙に有効化しない [S20]。
公開fraud filerを先に作れても、permissionlessなPanel seedの安全性は別のactivation gateである。
新classの公開registration/報酬適格性を広げる前に、Dispute Completenessを満たす必要がある。

[RFC-0015][R15]のoptimistic panel-free modeは、将来の別fenceでpositive Panel receiptを必須条件から外す。
そのmodeが全activation gateを満たして有効化されるまでは、§7.3のpositive evidence/Final条件を維持する。
旧claim・旧modeの条件はRFC15によって遡及的に緩めない。

#### 12. 実装対象と段階ごとの出口条件

| 段階 | 主な変更先・仮称 | 完了条件 |
| --- | --- | --- |
| P0: 契約・test固定 | dispute plan/template、既存T54g/T46・resource vectors | served captureの代役とfresh outsiderの差を再現。未対応familyを明示 |
| P1: held証拠の開示 | palw_da_rcore_v1、palw_held_da_v1、palw_shard_court_v1、palw_state_v2 | fused committed tileを認証取得し、返答/未返答の両方を合意処理 |
| P2: bounded局所化 | held retained tree、既存IR/pipeline node disclosure、palw_filer_held | canonical 8k条件でdense cap/prefix/tileの三gapを閉じる。小geometryだけで完了扱いしない |
| P3: 共通filer | proposed kaspad/src/palw_fraud_filer.rs、palw_filer_replay、palw_operator_da、RPC/SDK | seat・non-seat・operatorで同じgarbage/borrowed traceをexact proofへ変換 |
| P4: admission・Final | class/TIR admission、state deadlines/locks/sweep、proposed reservation | 完全性未達classは新reward対象外。外部追及でFinalを有界にblockし、spamで無限延長しない |
| P5: 大型constraint suite | RFC07/RFC11/ADR0171のkernel/checker/receipt/localizer | 全constraint、条件付きsoundness、独立witness取得、最大court負荷が測定・審査済み |
| P6: permissionless rollout | RFC10 beacon/binding、network schedules/fingerprint | genesis operatorなしの新参加者が登録・claim・外部追及・settlementを完結 |
| P-A: モデル資本・coinbase配分 / 不介入 | §16、distinct capital snapshot、versioned allocation/accounting、RPC/SDK、claim-specific court scope | network/model/bond予算保存、取得結果からの独立性、公開対閉鎖の経済評価とrecovery/移行を独立に検証。旧Transport統合・Seeder/取得gateは廃止 |

RFC15のPanel=0実装・有効化は本RFCの完了条件に含めない。依存はRFC14のpublic prosecution完成から
RFC15へ一方向とし、RFC14完成だけでPanel数・quorum・既存lifecycleを変更しない。

P1/P2のnode testとconsensus testは同時に設計し、P4のgateがない状態で
新証拠形式に依存するclassをreward-bearingにしない。
既存objectだけで成立するP3のoperator→proof接続は、合意変更と独立に進められるが、
全class対応や新Final保証の達成とは呼ばない。

#### 13. 必須の敵対的試験とrelease gate

##### 13.1 一人のfresh outsider

producerと3/4/5 malicious seatsが偽claimをlicenseする。
正直なverifierはPanel外の通常public bondのみで、producerのheap/鍵/drill instanceを持たない。
正しい固定modelを保有/任意配布から取得した条件で、public claimと認証されたclaim固有materialから処罰まで進む。
全保有者拒否時のmodel取得はこの試験の保証に含めない。

少なくとも以下を実行する。

| ケース | 必須結果 |
| --- | --- |
| junk root・materialなし | 適格なDA要求 → default、producerの該当義務を処罰 |
| 自己整合したgarbage trace | row-0が開いても処理を終了せず、局所化 → exact fraud verdict |
| 別jobの本物trace | job/input/binding不一致を客観証明。貸し元のhonest executionを誤って没収しない |
| 単一step/token、境界、checkpoint、routing/stateの改ざん | planに定義したfaultへ到達。sampling漏れは検出成功と数えない |
| canonical 8k held/fused | fresh verifierで三gapを閉じる。ServedView代役を禁止 |
| verifierが巨大captureをRAM展開できない | 宣言されたstreaming/retained-level経路で完結 |
| producerが異なるchunk/root/shape、偽operandを回答 | 認証拒否または定義された客観違反。未認証bytesでhonest producerを有罪にしない |
| 正しいproducerとmalicious challenger | 棄却・deadline/費用処理後にFinal可。不要なarithmetic slashなし |
| partial seatと他segmentの嘘 | scope外seatを処罰しない |
| 無応答・偽回答・court turn default | 各basisを区別し、producer/seatを誤帰属しない |
| non-seat予約spam、複数Sybil、既回答unit | 同時数/総資源/hard deadlineを守り、正当追及の進行も確認 |
| 悪意bondが先にcourtを開き、枠を占有 | direct objective proofを排除せず、別verifierの共有progressとsession終了を確認 |
| restart/reorg/重複proof/Final境界/担保退出 | canonical sessionを復元し、二重slash/bounty/weightと時計再開始なし |

##### 13.2 検証レベル

合意unit/fold試験だけでは完了にしない。
署名/object gate → mempool/fee/carrier inclusion → acceptance walk → state fold →
deadline/Final/reward/weight/担保 → restart/IBD/reorgまで実行するnode E2Eが必要である。

source/model/layout、kernel/plan/suite、network/fence、hardware、class/job/claim、
artifact digests、bytes/RAM/storage/work/DAA、実際のcarrierと裁定を記録する。
reduced fixtureの成功をcanonical rowの成功と呼ばない。
暗号学的soundnessと全入力の到達性はreview/templateの根拠で確認し、有限のtest件数だけから保証を導かない。

##### 13.3 activationを拒否する条件

次のいずれかが残れば新経路を有効化しない。

* producer内部instanceを使わないと証拠が作れない。
* DAに答えられる偽traceを、対応planの外部filerが有罪証拠へ変換できない。
* localizerが全dense capture、無制限replay、未認証boundary、未有効kernelを必要とする。
* non-seat追及中にFinal/報酬が確定する、またはchallenge spamが時計を無限に延長する。
* bytes/work/rounds/inclusion/deposit/retentionのworst-case boundが未定または実測条件で不成立。
* 固定model/root照合、distinct元本・model配分・三層予算が不成立、または取得成否を資格/weight判定へ持ち込む。
* claim固有証拠/court scope・認証/default・Final/責任解放が未定、または公開対閉鎖の経済条件が未評価。
* legacy claim、旧receipt、state root/fingerprint、IBD/reorgの意味が変わる。
* 大型constraint suiteのcoverage/soundness/localizationが未審査なのに大型対応済みと表記する。

#### 14. baseline確認記録と未決パラメータ

2026-10-07の前段コード確認では、baselineで次を実行し45件成功した。

~~~sh
cargo test -p kaspa-consensus-core --locked --offline --no-default-features \
  --test palw_operator_da_candidates --test rcore_m3_da_court
# 3 + 21 passed

cargo test -p kaspa-consensus-core --locked --offline --no-default-features \
  --test palw_tir_court --test palw_court_decode_close_door \
  --test rcore_s4_conviction_funnel
# 11 + 1 + 9 passed
~~~

これは既存機能の確認で、新しいDispute Completeness、新DA unit、共通filer、予約規則の
実装成功ではない。T46 [S18] はsource確認のみで、この記録では実行していない。
T54gのfresh-seat gapもsource確認である。
本RFC作成中に稼働ネットワークへclaim/challengeを送信したり、upgradeを適用したりしていない。

activation前に別途固定する項目:

* commitment/locator/witnessのcanonical wire schema、domain、tag、version。
* familyごとのmax material/carrier/total bytes、rounds、checker workとRAM/storage。
* class rate・同時session・duplicate/cache範囲とscheduler fairness。
* bond/deposit/exposure、実費とbounty、lock/retention/hard deadlineとnetwork inclusion envelope。
* encoded constraint suiteのparameterと全claim soundness accounting。
* independent implementation、template review、実network E2Eとmigration/fingerprint evidence。
* §16のimmutable identity、重複のないmodel元本、coinbase配分/残余、claim固有court scope、
  個別bond予算との接続、公開/閉鎖の経済評価と条件付き実効検出率。

これらが未定の間は設計・実装計画であり、「1 honest verifierで任意モデルが安全」の
稼働保証として利用しない。

#### 15. Pinned source references

現行コードへの参照は全てsource baselineへ固定する。
手元の古いcheckoutに同名fileがない場合でも、別版のコードを参照して設計を解釈しない。

* **S01** — [court verdict → conviction funnel][S01] — `consensus/core/src/palw_state_v2.rs`.
* **S02** — [licensed/Final claimへの処理][S02] — `consensus/core/src/palw_state_v2.rs`.
* **S03** — [ExecutorRefutedの許可証拠と客観判定][S03] — `consensus/core/src/palw_offence_attribution_v1.rs`.
* **S04** — [PanelFalseValidV2と署名範囲][S04] — `consensus/core/src/palw_state_v2.rs`.
* **S05** — [court開始条件][S05] — `consensus/core/src/palw_court_v2.rs`.
* **S06** — [seat replay filerと8k制約][S06] — `kaspad/src/palw_filer_replay.rs`.
* **S07** — [外部operatorの検出とDA告発の境界][S07] — `kaspad/src/palw_operator_da.rs`.
* **S08** — [整合した偽traceがFinalへ進み得るログ][S08] — `kaspad/src/palw_operator_da.rs`.
* **S09** — [held/fusedの三つの証拠取得gap][S09] — `kaspad/src/palw_filer_held.rs`.
* **S10** — [独立seatのgapとServedView代役][S10] — `kaspad/src/palw_filer_held_e2e.rs`.
* **S11** — [DA unit: IR/pipelineのleaf/node開示][S11] — `consensus/core/src/palw_da_rcore_v1.rs`.
* **S12** — [non-seat DAは現行ではclaimをpauseしない][S12] — `consensus/core/src/palw_state_v2.rs`.
* **S13** — [束縛されたone-move判定][S13] — `consensus/core/src/palw_shard_court_v1.rs`.
* **S14** — [class admissionとcourt資源][S14] — `consensus/core/src/palw_class_admission_v2.rs`.
* **S15** — [TIR admissionの資源・court規則][S15] — `consensus/core/src/palw_tir_admission_v1.rs`.
* **S16** — [現行Final floorと検証horizon][S16] — `consensus/core/src/palw_state_v2.rs`.
* **S17** — [没収・追加ペナルティ・slash額][S17] — `consensus/core/src/palw_state_v2.rs`.
* **S18** — [license済みclaimのstep proofによるconvictionテスト][S18] — `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs`.
* **S19** — [過去のsampling条件と検出確率][S19] — `docs/adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md`.
* **S20** — [permissionless bindingの未有効化とoperator依存][S20] — `docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md`.
* **S21** — [確率的検査・exact court・Final条件][S21] — `docs/rfc/0011-permissionless-model-and-long-context-onboarding.md`.
* **S22** — [大型モデルの検査設計、未実装・未有効化][S22] — `docs/adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md`.
* **S23** — [versioned kernel、VM/TEE/BFT代替依存の禁止][S23] — `docs/adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md`.
* **S24** — [constraint receipt、coverage、代数的検査][S24] — `docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md`.
* **S25** — [claim-backed work slicesとsettlement][S25] — `docs/rfc/0008-palw-claim-backed-consensus-blocks.md`.
* **S26** — [再現可能なlayoutと独立検査][S26] — `docs/rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md`.
* **S27** — [番号予約: next free RFC-0014][S27] — `docs/rfc/README.md`.

[S01]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L21619
[S02]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L23107
[S03]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_offence_attribution_v1.rs#L1996
[S04]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L22731
[S05]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_court_v2.rs#L333
[S06]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_replay.rs#L5
[S07]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_operator_da.rs#L27
[S08]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_operator_da.rs#L1052
[S09]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_held.rs#L5
[S10]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_held_e2e.rs#L12
[S11]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_da_rcore_v1.rs#L98
[S12]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L29613
[S13]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_shard_court_v1.rs#L341
[S14]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_class_admission_v2.rs#L424
[S15]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_tir_admission_v1.rs#L91
[S16]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L4845
[S17]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L26728
[S18]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs#L4450
[S19]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md#L54
[S20]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md#L3
[S21]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0011-permissionless-model-and-long-context-onboarding.md#L363
[S22]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md#L3
[S23]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md#L9
[S24]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md#L1
[S25]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0008-palw-claim-backed-consensus-blocks.md#L1
[S26]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md#L1
[S27]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/README.md#L1
[R15]: ../rfc/0015-panel-free-permissionless-verification.md

---

<a id="source-v1-0014-L00899"></a>

### 旧RFC-0014 / 原文899–904行

出典: [0014-panel-independent-fraud-prosecution.md](../rfc/0014-panel-independent-fraud-prosecution.md)。配置理由: 主題別統合。

#### Mission alignment amendment — 2026-10-07

本RFCはMISAKAの中核目標を実現する主実装設計である。各completion gateは普通のpublic bond・fresh verifier・公開transportと認証material・producer秘密状態なしを共通条件とし、kernel単体やliar-internal ServedViewだけの試験で完了扱いにしない。新しいclass/profileの報酬有効化は、この全経路の成立を必要条件とする。RFC15のPanel=0は本RFCの未完成部分が一つでも残る間は有効化しない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

---

<a id="source-v1-0014-L00914"></a>

### 旧RFC-0014 / 原文914–1043行

出典: [0014-panel-independent-fraud-prosecution.md](../rfc/0014-panel-independent-fraud-prosecution.md)。配置理由: 主題別統合。

#### 改定 — 2026-10-12（固定期限と有限終了、共通経済会計：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §5・§3A。

- **§7.2補足（X3）:** 経済的なclaim受理時点 `d0` で、rulesetに基づく最大の準備期間・観測期間・裁定期間と絶対deadlineを固定する。最初のcourt session受理時まで先延ばししない。

```text
receipt_deadline = d0 + T_prepare
open_cutoff      = receipt_deadline + T_observe
hard_deadline    = open_cutoff + T_resolve   (= dispute_hard_deadline)
effective_deadline = min(proposed_challenge_deadline, hard_deadline)
```

  すべて合意上のDAA単位とする。receiptが早くてもopen_cutoffを早めず、遅いreceiptで時計を延ばさない。新しいinteractive pursuitは `start <= open_cutoff` かつ `start + B_remaining <= hard_deadline` の場合だけ受理する（残り時間が検証・証拠開示・court完了に足りなければ開始させない）。通常Finalはopen_cutoff通過後、未解決の適格pursuitがない場合にだけ可能とし、期限到達だけで未解決claimを自動Finalにしない。direct objective proofは別sessionが開いているだけでは拒否しない。境界は受理 `d <= deadline`、期限切れsweep `d > deadline` のように一意にし、同一blockでの処理順をcanonical orderingで固定する。旧claimには旧rulesetを適用し、新しい期限規則は別のversioned fenceで有効化する。
- **§16（共通経済部分）:** モデル別claim bond台帳、配分snapshot、係数、正常Finalと非不正終了・没収時の取扱い、閉鎖/公開比較を置く（ADR-0177 2026-10-12改定、受入試験C01〜C18）。
- **G14との関係:** G14-B（`misaka/palw/kernel/src/budget.rs`）の最悪追及スケジュールは、この `hard_deadline` に収まることを要件にする。長いG14 staged経路にdrill暫定の予算window W = 400 DAAをそのまま適用しない（2026-10-11裁定）。

#### 改定 — 2026-10-19(異議があった claim だけ延長する期限、応答義務に基づく勝敗、court 提出経路の liveness と DDoS 対策:採用方針)

**状態:** ユーザーが方針として採用(2026-10-19「この方針で進める」)。実装・敵対的試験・実ノード試験・有効化は未実施。数値(3,120・10,000・容量の割合)は設計の起点で、本番の定数ではない。2026-10-12 改定(X3 の固定期限)とは、「異議がない claim はその固定期限のまま」「有効な異議がある claim だけ、事前に定めた規則で関連期限を一括して延長する」という形で両立させる。延長の規則は新しい versioned fence で導入し、旧 claim には旧 ruleset を適用する。関連:[RFC-0024](../rfc/0024-court-door-3200-to-ctx-2m.md)(door の短縮)、[RFC-0025](../rfc/0025-scalable-objective-court-many-models-mass-disputes.md)(大量 court の容量予約と混雑延長)。

##### A. 対話の回数(16 分岐・1 tile 16 位置の場合)

履歴 dissection の 1 往復は、Miner が子区間の部分集約値を開示し、Panel または外部検証者が不一致の子区間を選ぶ、の 2 投稿。tile 数は毎往復 `ceil(tile 数 / 分岐数)` に縮み、1 tile で終わる。

| context | 履歴 tile 数 | 最大往復数 | この探索部分の片道投稿数 | 縮まり方 |
|---|---:|---:|---:|---|
| 32K(32,768 位置) | 2,048 | 3 | 6 | 2,048 → 128 → 8 → 1 |
| 2M(2,097,152 位置) | 131,072 | 5 | 10 | 131,072 → 8,192 → 512 → 32 → 2 → 1 |

途中で不正が確定するか、どちらかが応答しなければ、そこで終わり得る。court 全体はこれに、申立て、Miner の root 集約値の開示、最後の局所証拠の提出と裁定が加わる。held regime の時間予算は `1 + 2R + t`(1 = root claim、R = 上の往復数、t = 終端処理の予算)。t = 2 なら 32K で 9 手、2M で 13 手。**これは往復の数ではなく、証拠の分割を含む実際の tx 数・ブロック数とも一致するとは限らない。** 分岐数は ruleset から導出されるので、上の値は 16 分岐を指定した場合の計算であり、稼働中の t12 が 16 分岐であることの確認ではない。直接反証できる leaf では、証拠が揃っていれば履歴の対話自体が不要。

##### B. 通常の期限と、係争の最終期限を分ける

| 項目 | 扱い |
|---|---|
| 通常の異議受付期間 | original claim の受付から 3,120 DAA |
| 異議がない claim | 他の保持・責任条件も満たしたら、通常どおり担保を解放する |
| 有効な異議がある claim | 関連する担保・報酬・Final を保留し、対話型 court を続ける |
| 係争の最終期限 | **original claim の受付から** 10,000 DAA |
| 各手番の応答期限 | 10,000 とは別に定め、応答義務者が守る |
| 終了後 | 裁定結果と、残る保持・復旧義務に従って担保を解放または徴収する |

- 起点は original claim の受付時とする。3,120 の直前に申し立てられても、残りは `10,000 − 3,120 = 6,880 DAA`。受入条件は、**遅い申立てでも、残りの対話・証拠配送・最終収録・回復の余裕が 6,880 DAA に収まること**。
- 「申立てから 10,000」を与える案は、original からの拘束が最大約 13,120 DAA になる別の仕様で、採らない。
- **bond の返還時点だけを延ばしても足りない。** 証拠の受付が 3,120 で閉じれば、後から証明できない。延長の対象には、court の最終期限に加え、該当経路の `collect_until`、開示義務、資料保持、Final の保留、bond の解放条件を含め、1 つの原子的な状態更新で動かす。現行の公開 allocation 経路は `material_until = collect_until + reorg_retention`、`bond_exit_after = material_until` と連動しており、clock を実行時に延長しない前提で書かれている。したがってこれは定数の変更ではなく、**有効な申立てを契機に関連期限を更新する、明示的な新規則**である。
- 同じ bond が別の claim の責任も負っている場合、このcourt の終了だけで全額は返らない。解放は、claim ごとの責任と予約を基準にする。

##### C. 「証明できなければ slash」を分ける

| 起きたこと | 判定・処理 |
|---|---|
| 局所再計算で、Miner の主張との矛盾が証明された | 計算不正として Miner 側を slash |
| Miner が、応答可能な正規の開示要求に期限内に応答しなかった | 開示・応答義務違反として責任者を slash / default 処理 |
| Miner は必要な応答をしたが、Panel が不正を証明できなかった | **Miner を slash しない。** 申立てを棄却し、Panel 側に規定の敗訴・費用処理 |
| Panel が次の区間を選ばない、または最後の証拠を出さない | Panel 側の放棄・期限違反として処理 |
| 正直な当事者に必要な収録容量が与えられず、手続きが終わらなかった | それだけを計算不正の根拠にしない。容量・期限保証の設計不備として扱う(E) |

- 最後の tile まで正しく絞れた後に Panel が証拠を出さずに期限を迎えた場合、「Miner の正しさが証明されなかったから Miner を slash」としてはならない。Panel が最後に何もしないだけで、正しい Miner を負けさせられるためである。既存仕様は既にこれを区別している:終端では Miner に新たな応答時計を課さず、誰も最後の証拠で閉じなければ、session の最終期限で challenger 側の敗北として終わる。途中の応答待ちでは、応答義務のある側が期限を超えればその側が負ける。
- 既存コードの、敗訴した challenger の担保からの費用徴収と、fence に応じた係争時間の課金を、新しい期限に整合させて使う。新しい勝敗原則は作らない。

##### D. 延長の濫用を防ぐ条件

- **申立てだけで、無料・無制限に他人の bond を拘束できないこと。** 申立てには、対象 claim・不一致の箇所・署名・申立人の担保・費用を必要とし、同じ争点の重複は統合する。ただし、申立ての時点で完成した不正証明は要求しない(これから対話で証明する経路が使えなくなるため)。
- 申立て権を Panel に独占させず、適格な外部検証者 1 人にも同じ経路を残す。
- **10,000 まで何度でもやり直せないこと。** 各往復で区間が縮まり回数の上限が決まる構造を保つ。新しい message や再申立てで 10,000 の時計をリセットしない。
- **各手番の期限を残す。** Miner が最初の応答を 9,999 まで遅らせて、Panel に選択・証明の時間を残さない進行を認めない。全体期限と各手番の期限は別の制約。

##### E. court 提出経路の liveness と DDoS 対策

目的:Miner が通信攻撃で応答できなくなり、不正をしていないのに slash される事態を防ぐ。同時に次の 3 つを成り立たせる。

- 攻撃者が、court の提出経路を大量の tx で占有できないこと。
- 正当な Miner と外部検証者が、期限内に必要な提出を行えること。
- 正規の提出機会を与えられなかった Miner を、単なる期限切れで slash しないこと。

少額の fee だけでは DDoS 対策として不十分。Miner に毎回高い fee を課すのではなく、悪意ある大量申立ての費用を攻撃者側に負担させ、正当な応答のためのブロック容量を予約する。

| 仕組み | 設計 |
|---|---|
| court 応答用の予約帯域 | 通常 tx の混雑で Miner の応答枠が消えないようにする。割合(例:通常 65% / court 応答 25% / 申立て 10%)は説明用の仮定で、容量試験で決める |
| 少額の応答 fee | 有効な court の正規応答は低コストにする |
| challenger bond | 根拠のない大量申立てに経済的負担を課す |
| 複数経路による提出 | Miner 本人だけでなく、署名済みの応答を第三者も relay できるようにする |
| 期限と収録保証の連動 | 正規の収録機会がないことを、Miner の計算不正と同一視しない |

- 予約帯域は、mempool の優先順位だけでは足りない。ブロック作成者が無視してよい優先順位では、悪意ある作成者の検閲を防げない。合意規則による容量予約、収録義務、または検証可能な収録機会の仕組みまで設計する(Ethereum の inclusion list の研究と同じ方向)。既存の `palw_court_capacity_v1`(休眠、値は仮)の court 専用 block 予算・予約枠・期限順の割当・新規 claim の受付制限を土台にする。
- **fee の負担:**
  - Miner の正規応答:低額の基本 fee のみ。各手番の応答回数・bytes を制限し、必要な応答容量は事前に予約する。
  - Panel / 外部検証者の申立て:申立て bond と、予想される court 処理費用を事前に拘束する。
  - 敗訴・悪意ある大量申立て:客観的な敗訴規則に基づき、費用を申立人側に配分する。
  - 不正が成立した申立て:正当な訴追者が費用で不利にならないよう、成功報酬・費用補填を設ける(OP Stack の fault dispute game で、手番に必要な bond を反証コストと結びつける設計と同じ方向)。

**DDoS を受けたときの判定:**

| 状況 | 処理 |
|---|---|
| Miner が正規の収録機会を与えられ、期限内に応答しない | 応答義務違反として default / 責任処理 |
| court 帯域が混雑し、予約済みの収録機会を提供できない | 合意規則に基づいて期限を補正する。単なる計算不正として slash しない |
| Miner がローカルで DDoS を受けたと申告しただけ | 無条件には延長しない(虚偽の延長請求を防ぐ) |
| Miner が無効な証拠を提出した | 無効な応答として処理し、期限をリセットしない |
| Miner の署名済み応答を第三者が提出した | 認証と期限条件が正しければ受理する |

全ブロック作成者が検閲し、Miner がネットワークから完全に隔離される状況まで、有限の期限内での救済を無条件には保証できない。保証できるのは、明示したネットワーク・正直な収録者・容量の前提が成り立つ範囲である。ネットワーク全体の障害と Miner 自身の応答拒否は、chain 上の「応答がない」という事実だけでは区別できない。だから slash の条件を、必要な収録機会が成立したことに結びつけ、その保証が成り立たないときの安全な処理を明文化する。

**同時大量 court:** 既存の court 経路は、`同時 session 上限 = 各手番の期限 × 1 ブロックあたりの court 処理枠` で容量を導く。session 全体の期限や bond の解放だけを 10,000 にしても、各手番を処理する容量や同時受付の上限は増えない。例として、総証拠 32 KiB の court が 1 万件、court 用帯域が 12,500 B/DAA なら、転送だけで約 26,215 DAA かかる(仮定上の算術)。10,000 への延長だけで任意の大量発生は吸収できないので、予約・待ち行列・新規 claim の受付制御、または容量の増強が引き続き必要(RFC-0025)。この構成なら、Miner の bond を常に 10,000 DAA 拘束する必要はない。

##### F. 追加する合意仕様(Court Submission Liveness & Anti-DDoS)

1. 有効な court の各手番に、最大 bytes・検証 work・必要な収録容量を予約する。
2. 正規の応答には低額で予測可能な fee を適用し、申立人の bond または費用預託から court の予約資源を賄う。
3. 不正な申立ての大量生成、同一争点の重複、無効な証拠の投入による容量占有を、有界な受理規則と費用で抑止する。
4. 提出者以外による署名済み応答の relay を許す。
5. 正規の収録機会を保証する scheduling を定義し、それを満たせない混雑経路では、事前に承認された規則で期限と bond の拘束を整合させる。
6. 提出しなかった事実だけから計算不正を推定しない。正規の応答義務違反は、別の客観的 default として裁定する。
7. 大量 court、tx flooding、検閲、ネットワーク分断、reorg を含む実ノード試験を有効化の条件にする。

##### G. 受入条件

最終的な受入条件は「期限が来たから誰かを slash できる」ことではなく、**「正直な Miner と正直な検証者のどちらにも、所定の資源で義務を果たす時間が確保され、違反した側だけを客観的に処罰できる」**ことである。10,000 DAA で実際に完了できることは未測定。1 位置化などの証拠削減(RFC-0024)は、期限・担保・証拠保持の整合、応答義務に基づく勝敗、混雑下での収録機会の確保の上に乗る最適化として扱う。

#### 整合性修正 — 2026-10-19(RFC 全件整合性監査)

- 本書の court に関する時計・帰責・責任期間・置換関係は [RFC-0026](../rfc/0026-court-policy-master-contract.md)(current court policy)にまとめた。2026-10-19 改定の 3,120 / 10,000 を主契約とし、door ≤ 3,200 は性能目標として区別する(RFC-0026 §2.1)。
- **E(DDoS 対策)は帰責の規則として未完成。** chain 上の「期限まで応答がない」は、応答しなかった場合と、署名・送信したが収録を妨害された場合とで同じ履歴になり得る。無応答 default の条件は RFC-0026 §4 に置き、具体的な判定規則は未決(P0、F02)。少額 fee は受理後の有料負荷にしか効かず、不正 codec・重複 message・接続負荷には受信側の上限が別に要る(§4.2)。
- **外部 1 人の訴追経路は未完成**(P0、F03。RFC-0026 §5)。申立て fee と bond だけでは閉じない。
- **10,000 への延長は、依存契約(EXEC 依存 worker、provider lease、kernel liability、custody pledge)が受付時に `a + 10,000` を覆っていることが前提**(F07。RFC-0026 §3.3)。覆わない claim は受け付けない。延長時に被告の同意を取り直さない。
- B の「通常どおり担保を解放」は無条件の全額返還ではない。post-Final の公開告発 window、Gate C、monetary material の保持、同じ bond の別 claim の責任を満たしてから(RFC-0026 §3.3)。
- A の分岐数の表は 16 分岐の仮定例。監査の読み取りでは出荷 t12 preset は 4 分岐で、32K は 6 往復、2M は 9 往復になる(RFC-0026 §7.1)。
- 10-12 改定の `d <= deadline` は例示であり、`collect_until` の排他的境界とは別の時計。変数ごとの境界表を RFC-0026 §2.2 で要求する(F09)。

---

<a id="source-v1-0028-L00001"></a>

### 旧RFC-0028 / 原文1–92行

出典: [0028-rooted-court-response-eligibility-history.md](../rfc/0028-rooted-court-response-eligibility-history.md)。配置理由: 主題別統合。

### RFC-0028 — rooted court response eligibility history

- Status: **Draft / Proposed / Review Required** (2026-10-21).
- Decision companion: [ADR-0195](../adr/0195-rooted-court-response-eligibility-history.md).
- Governing contract: [RFC-0026](../rfc/0026-court-policy-master-contract.md), [ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md).
- Implementation status: isolated candidate runtime with selected verification after `e82921ee1`; public schema/rules are not adopted. The previously committed information-loss counterexample remains valid for unbound retained TIR metadata.

#### 1. Required fact

For a reserved packet describing a concrete native response stage, the chain must be able to determine whether that stage's prerequisites were accepted before the first supply block for its assigned DAA began. A current response cursor, aggregate unused quota, wall clock, mempool observation or producer flag does not establish this fact. Demand opening time does not timestamp a later accepted Begin/Part.

The new native duty view identifies the actual original claim/allocation/source, owner-scoped demand/turn, literal Ready custodian and current body cursor. It intentionally has no invented predecessor-acceptance timestamp. The retained slot/chunk formats do not currently contain that timestamp. An external replay of retained block journals could reconstruct it, but is not supplied by the current state-only adapter.

#### 2. Proposed separate marker

The recommended candidate schema adds a separate versioned marker, leaving service v2 and old native formats unchanged:

| Namespace | Proposed append-only addition |
| --- | --- |
| Staged key tag 13 | `CourtDutyClock(duty_id: Hash64)` |
| Staged row tag 19 | `CourtDutyClockV1` |
| Payload version | 1 under its own marker domain; not service payload v1/v2 reinterpretation |

These are local candidate reservations, not public wire/schema adoption. X3 container key 11 / row 17, service key 12 / row 18, lifecycle tag 230 and every old variant remain unchanged. There is no new lifecycle kind or carrier signature authority in this proposal. New readers continue to accept old supported snapshots without a marker; they must classify marker-dependent historical eligibility as unavailable. Old readers are not claimed to understand snapshots containing the new appended variants.

The fixed-size marker needs the following data, derived by the native fold rather than submitted as a producer DTO:

- marker version, service profile ID and concrete duty ID;
- original claim, native allocation/source and actual acquisition key;
- literal answerer, actual demand ID and turn;
- actual predecessor accepting block ID and DAA;
- first eligible service DAA, derived by checked successor of that accepting DAA;
- original response deadline, original hard bound and original material-retention horizon.

The candidate fixes field order in §8 and checks Borsh/typed heap/allocation-free scanner together; public schema adoption remains pending. An auxiliary marker authenticates history/data identity, never correctness of model computation.

#### 3. Recording rule

For a claim already bound to the candidate service profile, compare actual native response duties immediately before and after a successfully accepted transition. When Demand/Reacquire opens a response or accepted Begin/Part changes its next concrete stage, record that stage's marker in the SAME native journal transaction as the original row/chunk changes.

No marker is created merely by observing an unchanged cursor again, by a later unrelated WorkDebit, by reading a snapshot or by a node's local RPC. Existing markers are immutable; a different purported prerequisite for the same duty must not replace them. Finish/collection or actor completion must not be read as evidence that an earlier unused supply slot was eligible. The original completion and liability rules remain separate.

A binding installed after an old stage already exists does not reveal its historical prerequisite DAA. Without a verified historical derivation, mark it unavailable; do not stamp the installation time as the old acceptance time. Supporting preparation/default duties and non-ORIGINAL routes needs the corresponding actual transition provenance, not this response adapter's name reused for them.

#### 4. First-block convention

The existing reservation component counts only the first accepting block at each DAA. A response stage whose prerequisite becomes accepted at DAA `d` cannot be certified as block-start eligible in that same DAA's first supply block. Its earliest counted service unit is `d + 1`, checked for overflow. This is a service-accounting convention, not a new minimum response latency: already-valid multiple native controls may still be accepted at the same DAA or block under the existing protocol.

Additional blocks at the same DAA cannot retroactively add another counted first opportunity. Skipped DAA have no supply. Absolute clock progress continues even when supply or a marker is missing. A valid response accepted too late to leave another service unit before its unchanged deadline may yield a marker with no usable future interval; that must not reject the valid response, clip/extend a deadline or count a nonexistent opportunity.

#### 5. Packet correspondence and responsibility

A positive aggregate `ServiceReservations::assess` is resource accounting only. Before normal-opportunity reasoning, a future adopted adapter must bind the reserved packet/template to the concrete logical request, role, stage, cost and branch, and check its marker against the assigned supply unit. The marker's profile, original scope, predecessor and service interval must match. Unknown marker, missing supply or ineligible start cannot be silently promoted to provided normal opportunity.

Markers do NOT authorize slash or prove that a response could have traversed the COMPLETE legal path. A party's deliberate delay of an earlier stage cannot automatically become a defense just because the final stage has no later eligible unit. Conversely a party cannot be blamed for capacity or authenticated prerequisite failures outside its adopted duties. The full causal graph and end-to-end obligation, every legal latest-response branch, actual reserved supplies, admitted CPU/IO/memory/codec costs and explicit delivery/inclusion assumptions remain necessary. Current denial-only default stays in place until that contract is reviewed and adopted.

Original acceptance still needs complete reservation including all permitted hostile branches. Incremental reservation only after a stage appears is insufficient for F03. An owner-scoped response marker is not an owner-independent shared-frontier packet. The complete canonical template and public shared ingress are separate required work.

#### 6. Atomicity, retention and load validation

Native action writes and new marker writes must either both commit or both roll back. A quota, heap, scanner or index failure after computing a marker cannot expose a partially applied response or silently omit its marker. Preflight the entire change set or use a verified native savepoint; tests must force failure between the planned writes.

Use rooted journal entries and deterministic index updates. Undo/replay of Demand/Begin/Part must undo/replay marker provenance, including block ID, with no local-cache dependency. Reorgs that accept the same logical stage at a different block/DAA must recover the corresponding selected history. New marker readers must enforce fixed version/key/scope/clock shape; exact current-stage validation at creation and contextual scope checks on snapshot load are both required.

Retain required marker history while the original service binding is unresolved, including past ordinary TTL where the existing hold requires it. Retaining a historical fact does not extend a foreign signed custody promise. Terminal archive/release and pruning cannot be invented here; F07/F08 still govern them. Every marker write, lookup, scan, index and retained byte must be priced from the complete profile; there is no free hidden journal or zero-cost assumption. Batch validation can be optimized only with equivalent scope/atomicity checks.

#### 7. Required verification

1. Execute the early/late accepting-context counterexample through native response transitions, showing equal retained TIR cursor/current duty but different prerequisite timing relative to a service unit. Keep fixture/node-history scope explicit.
2. Preserve all existing key/row encodings and t12 identity. Verify exact new marker codec, preallocation scan and typed heap together.
3. Record markers only on real stage changes; no restamping on unchanged state, reread, restart or WorkDebit. Old snapshots/bindings remain explicitly unavailable where provenance is absent.
4. Test same-block controls, repeated DAA, no block/skipped DAA, exact deadline boundaries and a valid late response with no remaining future interval. No deadline/cap increases.
5. Force quota/index/codec failure and prove atomic action/marker rollback. Reorg, native carriage reload and complete/pruned IBD retain the same required facts under their respective actual storage scopes.
6. Test packet-stage/profile/actor/turn/branch substitution refusal. Do not count aggregate supply before a prerequisite as normal opportunity; do not equate late prerequisite with innocence without its causal path.
7. Keep preparation, shared ingress, full graph, original admission, physical calibration, signer availability, R-D2-NODE and T01–T15 open until their own evidence proves completion.

The current quiet-devnet observations do not calibrate the new costs or a guaranteed reserved service rate. No numeric fee, turn duration, mass limit, model restriction or public activation is introduced by this Draft.


#### 8. Isolated runtime candidate (2026-10-21)

`CourtDutyClockV1` fixes Borsh field order as: version `u16`; profile, duty, claim, allocation and source `Hash64`; full native acquisition key; literal answerer bond; demand `Hash64`; turn `u64`; predecessor `BlockHash`; predecessor DAA, first eligible DAA, response deadline, original hard and material horizon (`u64` each). The acquisition key must be the existing `TirAcquisition` variant for the same claim. The full field type is written explicitly so the native inventory generator does not resolve a local `Key` alias from an unrelated module. Appended staged key/row tags are 13/19; payload version is 1. Old service/container variants are unchanged.

For an already service-bound acquisition, successful Demand/Reacquire/Begin/Part compares actual duty IDs before and after the native transition for the literal Ready response actors. Only newly appearing/changed duties get a clock from the real native context. WorkDebit, Finish, reads and a later service-binding installation do not stamp an unchanged or vanished duty. Same-block/same-DAA native responses remain valid. A marker can have its first eligible accounting DAA at the exclusive response deadline: it records the fact without inventing a usable future slot.

The candidate writes stage/chunk changes and clocks through `StagedCourtSeed` journal entries. A checkpoint at the service-bound verified-half boundary restores both on failure, in addition to the existing signature caller's transaction boundary. This added checkpoint, actual source/duty reads, comparisons, marker writes, derived indices and snapshot context validation all have costs; they are **not calibrated or adopted**, and are not treated as free in a service profile. The resource reservation primitive still admits typed state ownership before cloning. The original all-branch admission and profile costing remain incomplete.

Marker pruning uses its original material horizon, and the unresolved service binding retains it beyond that ordinary TTL. This is retained evidence; no extra signed service promise is manufactured. No new Final/release/default permission, court signer authority, carrier kind, production binding, admission or supply hook is introduced here. Full node pruned/restart/IBD campaigns and the remaining verification in §7 are still required.

Selected verification: **64 passed / 0 failed / 0 ignored**, plus non-test consensus check and generator idempotence. [Commands, raw logs, hashes, overlap and fixture scope](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#exclusive-hard-boundary-verification). This partially verifies §7; original admission, packet correspondence, full causal/default policy, actual node ingress/forks/pruned restart, all WorkDebit/restart branches and calibrated costs remain incomplete.

The service timing predicate uses the exclusive interval `first_eligible_daa <= unit_daa < min(response_deadline_daa, original_hard_daa)`, consistent with the existing packet `service_before` contract. This does not change native response acceptance for an old native deadline beyond the service envelope. A marker alone still grants no opportunity/default authority. The unit-cost reservation in `bind_clock_service` is a synthetic metadata fixture, not a measured or adopted capacity/fee profile.

---

<a id="source-v1-0031-L00001"></a>

### 旧RFC-0031 / 原文1–47行

出典: [0031-shared-court-initial-notice-boundary.md](../rfc/0031-shared-court-initial-notice-boundary.md)。配置理由: 主題別統合。

### RFC-0031 — 共有courtのinitial notice境界

Status: **Draft / Review Required** (2026-10-21)。利用者はA（終端DAAを含む）の詳細仕様化・隔離実装を承認済み。正本は[RFC-0026 §2.2](../rfc/0026-court-policy-master-contract.md#22-境界表f09値は未決形式だけ固定)と[ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md)。判断は[ADR-0198](../adr/0198-shared-court-initial-notice-boundary.md)。旧claim・t12・既存tag/clockを変更せず、本番採択・有効化は含まない。

#### 1. 固定値と隔離実装の選択

`a`はoriginalの実accepting-chain DAA。通常申立ての終端値は`a + 3,120`、hardは`a + 10,000`で固定する。両方をchecked arithmeticで求め、overflowは受付拒否とし、飽和値や別の起点に置換しない。

initial noticeが終端DAAと等しい場合は、Aの `a <= d <= a + 3,120` を詳細仕様化・隔離実装に使用する。RFC-0026は等号を未決としており、既存`collect_until`の排他的境界や旧courtのinclusiveな期限をそのまま流用する根拠にはしない。

| 候補 | initial noticeのDAA区間 | 最後のnotice DAA |
| --- | --- | --- |
| A（詳細仕様化・隔離実装を承認済み） | `a <= d <= a + 3,120` | `a + 3,120` |
| B | `a <= d < a + 3,120` | `a + 3,119` |

Aは「3,120まで」という終端を含める明示方式である。3,200へ変更する案やhardを後ろへ動かす案ではない。Bは新profileの全受付窓をhalf-openで表しやすいが、Aより最後のnotice DAAが一つ早い。どちらも実供給・適格性・全残作業の予約を別途要求する。

#### 2. 機械的な境界表

| notice DAA | A | B |
| --- | --- | --- |
| `d < a` | 拒否 | 拒否 |
| `a` | DAA条件を満たす | DAA条件を満たす |
| `a + 3,119` | DAA条件を満たす | DAA条件を満たす |
| `a + 3,120` | DAA条件を満たす | 拒否 |
| `a + 3,121` | 拒否 | 拒否 |
| `a + 10,000` | 拒否 | 拒否 |

「DAA条件を満たす」は受付成功や処理機会の供給を意味しない。元claimが実際に先に受理済みであること、現在の署名/資格、canonical object順、原資・全分岐予約を満たす必要がある。同一DAA内の前後関係をDAAだけで推測せず、実block/受理順を使う。原則としてservice機会は必要な全前提の実成立DAAより後から計算し、noticeそのものを実供給にしない。

#### 3. adaptive要求と原子受付への接続

initial noticeと後続adaptive要求を別のnative状態として検査する。後続要求は元の適時notice・同一original/source/rules/profileと全分岐予約に束縛し、後続要求や再投稿でnotice起点やhardをresetしない。ownerや最初のrequesterを共有経路の独占権にしない。

claim共通noticeの重複統合、後続actorの資格と費用、signer義務、全分岐・最遅提出の処理余裕はcoordinatorの別条件である。本書の等号選択だけではその実装・採択・保証を満たさない。現在の共有data候補のhard排他的検査を変更したり、旧court規則を上書きしたりしない。

#### 4. 次の実装・検証

Aのpredicateと境界試験を作る。最短/最大u64、overflow、同一DAA別block、originalより前のnotice、重複notice、後続要求の起点差替え、reorgでのnotice消失/復元を対象にする。最遅notice後の全作業がhard内に収まることは別の全graph検査で示す。deadlineの等号から供給やdefault許可を作らない。

本書は性能測定、数値fee、turn duration、配送遅延、公開wireの選択を行わない。F02/F03、全original有料受付、T01–T15と実doorの完了前は武装を拒否する。

#### 5. 隔離predicateの範囲

`shared_initial_notice_daa_in_window_v1(a, d)` はAのDAA条件をchecked arithmeticで検査する。notice終端だけが表現可能でもhardがoverflowする場合は拒否する。既存`CourtClockBounds::derive`、旧court、共有dataのhard排他的検査は変更しない。

このpure predicateは実originalの存在・同一DAA内の因果順・署名・資金・予約を認証せず、notice行も作らない。重複統合、後続要求、reorgによるnoticeの消失/復元の検証は、native notice coordinatorとともに残る。DAA境界試験だけをこれらの合格に数えない。

---

<a id="source-v1-0033-L00001"></a>

### 旧RFC-0033 / 原文1–32行

出典: [0033-d2-readiness-root-opening-availability.md](../rfc/0033-d2-readiness-root-opening-availability.md)。配置理由: 主題別統合。

### RFC-0033 — D2 readiness rootのdigest opening取得

Status: **Draft / Review Required**。ADR-0188 Bのcodec選択は承認済みだが、本書の供給方式・義務・料金・期限は未採択。判断案は[ADR-0200](../adr/0200-d2-readiness-root-opening-availability.md)。[正本RFC-0026](../rfc/0026-court-policy-master-contract.md)、[モデル二階層](../PRINCIPLES.md)を維持する。

#### 1. 未解決の条件

Bのrootとleaf数だけから、外部検証者はseatがcommitしたdigestとその認証pathを復元できない。検証者が自身のモデルを持っていても、誤ったdigestをcommitしたseatのpathを知ることにはならない。正しいdigestの計算やrootのdecodeだけで、外部からの反証可能性を成立済みにしない。

現codecはroot/pathの同一性と既存登録byteの再検査だけを行うpure predicateで、nativeのready行、権限、slashを作らない。この境界を保ったまま、pathの供給を閉じる必要がある。本書はモデル登録・係数・昇格・利用資格の審査を新設しない。対象はliteral seatが自ら署名したデータcommitmentと応答義務だけである。

#### 2. C1：F02に束縛したdigest/path開示court（推奨候補）

- literal seatの署名はnetwork、native draw/beacon/class/seat/span、root、leaf数、元の保持契約・profileへ束縛する。DTOの自己申告をauthorityにしない。
- drawで固定されたinventory座標だけを要求できる。応答はclaimed digestとMerkle pathだけで、seatにモデルbyteを要求しない。正当なopeningは全利用者で共用し、署名の必要なfresh応答義務とは分ける。
- rootの受理前に、全許容draw座標の要求・応答・再取得・不正/重複/拒否・終了後保持まで、原資・現在のauthority・全分岐packet・実費・保持horizonを原子的に予約する。未決draw上限/価格/供給profileなら拒否する。
- 開示の応答期限は前提成立と合意状態の正規の処理機会から決める。実供給・明示した配送収録前提がなければdefault/slashにせずholdする。Panelの成功申告やwall clockで代用しない。
- 完成openingはroot/count/draw座標へ認証する。誤ったdigestの客観的反証は検証者自身の登録openingと既存predicateを使う。無効pathと非供給の効果を、モデル計算不正やmodel reviewへ読み替えない。

#### 3. C2：補助materialの完成公開後にrootを受理

元draw全体の正規digest materialを、有限・有料・保持付きの補助経路で先に公開し、rootとの一致・完全性をnativeに検査してからrootを受理する。top-level root objectは小さいままだが、全digestの転送・保持・再取得・native処理費用が必要である。未完成の先着bodyに独占slotを与えず、partial公開を可用性と認定しない。料金・保持契約・全material量は未採択で、既存のwire上限を上げて収めない。

#### 4. 必須検証

偽digest rootだけを公開してopeningを拒むseat、全外部枠の共謀/再充填、外部一人の要求、誤path/別draw/別署名、サービス未供給時のno-slash、restart/reorg/pruned IBD、全資本/保持、最遅受付・全deadline境界を実native/nodeで確認する。C2なら全materialの欠落・部分公開・資金不足の拒否と再取得を含む。いずれもpure codec試験だけでは完了しない。

未決のdraw上限・応答期間・料金・保持義務を仮値で埋めない。本番採択・pre統合・武装・配備は別の承認を要する。


#### C1の詳細仕様化・隔離実装承認 — 2026-10-21

利用者はC1（F02のdigest/path開示court）を選択した。上の未選択記述は当時の履歴である。literal signer・実供給とno-slash経路・全draw分岐の原資/予約へ接続する。draw上限・本番料金・保持義務の採択、pre統合・武装・配備、モデル審査の導入は含まない。

---

<a id="source-v1-0024-L00001"></a>

### 旧RFC-0024 / 原文1–600行

出典: [0024-court-door-3200-to-ctx-2m.md](../rfc/0024-court-door-3200-to-ctx-2m.md)。配置理由: 主題別統合。

### RFC-0024 — ctx 2M・大きいモデルでも door ≤ 3,200 DAA に収める court 設計(単一位置裁定・先読み開示・先行取得)

- **Status:** Draft / Review Required。実装完了・採択済み・有効化可能という宣言ではない。
- **作成日:** 2026-10-19。ユーザー指示により RFC-0024 として登録。
- **対となる ADR(草案):** [ADR-DRAFT-palw-court-door-3200](../rfc/0024-assets/ADR-DRAFT-palw-court-door-3200.md)。正式な ADR 番号は採択時に割り当てる。
- **設計目標:** モデルの大きさ(≤10B は model size の制限であり ctx の制限ではない)と ctx(最大 2,097,152)に関係なく、すべての合法 job について TIR の公開 door を **≤ 3,200 DAA** に収める。producer・Panel・custodian の協力を安全性の前提とせず、適格な外部検証者 1 人による訴追を維持する。
- **採る変更:** A=最終 history 裁定の 1 位置化、B=最大 2 段の先読み開示、C=認証済み material の先行取得と強制開示の併用。
- **非変更:** モデルの算術、検出 policy、正しい署名・root の検証、計算不正と DA/default の区別、担保・責任保持、Gate C、客観的 exact court。ZK・SNARK/STARK・zkVM・PCS によるモデル計算の validity proof は導入しない([PRINCIPLES](../PRINCIPLES.md) §8)。
- **上限値:** 従来の `maximum_door_daa = 3,120` から 3,200 への変更は**明示的な承認対象**であり、本 RFC の登録だけでは gate を変えない。
- **有効化:** 新経路は休眠。wire tag、fingerprint、DAA 16,000、稼働 t12 の設定は本書で割り当て・変更しない。
- **Related:** ADR-0189(door 候補)、ADR-0190(bounded rounds)、ADR-0191(dispute-local decomposition)、`docs/evidence/palw/g14-measure-2026-10-18/`（別 branch の evidence。この snapshot には未同梱。以下の commit/path を参照）。

> **提案する決定:** court の判定をノード上に残したまま、裁定に必要な原本の範囲を小さくし、適応的な要求を前もって網羅し、待機の依存を減らす。3,200 を満たしたことにするために既存タイマーを切り詰めたり、オフチェーンの好条件を最悪時の保証に読み替えたりしない。

#### 最新 branch との照合(2026-10-19 時点、g14/door-design)

本文は `misakas-pre` の ZIP を基に書かれており、ADR-0191 系の最新実装を監査していない(§0.1)。登録時点で branch `g14/door-design` にある内容は次のとおり。いずれも休眠・fence 未武装で、door の値は**式による derived_upper であり実測ではない**。

| 項目 | commit | 状態 |
|---|---|---|
| history tile の b=16 二分探索(⌈log16 tiles⌉ 往復) | 16555f312 | 純関数・試験通過 |
| A に相当する tile 内 1 位置への追加往復(⌈log16 tiles⌉+1 往復) | 16458bbb1 | 純関数・試験通過。終端 ≈12 KB は**見込み**で認証証拠込みの上限ではない(本文 §4 の条件どおり) |
| 指名 custodian 回答の登録簿(tag 245)を state field 化・processor 接続・block 試験 | ec2c4ae94 / c718fa12e / 5d7905bdf | 休眠 |
| door 導出表 | bb4daa746 / b6e8a3d16 | `docs/evidence/palw/g14-measure-2026-10-18/door-bisection-derived-1019.md` |

1 位置化後の derived_upper(DAA):

| | 32K, 125,000 B/DAA | 2M, 125,000 B/DAA | 32K, 12,500 B/DAA | 2M, 12,500 B/DAA |
|---|---:|---:|---:|---:|
| 最終裁定の読み込みを含めない | 1,519 | 1,703 | 3,058 | 3,242 |
| 1 位置の最終裁定を含める | 1,635 | 1,819 | 3,408 | 3,592 |

125,000 B/DAA は楽観的な上限で感度分析用、12,500 はその 1/10 の感度値であり、どちらも実測ではない。低帯域時の主因は 17,408 B の step block の搬送が段ごとに重なることで、B(先読み開示)による段数削減と C(先行取得)が、本文 §8 の `S ≥ X − 4`(2M)を満たせるかが採否を決める。

- **有効化:** 新経路は休眠。wire tag、fingerprint、DAA 16,000、稼働 t12 の設定は本書で割り当て・変更しない。

> **提案する決定:** court の判定をノード上に残したまま、裁定に必要な原本の範囲を小さくし、適応的な要求を前もって網羅し、待機の依存を減らす。3,200 を満たしたことにするために既存タイマーを切り詰めたり、オフチェーンの好条件を最悪時の保証に読み替えたりしない。

#### 0. 根拠・対象版・未確認事項

##### 0.1 添付資料の位置づけ

本書は、添付 `misakas-pre(2).zip`、会話中に提示された door の数値、および本書で新たに提案する規則を区別する。ソースの行番号・抜粋・SHA-256 は [source-audit.md](../rfc/0024-assets/source-audit.md) を参照する。以下の `[Sxx]` は同ファイルの出典 ID、[U01](../rfc/0024-assets/source-audit.md#u01) はユーザー提示値を指す。

ZIP の SHA-256:

```text
136f95326e9831f7778b9f8e73028c12b1f8f8ae00468b22770f8e54bec4c512
```

ZIP 内では `ADR-0191`、`b6e8a3d16`、`PALW_TIR_FRAME_COST_RECORDS_V1` を確認できなかった。したがって、投稿にある `g14/door-design`、tag 245 の最新経路、1 位置化の実装、ADR-0191 の正式な door 式を監査済みとはしない。統合時に当該 branch と必ず照合する。`PALW_G14_ARMING_RECORDS_V1` が空であることは ZIP のコードで確認した。[S07](../rfc/0024-assets/source-audit.md#s07)

##### 0.2 既存実装から確認できること

| 事項 | 確認した根拠 | 本提案への影響 |
|---|---|---|
| TIR の history dissection は reduction の部分値を Sum/Max で exact に fold し、最後は 1 history tile を再計算する | [S02](../rfc/0024-assets/source-audit.md#s02)[S03](../rfc/0024-assets/source-audit.md#s03) | A/B はこの算術を再利用する |
| history row の通常の読み元は、完成済み HistTile またはその行の元の commit point で、任意には選べない | [S04](../rfc/0024-assets/source-audit.md#s04) | K/V 行を小さく運ぶには canonical operand resolver の版分けが必要 |
| step leaf は `values_le` 全体を含めて hash される | [S05](../rfc/0024-assets/source-audit.md#s05) | 巨大な leaf の一部だけを抜いても、そのままでは認証できない |
| original frame は preparation、acquisition、owner work、terminal 等の SUM と絶対期限を持つ | [S06](../rfc/0024-assets/source-audit.md#s06) | 時間を消したり、並列化で料金まで消したりしない |
| custodian の小分け作業は、承認済み WorkDebit から得た permit の後に実行する | [S08](../rfc/0024-assets/source-audit.md#s08) | 大きな遅延は単純なネット転送だけでなく、許可と収録の依存でも起き得る |
| annex の任意提供＋オンチェーン強制開示＋無応答 default という経路は既存文書にある | [S09](../rfc/0024-assets/source-audit.md#s09) | C は既存責任モデルを再利用・拡張する |
| owner 上限や複数 custodian、再取得、コピー、carrier framing の予算が存在する | [S10](../rfc/0024-assets/source-audit.md#s10) | 1 回の生データ量と最大時間・総費用を混同しない |

##### 0.3 本書で証明したとはしない事項

32K/2M の 3,200 達成、約 12 KB の完全 witness、最悪時の実効帯域、出荷 binary での span、実ノード E2E、D2/X3、最新 tag 230/245 の互換性は未確認である。本書はその実装契約と受入条件を定義する。

現行受入条件として投稿にある **3,120 DAA** と、今回要求された **3,200 DAA** は別の値である。3,200 を新上限とするには明示的なパラメータ承認と関連期限の再導出が必要であり、それまで現行の 3,120 gate は維持する。わずか 80 DAA の差であっても黙って置き換えない。[U01](../rfc/0024-assets/source-audit.md#u01)[S11](../rfc/0024-assets/source-audit.md#s11)

#### 1. 問題と設計目標

投稿にある感度分析は次のとおり。いずれも実測ではない。[U01](../rfc/0024-assets/source-audit.md#u01)

| 帯域（B/DAA） | 32K・terminal 読込なし | 32K・あり | 2M・なし | 2M・あり |
|---:|---:|---:|---:|---:|
| 125,000 | 1,473 | 1,857 | 1,657 | 2,041 |
| 12,500 | 3,012 | 6,048 | 3,196 | 6,232 |

低帯域時の terminal 増分は両方とも `3,036 DAA`。しかし、135,600 B を 12,500 B/DAA で 1 回転送するだけなら約 10.85 DAA である。**3,036 を全て raw tile の転送時間とは断定できず、これを単純に `12 / 135.6` 倍して新 door を求めてはならない。** 正式式に入っている物理転送・許可・再試行・収録・検証・保持の係数を分解する。

本 RFC の成功とは、平均的なノードや協力的な producer が速いことではない。対応する全 fault route と最悪の合法的な遅延の組合せについて、有界な実行契約の下で、同一始点からの door の導出上限が承認済み上限以下になることである。

#### 2. 脅威モデルと保証の正確な範囲

##### 2.1 維持する保証

不一致を発見した適格な外部 bonded verifier 1 人が、producer の秘密状態や Panel の協力なしに、認証された material から局所化し、以下のいずれかまで進められる。[S01](../rfc/0024-assets/source-audit.md#s01)

1. **計算不正:** authenticated inputs と既存算術による exact court で具体的な矛盾を示し、対応する conviction・claim 無効化・責任追及へ至る。
2. **開示義務違反:** 所定の義務者が、受理された有効な要求に期限内で答えなかった事実を、accepted chain の状態から DA/default と判定する。

DA/default は「隠された計算が誤っていた証明」ではない。計算不正用の FalseValid 責任を、資料不提供だけから自動適用しない。計算不正と開示義務違反の責任者・罰・返金は既存規則に従い区別する。

##### 2.2 明示する前提

- 外部検証者は対象モデルを取得し、既存の適格性・担保・実行資源条件を満たす。完全非公開モデルの検出確率を本書で保証しない。
- base chain は合意規則に従って進み、所定の有界な収録機会がある。全ブロック生成主体が永久に検閲する場合まで、1 人で期限内収録できるとはしない。
- network delay、帯域、CPU/IO、同時負荷、許容 reorg/restart は、承認された service envelope の範囲内。無制限の通信断や無限回の reorg を有限 DAA に収めるとはしない。
- 検出後の決定論的訴追と、検出自体の確率は別である。A/B/C は検査対象の抽選を予見可能にしたり、検出率を作り上げたりしない。

##### 2.3 最適化で削らないもの

original model/class/job/claim の同一性、input/output/state への拘束、全 reduction の被覆、最初の不一致の局所化、既存の interval/overflow 規則、無応答時の客観的手続き、責任期間中の担保・証拠、Gate C と Final の hold を維持する。[S01](../rfc/0024-assets/source-audit.md#s01)–[S06](../rfc/0024-assets/source-audit.md#s06)[S12](../rfc/0024-assets/source-audit.md#s12)

#### 3. 提案する全体経路

```text
original claim / class / execution / trace を固定
  ├─ C: material manifest・取得 index・保持義務を固定
  ├─ C: producer は生成時から保存、verifier は任意に先行取得
  └─ paid preparation / Ready / original deadline は既存責任と接続
               ↓
不一致の検出と「最初の不一致」への局所化
               ↓
B: canonical な最大 2 段の開示を 1 response phase で公開
   ├─ 全候補が認証済み → verifier が 2 段の選択をまとめて提出
   └─ 資料欠損 → 同じ絶対期限内の強制開示 / DA default
               ↓
A: history tile 内の 1 位置 [p, p+1) まで局所化
               ↓
認証済み Q・K[p]・V[p] と root の reduction 条件で exact 再計算
               ↓
計算不正 conviction / 開示義務 default / 正当な棄却
```

上記の「phase」は 1 ブロックを意味しない。大きい response は複数 carrier に分割できるが、その bytes・収録・組立・検証はすべて door に計上する。partial arrival で応答時計をリセットしない。

#### 4. A — 最後の裁定を 1 位置に絞る

##### 4.1 対象

まず、既存の exact な history reduction を持ち、局所 operand の由来を決定できる TIR/attention profile に適用する。一般の GDN state 更新、Float32 reduction、logits、tokenizer、全 kernel をこの変更だけで被覆したと扱わない。

現行関数をそのまま残し、versioned な `PositionDissectionV2` と `PositionCloseV2` を並置する。以下の名称は本 RFC の提案名であり、ZIP に実装済みという意味ではない。

##### 4.2 局所化単位

history tile `[a, a+t)` が選ばれた後、`t ≤ 16` なら同じ canonical 16 分岐を 1 段適用して `[p,p+1)` を選ぶ。端の不完全 tile も実在する位置だけを候補にし、重複・欠損・範囲外位置を拒否する。

tile→position の段は B の末尾 bundle に含めてよい。**論理的な検証段は残すが、独立した response 往復を必ず追加する必要はない。** tile が既に 1 位置なら追加段を作らない。

##### 4.3 算術

root で固定された reduction の種類、要求 element の順序、totals を保持する。court は既存の demanded-range evaluator を `[p,p+1)` に限定して使い、当該位置の部分値を再計算する。[S02](../rfc/0024-assets/source-audit.md#s02)[S03](../rfc/0024-assets/source-audit.md#s03)

attention では、たとえば `max(score)`、`sum(exp(score - m*))`、`sum(exp(score - m*) × V)` の関係を、既存の root total `m*` と数値規則のまま扱う。**1 位置だけで softmax を正規化し直す実装は禁止する。** 新しい softmax、量子化、丸め順序、FP の再結合は導入しない。

「Q/K/V の行を運ぶ」は最小例である。実際の primitive が追加の scale、mask、position、parameter、state 等を読むなら、その認証済み値も canonical witness に含める。

##### 4.4 行レベル認証 — 本変更の重要な条件

ZIP の `hist_row` は完成済み HistTile を読む場合があり、その leaf は payload 全体を hash する。したがって、単に payload から K/V の 1 行だけ抜いて既存 leaf hash に対する proof と呼んではならない。[S04](../rfc/0024-assets/source-audit.md#s04)[S05](../rfc/0024-assets/source-audit.md#s05)

**最小変更候補は、元の row append の commit-point leaf を開く経路である。** 新しい `PositionOperandResolverV2` は `hist_row_node_v1` 等で導出される当該行の writer と座標から、必要な既存 step leaf の集合を決定する。Q と K/V の writer leaf は original execution root に結び付け、disputed leaf より前の leaf であることを確認する。任意の mirror が作った別 root に差し替えない。

この resolver の採用には以下をすべて要求する。

- 元の writer row と HistTile コピーの同一性について、既存の first-divergence / HistTile concatenation 裁定と合わせた被覆証明を持つ。
- 不一致が HistTile 自体にある場合を、attention の 1 位置検査だけで飛ばさない。
- 正しい実行では新旧 resolver の verdict が一致し、改ざんの場所を移しても反証可能性を失わない。
- 既存 leaf を丸ごと開いた場合の必要 bytes を計測する。writer leaf 自体が大きければ「1 行だから小さい」と見なさない。
- 「任意の後続 leaf を選び、別の先行値を持ち込む」経路を認めず、認証経路と first-divergence の前提を検査する。

この同値性・被覆・サイズ条件が証明できなければ、当該 profile は **`PositionWitnessBudgetIncomplete` として新 gate を通さない**。行サブルート追加による commitment 形式の変更を、実装上の都合で暗黙に行わない。必要なら別の明示的な設計審査と互換性規則を追加する。

##### 4.5 Witness の構造とサイズ

概念構造は次のとおり。既存型との具体的な対応と wire tag は統合レビューで確定する。

```text
PositionCloseV2 {
  version, network_rules_id,
  claim_id, allocation_id, session_id,
  class_id, execution_root, trace_root,
  disputed_leaf, site_id, root_claim_digest,
  position, query_position, layer, head_or_group,
  selected_partial, root_totals_digest,
  canonical_step_preimages, canonical_step_multiproof,
  canonical_artifact_openings, required_input_openings,
  submitted_by, signature
}
```

重複した context、既に accepted state にある root/totals は canonical な参照にできる。ただし、pruning/IBD 後も検証に必要な bytes を取得できる状態参照に限る。ノード個別のキャッシュ命中を合意条件にしない。

`B_position` は Q/K/V の本文だけでなく、すべての座標・型・root・opening・署名・codec framing と必要な関連 carrier の合計。投稿の **約 12 KB は見込み**であり、上限ではない。正確な byte 上限と parser/CPU 上限を登録 profile から導き、超過する正当 witness を「不正」として slash しない。

#### 5. B — 最大 2 段の先読み開示で適応的な往復を減らす

##### 5.1 方針

単に現在の child と「選ばれるはずの次の child」だけを先送りしても、相手は別の child を選べるため最悪往復数は減らない。

そこで `LookaheadDisclosureV2` は、現在の disputed range の **次の最大 2 段で到達し得る全候補**を canonical な frontier として開示する。元の分岐は 16 のまま。深さ 2 の frontier は最大 256 個で、内部の親部分値は frontier から既存の Sum/Max で復元できるため省略できる。

##### 5.2 2 種類の frontier を混同しない

- **commitment tree の局所化:** leaf/subtree の hash と opening を使う。hash は値の由来を認証する。
- **attention/history の数値的な dissection:** reduction の部分値そのものを開示し、子→親の exact fold を検査する。hash だけを 256 個出しても数値的な整合性検査の代わりにはならない。

B の時間削減に算入するのは、対象 phase の必要情報が全候補について実際に開示・検証された場合だけである。[S02](../rfc/0024-assets/source-audit.md#s02)[S03](../rfc/0024-assets/source-audit.md#s03)[S09](../rfc/0024-assets/source-audit.md#s09)

##### 5.3 検証規則

`LookaheadDisclosureV2` は session、現在の round、親 range、root claim、schema に拘束する。

1. 現行の `palw_attn_child_ranges_v1` の切り方を再帰適用し、depth と frontier の座標列を導く。浮動小数の log で木の形を決めない。[S13](../rfc/0024-assets/source-audit.md#s13)
2. 各要素の型・個数・proven interval、各階層の Sum/Max を検査する。中間階層を省略しても、その境界・interval チェックは省略しない。
3. 全 frontier が親の主張に fold することを確認する。足りない child、重複、別 root、別 head/layer/position、範囲の gap/overlap は拒否する。
4. 検証済み bundle に対し verifier が `ChoicePathV2`（最大 2 個の子番号）を 1 回提出する。各親子関係と途中の主張を accepted state から再構成する。
5. 不正な要求や不正な bundle の拒否は、それだけで計算不正 conviction ではない。正規の obligation と期限は残り、有効な応答がなければ既存の適格 default へ進む。

##### 5.4 得られる論理段数

以下は `32K = 32,768`、`2M = 2,097,152`、tile=16、arity=16 の幾何からの計算であり、DAA の実測ではない。単位の違う 2,000,000 positions 等では recurrence から再計算する。

| context | tile 数 | 既存 tile 探索 | A の位置段を加えた論理段 | depth=2 bundle の response phase 上限 |
|---|---:|---:|---:|---:|
| 32K | 2,048 | 3 | 4 | 2 |
| 2M | 131,072 | 5 | 6 | 3 |

これは **history dissection 部分だけ**の表で、step-tree の第一差分探索、root claim、material 取得、検証者の replay、terminal の収録等は別に計上する。response phase ごとの verifier choice も残る。4→2、6→3 を door 全体の半減とは言わない。

##### 5.5 bytes 増大への制約

数値 frontier の要求値の合計個数を `r` とすると、完全な depth=2 の値本文は最大 `256 × r × 16 B`（i128）である。既存の generic ceiling `r = 4,096` では本文だけで 16 MiB になり、小さな 1 carrier には入らない。[S02](../rfc/0024-assets/source-audit.md#s02)

したがって、depth=2 は無条件に有利ではない。profile は事前に次を導出する。

```text
saved_wait = 2 段を別々に応答した場合の待機上限の減少
added_cost = 追加 frontier bytes の運搬・検証・組立・IO・許可・reorg 費用の時間上限

深さ 2 を時間最適化として選ぶ条件: saved_wait > added_cost
```

`r`、最大 body、carrier 分割数、parser/CPU/RAM 上限、response window を layout/site と固定 cost record から決定する。本文が大きいのに hash 数だけで見積もらない。

depth=1/2 の選択は **事前に拘束された deterministic profile schedule** とする。各ノードの live 帯域や cache によって合意の分岐・deadline を変えない。depth=1 の fallback を許す場合はその最悪経路も 3,200 以下でなければならない。足りない場合は profile 未対応として gate を閉じる。

##### 5.6 先読みと paid permit

新しい bundle は既存の支払義務を外さない。複数 carrier の提出、CPU 評価、capture の読み込みについて必要な permit は paid envelope から取得する。

既存 WorkDebit の列を単に省略するのではなく、同一の合計 debit を原子的に扱える transport batching と、合意上必須の逐次依存を区別する。範囲許可やまとめ払いの新しい意味を導入する変更は、本 RFC の暗黙の付随変更とせず、独立した仕様・レビュー・試験を要求する。[S08](../rfc/0024-assets/source-audit.md#s08)

#### 6. C — 原本の先行取得と、非協力時の強制開示

##### 6.1 manifest と index

original claim に対応する `CourtMaterialManifestV2` を拘束し、次を含める。

```text
version / schema / network_rules_id
claim_id / allocation_id / class_id / execution_root / trace_root
material_source_id / authenticated_index_digest
position/head/layer → canonical source request の導出規則
custody obligation の参照
maximum material bytes / response bytes / work / concurrent service
material_until_daa / bond_exit_after_daa
```

manifest の hash は **material が取得可能という証明ではない**。責任の対象と値の由来を固定するものとする。

producer は推論中に既存の canonical material と取得 index を保存・整列し、execution root 確定後に同じ material を配布する。mirror や verifier は任意に先行取得できる。ただし、検査乱数の決定順序は維持し、未来の challenge を予知する必要がある全候補の巨大な証拠生成を通常処理に課さない。

B の数値 frontier は dispute の root totals に依存し得る。キャッシュする場合は root totals digest と site を含めて key を作り、別の root claim の集計を流用しない。

##### 6.2 2 経路

**任意取得の速い経路:** 認証済み bytes をオフチェーンで取得し、検証者が対応する既存の on-chain opening または bounded filing を提出する。第三者から取得した bytes でも root が一致すれば資料として使えるが、第三者の署名だけを court の正しさの根拠にしない。

**強制経路:** 任意取得が失敗したら、既存 original obligation に対して on-chain の有効な要求を受理する。指名された義務者、request、deadline、最大サイズは accepted state で確定する。投稿の tag 245 経路との対応は最新 branch の schema を確認して接続し、tag を推測で割り当てない。

最悪 door は強制経路で評価する。「事前に取れているはず」を理由にこの経路を削除しない。

##### 6.3 起点をずらして時間を隠さない

ZIP の frame では `accepted_daa` から preparation/open/stream/collect が連続し、Final の hold と接続している。[S06](../rfc/0024-assets/source-audit.md#s06)[S12](../rfc/0024-assets/source-audit.md#s12)

本 RFC は、door の始点を単に Ready 後へ移して 3,200 達成と表示することを禁止する。最小限、次を別々に報告する。

- `D_frame = collect_until_daa - accepted_daa`。
- `D_prosecution`＝不一致の検出または訴追開始から terminal の収録まで。起点を明示する。
- `T_publish_before_acceptance` と、その bytes・CPU・保存費用。

最新 ADR-0191 の door 定義と対応させるまでは、最も都合のよい短い区間だけを door と呼ばない。C で前倒しできても、original acceptance 後に残る冷起動・再取得・replay は最悪予算に残す。

##### 6.4 timeout と責任

任意の HTTP timeout、ローカル disk error、mirror の「持っていない」は slash 根拠にしない。default は次がそろった場合だけ成立する。

1. 対象 original claim と義務者が拘束された、有効かつ支払い済みの要求が accepted chain に存在する。
2. 義務者に必要なサイズ・計算・収録機会を含む正規の応答窓が与えられた。
3. canonical deadline 後も有効な完了応答が chain にない。
4. reorg、重複要求、既に履行済みの応答、無効な要求を規則どおりに処理した。

本 RFC は、複数 custodian への「失敗したら次の人へ新しい full window」という無制限の逐次回しを導入しない。既存の指名・再指名回数の上限を予算に入れる。同じ原本の再要求で original door を再開しない。

##### 6.5 保持・共用・費用

同じ root の material はノード内で共用してよい。ただし保持責任・他 claim の lease・reorg での再参照・課金対象は消さない。最終 lease の責任期間が不可逆に過ぎるまで削除しない。

事前公開は費用をゼロにするのではなく、作業時期と重複を変える。producer/custodian/verifier の CPU、wire、disk、保持、WorkDebit、料金と capital は SUM で会計し、実際に並列な工程の時間だけを critical path で計る。[S06](../rfc/0024-assets/source-audit.md#s06)[S08](../rfc/0024-assets/source-audit.md#s08)[S10](../rfc/0024-assets/source-audit.md#s10)

#### 7. 合意状態と codec の契約

概念上の状態は `Prepared/Ready → Open → AwaitDisclosure → AwaitChoice → PositionClose → Resolved`。material 要求の待ちと DA default は、同一 original deadline に拘束された既存 obligation として接続する。新 enum を既存 wire の末尾に無断追加しない。

必須規則:

- claim、allocation、source、session、round、root claim、ruleset で全 message を domain-separate する。
- canonical な byte 順序・整数幅・child 順序・proof の重複排除・長さ上限を定義する。
- 長さと上限を allocation 前に検査する。無効 bytes を大量に解釈する無料 CPU 経路を作らない。
- timeout ぎりぎりの無効 message と partial response は deadline を延長しない。
- 有効な応答が早着しても、正直な verifier に必要な replay/選択/収録時間を奪わない。
- 対応する version、request class、proof schema が全ノードで同じでなければ拒否する。
- tag 230 の codec 案 A/B、tag 245、D2/X3 の状態差分と再生規則との整合を統合 gate にする。

#### 8. door の算出契約

##### 8.1 時間と費用を別にする

料金の SUM はすべての主体の作業を含む。一方、door は依存関係と共有資源の競合を反映した最大完了時間である。

```text
D_new = max(D_honest_close,
            D_latest_legal_responses,
            D_default_at_last_obligation,
            D_restart_path,
            D_reorg_reacquire_path,
            D_admitted_load_path,
            D_other_covered_fault_routes)
```

各 path は preparation、公開、要求、開示、choice、permit、検証者 replay、terminal、収録、recovery から成る。初期の全員無応答だけではなく、**最後まで期限ぎりぎりで正常応答し、最後の要求だけ隠す経路**を含める。

##### 8.2 各段の上限

最小の監査項目は以下。

| 分類 | 記録する値 |
|---|---|
| wire | body、opening、署名、metadata、carrier framing、分割数、再送数 |
| CPU | decode/hash/fold/exact evaluation、正直な verifier の replay、拒否経路の上限 |
| IO | disk 読書き、capture reopen、cache miss、実コピーと再取得 |
| 待機 | request/response/choice/WorkDebit の依存、inclusion margin、queue span |
| 回復 | reorg 深さと回数、restart 回数、再認証、再提出 |
| 同時性 | admitted load、共有帯域・CPU・disk、owner/custodian の競合 |

`bytes / bandwidth` は該当する 1 項だけを表す。メモリコピーの bytes をネット転送として数えたり、同じ再送を 2 回計上したりしない。減算する重複は根拠を記録する。

上限計算は整数・切上げ・checked arithmetic とし、overflow や未知の入力は拒否する。実測の平均や p50 だけで最悪上限を置かない。実測は有界な service envelope を支える証拠であって、任意の Internet 状態に対する数学的な帯域保証ではない。

##### 8.3 3,200 の達成に必要な削減量

旧 terminal 読込を含まない値を基準にする。各 profile で、B/C 等による **検証済みの前段削減**を `S`、A/B/C が追加する末尾位置段・新 terminal・増加 bytes・新 metadata・追加 recovery 等の、旧基準に含まれない時間を `X` と定義する。項目は重複しない ledger で管理する。

```text
32K: D_new = 3,012 - S_32K + X_32K
  → S_32K ≥ max(0, X_32K - 188)

2M:  D_new = 3,196 - S_2M + X_2M
  → S_2M  ≥ max(0, X_2M - 4)
```

これは旧の全体式を新型へ正式に移す前の差分計画に使う必要条件であり、それ自体は実装の達成証明ではない。元の 3,120 上限を維持する場合は、32K が `S ≥ max(0, X - 108)`、2M が `S ≥ max(0, X + 76)` となる。0 で切り上げる前の不足額がそれぞれ 80 増える。

仮に `X` が以下の値だった場合の算術例:

| 未測定の仮定 X | 32K で必要な S | 2M で必要な S |
|---:|---:|---:|
| 128 DAA | 0 DAA | 124 DAA |
| 256 DAA | 68 DAA | 252 DAA |
| 384 DAA | 196 DAA | 380 DAA |

**128/256/384 は測定値でも提案 deadline でもない。** 「2M では前段から数百 DAA を根拠付きで減らせれば目標に届く場合がある」ことを示す計算例に限る。実際の可否は B の増分 bytes と C の強制経路を含む正式な bound による。

##### 8.4 絶対期限と最終段の予約

original acceptance 時に `collect_until_daa`、必要な terminal の最終収録余地、default 処理、保持と bond exit を整合させる。新しい challenge ごとに時計を 0 からやり直さない。[S06](../rfc/0024-assets/source-audit.md#s06)[S12](../rfc/0024-assets/source-audit.md#s12)

終盤の段を単純に `min(now + round_window, collect_until)` で切って、正直な responder の必要時間を失わせる実装は禁止する。各段は残りの必要時間を予約したうえで開始し、そもそも合法的な最悪進行が収まらない profile は admission / activation を拒否する。

ZIP の public conviction は `accepted_daa ≤ now < collect_until_daa` で受理する。したがって、この経路で `collect_until_daa = accepted_daa + 3,200` とする場合、terminal がちょうど `+3,200` で収録されるのは遅い。最終許容収録位置は `+3,199` であり、上限計算にはこの排他的境界の余裕を含める。DA/default の境界と同一ブロック内の処理順も、最新 fold の規則と照合する。[S12](../rfc/0024-assets/source-audit.md#s12)

「入らなかったら timeout で落とせば 3,200」は達成ではない。正直な protocol participant が所定資源で期限を守れることを示す。

#### 9. 記録と arming gate

`PALW_G14_ARMING_RECORDS_V1` の空 record 拒否を外さずに、審査済み evidence を pin する。投稿の `PALW_TIR_FRAME_COST_RECORDS_V1` は最新 branch の型を確認して統合する。存在しない型を本 ZIP の既存 API として実装指示しない。[S07](../rfc/0024-assets/source-audit.md#s07)

必要記録:

```text
source_commit / dependency commits / binary SHA-256 / build flags
network genesis / ruleset fingerprint / proposed activation DAA
class / artifact root / layout / semantics / context / site coverage
A/B/C の version と profile schedule
B_position の body/proof/signature/total と評価 work
各 bundle の frontier 値数 / body / carriers / fold work
effective service envelope と使用機材・負荷・帯域制限
全 path の段別 bound / observed span / D_frame / D_prosecution
reorg / restart / IBD / pruning / sustained-attack の evidence
予算・保持価格・Δ の 5 項との対応
independent review / approval / hashes of logs and results
```

UNKNOWN、未実測、未被覆 fault route、未承認の料金・期限を含む record は arming 不可。測定値や hash の仮置きで gate を通さない。出荷 binary の hash が変わった場合、影響分析と必要な再測定・再 pin を行う。

#### 10. 安全性・完結性のレビュー義務

| 義務 | 示す内容 |
|---|---|
| A-Soundness | 正しい attention/reduction と正しい原本に対し、1 位置経路が誤って有罪にしない |
| A-Completeness | false parent が有効に fold するなら、少なくとも 1 child が false。全 reduction を維持して 1 位置へ至る |
| A-Source | writer-row route が original commitment・HistTile 整合性・first divergence を保ち、別の Q/K/V への付替えを許さない |
| B-Simulation | 新しい depth=2 の開示・選択を展開すると、同じ root と range を持つ旧 2 段の合法遷移になる |
| B-Resources | 全候補を実際に運んだ bytes/work を数え、未開示 branch への新規往復を隠さない |
| C-Availability | 任意取得に依存せず、有効要求の履行または客観的 DA/default まで進める |
| One-outsider | owner 枠・queue・colluding reservations・費用で正直な 1 人の訴追を排除できない |
| Deadline | 最遅合法応答・最後の default・承認範囲の回復でも、必要な各段と terminal が期限内 |
| Retention | 係争と責任期間の間、root だけでなく反証を作る bytes が restart/reorg/IBD 後にも残る |
| Credit | pending な具体的不正や未解決義務を無視して Final・認証・払い出し・bond 解放へ逃がさない |

`One-outsider` は「owner が 1 人で動く fixture」だけでは満たさない。ZIP には original frame 当たり owner 数の上限があるため、共謀者による占有や正直な filing の starvation を含む試験が必要である。[S10](../rfc/0024-assets/source-audit.md#s10)

未知の coverage を「attention は 1 位置になった」で埋めない。重い HistTile-copy fault、別 kernel、decode/input/output/state fault が既存経路に残る場合、その経路も同じ profile 全体の door gate を通す。

#### 11. 必須試験

##### 11.1 単体・性質試験

| ID | 条件と期待結果 |
|---|---|
| A-01 | 任意の端 tile 長 1〜16、head/layer/position で新旧の正直な結果が一致 |
| A-02 | Q/K/V の任意の必要 lane、mask/scale 等を改ざんし、対応する具体的不正を検出 |
| A-03 | wrong root、wrong source、別 claim、別 position、同値 bytes の型違い、重複/欠損 proof を拒否 |
| A-04 | root max/sum/weighted-sum の各種 false total を局所化し、1 位置 softmax の誤実装を検出 |
| A-05 | HistTile コピーだけを改ざん／writer だけを改ざん／両者と下流を整合させて改ざん。被覆漏れなし |
| A-06 | 完全 witness の bytes・parser・CPU が profile 上限内。超過は未対応/拒否であり無実の slash ではない |
| B-01 | depth=2 bundle を旧 2 段に展開した遷移と同値。非 16 冪、端 tile、1 位置を含む |
| B-02 | 中間 child を明示しない frontier でも全階層の fold/interval を検査 |
| B-03 | 256 候補中の任意 1 候補の欠損、不正値、重複、gap、overlap、異なる親を拒否 |
| B-04 | fragment の最遅到着・欠損・順序違い・同じ fragment 再送で deadline が延びない |
| C-01 | mirror 全停止、producer/Panel/custodian 全非協力で on-chain 有効要求→適格 default |
| C-02 | 有効原本を出し続ける共謀者に対し、外部 1 人が exact conviction まで到達 |
| C-03 | ローカル timeout、無効要求、資料の取得済み状態だけで誤 slash しない |
| G-01 | 既存 permit、料金、lock、class/source binding の迂回を拒否 |

##### 11.2 実ノード E2E

32K と 2M を別々に、出荷候補 release binary で実行する。profile を増やす場合も個別の coverage と最大値を記録する。

必須シナリオは、正常系、正直な cold verifier、誤った計算への全員共謀、資料完全不提供、最後だけ不提供、最大に近い合法遅延、無効・巨大メッセージ、連続 false claim、colluding owner 枠の占有、queue 飽和、再起動、reorg、IBD、pruned import、fence 前後である。

帯域 125,000 と 12,500 B/DAA は再現条件として使えるが、出荷 envelope の保証値に自動採用しない。公称帯域だけでなく、競合と carrier/permit 制約を含めた実効値を測る。許容範囲外の障害は別記し、有限時間保証の前提と混同しない。

合格には、導出上限 ≤ 承認済み上限、全実測 span ≤ 対応する導出上限、正直な claim の不当 default なし、担保・credit・保持の整合、全ノードの同一結論を要求する。

#### 12. 実装手順と変更箇所

| 段階 | 作業 | 確認する現行箇所 |
|---|---|---|
| 0 | 最新 door branch、ADR-0191、tag 245/230、D2/X3 を ZIP と照合し、正式式と版を固定 | 添付にない branch。未確認のまま arming しない |
| 1 | A の 1 位置 phase、canonical writer-row resolver、完全 witness builder と反例試験 | `palw_tir_dissect_v1.rs`、`palw_tir_court_v1.rs`、`palw_tir_step_v1.rs` |
| 2 | B の depth=2 disclosure/choice、codec 上限、展開同値性、増分 bytes と時間比較 | `palw_attn_dissect.rs`、TIR dissection / public source / owned state |
| 3 | C の manifest/index、先行保存と取得、強制経路、永続 journal・保持 | `palw_tir_public_*`、`kaspad/src/palw_tir_public_custodian.rs`、既存 acquisition driver |
| 4 | A/B/C を SUM・期限・料金・capacity・profile admission に接続 | `palw_tir_public_allocation_v1.rs`、`palw_tir_public_owned_v1.rs` |
| 5 | 出荷 binary で測定、全 fault route と E2E、D2/X3 の回復試験を完了 | node / processor / drill / evidence |
| 6 | 新 RFC/ADR 採択、台帳・Spec 更新、record pin、fence-crossing drill | G14 gate、最新 frame-cost gate、release manifest |

carrier の wire 番号、state version、delta codec、API 名の正式割当は段階 0 の照合後に行う。説明用の新型名をそのまま既存 API と誤認しない。

#### 13. 移行・有効化・失敗時対応

- fence 前は byte 単位で旧動作を維持する。open session は開始時の ruleset で完了させ、途中で depth や deadline を変更しない。
- 新旧 witness の受理条件を明確にする。旧経路の fallback が残るなら、その最悪値も含めて新 profile の上限を評価する。
- 新規受付を閉じても既に発生した責任・必要証拠・lock は解放しない。
- 一部 profile だけ合格しても「全 fence が有効化可能」と報告しない。2M の未達を 32K の成功で覆わない。
- 3,200 未達なら、未達の phase・route・資源を報告し、追加最適化、対象 profile の延期、または明示的な別上限の承認へ進む。10,000 へ自動変更しない。
- DAA 16,000 に間に合わせるために未測定 record を pin しない。最新の network DAA を読み取り確認し、fence を跨ぐ余裕がない場合は高さの延期を別途承認する。
- t12 への配備、main の更新、実際の fence 設定は本 RFC の成果物作成に含めない。

#### 14. 受入チェックリスト

```text
[ ] 最新 ADR-0191 / door branch / tag 245 / tag 230 / D2/X3 と整合
[ ] 3,120→3,200 の変更と、関連 liability/retention/Final への影響を明示承認
[ ] A: 行の original authentication と HistTile-copy fault の被覆を証明
[ ] A: 完全 witness の最大 bytes/work を実測・導出
[ ] B: depth=2 を旧遷移へ展開でき、全候補を開示・検証
[ ] B: 増加 bytes/work を含めても、最悪 wait 削減が有効
[ ] C: オフチェーン取得なしでも on-chain 履行または適格 default
[ ] door の始点を変えず、preparation/冷起動/再取得/permit を含む
[ ] 32K と 2M の全 fault route で導出上限 ≤ 承認済み上限
[ ] 全員共謀・外部 1 人・capacity/owner 競合を含む node E2E
[ ] restart/reorg/IBD/pruning 後も同じ裁定と責任保持
[ ] Δ の 5 項、保持・court・転送の価格、必要 capital を実測・承認
[ ] 出荷 binary の evidence と独立レビューを record に pin
[ ] RFC と ADR の明示採択、Spec/台帳/状態の更新
[ ] fence-crossing drill と配備の別途承認
```

#### 15. 不採用・本 RFC に含めない案

| 案 | 採らない理由 |
|---|---|
| door をただ 10,000 にする | 本 RFC の目標を満たさず、原因と資源上限を残す。別承認の選択肢としては排除しない |
| root だけを公開し、非協力時は諦める | material の取得・客観的 default が閉じない |
| 1 位置の Q/K/V 本文だけ運ぶ | original commitment の opening と reduction 条件が欠ける |
| 2 段目の選ばれそうな branch だけ先送りする | 非協力相手が別 branch を選べるため最悪往復数を減らさない |
| 全 frontier は hash だけなので安いとする | 数値 fold の確認に必要な partials を数えていない |
| off-chain cache がある場合の値で arming する | cold/withholding/reorg の最悪経路を保証しない |
| 巨大な準備を Ready 前に動かして door を短く表示する | 始点を変えただけなら E2E 改善の証拠にならない |
| timer を短くして応答不能な正直な主体を default にする | 客観的手続きと正直な実行可能性を損なう |
| signature、多数決、計算 validity proof で exact court を置き換える | 本提案の保証と現行 PRINCIPLES に適合しない |

#### 付録 A. 外部参考の位置づけ

外部の一次資料として、OP Stack の Fault Dispute Game specification（single-step / preimage の扱い）および Arbitrum の BoLD overview（有界な時間と 1 人の正直な参加者を扱う説明）を参照した。これらは設計の一般的な比較材料に限る。**MISAKA の実装・door 式・閾値・採用済み規則の根拠には用いていない。** 他 protocol の時計、VM、trusted role、例外措置を移植しない。URL は source-audit の外部参考節に記録する。

#### 付録 B. 同梱の計算補助

`door_budget.py` は、既存の 16 分岐 recurrence、A/B の論理段数、§8.3 の必要削減量、明示入力された path 上限の加算を検査する補助ツールである。**最新 ADR-0191 の正式式でも、出荷 node での性能測定器でも、arming の認可器でもない。** テンプレートの UNKNOWN は UNKNOWN のまま残し、測定済みと装わない。

#### 付録 C. 逐次 step block 搬送の削減を優先する(2026-10-19 追記、ユーザー指示)

**優先順位:** ML-DSA の大きさの変更より先に、court の「段ごとに 17,408 B の step block を逐次搬送する」構造を減らす。ML-DSA-87(署名 4,627 B + 公開鍵 2,592 B ≈ 7.2 KB/carrier tx)は 2 番目の要因であり、本付録の対象外。

##### C.1 現在の値(2M、12,500 B/DAA、式による derived_upper。実測ではない)

| 項 | DAA |
|---|---:|
| 逐次 step 搬送 (K+1)·span = 9 × 234 | 2,106 |
| 探索往復 (16+2) × 46 | 828 |
| 最終裁定(1 位置、≈12 KB 見込み) | 350 |
| ID・提出・open 窓など | 190 |
| **合計** | **3,474** |

3,120 まで 354 DAA(約 10.2%)の削減が要る。混雑・reorg への余裕を見込み、**単発 door の設計目標は 2,500〜2,700 DAA** とする(承認済みの上限ではなく設計目標)。

**span = 234 の内訳(要確認、第一段階):** 17,408 B ÷ 12,500 B/DAA ≈ 1.4 DAA なので、234 DAA は純粋な bytes の転送ではない。現行の fit `span(B) = 44 + ⌈B / 920⌉` は ADR-0190 の 2 点から外挿したもので、1/10 の感度ではバイト項を ×10 して 44 + ⌈174,080 / 920⌉ = 234 としている。つまり実効は「1 DAA あたり約 920 B(感度時 92 B)」で、取り込み待ち・確認深度・安全窓・frame の分割などが含まれていると考えられる。**この内訳を分解するまで、bytes 削減による door の改善量は正確に計算できない。**

逐次搬送回数を減らした場合の条件付き試算(234 DAA/回がそのまま消え、他の項が増えない場合。先読みで増える bytes・往復は含まない):

| 逐次 step 搬送 | 搬送 DAA | door 合計 |
|---:|---:|---:|
| 9(現状) | 2,106 | 3,474 |
| 8 | 1,872 | 3,240 |
| 7 | 1,638 | 3,006 |
| 6 | 1,404 | 2,772 |
| 5 | 1,170 | 2,538 |

##### C.2 設計案(組み合わせ)

1. **commitment による局所化:** 各段で子 16 個の部分値を全部開示せず、子ごとの commitment(例:32 B × 16 = 512 B)で不一致の子を特定する。
2. **実データは最後の区間だけ:** 不一致が特定された区間についてだけ、17,408 B の部分値と裁定に必要な operand を開示する。
3. **2 段先読み(本文 B):** 先読みするのは大きな部分値ではなく、可能な限り小さな commitment に限る。全分岐の部分値を先送りすると総 bytes が増え、逐次待ちの削減を打ち消し得る。
4. **認証経路の集約:** 同じ root から複数の値を開くときは Merkle multiproof で共通経路を共有する。fixture の実測では認証経路 27,156,480 B に対し、重複除去後の operand は 74,720 B(`release/t12-16000` 20fff6d90)。1,000 件同時の court での disk I/O・検証 CPU の詰まりを避ける。

これは ZK・SNARK を導入する話ではない(PRINCIPLES §8)。既存の interactive dissection を、hash による commitment と opening で効率化するもので、optimistic rollup の二分探索型 fraud proof と同じ考え方である。

**成立条件(どれか一つでも欠ければ採用しない):**
- 既存の trace commitment が、各段の子の状態を認証できること。
- 双方が提出する子の commitment と元の root との整合性を、ノードが決定論的に検証できること。整合しない側は、その時点で負ける。
- 最後に開示する実データを、元の commitment に結びつけて検証できること。
- **分岐点:** 現在の 16 個の部分値が「比較のため」だけのものか、「各段の fold の計算を客観的に検査するため」に必須かを確かめる。前者なら hash 化できる。後者なら、その検査を最後の局所裁定へ移せるか(最終区間で、子の部分値から親の値への fold を 1 回だけ検査すれば足りるか)を検討する。

##### C.3 同時発生と証拠サイズ

- 単発の door だけでなく、同時に N 件の court が走るときの `max_i door_i(2M, N) ≤ 3,120` を受入条件とする。court の容量は `palw_court_capacity_v1` が扱う(court 専用の block 予算、予約枠、期限順の割当、新規 claim の受付制限)。ただし値は仮で、step block の搬送がその予算に計上されているかは未確認。
- court 1 件の証拠サイズの最悪値を、固定上限として証明する。door が小さくても、証拠量が大きければ 1,000 件同時発生時の帯域問題は残るため。

##### C.4 進める順番

1. `span = 234` の内訳を分解する(通信待ち・確認深度・安全余裕・bytes)。
2. hash による局所化と 2 段先読みを、2M・正直な相手・悪意ある相手の最悪時の往復数と bytes で比較する。
3. 認証経路をまとめ、検証速度とメモリを測る。
4. 隔離環境で 1・10・100・1,000 件の同時 court と reorg を再現し、どの claim も締切を超えないことを試験する。

##### C.5 目標案

| 項目 | 目標案 |
|---|---|
| 2M の単発 door | 2,500〜2,700 DAA |
| 不一致の局所化 | 小さい commitment が中心 |
| 大きな step block | 最終的に必要な区間だけ |
| court の証拠サイズ | 最悪時の固定上限を証明 |
| 1,000 件同時の court | どの claim も締切超過ゼロを試験 |
| chain 外の認証経路 | 共通経路を重複排除(multiproof) |
| IBD | 10,000 DAA ごとの checkpoint と安全な pruning を検証 |

#### 整合性修正 — 2026-10-19(RFC 全件整合性監査)

- door ≤ 3,200 は**対象 profile の性能目標**で、主契約(RFC-0014 2026-10-19 改定の 3,120 / 10,000)とは別に管理する([RFC-0026](../rfc/0026-court-policy-master-contract.md) §2.1)。付録 C.3 の「同時 N 件で max door ≤ 3,120」は、この主契約の申立て窓ではなく性能の受入条件として読む。
- **本文 B の各 round の数値 fold 検査を現行の規範として維持する。付録 C.2 の commitment(hash)中心の局所化は未採用の研究・設計候補。** 親 100・子 40 と 50 がすべて正しく認証されていても `40 + 50 ≠ 100` の集約不正は残り、最終 leaf だけの検査ではこれを見落とす。採用するなら内部の親子 fold 不正を局所的に反証できる証拠と完結性の証明が先(F06、RFC-0026 §7.2)。
- 最終裁定の約 12 KB は 1 位置の見込みで、court 全体の証拠量ではない(RFC-0026 §7.1)。
- 土台に挙げた `palw_court_capacity_v1` は別 branch(`release/t12-16000`)の休眠実装で、`pre` には未統合(F10、RFC-0026 §8.1)。

---

<a id="source-v1-0025-L00001"></a>

### 旧RFC-0025 / 原文1–705行

出典: [0025-scalable-objective-court-many-models-mass-disputes.md](../rfc/0025-scalable-objective-court-many-models-mass-disputes.md)。配置理由: 主題別統合。

### RFC-0025 — モデルが増え、不正に対する court が大量発生しても処理を継続できる客観的 court

- **状態:** Draft / Review Required / 未実装・未有効化。2026-10-19、ユーザー指示により RFC-0025 として登録。
- **主目的:** モデル数と不正に対する court 件数が増えても、訴追権・客観的裁定・担保回収の可能性を失わず、有限の混雑を処理して復帰できるプロトコルを定める。
- **通常時の目標:** original claim の `accepted_daa` を起点として `door ≤ 3,200 DAA`(3,120 → 3,200 の変更は明示的な承認対象。§16)。
- **混雑時の方針:** 合意状態から導出した処理予約に基づき、必要な案件の証拠提出期限・Final 保留・証拠保持・bond 返還可能時点を連動して延長する。全件の固定 3,200 は約束しない。
- **前提とする RFC:** [RFC-0024](../rfc/0024-court-door-3200-to-ctx-2m.md)(A=最終裁定の 1 位置化、B=2 段先読み開示、C=先行取得と強制開示、付録 C=逐次 step 搬送の削減)。本文中の「前版」は RFC-0024 を指す。
- **関連する既存実装:** `consensus/core/src/palw_court_capacity_v1.rs`(休眠。court 専用の block 予算・予約枠・期限順の割当・新規 claim の受付制限・囮の価格・未裁定は無報酬。値は仮)。本 RFC の §5〜§7 はこれを一般化・強化する位置づけで、統合時に対応を照合する(§15 段階 0)。
- **非変更:** ZK・SNARK/STARK・zkVM・PCS による計算の正しさの証明は使わない([PRINCIPLES](../PRINCIPLES.md) §8)。モデル単位の審査は行わない(PRINCIPLES §9)。本文 §15 段階 1 の「モデル profile を審査」は、claim 受付前に class の形状が証拠上限に収まるかを決定論的に判定することであり、モデルの品質・公開状況の審査ではない。
- **付属資料:** [0025-assets/](../rfc/0025-assets/README.md)(ADR 草案、source-audit-v2、capacity_planner.py と 27 試験。登録時に 27 試験の合格を再確認)。前版の資料は RFC-0024 の [0024-assets/](../rfc/0024-assets)。

---

> **決定案:** 個々の証拠と検証作業を小さく有界にする。その上で、有限の処理容量を予約・配分し、混雑時には時間と担保・保持責任を一緒に延ばす。容量限界を超える新しい経済的責任の受付を抑制し、既に受け付けた不正の訴追を優先して処理する。ブロック容量の拡張は、実測・安全性審査を経た別の容量 profile として行う。

#### 0. 前版との関係・根拠

本書は前版の A＝最終裁定の1位置化、B＝最大2段の先読み開示、C＝先行取得と強制開示を引き継ぎ、モデル横断の容量管理・混雑延長・pruning/IBD を追加した**統合版**である。実装では本書を上位とし、衝突しない算術・証拠認証の詳細は [前版RFC](../rfc/0024-court-door-3200-to-ctx-2m.md) を参照する。

前版の「全受理負荷で必ず3,200以下」「期限を途中で延長しない」は、v2 の **通常予約と、事前合意済みの有界な混雑延長**に置き換える。待ち行列の開始を遅らせて、元の受付からの時間を隠すことは禁止したままである。

対象は添付 `misakas-pre(2).zip` の snapshot とユーザー提示の数値であり、最新 branch・出荷 binary・現在の t12 の監査ではない。追加の読解根拠は [source-audit-v2.md](../rfc/0025-assets/source-audit-v2.md)、前版の根拠は [reference-v1/source-audit.md](../rfc/0024-assets/source-audit.md) にある。

コード上の重要な確認事項:

| 確認事項 | 根拠 | v2 への影響 |
|---|---|---|
| 既存 frame は `collect_until`、`material_until`、`bond_exit_after` を連動して導出する | V2-S01 | bond返還だけの延長では不十分 |
| public conviction は `[accepted_daa, collect_until_daa)` で受理。現行コメントは期限固定・非延長を要求する | V2-S02 | 新ruleset・明示的移行が必要。排他的境界に注意 |
| custodian の署名済み pledge の保持期間と frame の期限が照合される | V2-S03 | 期限だけ書き換えると署名・契約・料金が不整合になる |
| owner 上限は8で、全ownerがactiveなら追加Openを拒否する | V2-S04–05 | idle回収だけでは共謀者による枠占有を解決していない |
| pruning snapshot は未完の close group のbytesも運ぶ | V2-S06 | rootだけ残して未完courtを削除できない |
| snapshotの既存close上限の試験値は2,250,000 B | V2-S07–09 | 新しい小型証拠上限を既に達成しているとは扱わない |

ユーザー提示の **17,408 B** は新しい分割片サイズの提案値として扱う。snapshot検索では、この値を現行court stepサイズの定数として裏付ける一致は確認していない。モデルのFFN幅の17,408等と混同しない。

以降の「MUST／禁止／必要」は提案する規範要件を表す。未測定の数値・未証明の被覆を採用済み実装の事実として扱わない。

#### 1. 保証と非保証

##### 1.1 維持する安全性

producer・Panel・custodianが共謀しても、適格な外部検証者1人が、公開・認証可能なmaterialと所定のオンチェーン手続きから訴追できなければならない。

- **計算不正:** 認証済みoperandと既存のexact算術から矛盾を示し、ノードが同じconvictionを導出する。
- **開示義務違反:** 有効な要求と正規の応答機会があったのに資料が開示されなかった場合、ノードが同じDA/defaultを導出する。
- **混雑:** 証拠の配送待ちを理由に、正直な参加者を不当defaultにしない。未解決の責任をFinal・報酬支払い・bond解放で逃がさない。

DA/defaultは、隠れた計算が誤っていたことの証明ではない。両者の責任・罰・返金・判定コードを分ける。

##### 1.2 条件付きの生存性

通常時3,200 DAAおよび混雑時の有限完了は、次の明示的な条件の下で保証する。

1. base chainが進行し、適格なcontrol/noticeと証拠に対する有界な収録機会がある。
2. ネットワーク・CPU・IO・reorg・復旧が、承認済みservice envelope内にある。
3. 新規流入はadmissionによって有界化され、既存予約の容量が後続流入に奪われない。
4. 1人の検証者が追う案件について必要な検証者資源と費用が、当該profileに含まれる。

「全員共謀」はproducer等の協力を前提にしないという意味であり、**全ブロック生成主体による永久検閲、無限の通信断、無制限の有効申立てを有限容量で全て即時処理することまで保証しない**。1人が1案件を訴追できることと、その1人が1万案件を並列処理できることも別である。

##### 1.3 モデル数と負荷の区別

登録モデル数 `N_models`、未確定original claim数 `N_claims`、独立な係争数 `N_courts`、異議参加者数 `N_filers` を別々に計数する。1万モデルが登録されても、1万courtが常時発生するとは限らない。一方、1モデルへの1万の独立な実行claimが1万courtを生む場合はある。

容量・料金の単位はモデル名ではなく**検証作業と未解決責任**とする。モデルを多数登録しても帯域割当を無料で増やせない。クラス別制限は補助であり、アドレス・class・モデルを分割するSybilへの防御をそれだけに依存しない。

#### 2. 固定上限の証拠と局所演算

##### 2.1 3種類のサイズを区別する

| 項目 | 定義 | v2 draftの扱い |
|---|---|---|
| `S_step_fragment_max` | 1個のcanonical proof fragment。定義したheader・認証metadataを含む | **17,408 Bを暫定候補**とする。carrier追加分は別計上 |
| `S_close_max` | 最終exact courtに必要な全operand・proof・規則・metadata | **32 KiBを開発目標候補**とする。未達なら有効化不可 |
| `S_transcript_max` | 1つの独立なcourtの、開始からterminalまでの最大オンチェーン総量 | **64 KiBを`SC64`の開発目標候補**とする |

32/64 KiBは今回提案する設計目標であり、導出済み・実測済みの上限ではない。正式な`S_*`は完全なcodec、全fault routeと最悪分岐から導出・審査・pinするまで`UNPINNED`とし、armingを拒否する。`SC32`等は将来候補であり、本書の容量表では感度分析にだけ使う。

`S_transcript`には、notice、request、先読みfrontier、部分値、source opening、原本強制開示、permit/WorkDebit、choice、close、terminal、署名、carrier framingを含む。裁定成立に必須の準備データがチェーン上に載るなら、名前を「preparation」と変えて除外しない。全非協力・キャッシュなしの強制経路を算入する。

共有物は**各courtを単独で成立させるときの上限**には全量を算入する。並列court間のdedupは実績として別会計する。ネットワーク再送・orphan/reorg再提出は`S_wire_total`と別の回復予算に計上し、費用や時間から消さない。

分割片が17,408 B以下でも総量が448 KiBなら、容量計算には448 KiBを使う。**fragmentへの分割は運搬を可能にするが、総証拠量の削減ではない。**

##### 2.2 固定上限が成立する範囲

受理するprofileの集合 `P`について以下を要求する。

```text
すべての p∈P、許容M,C、対象fault routeについて:
  S_close(M,C,p)       ≤ S_close_max
  S_transcript(M,C,p)  ≤ S_transcript_max
  W_verify(M,C,p)      ≤ W_verify_max
  IO_verify(M,C,p)     ≤ IO_verify_max
  RAM_peak(M,C,p)      ≤ RAM_peak_max
  rounds(M,C,p)        ≤ R_max
```

初期context対象の上限候補は `2,097,152 tokens`。モデルの層数、tensor形状、head幅、program長、trace leaf数、address幅、Merkle深さ、operand数も有限の上限を持つ。これらから分岐数・proof長・parser作業の上限を導出する。

これは**許容範囲内で共通の上限を持つ**という意味であり、無制限のモデル・contextに対してMerkle proofまで真に定数長になるという主張ではない。profile追加はグローバル上限を自動で引き上げない。

##### 2.3 Aの一般化: 1位置でも大きければさらに分割する

attentionのhistoryを1位置に絞っても、head幅・reduction出力幅・複数headのK/V・query・原本leafが大きければ上限を超える。この場合、演算を固定幅の積和、reduction部分値、state更新等の**最小検証単位**にloweringする。

canonicalなoperandアドレス・型・値・program fragment・前後state・original rootへのpathを認証する。sourceの部分領域だけを開くには、元のwriter leafか明示的なsubcommitmentが必要である。巨大leafの一部のbytesだけを全leafのhashに対するproofと称してはならない。

分割で元の算術、丸め順、overflow、mask、正規化、first divergenceが変わらないことを証明する。整数Sum/Maxのexact foldを使える場合と、一般の浮動小数点reductionを区別する。FPを勝手に再結合しない。softmaxを1位置だけで正規化し直さない。

多段化すると総round数・総proof量が増えるため、`S_close`だけでなく`S_transcript`も再導出する。上限超過profileは、**claim受付前**に未対応として拒否する。受理後に「モデルが大きく証拠が入らない」ことを検証者の過失にしない。

##### 2.4 全fault routeの被覆

attentionだけでなく、GDN/recurrent state、FFN・MoE、logits、token selection、tokenizer/入力解釈、出力同一性、HistTileコピー、memory/address、artifact→operand認証を対象にする。重い旧fallbackが残る場合、その最悪値をprofileに含める。

全ノードが読む必要があるmodel/program materialを小さい証拠に置換できるかは別の実装義務である。冷状態の準備・モデル取得・外部検証者の再実行費用がモデルサイズに依存することはあり得る。これをcourt最終証拠の定数上限と混同しない。

#### 3. A・B・Cを統合した経路

```text
モデルprofileを審査・rootに固定
        ↓
original claim受付: 容量・保持・最大延長費用・訴追機会を予約
        ↓
C: 原本と取得indexを生成時から保存し、任意に先行取得
        ↓
有効noticeを小さい保護control経路で収録
        ↓
合意状態にcourtを登録し、Final/bond/保持を保護
        ↓
B: 最大2段の全候補を先読み開示 → 検証者がまとめて選択
        ↓
A: 1位置、必要なら固定幅primitiveへ局所化
        ↓
認証済み最小witnessをノードがexact検証
        ↓
conviction / DA-default / 正規の棄却
        ↓
決済・回復安全性を確認して責任を解放 → 安全な証拠pruning
```

Bは、各段で次に選ばれ得る**全候補**を含む。16分岐2段なら最大256のfrontier候補になる。部分値・認証path・CPUを含めて予算内の場合だけ使用する。大きいprofileではdepth=1等を開始前に固定し、相手の協力を前提にした省略をしない。

tile→positionの段は最後のB bundleに含め、独立した追加往復を避けてよい。ただしlogicalな被覆を省略しない。

Cのオフチェーン経路は通常時の高速化であり、最悪上限はオンチェーン強制開示または客観的defaultに到達する経路で算定する。hashだけの事前公開をデータ可用性保証と呼ばない。

#### 4. 時間の定義: 通常3,200、混雑時は予約に応じて延長

##### 4.1 変数

| 変数 | 意味 |
|---|---|
| `a_i` | original claimのaccepted DAA。後から変えない |
| `n_i` | 有効なnoticeが受理されたDAA |
| `s_i` | 重いcourt処理の予約開始。受付の起点とは別 |
| `F_i` | 全合法進行・最遅応答・default・必要回復を含む、terminalの最終収録可能位置 |
| `d0_i` | 通常の排他的期限 `a_i + 3,200` |
| `E_i` | 合意状態から導出する混雑延長。後記の上限内 |
| `d_i` | 有効な排他的`collect_until` |

```text
d_i = max(a_i + 3,200, F_i + 1)
E_i = d_i - (a_i + 3,200)
D_frame_i = d_i - a_i = 3,200 + E_i
```

`F_i`はcapacity schedulerとphase別の有界テンプレートから導出する。`N×bytes/rate`だけで求めない。単発のbound自体が3,200を超えるprofileを「混雑した」として隠さない。そのprofileは通常時未達である。

`D_active`、`D_queue`、`D_frame`、`D_notice_to_terminal`、`D_bond_lock`を別々に報告する。実処理だけ3,200以下でも、`a_i`から1万DAA待った案件を「door3,200達成」と表示しない。

##### 4.2 受付窓とterminal余裕

`notice_until`と`collect_until`を分離する。有効noticeの受付窓の後にも、反証作成・応答・最終収録に必要な時間を予約する。`notice_until`は最小検査時間と所定収録機会を奪わない形でprofileから導出する。

有効noticeの到着がその許容窓の最終位置だった場合も、`F_i`を守れることを要求する。準備や異議検出に必要な仕事を開始前へ移して計上から消さない。modelを事前取得済みとするprofileはその前提を明記し、完全cold profileとは区別する。

`collect_until`は排他的境界なので、terminalは`now < d_i`で収録する。境界ぴったりに収録してよいとするoff-by-oneを禁止する。[V2-S02](../rfc/0025-assets/source-audit-v2.md#v2-s02)

##### 4.3 通常・混雑・過負荷

- **Normal:** 予約済みの全phase・冷状態・保証対象負荷が`a_i+3,200`に収まる。`E_i=0`。
- **Congested:** 審査済みの有限burstにより将来の処理枠を予約する。`0<E_i≤E_max`。その案件の期限と責任を連動延長する。
- **Overload admission:** 新しい経済的claimを受けると`E_max`、保持予算、active state上限、control容量を超えるなら、その新規受付を抑制する。既存の有効courtを捨てない。
- **Service failure:** 合意されたservice envelope自体が崩れた場合は別扱い。通常の混雑達成として報告せず、安全なFinal保留と復旧手続きへ移る。無制限障害時の有限返還を約束しない。

`E_max`は一律10,000という暫定値で埋めず、処理容量、burst契約、保持・担保費用、復旧条件から決める。10,000件のburstを対象にするなら、その最終案件まで収容できる値かを確認する。

##### 4.4 1 originalに複数courtがある場合

各courtは個別の`d_i`と予約を持つ。original frameの実効`collect_until`と経済的保留は、そのframeを参照する有効な未解決courtと元の責任窓から必要な最大値を導出する。

```text
frame_collect_until = max(base_frame_collect_until, applicable_court_deadlines)
```

frameの保留が伸びても、既に走っている別courtの個別deadlineや`notice_until`を一緒に延ばしてはならない。全ての判定入口はv2の個別期限・frame保護を正しく参照し、v1の単一termsだけを読むconsumerを残さない。

#### 5. モデル横断の容量予約とスケジューリング

##### 5.1 service profileと資源ベクトル

`CourtCapacityProfileV2`は、次をrulesetに固定する新しい提案型である。

```text
profile_id, ruleset_id, activation_rule
court_wire_rate_lower_bound      [B/DAA]
control_wire_rate_lower_bound    [B/DAA]
verify_work_rate_lower_bound     [work/DAA]
disk_service_rate_lower_bound    [B/DAA]
max_service_gap_daa
max_court_mass_per_block / max_control_mass_per_block
max_active_state_bytes / max_inflight_witness_bytes
max_notice_bytes / max_notices_per_window
max_nonpreemptible_work / max_parser_work
max_reorg_depth / replay_budget / inclusion_margin
E_max / escrow_price_id / resource_template_root
```

各courtは`(wire bytes, consensus work, verifier work, IO, live state, retained byte-DAA, control数)`の資源ベクトルを持つ。bytesだけで軽重を決めない。CPUを大量消費する小さいwitnessも容量を予約し、無効codecの拒否処理にも上限を設ける。

実測値の平均を合意上の下限にしない。実測は、指定機材・ネットワーク・負荷に対するservice envelopeの根拠として使用する。各ノードが自分の回線速度を測り、独自に`E_i`を計算する実装は禁止する。

##### 5.2 original claim受付で責任容量も確保する

新しいoriginal claimを報酬対象として受理する前に、そのprofileの検査窓・準備・保護notice・許容する敵対的分岐・最悪court・保持と回復を予約する。全claimを先に無制限受理し、後からcourtだけを容量不足で拒否する構造は禁止する。

異議が生じないときの未使用容量は他のbest-effort処理に貸してよい。ただし、予約を行使された時点で取り戻せることと、既に開始した非preemptible処理の最大遅延を含めて保証する。

複数claim間で同時不正が起きないという仮定は使わない。指定burstの全claimが同時にcourtを必要とする場合をadmissionで評価する。確率的overbookingを採用するなら、それを全員共謀時のworst-case保証として表示しない。

実装の強化前に、既存owner上限等によってこの予約モデルを満たせないprofileを無理に有効化しない。

##### 5.3 予約カレンダー

各courtのphaseテンプレートは、依存順序、最大bytes/work、相手の合法応答窓、所定収録余裕、terminal/default、回復を含む。frame受付時の最悪notice時刻と、実際のnotice受理後の残余予約を整合させる。

accepted chainの決定的な順序から`reservation_seq`を割り当て、空き処理枠へ予約する。local mempool順、local clock、見えたpeer数、operatorの判断は用いない。予約は合意stateに記録し、再起動・reorg・IBDで再生可能にする。

**先行案件の予約を、後から到着した通常の申立てで後ろへ動かしてはならない。** 早期完了分で後続処理を前倒ししてよいが、参加者への応答義務・deadlineを不意に短縮しない。

資源`r`と任意のカレンダー区間`[u,v)`について、予約需要が、その区間で供給可能な下限と既存予約を超えないようにする。

```text
Σ demand_of_reserved_phases(r,[u,v)) ≤ guaranteed_supply(r,[u,v))
```

これはphaseのrelease時刻、相手の応答依存、最大service gap、fragmentの非preemptible時間、control枠を考慮したチェックでなければならない。期間全体での総bytes一致だけでは十分でない。

初期実装は、固定比率の複数laneに完全なphaseテンプレートを予約する保守的な方式でよい。将来、EDF/DRRやphase単位詰込みで効率を上げても、各資源の供給保証と既存予約を維持する。アルゴリズム名だけでdeadline保証を宣言しない。

##### 5.4 controlと証拠本体を分離する

notice、応答要求、extension record、terminal/default、bond/finality更新には保護control容量を与える。巨大witnessがブロックを埋めて、訴追開始や既存処理の完了まで止めることを避ける。

上限のあるnoticeには、original claim、scope、争点へのcommitment、profile、料金/担保、再送識別子を含める。notice受理は有罪認定でも全証拠検証でもないが、所定の継続義務を開始し、Finalと責任を保護する。

「control枠がある」だけで1人の外部検証者の収録を保証したとは扱わない。control帯域への敵対的流入、料金、枠の占有、正規noticeの最大収録遅延も明示的に評価する。

##### 5.5 容量枠と実際の収録保証は異なる

ブロックの上限やcourt用空き容量を定義することは、特定の未収録transactionが必ず入ることの証明ではない。ローカルmempoolにしか存在しないtransactionを「含めなかった」としてブロックをinvalidにしてはならない。

本書の保証には、acceptedな義務・予約に対応するpublic carrierへの有界な収録規則または明示的なbase-chain inclusion前提が必要である。consensusで強制できる容量隔離と、proposerの収録行動・ネットワーク配送の前提を分けて記録する。

応答時計は**予定された正規の処理機会**に基づいて進める。有効bytesが届いたときだけ時計を進める方式は禁止する。相手が永久に沈黙すると時計も永久停止するためである。

逆に、合意上必要な収録機会が与えられなかったことを、相手の資料不提供と同一視しない。service違反の扱いと参加者defaultを区別する。

#### 6. 混雑時の延長・bond・経済的Final

##### 6.1 有界なextension contract

v2のoriginal acceptanceに、`base_collect_until`、`extension_policy_id`、`E_max`、最悪保持期限、追加費用の予約を含める。custodianと責任主体は、この最大範囲を含むtermsに署名する。

```text
CourtExtensionV2 {
  frame_id, prior_effective_terms_hash,
  scheduler_profile_id, reservation_seq, reservation_root,
  old_collect_until, new_collect_until,
  new_material_until, new_bond_exit_after,
  new_final_floor,
  extra_resource_charge, escrow_debit,
  reason_code
}
```

新しい名称は提案であり、既存wire tagは割り当てない。extensionはノードが予約状態と承認済み規則から検証する。operator・custodianの申告だけで期限を延ばさない。

元の署名対象frameを破壊的に書き換えず、versionedなeffective termsとextension chainを導出する。旧hashに依存するpermit、lease、allocation、source binding、料金照合をすべて更新対象にする。

##### 6.2 必ず同時更新するもの

1つの原子的状態遷移で、影響するcourt/session/原本義務の期限、`collect_until`、Final floor、credit/receipt/escrow保留、`material_until`、custody pledgeの実効責任、bond exit hold、保持料・予約量を更新する。

**bond返還だけを延期し、古い`collect_until`で正しいconvictionを拒否する実装は禁止する。** 逆に、courtだけ延ばし担保が先に抜けることも禁止する。

bondの関係は、例えば次のような上界関係を満たすように導出する。

```text
material_until_i ≥ max(collect_until_i,
                       terminal_settlement_safe_i,
                       latest_reference_or_recovery_need_i)

bond_exit_after_i ≥ max(material_until_i,
                        settlement_and_liability_end_i)
```

単発のdoorが3,200でも、reorg・settlement・責任保持の余裕によりbondの返還可能時点は3,200より後になり得る。実際の返還transactionの収録はさらに別である。

##### 6.3 後続攻撃による無期限延長を防ぐ

通常の混雑延長は、original受付時と所定のnotice予約時に合意stateから決める。いったん成立した処理予約を、後続の別claim、大量の重複notice、無効応答、partial fragment、再要求で繰り返し延期しない。

同一争点の重複は既存の予約を参照し、時計をリセットしない。新しい独立な争点は別予算を消費するが、他の既存予約を奪わない。`notice_until`自体を新しいnoticeのたびに後ろへ延ばさない。

reorg等による変更は事前合意済み回復予算に限る。予算を超えるservice障害は通常延長の成功として扱わず、事故記録・新規受付停止・既存責任の安全な保護に移る。`E_max`に達したからという理由で、未解決不正を無罪・Finalにすることは禁止する。

##### 6.4 延長費用と資本拘束

延長は無料ではない。保存byte-DAA、追加I/O、再取得、witness carriage、追加検証、bondの機会費用を別々に見積もる。簡単な保存料項の形は次のとおり。

```text
追加保持料 = Σ retained_bytes(object) × 延長DAA(object) × price_per_byte_DAA
```

通常延長はoriginal frameとchallengerの事前予約資金から賄う。最終的な費用負担は、確定した不正・DA/default・無効な濫用・正当な棄却に応じた承認済み規則で配分する。正当な検証者に、全共謀者の無制限な作業料を負担させない。

他人のbondを安価なspamで長期間拘束するgriefingを評価する。返金は二重払いを許さず、同じbondの複数責任には露出額合計の上限を適用する。混雑と無関係な解決済みclaimの担保を、ネットワーク全体の混雑だけで一律に拘束しない。

#### 7. 外部1人の訴追権と大量濫用

##### 7.1 owner枠占有を安全性の穴にしない

既存のK_owner=8とidle回収は、全員がactiveに動く共謀を排除できない。[V2-S04](../rfc/0025-assets/source-audit-v2.md#v2-s04) v2では、枠を持つownerだけが反証できるという構造から分離した、permissionlessな反証提出・共用開示への接続が必要である。

共通の認証済みmaterialは誰でも再利用できる。同じclaimの不正を直接示す有効witnessを、既存ownerの署名・承諾・辞退に依存させない。局所化が必要な検証者にも、control受付と処理予約を含む経路を残す。

**「外部検証者用1枠」と名付けるだけでは不十分。** 共謀者も別アドレスで取得できる。identity別の優先枠、料金、単なるFIFOだけでこの問題が解決したとは扱わない。

##### 7.2 boundedな対話とdedup

同じoriginal claim、同じroot、同じ争点・範囲・rulesetの再提出は、canonical keyでまとめる。異なる証拠や独立した不正を「同じモデルだから」とまとめて捨てない。counterclaim、branch、最後の反証を含む敵対的増幅をprofileに計上する。

有効/無効の判定に仕事が必要な以上、「正当なものだけ無制限に無料受付」は採用しない。一方、capacity capを共謀者が先に埋めれば正しい外部1人を排除できる、という設計も採用しない。

admissionの保証範囲は、**正規noticeの収録機会が与えられ、既存責任について予約されたboundedな対話経路**である。その経路に対して全ての敵対的分岐・枠占有を展開しても反証できることを、形式化または反例探索と実ノード試験で示す。未証明なら`OneOutsiderCapacityIncomplete`としてarmingを拒否する。

##### 7.3 停止すべき入口

容量が不足するときは、まず新しい報酬対象original claim、optional work、新しい保持義務を抑制する。既存の有効noticeのdeadline・権利を消して帳尻を合わせない。

新規モデル登録が共通資源を増やすなら、登録CPU・active metadata・保持負担にも上限と料金を設ける。モデル数を増やしてcontrol枠や検証枠を無料で取り直せない。

##### 7.4 持続負荷の限界

resource `r`について長期平均の受理仕事率を`λ_r`、保証サービス率を`μ_r`とする。定常的な有限queueには、余裕を含めて`λ_r < μ_r`が必要である。到着が処理を超え続けるなら、期限延長だけではqueueと担保拘束が増え続ける。

その場合は、原本claimの受付制限、work/rent価格の見直し、承認済み容量増強を行う。無限の攻撃を有限期限で処理できるとは表示しない。有限burstの処理完了と、持続attack中の有界な受理・既存案件の完了を別の受入条件にする。

#### 8. 容量試算と3,200 DAAの達成判定

##### 8.1 総量から得られる必要条件

同時に全量がreadyな同一サイズのcourtが`N`件、1件のcharged transcriptが`S` B、court用実効帯域が`B_c` B/DAAなら、少なくとも次の転送容量が必要になる。

```text
D_wire_lower ≥ ceil(N × S / B_c)
N × S ≤ B_c × D_wire_budget
```

これは**理想化した容量の下限**であり、実際のcourt完了期限ではない。応答の直列依存、CPU/IO、control、reorg、収録gapはさらに確認する。wire以外の時間を別に予約するなら、3,200全体ではなく残りの`D_wire_budget`を用いる。

125,000 B/DAAはユーザー提示の感度分析用の値、court割当20%は試算仮定であり、出荷ノードの測定値・現行ブロック容量・合意上保証値とはしない。

##### 8.2 20%配分の感度分析

`B_c=125,000×0.20=25,000 B/DAA`。括弧内は元の総帯域が1/10、`B_c=2,500 B/DAA`の場合。Bはdecimal、KiBは1,024 B。

| court総量/件 | 100件の転送下限 DAA | 1,000件 | 10,000件 |
|---:|---:|---:|---:|
| 32 KiB | 132（1,311） | 1,311（13,108） | 13,108（131,072） |
| 64 KiB | 263（2,622） | 2,622（26,215） | 26,215（262,144） |
| 448 KiB | 1,836（18,351） | 18,351（183,501） | 183,501（1,835,008） |

448 KiB×1,000件は458,752,000 Bで、25,000 B/DAAなら18,351 DAA以上となる。1件の総量を64 KiBにできても、1万件では26,215 DAA以上であり、全てを3,200に入れることはこの仮定下では容量だけで不可能である。だからこそ混雑延長とadmissionが必要になる。

32/64/448 KiBは比較用サイズであり、実モデルのcourt総量を測定した値ではない。前版の約12 KBは1位置の見込みであり、総transcriptとして表に流用しない。

##### 8.3 1万件を3,200に入れる帯域の最低条件

他の全コストを仮にゼロとした場合の必要court帯域は次のとおり。

| 総証拠/件 | 必要court帯域 B/DAA | court20%なら必要な全体帯域 B/DAA |
|---:|---:|---:|
| 32 KiB | 102,400 | 512,000 |
| 64 KiB | 204,800 | 1,024,000 |
| 448 KiB | 1,433,600 | 7,168,000 |

実際には待機・計算・復旧余裕が必要なので、これより大きい容量が必要になり得る。wireだけ改善してもCPUやstate root更新がボトルネックなら、doorは比例して縮まらない。

##### 8.4 正式な判定

通常時3,200のgateは、各profileの全fault route、最後だけ資料不提供、合法的な最遅応答、指定同時負荷、control競合、回復まで含めた`F_i+1−a_i≤3,200`とする。

混雑時のgateは`F_i<d_i`、`E_i≤E_max`、正直な参加者の不当defaultゼロ、担保と保持の整合、有限burst後のbacklog排出である。混雑時に通常3,200を超えた割合と、**予約期限に違反した割合**を別々に報告する。

初期の旧感度分析3,012/3,196 DAA等との比較は、最新ADRの正式なdoor式と対応付けるまで参考にとどめる。旧doorに`N×S/B`を無条件に足すと、既に含む通信・待機を二重計上し得る。

#### 9. ブロック容量拡張の検討

##### 9.1 同じブロックサイズでもできる改善

最初にA/B/C、canonical encoding、pathのdedup、共有material、court/controlの予約枠、boundedな並列検証、state更新のバッチ化を評価する。これらは通常transaction容量との配分変更やCPU使用増を伴うことがあるので、無料の改善として扱わない。

court比率を20%から40%にすれば、他条件一定のwire容量は2倍だが、通常transactionの利用余地を減らす。容量配分を変えただけでネットワーク全体の物理帯域が増えたとは扱わない。

##### 9.2 上限を増やす場合

同じ合意・ブロック生成条件の下で、総byte上限、court carriage上限、control上限、検証work/mass、最大body伝搬時間、state書込量の組を`CapacityProfile`として審査する。単一の`block_size`だけを変更しない。

比較候補を`1× / 2× / 4×`としてよいが、これは試験候補であり自動採用ではない。単一巨大tx/closeの上限と、ブロック全体の上限も分ける。

ブロックbytesからB/DAAへ換算するときは、accepted DAG、DAA score、実際の収録機会、重複/孤立blockを含む関係を明示する。1 DAA＝1秒、1 DAA＝1物理blockと勝手に置かない。

##### 9.3 採用条件

候補を、同じreference node/帯域制限で次の観点から比較する。

- propagation、検証wall time、p95/p99と観測最大、導出最大service gap、CPU、RSS、disk/DB write amplification。
- 通常transactionの進行、10,000件burst、巨大/無効witness、同一モデル/異種モデルによる競合。
- orphan/reorg挙動、復旧時間、checkpoint作成時の遅延、snapshotサイズ、pruned IBD、catch-up速度。
- 低い性能の承認対象ノードが脱落しないこと。並列化しても全ノードが同じ結果を導出すること。

最終的に増やすのは、**出荷可能と審査された実効service envelope**である。ブロック上限を2倍にしただけで保証帯域を2倍にしてはいけない。

##### 9.4 動的増大は初期v2に入れない

混雑件数やローカル帯域を見てブロックサイズを無制限に自動拡大する機能は導入しない。攻撃者がノード負荷を引き上げる経路になり得るためである。初期版は明示的なruleset/fenceで容量profileを変更する。

将来の弾力的容量は、合意から導出できる指標、hard ceiling、変更率制限、hysteresis、料金、ノード要件、伝搬安全性を別RFCで定める。

#### 10. 10,000 DAA checkpointと安全なpruning

##### 10.1 4つの時計を分ける

```text
checkpoint_interval       = 10,000 DAA（作成間隔の提案）
pruning_point             = base consensusが安全と認める位置
court/material retention = 各objectの未解決責任と必要期間
bond exit                 = 判定・保持・決済の必要期間後
```

checkpoint間隔10,000は、直近10,000だけ残して全削除する規則でも、全courtの証拠保持期限10,000でもない。10,000件混雑によってcourtが26,215 DAA以上かかる場合、必要証拠は複数checkpointをまたいで保持する。

##### 10.2 削除可能性の条件

objectごとの`retain_until`を、全参照元の要求から導出する。

```text
retain_until(object) = max(
  unresolved_court_need_end,
  material_lease_end,
  settlement_or_liability_need_end,
  reorg_and_restart_need_end,
  validated_snapshot_recovery_need_end
)
```

各項は同じ絶対DAA時刻で表す。相対期間を異なる起点のままmaxで混ぜない。notice→court→settlement→recoveryと直列に必要な期間は、まず足して絶対期限に変換する。

削除は、合意のsafe pruning pointより古く、未解決参照がなく、保持義務と再構築上の必要性が終わり、対応stateが検証可能なcheckpointに反映された場合だけ許可する。pruningされたデータを新しい有効courtが暗黙に読み直す構造を禁止する。

##### 10.3 データ階層

**Active state:** reservation、期限、bond exposure、未解決session、必要な部分witness、extension chainの再構築に必要な状態。全検証ノードが同じ意味で復元できる必要がある。

**Retained evidence:** 未解決または回復上必要な認証済みmaterial。ノード・custodianの責任区分を明示し、rootだけをもってbytesの代替にしない。

**Resolved history:** finality・reorg・責任期間を過ぎた詳細証拠。検証可能な結果commitmentと残存状態を保ち、履歴はpruneしてよい。archival nodeは監査・再取得の任意提供者であり、現に有効なcourt成立を善意archiveにだけ依存させない。

全ノードが全モデル・全原本を永久保持する義務は設けない。ただし現在のconsensusが必要とするbytesを省けることは、具体的なstate/commitment/availability設計で示す。

##### 10.4 保存量の上限

queueが伸びればlive stateとretained bytesも増える。延期はstorage問題を消さない。`max_active_state_bytes`、`max_pending_notice_bytes`、`max_inflight_witness_bytes`、`max_retained_byte_DAA`をadmissionのhard resourceに含める。

最終witnessが小さくても、原本モデル・履歴の保持は大きい場合がある。モデル共用による削減は参照countとleaseを正しく持ち、最後の未解決参照を消すまでpruneしない。

#### 11. 検証可能なcheckpointとpruned IBD

##### 11.1 checkpointに必要な状態

既存base-chain pruning proof/UTXO/state検証に加えて、court関連のprimary stateを含める。

```text
ruleset / active capacity profile / resource template root
class/profile registryと必要な認証済み参照
未確定claim、court/session、notice、next sequence
処理予約カレンダー、残余予算、queue、phase cursor
base terms / effective terms / extension policy / 延長記録
bond exposure、exit hold、Final/credit/escrow hold
custody obligation、lease、retention reference
未完fragmentの必要bytesまたは合意上十分な認証可能表現
既に処理済みのkey/重複防止状態、settlement結果
```

derivableなindexは再構築し、primary stateと食い違えばsnapshotを拒否する。partial close、予約、extensionを落とすとarchival nodeと異なる判定になるため禁止する。既存snapshotが未完close groupのbytesを保持する点を維持する。[V2-S06](../rfc/0025-assets/source-audit-v2.md#v2-s06)

##### 11.2 rootの一致だけでは状態遷移の正しさの証明にならない

snapshot bytesが指定rootに一致することと、そのrootが正しいchain状態を表すことは別である。新ノードは、base consensusの安全性前提の下でcheckpointを認証する既存のpruning/chain-selection/状態検証経路を通す。

管理者署名だけ、rootを載せただけ、checksumが一致しただけのsnapshotを「trustless IBD」と称さない。検証済みcheckpointからcatch-upして、同一tipのarchival nodeとstate root・court verdict・bond残高・deadlineが一致することを要求する。

このRFCは新しいSNARK/STARK等のvalidity proofを導入しない。全履歴再実行なしで何をどの前提で検証できるかは、既存pruning proofの信頼・攻撃モデルと合わせて明記する。

##### 11.3 測定項目

IBDを、proof/header、snapshot、active court payload、必要material取得、catch-up replayに分解する。100、1,000、10,000件の未解決courtを持つcheckpointでbytes・時間・CPU・RSS・diskを測る。

pruningにより古い履歴を減らせても、現在のmodel registry・UTXO・live courtが増えればIBD状態量は増える。全履歴依存を減らすことと、モデル数・未解決件数に対してIBDが定数になることを混同しない。

10,000 DAA境界の作成中にブロック処理を長時間止めない。atomic snapshot、整合性確認、失敗後再開、増分生成の方式を実装し、そのコストを通常service envelopeに計上する。

#### 12. ノード実装のスケーリング条件

1万courtをon-chainで有界化しても、実装が毎ブロック全courtの全witnessをコピー・再hashするならCPU/RAM/IOが詰まり得る。byte容量の改善とノード処理の改善をセットで評価する。

##### 12.1 必須の実装制約

- message長・proof深さ・operand数・work上限をallocationや重いhashの前に検査する。
- witness組立はbounded bufferと認証済みfragment単位で行い、未完案件数×巨大連続bufferをRAMに常駐させない。
- schedulerは期限/phaseのindexを持ち、全model×全court×全DAAの無制限scanを作らない。1ブロックの更新件数・CPUをmassに計上する。
- court間の検証を並列化してよいが、state適用順と競合解決は合意順序に固定する。
- timeout、default、bond release、checkpoint境界に1万件が集中する場合も、処理上限と決定的な分割・保留規則を持つ。単に一括処理を次へ送って既存deadlineの意味を変えない。
- RAM上限だけでなく、database write amplification、journal、reorgでのundo/redo、snapshot exportのピークを計測する。

##### 12.2 state commitmentの効率化

同じ意味のstate rootを保てるindex/cachingの最適化と、root形式そのものを変えるincremental Merkle化を区別する。既存snapshot digestはcanonical carriage全体をserializeしてhashする。[V2-S11](../rfc/0025-assets/source-audit-v2.md#v2-s11) これがどの頻度で呼ばれるかをprofileし、毎ブロックのボトルネックと未測定のまま断定しない。

root形式を変更する場合は、新version・domain separation・reorg・pruned IBDの同値性と移行規則を別途定める。hashを省いても「同じ保証」と称してはならない。

#### 13. 状態・codec・再生の契約

提案するprimary stateは次のとおり。具体的なwire番号と既存型とのmappingは統合時に割り当てる。

```text
ScalableCourtStateV2 {
  capacity_profile_id,
  reservation_calendar,
  next_reservation_sequence,
  original_frame_reservations,
  pending_notices,
  active_courts,
  effective_terms,
  extension_records,
  exposure_and_holds,
  material_references,
  bounded_resource_counters
}
```

全recordをnetwork/ruleset、original claim、class/artifact、execution、court/scope、sequenceでdomain-separateする。整数はchecked arithmetic、切上げ、canonical orderを使う。UNKNOWNな帯域・仕事量・保持料はゼロ扱いせずadmissionを拒否する。

extension処理は原子的に行い、失敗時は期限・料金・担保を全てrollbackする。checkpoint取込時にはreservation、effective terms、retention、bond exposureの整合性を再構築して検査する。

reorgで取り消されたnoticeやextensionをローカルcacheだけで生かさない。accepted chainに基づいて再適用し、古い枝のbond解放・slash・二重返金を禁止する。snapshotのchecksum一致だけでこれらの状態遷移が正しいとみなさない。

#### 14. 受入条件と試験計画

##### 14.1 規範的な受入条件

| ID | 必須条件 |
|---|---|
| C1 | 全対応モデル/形状/context/fault routeに共通の`S_close/S_transcript/W_verify/IO/RAM/rounds`上限を導出し、悪意ある最大codecを含めて検査する |
| C2 | 単発および宣言した通常予約負荷で、ctx2Mを含め`D_frame≤3,200`。queue/準備を起点から隠さない |
| C3 | 100/1,000/10,000件の有限burstを予約できる範囲で受理し、全件が自分の予約期限内に完了。通常超過は`E_i`として公開 |
| C4 | 新規流入とモデル分割によって、先行予約がstarvation/無期限延長にならない。過負荷時に新規責任の入口を抑制する |
| C5 | 全custodian/producer/Panel非協力、全owner枠占有、適格外部1人で、exact convictionまたは適格DA/defaultに進める。初期の無応答だけでなく最後だけ隠す経路も含む |
| C6 | 期限・Final・bond・保持料・署名済み最大責任が一致。3,200および排他的境界を厳密に検査する |
| C7 | 10,000 DAA間隔のcheckpointをまたぐ長期混雑でも、未解決証拠をpruneしない。責任終了後は安全に削除できる |
| C8 | genesisから全bodyを取得せず検証可能なcheckpointからIBDし、archival nodeと同一のstate/verdict/bond/deadlineへ到達する。信頼前提を記録する |
| C9 | court容量比率変更・block容量1×/2×/4×候補で伝搬・CPU・RAM・IO・reorg・IBDを測り、採用するprofileを審査する |
| C10 | source/binary/codec/profile/service/料金/全実測/evidenceをpinし、未被覆・UNKNOWN・未承認recordでfenceを武装できない |

##### 14.2 負荷matrix

件数だけではなく、次の軸を組み合わせる。

```text
モデル数:     1 / 10 / 100 / 審査対象の最大registry
context:      短文 / 32K / 2M
court件数:    1 / 100 / 1,000 / 10,000
流入形態:     一斉burst / 時刻分散 / capacity以下の継続 / capacity超過の継続
争点:         同じ実行の重複 / 独立実行 / 独立モデル / 全fault-route混合
帯域:         profile下限 / 感度分析125,000 / 12,500 B/DAA
状態:         warm / cold-state / 完全cold（要件を区別）
相手:         協力 / 最遅合法応答 / 最後だけ不提供 / 全不提供 / 無効応答spam
ノード:       reference最小機材 / 高性能 / pruned IBD直後 / 再起動直後
```

##### 14.3 個別の攻撃・境界試験

| 試験 | 期待結果 |
|---|---|
| サイズ超過、偽proof深さ、巨大operand、無効codec | 無制限allocation/hashなしで拒否。誤slashしない |
| 小さいpayloadだが高CPUのwitness | CPU予約/上限が働く。wire枠だけで通さない |
| 256候補bundleの1候補欠落、fork間で矛盾した部分値 | 全候補整合性で拒否。未開示分の後続待機を隠さない |
| 1万件到着後にさらに1万件、モデル/アドレスを分割 | 先の予約は不変。後続は別予約またはoriginal admission抑制 |
| 同じ争点を大量複製 | bounded dedup。originalの期限をリセットしない |
| owner全枠を共謀者がactive維持 | outsiderの反証経路を排除しない。失敗ならarming不可 |
| 応答側が完全沈黙 | 処理機会の時計は進む。適格な期限経過でDA/default |
| controlが巨大witnessや大量noticeで競合 | 宣言したcontrol inclusion boundを維持する |
| extensionなしで古いbondが解放される、または期限だけ延長 | validation拒否。原子的更新がなければ成立しない |
| `collect_until−1`と`collect_until`でterminal収録 | 前者は他条件を満たせば受理、後者は範囲外 |
| extension record重複、順序違い、overflow、資金不足 | 状態変更なしで拒否 |
| 延長直前/直後のreorg、再起動、snapshot import | canonical stateに復元。同じverdict/hold/返金 |
| queue中にcheckpoint、2回以上checkpointを跨ぐcourt | reservation/material/holdが消えない |
| 1万件が同時terminal/default/返金可能に到達 | 決済処理もbounded。CPU spikeで全block処理を止めない |
| capacity下限未満のservice障害 | 正直な相手を資料不提供と決めつけない。事故経路を記録 |
| 小/大block profileで同じ有効証拠を処理 | consensus結果同値。性能差・ノード要件だけを評価 |

##### 14.4 合格の報告形式

全件について`a/n/s/F/d/E`、reserved/actual bytes/work/IO、bond lock期間、保持byte-DAA、cold条件を出力する。集計は、3,200以内率、予約期限超過数、backlogピーク/排出期間、不当default数、最大RAM/DB増分、IBD量を含める。

平均やp99が良くても、宣言した保証範囲で予約期限に違反した案件があれば、そのprofileのhard-deadline gateは不合格。実測が全て速くても、形式的な上限・敵対的経路・inclusion前提の穴があれば保証完成とはしない。

#### 15. 実装順序と統合箇所

| 段階 | 作業 | 主な候補箇所・成果物 |
|---|---|---|
| 0 | 最新door branch/ADR-0191、tag230/245、D2/X3、既存期限とsnapshot版を照合 | snapshotとの差分台帳、正式door定義 |
| 1 | 最小primitiveと行レベル認証、全fault被覆、完全transcript/work bound | `palw_tir_dissect_v1.rs`、`palw_tir_court_v1.rs`、class admission、codec |
| 2 | A/B/Cの小型profileを作り、cold/強制経路の単発3,200を検証 | witness builder、frontier、source/custodian経路 |
| 3 | モデル横断capacity ledger、control保護、予約calendar、notice、one-outsider経路 | `palw_tir_public_allocation_v1.rs`、public/owned/fold、node scheduler |
| 4 | v2 effective terms、bounded extension、Final/bond/lease/料金の原子的更新 | `palw_tir_public_custody_v1.rs`、`palw_state_v2.rs`、staged folds |
| 5 | node全体の100/1,000/10,000件burstと持続attack試験 | 出荷binary、reference機材、負荷generator、回復drill |
| 6 | checkpoint10,000、active state保存、pruning/IBD、必要なroot性能改善 | carriage、pruning proof、DB/journal、snapshot codec |
| 7 | court比率とblock容量候補の比較、価格/保持/露出/同時件数を確定 | capacity profile、service evidence、承認 |
| 8 | record pin、fence drill、段階配備 | release manifest、network identity、運用承認 |

live node・main branch・fence・t12を本書の作成作業で変更しない。未確認のwire番号や動作中networkの高さを文書に仮置きしない。

#### 16. 移行と互換性

v1のopen frameは受付時のrulesetで処理する。新しい容量予約が旧frameの保証を奪わないよう移行用容量も予約する。v1にv2の延長機能を勝手に適用しない。

既存署名済みpledgeやasset lockを越える期限延長は、新しい合意規則と責任主体の事前契約なしに導入しない。v2は、max extensionを含む署名・料金・capを備えた新規frameから導入するのを基本とする。

既存の3,120 gateを3,200へ変更することは、別の明示的パラメータ承認を必要とする。80 DAAの差を誤差として扱わない。新しい小型profileと旧大型witnessの受理条件をversionで分離し、旧fallbackを新上限の例外にしない。

fence前後、旧/新frame混在、旧/新checkpoint、reorgがfenceを跨ぐ場合を試験する。capacity profileの増強で既存参加者の応答時間を突然短縮しない。

#### 17. arming recordと未確定事項

##### 17.1 pinする証跡

`PALW_G14_ARMING_RECORDS_V1`等の既存gateを迂回せず、統合時にv2の審査recordとの対応を定める。投稿中の`PALW_TIR_FRAME_COST_RECORDS_V1`は最新branchと照合する。

```text
source / dependency commit / binary SHA-256 / build flags
ruleset / class-artifact / witness codec / resource-template fingerprints
S_step / S_close / S_transcript / W / IO / RAM / rounds の導出
全fault route、cold条件、強制開示とdefaultの最大値
service lower envelope / inclusion前提 / control容量 / block profile
予約アルゴリズム / bounded adversarial branching / one-outsider evidence
E_max / effective-terms / retention / escrow / pricing / exposure
100/1,000/10,000件burst、持続attack、restart/reorg、pruning/IBDログ
参照機材・通信条件・最大値・観測span・全結果のhash
独立レビュー / 採択 / 有効化条件
```

##### 17.2 採択前に確定すること

`SC64`の64 KiB総量と32 KiB close目標が全対象routeで成立するか、17,408 Bの定義/codec、min検査窓、service rate/gap、control収録条件、`E_max`、保持単価・延長funding、one-outsiderの枠占有耐性、checkpoint検証経路、block容量の採用値は、現時点では未確定である。

これらは「実装者が適当に値を入れてよい」項目ではない。schemaでUNPINNEDを明示し、1つでも残るprofileは武装不可とする。目標サイズが成立しない場合は、lowering/codec/対象範囲を見直すか、別の上限変更を明示的に再審査する。

#### 18. このRFCが達成したと主張しないこと

本書は、1万件courtの実ノード処理、SC64上限、単発3,200、network帯域保証、node E2E、暗号学的soundness、pruning/IBDを実証したものではない。

同梱`capacity_planner.py`は、容量下限の整数計算と、先行予約を後続が動かせない**保守的なtoy scheduler**を提供する。実行した27試験はその算術・原子的予約・snapshot再生・1万件有限入力の整合性に対するものであり、1万件のネットワーク試験ではない。

本RFCの設計上の到達点は、**「各courtを小さく有界にする」「通常3,200と混雑時の実期限を分ける」「有限burstを安全に排出する」「持続過負荷を新規受付で止める」「責任と証拠を保ったままpruning/IBDを成立させる」**ための実装・検証契約を一体化したことである。

#### 19. 参考一次資料

プロジェクト固有の事実は[source-audit-v2.md](../rfc/0025-assets/source-audit-v2.md)および前版のsource-auditに限定して根拠を示した。次は設計上の参考であり、このnetworkへの適用や性能を証明する資料ではない。

- [RFC 2212 — Specification of Guaranteed Quality of Service](https://www.rfc-editor.org/rfc/rfc2212.html), 1997-09。帯域予約・流量制御・遅延上限を組み合わせる設計と、その経路条件を参考にした。
- [OP Stack Specification — Fault Dispute Game](https://specs.optimism.io/fault-proof/stage-one/fault-dispute-game.html), 参照2026-10-19。単一stepの認証済み入力、ゲーム時計の有界延長、裁定とbond分配までの確定待ちの分離を比較対象とする。既存実装の移植ではない。
- [Kaspa Research — Pruning safety in the vProgs architecture](https://research.kas.pa/t/pruning-safety-in-the-vprogs-architecture/411), 初回投稿2025-08-25。pruned dataを後続検証が暗黙に要求しないことの論点を参考にした**研究提案**。採用済みの一般規則や、このRFCのIBD検証の根拠としては扱わない。

#### 整合性修正 — 2026-10-19(RFC 全件整合性監査)

- **混雑延長は hard deadline の内側に限る。** ユーザーが採用した RFC-0014 2026-10-19 改定の `a + 10,000` を主契約とし、`a + 3,200 + E_i ≤ a + 10,000`(`E_max ≤ 6,800`)と読み替える。これを超える延長は採用していない別の将来方針。10,000 の内側で処理できない負荷は、original claim の受付で抑制する([RFC-0026](../rfc/0026-court-policy-master-contract.md) §2.1・§3)。本書 §4.3 の「`E_max` を一律 10,000 で埋めない」は、この上限の内側で容量から導く、という意味に限る。
- **SC64 は現行 profile の達成値ではない。** 既存の各手番の署名(ML-DSA-87、4,627 B)を残すと、2M・4 分岐では署名だけで 19 × 4,627 = 87,913 B となり 64 KiB を超える(算術上の下限)。SC64 は認証方式・投稿数を含めて別 profile として検証する目標(F05、RFC-0026 §7.1)。
- §0 の「既存の休眠実装」`palw_court_capacity_v1.rs` は別 branch(`release/t12-16000`)にあり、`pre` には未統合(F10、RFC-0026 §8.1)。
- 「本書を上位とする」は Draft どうしの参照関係であり、採択済みの規範に対する置換ではない。正式な置換は RFC-0021・凍結規程どおり ADR と置換節で行う(F11、RFC-0026 §8.2)。

---

<a id="source-v1-0002-L00456"></a>

### 旧RFC-0002 / 原文456–504行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### 5.4 Court feasibility and size caps

For every commit point, the *cone* of its output — the nodes back to the nearest commit points, params,
consts, states and inputs — must fit the court's existing ceilings (`derive_court_cost_v1`: ≤ 16 Mi
terminal MACs per tile, ≤ 8 operands, ≤ 27 close chunks; `palw_mode_v2.rs:551,616,620`). Large nodes
are adjudicable at three levels, **IR node → canonical chunk → tile**:

- a `MatMul` output is committed in tiles; one tile is `tile_len` dot products, each within the
  contraction bound;
- a reduction over `H` must be **dissectable**: between the `HistRead` and the reduction there are
  only exact primitives and lossy primitives that act on one history position (a per-position
  `IntExp` of a score is fine; rounding a running sum is not). The reduction is exact, so the court
  dissects it by partial sums over canonical chunks of `H`, and the partials add without error. This
  is ADR-0082's attention dissection stated as a property of the program, and it fixes the canonical
  softmax as two-pass (maximum first, then exact sums);
- a cone that crosses a `StateRead` stops there: the state is an operand opened from the checkpoint
  leg, or replayed from the nearest checkpoint. Admission derives each `Fixed` state's maximum
  checkpoint interval `C` (the canonical chunk of the position scan) so that replaying `C` positions of
  its update cone fits the ceiling.

Size caps (proposed; sized in Phase D): program ≤ 256 KiB; blocks ≤ 16; nodes per block ≤ 512;
layers ≤ 1024; inputs per node ≤ 8; rank ≤ 4; any dimension ≤ 2^24; elements per tensor ≤ 2^28;
consts ≤ 64 KiB; states ≤ 16 per layer; step leaves ≤ 2^22 (existing). The registration price (1 BILI
burned, ≤ 4 bought registrations per block, `palw_state_v2.rs:7411,7426`) bounds how often admission
runs.

#### 6. Commitment and the court

- **Commit points.** A node with `commit = true` materialises its output as committed tiles in the
  step leg, as a node output does today (`(call, node_slot, position, tile)`,
  `palw_step_leg.rs:1207-1228`), with `node_slot` = the node's unrolled index. Lanes stay 4 bytes.
- **Required commit points:** every block's `carry_out`; the logits; every `idx` tensor produced by a
  selection primitive that feeds a `Gather` over params (a wrong selection makes everything
  downstream unrelated, and no arithmetic bisection converges — ADR-0052 B — so the selection itself is
  an opened value); the input of every `StateWrite` and `HistAppend`. Lowerers add further commit
  points until §5.4 passes.
- **A cone is the unit of adjudication.** The terminal refutation opens one committed output tile and
  its cone's committed inputs, proves params against the artifact root (as today), and **interprets
  the cone** with the reference evaluator. One court arm replaces the per-kernel arms (`qwen36_row`,
  `base0_row`, `kimi_row`, …).
- **The court knows no model.** It sees a node, its input commitments, its params, its output
  commitment and the primitive semantics. A4 becomes "the court covers PALW-TIR v1", proved once;
  `palw_court_catalog_root_v1` gains `prim_set_id`; the coverage certificate for an IR class is "every
  node's primitive is in v1". For an admitted program `Unadjudicable`
  (`palw_step_refute.rs:2514-2520`) can only mean an interpreter defect, which the second
  implementation and the golden vectors exist to catch.
- **Bisection is unchanged** (`palw_bisect.rs`: step leaves or trace events, ≤ 48 rounds, rung
  deadlines, silence loses) over the same step space.

---

<a id="source-v1-0002-L02296"></a>

### 旧RFC-0002 / 原文2296–2314行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

###### 14.7 Bounded flat Gather courts and a real-program static regression

The dormant segmented decoder/encoder kernels identify whole-value pricing revision 2 in new
v4/v5 descriptor digests. A rank-one axis-0, unbatched Gather with output shape equal to its index
shape uses allocation-free direct lookup, with the same index refusal, operand reads and exact
result as the reference. Only this evaluator's per-output price changes (32 to 16 abstract units,
then the existing factor 64); authenticated bytes and both commitment hashes retain their prices.
The previous conservative DA16b withholding selection remains fixed. No resource limit, primitive
semantics, v1–v3 descriptor or activation fence changes.

The exact real Qwen2.5 pilot's 32-position TIR is committed as static regression material; its
worst court work is 473,589,248, below the unchanged guaranteed proof reservation 536,870,912.
Its direct TIR route is hypothetically eligible and remains inactive in the shipping schedule.
This closes the pilot's previous static court-price refusal only. A vocabulary-sized synthetic
court and differential/reference/root tests verify the optimization; they do not prove the
real-weight node path, source fidelity, long context, model security or Final/redemption. Evidence,
commands and remaining full-scope gates are in the implementation ledger's
[whole-value Gather pricing revision](../design/palw/tir/rfc0002-implementation.md).

---

<a id="source-v1-0002-L02358"></a>

### 旧RFC-0002 / 原文2358–2383行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### 14.10 Native segmented roots and real delivery-court evidence

A canonical decoder artifact can now produce existing v3 node and segmented roots through the
node's typed executor, without a ModelSpec or compiler registry. The local SDK path rederives
its exact plan and reuses unchanged node static prosecution/carrier/block ceilings. It streams
one successful complete position at a time, checks native values against declared wire types,
and keeps only position roots unless private capture is explicitly requested. Local hash/trace
caps exclude native buffers, mapped weights and caller retention; activation and economic
rights remain independent chain facts.

The fixed real Qwen2.5 lowered pilot completed 32 positions, committing 245,056 node values.
Native computation plus node hashing/capture took 169.916 s; whole-process maximum RSS was
2,143,010,816 bytes. Its public reproduction bundle is 161,675 bytes, with no private position
values or raw weights. A 17,924-byte existing Decode court proof convicts a substituted output
token, while honest delivery is dismissed. A separate offline test consumes only that public
witness to reproduce the outcomes and refuse copied/tampered openings. Eight relevant tests
and five refusal checks passed.

This establishes actual-weight trace construction and offline terminal delivery-court evidence.
It does not establish registration, public RPC acquisition, full computational integrity,
independent source/task fidelity, original full context, outsider node G14 or Final. Same-node
registration/public claim/prosecution/settlement and the remaining full-scope requirements are
still required. Protocol, commands, measurements, raw evidence and hashes are in the
[RFC02 implementation ledger](../design/palw/tir/rfc0002-implementation.md).

---

<a id="source-v1-0002-L02646"></a>

### 旧RFC-0002 / 原文2646–2802行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### 14.18 Public trace courts for unregistered scoped candidates

A new versioned, non-reward statement uses inner **26 `PostModelVectorV2`** and
**27 `RefuteModelVectorV2`** under authenticated outer 110 and dormant PanelFree.
An active bond may publish a vector of an admitted scoped candidate without creating
an execution class, job or kernel claim. It need not obtain a preceding Final. The
statement binds the exact candidate and model binding, the public prompt, exact delivery
length and delivered ids, network/ruleset, program/plan/PC identity, segmented evidence
and segment roots. For a decoder, the last delivered token is not fed back as input.

The three claimed implementation roots must agree with this explicit typed trace grammar.
It commits intermediate values and logits through the existing v3/segmented trees, so a
shared wrong computation is subject to the existing bounded element/type/decode courts.
A public verifier with the matching model reconstructs this context and supplies a fault
against its roots. A proof cannot be borrowed by another vector, source binding, program
or plan. This is a new statement, not a reinterpretation of legacy tag-109 `a`/`b` flat
logit/commit digests. Equality of role roots still does not establish actual independent
software execution or source-checkpoint fidelity.

Aux **51** stores a fixed reservation/court header followed by bounded post bytes; **52**
is the poster's lifetime catalog, capped at eight statements. A new distinct assertion
reserves 100 BILI of the poster's own free capital for the existing 200-DAA liability
horizon. Both withdrawal duty gates and ordinary committed/free-capital calculations read
this reservation together with all other commitments. Expiry is a clocked read and does
not erase the catalog or scan all vectors per block. The model binder owns no liability
for another bond's vector. Identical content shares a statement and grants no priority.

Posting and refuting reserve the existing shared block work before program/post/proof
decoding. Every judged post, including a truthful duplicate, pays the actor's policy fee.
An invalid judged refutation also pays the actor's scaled fee; an over-budget filing is
not judged or charged. The fixed header prices the worst admitted court plus program/wire
work; admission requires this whole envelope to fit the work reserved for prosecution.
Nested counts, v3 leaf sizes and paths are bounded before allocation. §14.19 adds bounded
WholeValue optional tensors within the candidate's already admitted filing/work envelope.
A valid fault slashes the poster's reservation once and pays only the existing 49% share
of the collected slash; self-operator bounties are refused.

This path offers falsifiable integer trace assertions for domains too large to enumerate.
It is **not** complete-domain conformance, an OPV eligibility source, or a sampled reward
gate. §14.19 supplies public point-unit trace demand/default. Vector discovery/transport
tooling, immutable policy selection and its whole-statement soundness, legacy flat-digest courts and independent
runtime certification remain required before integrating it into reward eligibility.
Opaque/withheld source-PC relation cases and every other §II.14 requirement remain open.
Shipping activation is unchanged.


##### 14.19 Candidate trace unit availability and verifier-owned whole-value courts

Inner **28 `DemandModelVectorV2`**, **29 `AnswerModelVectorV2`**, and **30
`SettleModelVectorDemandV2`** use the same signed, dormant PanelFree consumer. A demand
names the exact statement and `(position, occurrence, node, axis, line, tile)`: row/column
tiles (axis 0/1), or node commitment and Merkle paths only (axis 2, zero line/tile). No
parameter, model file or inventory range can be requested. The candidate commitment-only
mask (§14.20) is authoritative for this new statement; a value marked withheld owes only its node opening. The v4/v5 claim
disclosure mask remains unchanged.
The Level-1 model-byte mask also forbids requesting a copied or model-only tensor even
when its WholeValue envelope is too large for the existing disclosure mask.
Where a bounded court exists (§14.20), its tensor is supplied by the verifier's
own model under that candidate court.
Conditional model acquisition remains outside consensus.

A requester owns four reusable slots, backed by its free collateral together with all
other commitments and both withdrawal duty gates. Other bonds cannot occupy its slots.
A replaced settled slot deletes its old demand row; answers/closing calls bind the new
request id, including vector, unit, requester, slot and filed DAA. Demands decode and
validate the stored program/trace only after reserving shared admission work and charging
the actor's fee. The fixed request retains its admitted node index/count, positions and
containing segment root; response classification does not decode the program or trace.

The policy's court deadline plus proof grace must fit the vector's liability horizon. An
answer is timely through its deadline; a default is closable strictly after it. Pending
requests keep the poster's reservation held even after that horizon, until explicitly
closed. Aux **53** holds requests, **54** their bond/slot indices, **55** authenticated
public-unit receipts, and **56** the outstanding count per vector. No periodic scan over
all vectors/requests is added. Reservation reads visit at most eight vectors or four
request slots per bond. Any active bond may close a request; a default bounty goes to the
requester's recorded payout, not the closer. A poster's own operator cannot demand it.

Answers have an 80 KiB ceiling, two bounded paths and at most one existing 4096-element
v3 tile. Counts, type/rank and paths are checked before allocation. Like the existing
segmented response lane, this bounded Merkle/byte classification uses no shared court
adjudication run; exhausting proof runs cannot by itself prevent a valid answer. Invalid
answers pay their actor's scaled fee. A timely authenticated answer is public thereafter;
its collected requester deposit compensates the poster once per admitted request. The
deposit is at least the policy demand bond and the fee floor of the maximum bounded
answer. Other pending requests for that same unit close from its public receipt, subject
to their own original deadline. No new demand is admitted for an already public unit.
This local fee floor is not a measurement of complete network/producer economics.

An unserved request's explicit close slashes the poster's remaining vector reservation
once, sets **unavailable**, refunds pending requester reservations, and pays only the
existing 49% collected-slash bounty. Availability failure is distinct from a proven
integer fault (**refuted**); either outcome prevents a second slash. A timely service
and later integer conviction remain separate operations. Closed/late requests are
refused before classification and no mutable deadline extension is allowed.

The existing WholeValue proof variant is admitted with bounded node/leaf paths, input
counts and optional whole tensors. Rank, dtype, checked dimension product and exact byte
length are verified before allocating tensor bytes. Both wire and stored trace bytes
join the program/court work envelope; the candidate's derived prosecution bounds and
reserved proof-work/carrier limits still apply. This allows a verifier to reconstruct
a falsifiable withheld-node relation from public openings and its own model, without
requiring that model's weights from the poster. It does not certify independent runtime
implementations or complete-domain conformance.

These routes remain non-reward conformance signals and grant no execution eligibility.
Legacy flat-vector courts, immutable whole-statement policy/soundness, actual checkpoint
fidelity and task/context/feature coverage, discovery/transport tooling and the complete
network resource/liveness/economic contract remain required. Opaque source-PC fidelity
relations and large model-only trace relations outside the bounded candidate courts
remain open; §14.20 extends the parameter-Gather case without forced weight disclosure.
All other §II.14 requirements remain open. Shipping fences stay dormant.


##### 14.20 Candidate Gather dependency courts without a full weight opening

Candidate statements (inner 26–30) use `model_vector_withheld_mask_v2`,
`check_model_vector_positions_v2` and `verify_model_vector_fault_v2` together.
The existing v4/v5 program arithmetic, plan, trace commitments, claim checker/court,
`seg_withheld_mask_v1` and reward eligibility retain their meaning. These non-reward
statements do not change the disclosure contract of an execution claim.

For `Gather(axis=0,batch_dims=0)` whose data operand is a registered model parameter,
the candidate whole-value court price includes the row leaves covering at most the
number of selected items. It includes each selected item's full trailing dimensions,
all row tiles and each leaf's full Merkle depth. For rank-one data, each scattered
index may touch a distinct 4096-element tile. Repeated indices never increase this
worst bound beyond the parameter's complete row tree. The independent index operand,
node paths, reference element evaluation and both output commitment hashes retain
full prices. Other axes, batch dimensions, primitives and committed data operands
retain the established conservative whole-value price.

A model-dependent node is commitment-only for this statement when its whole-value
opening and work fit the existing one-MiB and 2^36 ceilings, or when the frozen
v4/v5 mask already withholds it. Admission takes the maximum of the established
class bounds and these additional candidate court bounds before checking the shared
reserved proof work and actual carrier. Refutation must fit the stored admission
header and its own node's dependency opening bound. An unnecessarily large filing
cannot borrow another node's larger byte envelope. These bounds are derived, never
supplied by the poster.

The verifier acquires the matching model outside consensus and re-executes. At the
first divergent bounded Gather, it files only the output's public authenticated node
opening, the authenticated selected parameter leaves from its own model, and the
index operand. The court recomputes every output element and the v3 commitment;
it need not receive the poster's Gather tensor or the full weight file. Bad parameter
bytes or a borrowed commitment dismiss at the challenger's cost. The same request
lane refuses copied-model tensor tiles and serves the node commitment only.

This closes the bounded parameter-Gather dependency case, not all large model-only
relations. A whole output beyond these limits, another axis, a large committed data source,
opaque source-PC fidelity or a relation outside these ceilings still needs a separate
bounded court. No sampled security/complete-domain conformance, actual checkpoint
support, independent runtime identity or reward eligibility follows. Every remaining
§II.14 completion requirement and the dormant shipping fences remain in force.

---

<a id="source-v1-0002-L02855"></a>

### 旧RFC-0002 / 原文2855–2928行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### 14.22 Staged flat-profile dependency-leaf terminal court

`misaka-palw-kernel::flat_court4` adds a **separate, explicit profile context**
for a terminal relation court over the §14.21 material. Its profile ID must be
bound by a future consumer's typed statement, alongside the program, parameter
commitments, job input, extents and segmented evidence roots. No existing
execution descriptor, candidate-v2 statement, route discriminant, class,
reward or shipping fence selects this context. Passing this component's tests
cannot establish node prosecution, Final/redemption, complete-domain security
or real-checkpoint support.

Under this staged profile, a committable node value with at least 64 elements
uses flat-v4. Other node values retain v3, including small and wide intermediate
values; registered parameters retain their existing v3 commitments. This is an
explicit mixed grammar, not reinterpretation of a shape in an existing claim.
Non-committable values are not thereby legal commit points. The existing TIR
primitive set, integer arithmetic, program validation and state wiring are
unchanged. The profile ID and fault version are checked independently of the
untrusted filing.

A filing contains the output's authenticated node opening and **one hash-only
flat leaf opening**, plus a sorted, duplicate-free union of operand value
leaves. It contains no producer output values. The court authenticates the
output coordinate against its segment, resolves each source from validated
program wiring, and authenticates every filed operand against that exact
source. Carry-in names the preceding occurrence's carry-out; fixed state names
the preceding position's writer, or zeros at initialization; history names the
preceding position's window and the current row. Crossing a 1024-position
segment boundary does not permit a replacement state or window. A parameter
leaf names its declared parameter and layer instance. Public tokens, positions,
constants and encoder job inputs come from their bound job/program context.
Borrowed coordinates, substituted weights/routing, missing dependencies and
malformed material dismiss rather than convict the output.

The court calls the frozen `element_value_v1` for every element of the declared
64..127-element span, hashes the resulting raw-width bytes using the flat leaf
preimage and compares that digest. A different digest, or authenticated operands
for which the reference semantics refuse, proves this leaf relation false. A
matching digest is `NoFault`. The model holder's dependency recorder calls the
same evaluator over its own operand tensors and deduplicates the actual leaf
union. The court reads sorted proof leaves through cached layouts and binary
search; it does not build a full tensor, a per-element map or an operand tree.

The staged per-leaf admission derives maximum dependency reads, leaf counts,
full paths, raw-width carriers, reference line/sort scratch, hash/lookup work,
and bounded program/wiring work. Identity-coordinate relations use a contiguous
flat-span bound. Strided/Gather/MatMul/reduction bounds conservatively cap the
union by both dependency reads and the source's leaf count, not by opening an
entire source unconditionally. Every ragged output tail is priced at its actual
count, up to 127. A relation exceeding one MiB, 2^30 abstract work units, 32 MiB
scratch or 1024 operand leaves is refused before reference evaluation. These
are component bounds, not measured peak RSS/time or network-wide admission;
a consumer still needs worst bounds across all positions/leaves and shared
per-block/per-bond/network accounting before accepting this profile.

The strict fault decoder caps all vector counts before allocation: rank four,
64 node/hash siblings, ten position/prompt siblings, nine operands, 1024 total
operand leaves, 4096 values per legacy row leaf and 4096 prompt IDs. It checks
exact value spans, known dtype/version/option tags and end of input. The
in-memory verifier enforces the same carrier/shape/count ceilings and each
relation's derived opening limit. Legacy row leaves are filer-owned proof
material; no public DA service of them is introduced. Root-only hashing of an
already resident tensor uses fixed scratch and avoids retaining another tree.

Still required: typed node admission/statement binding, public hash-unit
DA/default/session accounting and deadlines, complete-profile malformed/decode
and small/wide-node courts, acquisition/localization from actual chain carriers,
fresh model-holder prosecution with independent node replay and Final/redemption,
measured verifier resources and honest liveness under aggregate invalid filings.
Real checkpoint/task/context/fidelity, arithmetic/backend conformance, whole
statement security and all other §II.14 requirements also remain open. This
stage implements the dependency-leaf relation component; it does not activate
or declare completion of the large-model path.

---

<a id="source-v1-0002-L02981"></a>

### 旧RFC-0002 / 原文2981–3097行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### 14.24 Staged flat-v5 all-position terminal-court resource admission

`misaka-palw-kernel::flat_scope5::flat_resource_scope_v5` derives a canonical
resource declaration from the validated program, maximum position count and
optional exact encoder binding. `check_flat_resource_scope_v5` independently
re-derives it and rejects any difference, including omitted relations, smaller
costs, altered occurrence counts, profile/program/extent substitution or a
changed job mode. The declaration has its own v5 root domain. There is no model
name, frontend identity or operator allowlist input.

The derivation validates wiring once, prices one representative per reachable
block node and counts every layer occurrence. The same shared price function
prices individual §14.23 filings and the registration envelope. No position
array, parameter tensor, output tensor or Merkle tree is constructed; program
serialization size is measured once and reused across relations. v1 permits
at most one H per tensor, and H is `min(position + 1, window)`. The maximum
resolved shape bounds every contraction/reduction/sort line, tree depth and
source size, but is **not sufficient alone** for the commitment grammar:

- Every large output span is conservatively priced at 127 elements, including
  an earlier merged tail when the final shape has only 64-element leaves.
- A source node that can cross 64 elements is priced for both flat-v5 and its
  largest sub-64 v3 shape. The envelope retains the larger leaf count, carrier
  and scratch bound; flat contiguous-span optimizations cannot discard the
  earlier v3 row openings.
- Initialization requires fewer sources than later positions. The maximum
  position includes the previous fixed-state writer and HistAppend window
  whenever either can occur in the admitted extent. The same symbolic block
  types and fixed proof depths apply to every layer instance.
- Constants and public inputs need no Merkle leaves, but still pay reference
  contraction/reduction/sort work and scratch. The previous staged flat price
  omitted those costs when skipping leaf authentication; both filing and scope
  prices now include them. This correction activates no shipping profile.

Every reachable relation must fit the existing one-MiB, 2^30 abstract-work,
32-MiB scratch and 1024-operand-leaf caps, including the v5 envelope bytes.
Malformed-header and decode costs join the maximum court vector. Logits have
no H under v1, so decode pricing is invariant across positions. The declaration
also records relation instances, maximum raw trace bytes, node commitment
bytes, flat-tree bytes per position and flat hashing input bytes per claim.
These are conservative independent maxima. They are neither a promise to
serve clear model values nor a requirement to retain every position's full
tree simultaneously. They are not yet the complete execution/checker/DA/network
resource vector.

An exhaustive small-program price oracle enumerates every position and leaf
for all 25 primitive samples and growing/sliding histories with differing row
sizes; every individual price fits the derived scope. Tests include earlier
63-row v3 paths and 127-element tails, wide intermediate histories, exact
encoder binding, 1,024 per-layer state instances, changed declarations and a
2^21-position extent with a 2^24-entry external parameter table, without passing
parameter images or position arrays to admission. Constant TopK beyond the
court work cap is refused by both admission and the exact filing guard before
reference sorting. Actual synthetic courts also cross 63→64→65 elements and
a sliding window, convict false relations and dismiss missing/borrowed sources.

This closes the staged **terminal-court resource preflight component**. A
shipping consumer still must bind and check this exact scope in typed class/job
statements, combine it with execution/checker/DA and aggregate budgets, and
implement bounded public material/default/session acquisition. Actual-node
fresh prosecution and independent Final/redemption, measured costs, real
checkpoint/task/context/fidelity, privacy, economic and liveness requirements
remain open. No shipping descriptor/class/route or fence selects flat-v5, and
this declaration does not confer reward eligibility or full RFC02 completion.

##### 14.25 Dormant actual-node flat-v5 candidate statements and courts

The consumer now has explicit `PostFlatModelVectorV3` and
`RefuteFlatModelVectorV3` route kinds, inner 31/32, owned by the existing
`palw_panel_free_v1` fence beneath the kernel-route fence. Shipping activation
parameters remain unchanged. These are non-reward candidate trace statements,
not execution claims or full conformance certificates.

The outer v3 statement binds the flat-v5 profile and the independently derived
§14.24 scope root to the descriptor-scoped class, model binding, program, PC,
job inputs, generation and segmented commitments. Its nested typed job/evidence
carrier retains the frozen v2 grammar; the new wrapper, trace/statement domains
and route kinds select the new interpretation explicitly. Existing v2 vector
commitments and wire encodings keep their meaning. Aux tables 57/58 store the
new statements and bounded bond catalog separately from v2 tables.

Admission derives the scope over the candidate's full admitted maximum extent,
not merely the shorter posted trace, and persists exact filing/work/scratch
ceilings. Every court rebuilds the typed context and scope and requires equality
with the stored ceilings. Cached fixed-size program/PC/body lengths price and
reserve the shared block budget before variable stored program, PC, post or
fault decoding. The PC-map tariff includes every admitted parameter instance.
Worst-case court admission must fit the existing guaranteed court budget and
one-MiB proof carrier. Invalid declarations, duplicate admissible posts and
invalid filings pay the actor's fee; exhausted block budgets stop further work.

The v2 and v3 catalogs share the same eight-statement limit per bond. Both
reserve the same original collateral through the existing global commitment
and exit-duty accounting. Adding a profile does not add capital or court
capacity. A successful bounded court slashes the statement poster once and
pays the existing challenger share through ordinary payout/redemption; the
poster's operator cannot collect that bounty. Invalid filings cannot refute
an honest statement or slash its model binder.

Actual-node synthetic trials cover the typed decoder and encoder routes,
large leaf and small whole relation convictions, borrowed proofs, forged
profile/scope/program/PC/role bindings, duplicate accounting, invalid-proof
block budgets, mixed v2/v3 catalogs and independent replay/redemption. The
filer reconstructs operands from its own model copy and authenticated public
job input and receives only commitment/hash openings for the producer output.
In these trials a test helper supplies those public hash openings; it is not a
node public-acquisition protocol. No execution Final is claimed by these tests.

Public flat hash-unit acquisition, localization, retention, default/session
rules and proof pre-emption seals remain open. Candidate admission still uses
the existing candidate plan, so this does not admit every large shape allowed
by the staged terminal-scope component. Full execution/checker/DA/network
resource accounting, fresh outsider acquisition through actual node APIs,
execution Final/redemption, real checkpoint/task/context/fidelity, privacy,
economics and honest liveness requirements remain open. The full RFC02 goal
remains active, and no shipping fence is armed by this increment.

---

<a id="source-v1-0002-L03374"></a>

### 旧RFC-0002 / 原文3374–3409行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### §14.32 — Mature actor-owned protected proof chunks (2026-10-11)

Append `FlatProofV6 { vector, actor } = 3` to the existing kernel chunk target
grammar. It carries only the protected reveal of that vector signed by the
sealing actor, and each part is signed by that same bond. Opening/continuing
requires an open protected vector and that actor's mature unspent seal; the
target deadline is strictly before seal expiry and vector liability. Completion
still requires the exact salt/proof seal and independently bound native court.

The existing two kernel chunk slots per bond, per-part original collateral,
64-DAA clipped TTL and completion/refund/expiry-forfeiture apply. These are the
bond's own slots, separate from the generic shared eight-group table. Bound
protected assembly to one MiB plus 16 KiB of signed envelope before variable
decode. Charge the part and a conservative three-assembly-byte allowance for
own stored-group handling before decoding that group. Opening/continuing also
needs capacity for the completing part and terminal court together, including
two runs. Candidate admission alone does not grant that transport capability.
An authenticated mature
target uses the proof reserve; missing/borrowed/expired targets use admission
capacity and do not open a group. Completion also pays the native reveal debit.

Common chunk-count/deposit queries now read only the bond-key prefix and fixed
headers; they do not decode all actors' variable payloads while calculating free
capital. Stored parts and signatures retain their existing serialization. The
SDK/CLI directs large protected reveals through signed kernel parts and refuses
a silent generic split; large posts retain generic carriage. Supplied offline
seal rows remain unverified until authenticated locally.

The actual-node saturation trial forces a synthetic valid court into multiple
parts and holds all generic slots open. It checks absent/borrowed seals, owned
deposits, one conviction/payment, cleanup and independent replay. This tests
the route, not a real large checkpoint or whole-network saturation bound. Shared
invalid-proof load, all-target aggregate metadata/state and expiry-sweep costs,
worst-job deadlines, transport/acquisition/retention and complete resources still
need evidence. No shipping fence or full RFC02 acceptance is granted.

---

<a id="source-v1-0002-L03516"></a>

### 旧RFC-0002 / 原文3516–3586行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### §14.35 — Explicit mixed-court graph scope and prepaid independent replay (2026-10-11)

`flat-resource-scope/v7` now selects the point court through an independently
derived **complete graph** contract. The commitment grammar remains flat-v5
internal/v3 parameter; the separately domain-bound court-set id and scope root
select the v7 terminal semantics. Every reachable block node, every unrolled
layer occurrence, the admitted maximum positions, decode and malformed-header
courts are included. A declaration must equal the complete derivation in every
field. A small TopK filing cannot admit another oversized relation or replace
the graph's exact program, mode, extent or resource ceilings.

Committed TopK sources select the v7 point court. Public/constant/initial-zero
sources retain bounded v5 courts. When initial state and a later state writer
require different courts, the scope prices both and selects the court from the
exact typed wiring at that position and occurrence; neither a registrant nor a
filer chooses the cheaper kind. Frozen state roles, per-layer state namespaces,
history windows, source coordinates and v1 type/shape rules remain unchanged.
Admission visits metadata, not model images or position arrays. It accounts for
earlier sub-64 legacy layouts when monotone H crosses into flat commitments,
including the case where an earlier legacy row tree has more paths than the last
flat tree. It serializes/hashes the program once for the unrolled TopK prices.

`flat-fault/v7` wraps either the exact bounded v5 court bytes or v7 TopK point
bytes under a new version and discriminant. The mixed court rederives complete
scope, checks the wiring-selected kind and exact filing bounds, then dispatches
the existing authenticating court. All historical v5/v6 decoders and routes
retain their previous acceptance behavior; none implicitly accepts this wrapper.

The ordinary v7 verifier reexecutes from independently acquired registered
parameters and authenticated public job ids. Its shared replay engine still
checks ordinary relations and delivered decode ids. When TopK differs, it reads
selected ids through a new **16-KiB, single-output-leaf, bounded byte codec**,
authenticates every reply to the same node/type/shape/root, scans its own scores
under the derived budget and builds a bounded point counterexample. A public
value reply is evidence for the selected member, never the reference score or
the correctness of the computation. Missing, oversized, wrong-version or
borrowed material yields a demand, never a Clean verdict or conviction.

The versioned local replay session independently binds the complete scope and
prices reference i128 tensor payloads, parameter/trace hashing, history rows,
node/hash/value reads, reply bytes, TopK scan work, temporary selection memory
and attempts. The full hashing/scan attempt is reserved before artifact or public
callbacks; missing material or an inconsistent artifact consumes that attempt.
Each response consumes its read and byte allowance before delegation. Retries
share the same counters. Storage bytes, the reference i128 representation and
temporary selection bytes are distinct quantities. These component prices do
not yet certify total allocator/RSS, all-primitive CPU/deadlines, public storage/
delivery, per-block/per-bond or network-wide economic and load admission.

An actual synthetic N=10,485,760, K=2, **one-position graph** now passes honest
independent replay and prosecutes a wrong selection through the mixed court.
The mixed filing is 17,488 bytes; its reference source occupies 167,772,160 bytes,
and its scan charge remains 251,658,288 with 128 temporary selection bytes.
Separately, metadata-only admission covers TopK→Gather→state update with 1,024
layers and the existing 2,097,152-position ceiling; this is a derived envelope,
not execution or a materialized maximum-length job. Synthetic graphs exercise
range/duplicate/dominance faults, state and history transitions, all sub-64/64/127
thresholds, decode, encoder inputs, substitution refusal and prepaid retries.

No consensus post/seal/reveal/chunk/DA or reward route selects the mixed v7
contract yet. The protected-v6 actual-node regressions remain compatibility
evidence, not v7 native acceptance. A subsequent consumer must bind the new
court set and complete scope to admission, public acquisition, actor-owned
prosecution, reserved transport, shared resources and original-capital payout.
Versioned token/patch/audio job lengths, H-axis sparsity/state semantics, the full
shared resource vector and economics, real checkpoint/task/context/fidelity/
performance, fresh outsiders, independent execution Final/redemption and A6/A8
remain required. Conditional external-verification mode A remains unchanged;
this increment adds no consensus model-availability obligation. Full RFC02 stays
active and shipping fences stay dormant.

---

<a id="source-v1-0002-L03877"></a>

### 旧RFC-0002 / 原文3877–3905行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### §14.43 — Actual checkpoint point court and parameter continuity, v9

The pinned Qwen2.5-1.5B integer artifact is executed twice through independently
opened mapped executors for one fed token (4498), with all 7,658 nodes. A finder
without the producer's mutation coordinates locates the first divergent node
after 35 agreeing prior roots. A fixed first-leaf typed reply supplies the
divergent element; this trial does not establish arbitrary-leaf public hash
descent or live transport. It independently authenticates the selected real
matrix parameter against PC-v3, convicts the ordinary point and dismisses
missing source. The proof is17,944 bytes, with combined court work151,708,672
and scratch21,316,001 bytes. Whole-process time/RSS is16.30s/3,995,418,624B.
The earlier quadratic node-directory localization trial also passed at76.06s;
its source and result are retained rather than rewritten as the linear trial.

A separate streamed check authenticates every one of the1,636 registered
parameter instances against the exact mapped integer artifact:1,835,516,100
raw payload bytes read,27,275,264-byte maximum derived helper workspace below
32 MiB,81.07s whole-process time and1,853,112,320B maximum RSS. Artifact SHA is
checked before and after both trials. This is complete integer-artifact-to-PC
continuity, separately from empirical original BF16 fidelity; it is neither
original arithmetic bit equality nor a native source-binding substitution.

[The checkpoint observation](../design/palw/tir/evidence/qwen25-real-courts9-observation.json)
records immutable executable/source hashes, commands, actual/max positions,
the exact proof and limitations. Scope metadata covers32 positions, whereas
this prosecution executes one position. No model admission, real native
execution Final, maximum-context or cohort/A6/A8 credit follows from these
component results. Those acceptance steps remain open.

---

<a id="source-v1-0002-L04060"></a>

### 旧RFC-0002 / 原文4060–4092行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### §14.48 — Native mixed-court conformance and bounded candidate restoration, v9

The explicit opt-in v9 route registers the candidate, posts its rooted typed
trace, serves selected values, and connects own seal/reveal/chunk and settlement
to the same candidate scope and court. Legacy roots remain unchanged when the
new tables are empty; v9 grants conformance rights without activating a funded
execution descriptor. SDK checking independently derives the full candidate
scope/replay/resource10 rather than trusting transmitted plan metadata.

Ordinary native ledger restoration prepays and reads at most128 fixed candidate
headers. It preserves large raw rows outside the journal diff; candidate-local
DA/court paths reconstruct only their authenticated candidate scope. Public
bulk import independently validates rows but first bounds row count, total raw
bytes and aggregate derivation work. Owner withdrawal is refused while either
v7 or v9 protected traces or pending own proofs/DA still need that candidate's
metadata. Expired candidates with no dependencies may retire without expanding
the catalog or deleting live evidence.

The actual Qwen fixture reaches authenticated selected DA, own seal/reveal,
original-collateral finder payout and independent IBD replay. Its node finder
uses a prepared actual-model proof; a fresh finder running the model and locating
an unknown bad coordinate through only public node data remains separate work.
This result grants no honest funded execution Final or A6/A8 coverage.

The coherent native milestone passes51 selected core regressions,35 real-node
conformance regressions,134 SDK tests and five candidate-import/adversarial
tests. The new distinct tests are five candidate, four SDK, three typed-DA and
eight node tests; these20 increase the protected component pool from341 to361.
Existing regression tests and previous quota snapshots are not counted again.
Exact source preimages, immutable binaries, trial failures and test-only
inventory corrections are retained in
[the native milestone verdict](../design/palw/tir/evidence/rfc02-native9-typed-da-node-verdict.json).

---

<a id="source-v1-0002-L04368"></a>

### 旧RFC-0002 / 原文4368–4420行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIRのCourt専用契約をCourt統合RFCへ。

##### 14.56 Original burst residency and the new full-court contracts

The full-court25 checker, actual/max EOS stopping29 and Stage28 contracts have
their own records, roots and bounded decoders. They do not enlarge the frozen
v7/v9/v11/v14 court, count, pricing or length semantics. Text29 reserves the
original maximum before Fund, checks the actual first-EOS-or-cap length at
Commit, and retains the maximum reservation after early stopping. Stage28
reserves the full original component bundle, real source namespace and the
whole acquisition/discovery/prove/delivery horizon before Fund. New paid
transport29 uses u16 parts and at most512 parts; old target6/7 carriers retain
their u8 and64-part meanings.

Every new Final27 consent must additionally bind Queue30's original policy and
independently derived plan. Delivery density alone cannot admit a burst: account
for all answer/proof variants, complete carrier envelopes, populated input UTXO
scripts and retained index/transaction copies. Reserve both packet count and
owned bytes in the SAME shared native global before collateral or escrow moves.
Text and Stage share that global. Joint physical22/Queue30 admission is atomic;
a live legacy23 lease without an original Queue30 promise cannot be silently
relabeled. A caller-supplied serialized memory quote is not a checked plan.

An honest consumer provisions the declared native queue in addition to its
ordinary mempool. Ordinary transactions, including local high-priority entries,
cannot borrow that earmarked capacity. Local RAM scaling cannot reduce the
original native reservation. The new queue's complete singleton envelope uses
confirmed inputs; an unconfirmed ancestor is not an additional free carriage
slot. Same-unit fee replacement subtracts the superseded original variant once,
while a third independent variant gets no position. Generic fee/expiry removal
must retain live indexed native units. Genuine terminal collection releases
the exact Queue30 and physical22 leases together after their original horizon,
all pending answers/owners/fragments and physical cooldowns close; it never
restores frozen21 quota credits.

Final27's trusted public profile binds Queue30 policy in network identity.
An implemented node selector is distinct from measured whole-claim detection,
complete cold-reference cost, actual delivery and independent Final/redemption.
These conditions and A6/A8 evidence still gate activation. All shipping fences
remain absent until completion evidence passes the contract.

Initial registration31 is a separate, ordinary paid pre-Fund transport for a
large canonical source/class body. Its signed Start binds original owner,
permanent network sequence, exact target (Text29 class or Stage28 source/class),
whole-body root, byte extent, u16 count and expiry inside the original interval.
At most4 objects and1 per mature operator are retained, with512 parts of32KiB,
16MiB total and2 original attempts per part/assembly. Charge whole retained and
assembly buffers in the SAME21-axis global before allocating/storing them;
malformed inner lengths consume their paid attempt before decoding. Assembly
rechecks all extents and the original hash, then invokes the target's genuine
registration checks. The transport cannot dispatch Fund, Commit or Final and
offers no protected delivery guarantee before Fund. Expired paid collection
deletes exact bounded fragments while retaining the permanent sequence.
Frozen object/proof chunk counts and canonical TIR program limits are unchanged.

---

<a id="source-v1-0007-L00508"></a>

### 旧RFC-0007 / 原文508–532行

出典: [0007-palw-verification-certificates-and-algebraic-checks.md](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)。配置理由: 異議・exact courtへのescalationを一元化。

##### II.8 Failure, escalation, and the exact court

**A failed check proves the witness wrong, not the claim.** A producer can serve a bad witness for an honest claim, and gains nothing
by it. So a failure is never a verdict: the seat files no `Valid` (ADR-0098 Decision 2) and escalates. Escalation finds **the first
committed row the seat disagrees with** — the named leaf of ADR-0111, which the unchanged one-move court then tries exactly. That needs
the honest values from the failing node to the next commit point, which needs that node's weight rows. Two paths, either or both:

- **(a) Fetch the rows.** The seat fetches the failing node's weight rows as inventory openings: leaves of ≤ 32 KiB with closed-form
  indices, opened against `artifact_root`, from the producer or from any holder. A request that is not answered goes to ADR-0111's
  bounded on-chain availability request.
  - The worst-case fetch is the largest served weight: the vocabulary head, 311–927 MB for the classes measured.
  - **Block sketches are the default.** For each weight site, a seat keeps `B = ⌈|W| / F⌉` row-block sketches, `F` being a fetch cap
    (proposed 2 MiB). A block sketch sums only its block's rows, and the routine check uses the whole-site sketch, the sum of its blocks.
    The blocks are read only after a failure, to name the failing block's `|W| / B ≤ F` bytes of rows.
    - At `B = 256`, the 151,936-row head of Qwen2.5-1.5B costs 3.1 MB of extra sketch and a 1.8 MB fetch instead of 467 MB.
    - The overhead is `K · 8 / F` of a site's weight bytes: 0.6 % at `K = 1,536`, 3.4 % at `K = 8,960`.
    - A routed weight's sketch is already per expert, so a failing row names one expert slice (1–3 MB).
  - The **transfer bound** per escalation is therefore ≤ `F` per failed node, plus its paths. The seat then recomputes up to the next
    commit point, a cone the court's ceilings already bound (≤ 16 Mi terminal MACs a tile).
- **(b) Hand the interval to a holder.** The seat hands the interval to a seat that holds the class: a full-coverage holder, or the
  layer shard of RFC-0006 that holds the failing layer. The holder replays the interval and names the leaf.

Either way the court is unchanged. It sees the named leaf, its cone's committed inputs and the artifact openings, never a sketch or a
witness.

---

<a id="source-v1-0008-L00154"></a>

### 旧RFC-0008 / 原文154–192行

出典: [0008-palw-claim-backed-consensus-blocks.md](../rfc/0008-palw-claim-backed-consensus-blocks.md)。配置理由: EXEC固有の裁定・証拠保持契約を一元化。

#### 6. 公開検証・裁定・証拠保持

[RFC07 Part V](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)、[RFC11 §15](../rfc/0011-permissionless-model-and-long-context-onboarding.md)、[ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)に従い、active Kernel/plan/suiteをroot・slice・receiptにbindする。通常検証は指定されたencoded/algebraic constraint検査、異常時は有界exact courtへのlocalizationとする。未対応演算にVM代替経路を設けない。

各sliceのinitial/final state、predecessor、非重複range、証拠とboundary間の最初の不整合を、普通の非Panel public bondがproducer秘密状態なしに検証・訴追できる必要がある。[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)のdispute、model/material availability、liability、retention gatesを保ち、多数派receiptで有効fraud proofを無効にしない。[RFC15](../rfc/0015-panel-free-permissionless-verification.md)のPanel=0は別の全gate成立前に有効化しない。

whole-claim誤受理確率はslice/boundaryの検査を合成する。同じ証拠やchallengeの複数carrierを独立検査と数えない。reorg/pruning/IBD後もpublic prosecutionとFinal後の回収に必要なmaterialを保持する。EXECのweightが0でもpending reward、collateral、DA、courtのexposureは0ではない。

**2026-10-08共通challenge protocol:** [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)を唯一の正本とする。root/class/planは`challenge_policy_id`をbindし、各`WORK_SLICE` subjectはroot、index、canonical range、前後state、evidence/oracle rootsをchallenge前に固定する。future source windowはそのslice statementのcommit後に始める。root-open時に既知になったseedを、後で自由に作れる全slice statementへ再利用しない。whole-root検証も同じpolicyのbinding/coverageに従い、seed式をRFC8側で追加しない。

EXEC_TX、heartbeat、BASE-0、未検証sliceはbeacon entropyにならない。独立検証・Final・G14・source資格を満たした長いroot useful workを将来sourceにする場合も、root work identity単位で一回だけであり、N carrierをN sourceに数えない。beacon不足はpending/既定deadline処理であり、sliceにweight/clockを与えたりheartbeat・BASE-0を抑止したりしない。

deterministic traceのcommitmentだけで物理的な計算時刻を証明できるとは主張しない。precomputation、fork reuse、root/jobの複製、Sybilによる計算credit再利用をwork identityと台帳で防ぎ、その限界を仕様とUIに表示する。

##### 6.1 モデル入手への不介入・bond配分と規則E — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。

EXEC_TX/EXEC_SLICEのweight/DAA=0、unique-work、root単位一回精算を維持する。
適用modeのclaim固有Final/court条件・暫定creditとsafe settlement・規則Eの全writer/reader/undoを検証する。
model配布不能だけの停止を起動せず、閉鎖時の未検出workを必ず排除できるとは主張しない。

---

<a id="source-v1-0032-L00001"></a>

### 旧RFC-0032 / 原文1–68行

出典: [0032-shared-court-terminal-material-and-nonblocking-ingress.md](../rfc/0032-shared-court-terminal-material-and-nonblocking-ingress.md)。配置理由: 主題別統合。

### RFC-0032 — 共有courtの終端materialと非独占提出

Status: **Draft / Review Required**。利用者はA（認証済みmaterialと終端intent）の詳細仕様化・隔離実装を承認済み（2026-10-21）。採択・本番規則の置換・公開wire・有効化は行わない。判断案は[ADR-0199](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md)。既存の[共有frontier契約](../rfc/0029-owner-independent-shared-tir-query-data.md)と[正本](../rfc/0026-court-policy-master-contract.md)の未完を埋めるための提案。

#### 1. 解く問題と確認済み範囲

全owner枠が共謀者でも外部検証者一人が進めるF03では、最初の不完全な証拠bodyがroot/path共通slotを占有して他の提出者を止めてはならない。現行の数値fragment adapterはcomplete bundleだけを検査し、未完成prefixの合意保存・独占を導入していない。このままではmulti-block提出は未実装である。

[依存検証](../evidence/palw/g14-integration-candidate-2026-10-20/terminal-dependencies.md)では既存GQA fixtureの一つのnamed leafの二履歴・24終端でparameter opening/carriageを実際に使用し、個別欠落48ケースは拒否した。共有DATAのStep/Rows/Tile/Trace/Idsだけで終端証拠を再構成できるという前提は置けない。prompt/decodeはその24終端では未使用で、全original・全modelの依存を測定した結果ではない。

#### 2. 比較する方式

| 方式 | 未完成bodyの扱い | 追加で閉じる必要があるもの |
| --- | --- | --- |
| A: 認証済みmaterialと終端intent | 任意filerのbody prefixを共有終端slotの所有権にしない。原始materialを認証・保持し、適格な外部filerのintentに対して既存builderで終端を構成する | parameter/promptを含む閉じた依存集合、担当者・保持契約・搬送予約、構築予算、決定論的canonical化、全合法分岐費用と現実の原資 |
| B: filer別の独立証拠stream | filer間でbody進行を分離し、完了証拠のみroot/path結果へ統合する | 無限のfiler/streamに対する有限の有料受付・記憶上限、共謀者の再充填下でも一人が完了できる予約・公平性の実証 |

利用者の選択に従い、Aで詳細仕様化・隔離実装を進める。Aも現段階ではF03を保証しない。Bを単にActive bond/fee/先着slot数で解決済みと扱わない。Aを採る場合でも、既存のcomplete-bundle入力を無条件の有料・公平提出保証に昇格させない。

#### 3. Aの認証・canonical化境界

- 合意状態の実original、登録program/layout、artifact root、prompt commitment、native rules/profileを起点にする。受信者指定のrootやcallerのchecked flagを信用しない。
- parameter materialの識別はartifact identityと既存inventory座標。受理時にleaf数・座標・tensor/layer/row piece・dtype/長さを登録programから照合し、既存Merkle openingを検査する。rootが合うだけで別inventory形状を許さない。
- promptは実nativeのFlat/Merkle形に従いoriginalへ結び付ける。生成IDとpromptを混同しない。既存の無効なモデル改善・画像動画経路を追加入口として有効化しない。
- 同じ認証済みmaterialはfilerの所有にしない。同じ論理座標への異なる有効なencodingを新報酬・別slot・timestamp resetにしない。断片のhashだけでmaterial認証済みとしない。
- `PalwTirEvidenceStoreV1` adapterは保持済みの認証済み入力だけを読み、任意のlocal cache/network storeへ黙ってfallbackしない。欠落は明示的refusal/未供給であり、数値有罪・default・slashへ変換しない。
- 既存builderのcanonical証拠と同じbyte列・native local outcomeを生成する。構築は全投入資源を事前にadmitし、失敗で状態を部分更新しない。intent署名は依頼者の認証であり計算の正しさの証明ではない。

#### 4. 供給・原資・期限は別の必須条件

Aの原始materialも、最初の不完全な送信が唯一slotを占有するなら同じ問題を再発させる。canonical materialの供給担当者と代替配送、必要fragment集合、実供給・配送・収録、回復義務を既存F02契約へ結び付ける必要がある。producer以外のmaterial保持者へ義務を新設した扱いにしない。既存契約を用いるなら、その署名authority、実支払原資、保持horizon、履行可能性を実データから照合する。契約が不足すればoriginal受付を成功扱いにしない。

費用はoriginalの全許容分岐について、material fetch/authentication/retention、root/round/intent、builder再実行、multiproof結合、署名・hash・拒否/重複ingress、snapshot/restart/IO、未決責任archiveまで積算する。共有重複排除は実装と費用証拠がある箇所だけに適用する。未没収bondをPanel売上としない。最低Account Bondが運用資本を保証するとも扱わない。

最初のnoticeは承認済み隔離Aの `a <= d <= a+3,120`、hardは元originalの `a+10,000` を維持する。後続intent/material/再提出でresetしない。全前提の実成立後に必要処理がhard内へ収まること、未供給ならno-slashとなることを検証する。hard到達やTTLだけで担保返還・証拠削除しない。

#### 5. 検証と進行順

1. 固定した既存fixtureで、原始materialだけから既存builderへのadapterを作る。正直/偽数値履歴、全兄弟、parameter/prompt/decode依存、欠落・改変・別original/別artifactを検査する。元fixtureを通るよう差し替えない。
2. 実native shape preflightと原子writer、独立namespace、保持/復元/journal/undo/reorgを揃える。旧wire tagとX3の意味を保持する。
3. 全originalのmaterial依存・資源・担当者・予約・資金を一括で照合し、受付・最遅notice・回復・終了効果を接続する。単一leafの試験を全分岐保証にしない。
4. 実nodeで全owner共謀・不完了/再充填・外部一人・restart/reorg/pruned IBDを含むT05を実施する。全T01–T15/R-D2、実費用、実model doorを別途満たすまで武装を拒否する。

未決の単価・受付率・配送時間・保持期間・actor契約は仮値で埋めない。本書だけで上限引上げ、model排除、モデル審査、暗号学的計算証明、本番規則の採択を行わない。

#### 6. 隔離borrowed material viewの実装範囲

`SharedTirBuilderStoreV1`は保持済みoriginal/sourceへ束縛した認証済みunitと、登録programで再検査したparameter openingだけを読む。必要materialがなければNone/構築拒否とし、fixture execution・disk・networkへfallbackしない。元sourceに束縛した両端のStepLeaf openingから連続runのcanonical siblingsを導くnative変換を追加し、全leaf hash treeの隠れた読み出しを避ける。新しい計算証明ではなく、既存Merkle commitment上のデータ変換である。

[実装・検証記録](../evidence/palw/g14-integration-candidate-2026-10-20/material-builder.md)では既存GQA fixtureの正直/偽数値履歴・24終端で従来builderとのbyte一致と欠落拒否を検査する。parameter集合の試験供給には直接builderのread-setを用いるため、全originalの先読み・依存閉包を実装済みとはしない。

このcheckpoint（7de51563d）ではborrowed read-only viewであり、prompt・row-pin、永続化・再起動、materialの供給/保持/原資・全分岐予約、実際の処理機会と公平なmulti-block提出は未完。clone・multiproof/range組立・builder outputを含む完全な資源/費用profileも別途必要である。本節はAの限定的な隔離実装記録であり、F03/T05や本番規則の採択・有効化を示さない。

#### 7. Promptとrow-pinの隔離projection

保持済みoriginalのwork rulesからprompt形式を解決し、Flatの全listまたはMerkleの供給済みtileを既存commitmentへ認証する。形式の差替え、改変、個数違い、重複/逆順tileは拒否する。未供給tileを取得済みにはしない。

row-pinは認証済み全IDと、committed token/要求laneの二つのTraceTileから既存native形式へ構成する。whole rowsやlocal cacheへfallbackしない。これらはidentityを保持するデータ変換であり、裁定結果・計算credit・供給証明ではない。

[検証範囲と限界](../evidence/palw/g14-integration-candidate-2026-10-20/prompt-row-material.md)。永続化、promptのbounded wire入口、全originalの依存閉包・供給/保持/原資・予約、完全な費用profile、F02/F03と実node T05は引き続き未完である。

#### 8. 保持された終端への署名付きintent（隔離・限定検証済み）

A案のprivate構築入口では、intentがnetwork、original claim、scope、native policy、leaf、canonical root ID、child path、全ancestorに束縛されたparent ID、literal filerを署名する。新しいdomainを使い、既存bottom全文の署名を転用しない。DTOにpublic wire decoder・lifecycle tag・native rowは追加しない。

`SharedTirBuilderStoreV1::build_terminal`は保持されたrootと各ancestorを再検査し、実native規則・元hard内の時点と照合する。callerのphase/binding/evidence bodyを入力にせず、登録programを既存resolverで戻して、認証済みmaterialだけから既存bounded builderを呼ぶ。元leaf/output preimageと一致する完成証拠だけを返し、programの既存carriage省略とnative close byte上限を維持する。資料不足・不正署名・不正path・予算不足では構築拒否となる。状態・予約・最初のfilerの専用slotを作らないため、別の適格署名者のintentは同じmaterialから同じ完成bytesを構成できる。

この入口はread-onlyであり、署名・source/root/ancestor検査・scratch/material・outputの所有費用はcallerの事前admissionを必要とする。返すbuilder meterはその一部分で、実CPU・fee・資本・保持・全original全合法分岐profileではない。public有料ingress・原子的な費用/資本予約・実配送/供給・materialの永続化・qualified裁定/精算・T05/T01–T15と実model doorは引き続き未完。[実行結果・正確な範囲・hash](../evidence/palw/g14-completion-2026-10-21/terminal-intent.md)に保存する。

---

<a id="source-v1-0035-L00001"></a>

### 旧RFC-0035 / 原文1–135行

出典: [0035-court-original-funded-contract-and-role-consent.md](../rfc/0035-court-original-funded-contract-and-role-consent.md)。配置理由: 主題別統合。

### RFC-0035 — shared court originalの実原資と役割別同意

Status: **Proposed / Review Required** (2026-10-21)。[ADR-0202](../adr/0202-court-original-funded-contract-and-role-consent.md)ではA（実UTXOで裏付けた独立escrow）の詳細仕様化・隔離実装がユーザー承認済み。金額・新料金・本番採択・pre統合・武装・配備は未承認。RFC-0026の原子的受付を実装するための未確定な金融・署名契約を具体化する。実装済み・採択済み・有効化済みとは扱わない。

#### 1. 問題と既存経路

[RFC-0026](../rfc/0026-court-policy-master-contract.md)と[RFC-0032 A](../rfc/0032-shared-court-terminal-material-and-nonblocking-ingress.md)は全合法分岐の容量・依存・保持・費用・原資をoriginalで閉じる。しかし現在のnative originalのrentは実carrier feeからburnされる。frame depositはbond上の名目exposureで、将来のPanel/relay作業料を支出できるescrowではない。既存slashもcollateralとslashedを更新し最終releaseのburn obligationへ接続するため、支払原資と呼び替えない。

旧TFP1 envelopeはversionとEOFを厳密検査する。旧custody pledgeは旧resource planとsource・claim・job・horizonを署名するが、新しい共有全分岐graph、将来の役割、実報酬・返還条件には同意していない。既存wireへoptional tailを足したり、旧署名を新契約の同意に流用しない。

#### 2. 選択肢

##### A — 実UTXOで全額裏付けた独立運用escrow（推奨候補）

original carrier transactionの実入力から、元のbond/予約資本・既存rent・実transaction feeとは別に運用資金を確保する。新契約に束縛された専用escrow出力/支出規則を設け、実際のUTXO金額・spend script・唯一のoutpointをnative UTXO検査とoriginal state遷移の両方で照合する。state rowに金額を書いたことだけでは資金確保としない。新署名domainでpayerが契約と支出上限に同意する。

各合法分岐の必要支払を、単に互いに排他的と仮定して減額せず、採択する正規化・共有・反復受付の規則から導出する。作業料・carrier fee償還・保持・復旧・終了効果について、誰が何を満たせばいくら支出できるかを明示し、出力の実spendとstateのonce-only accountingを同じ原子遷移で更新する。UTXOが未支出なら支払済みとは記録しない。UTXOとstateのいずれかの検査失敗でoriginal/支払の全体を巻き戻す。

保存則は `実入金 = 実支払 + 実burn + 実返還 + 未解放残高`（すべて同一資産、checked integer arithmetic）である。pledged/nominal金額を入金に含めず、同一outpointの二重割当・二重支出・他claimの原資流用を拒否する。escrow返還には未決作業・復旧・費用・保持・他の明示された責任の解消が必要で、hard到達だけでは返還しない。未確定受取人への恣意的な送金権限をproducerやPanelへ渡さず、採択された決定論的な履行/精算規則とその証拠を支出条件にする。

これは新しい本番資金方式を黙って採択する提案ではない。専用script/UTXO検証・wallet構築・consensus accounting・reorg/IBDの設計と隔離実装が必要であり、作業完了まで有料公開入口を開かない。新しいscript識別子・wire tag・数値料金はこの文書で割り当てない。

##### B — 新carrier feeの実徴収額を運用口へ分配

新original専用の実carrier feeから、既存burnと区別して運用原資を徴収し、受益者と未解放残高を追跡する。fee/coinbase collectible/burnの規則に新しい会計経路が必要で、旧rentを遡及して再配分しない。実資金の保持と将来支払の裏付けが成立するまでstate上のcreditを発行しない。既存fee経済への影響がAより広いため、Aを推奨する。

#### 3. A/Bに共通の原子的契約

- 新しい別envelope/versionと新署名domainを使う。既存TFP1・旧object tag・旧claim identityの意味を変えない。新入口は独立候補profileで、本番presetへ登録しない。
- 元claim/job/source、登録program/layout、materialの閉包、全合法分岐のcanonical相対graph、費用・資本・保持・復旧の規則、実原資、役割別同意を束縛する。callerが作ったgraphや`paid=true`を完全性/支払の証明にしない。
- 通常の事前署名で未知のaccepted DAAへ同意したことにせず、明示したacceptance intervalとcanonical相対graph/必要horizonを署名する。実accepting DAAで一意に絶対化し、元の3,120/10,000と依存契約の既存horizon内に収まる場合だけ受理する。正確なtarget DAA限定の署名を将来サポートする場合も別の明示形式にする。
- producerの署名で将来Panel/custodian/relayへ新義務を課さない。既存契約の明示した範囲で足りるか検査し、不足する新しい役割には本人の署名済みconsent・現在authority・受付/応答/保持期間・資本/報酬条件が必要。未同意の将来actorを仮定して受付を成功させない。
- 将来の外部filerの本人同意はその有料経路を使う時点で検査する。originalでは、任意の適格な一人が使える共通frontier・容量・実費原資を確保する。owner枠/先着不完全bodyを占有権にしない。
- prepareで全検査・必要物の有界導出を行い、普通のFP書込前のcheckpointへ新pairingを含める。installではFP/frame/preparation・rooted全graph・同意・実原資と資本予約を一括更新する。見積り成功だけで書込済みとしない。
- graphをrooted契約へ保持/元sourceから再導出し、load・forward mutation・repair・undo/replay・pruned IBDで同じ拘束を検査する。部分graphを渡したcausal予約成功はoriginal受付成功ではない。

#### 4. 未決値と拒否条件

料金・担保額・draw上限・保持期間・配送/収録前提は、本書の構造選択から自動採択しない。RFC-0027/ADR-0193の100,000 MSKは実用運用資本の十分性を保証しない。12,000 MSK/claimを料金へ昇格しない。本契約をその未実装Account Bondへ依存済みとも扱わない。

実fee schedule、全費用の同一profile、必要な署名契約、実UTXO資金/決済規則または原子的予約が未完成なら新入口を拒否する。仮単価・仮資金・nominal deposit・非供給slotを使って合格させない。旧入口の本番規則をこの候補へ置換しない。

#### 5. 受入条件

1. 未資金/偽outpoint/他claimの資金/二重割当/非署名actor/不十分horizonをordinary FP書込前に拒否し、FP/frame/新court・escrow会計のdeltaが空である。carrier transaction自体をUTXO検査で拒否した場合だけ、そのtxのUTXO deltaも空となる。PALW objectだけが不成立の場合、実際のordinary fee/rentやpayerの通常fallback outputまで不存在と報告しない。
2. すべての資金入出力で実UTXOとrooted accountingが一致し、保存則・部分支払・一回限りの徴収/返還・reorg・二重支出を検証する。実burnと支払とnominal exposureを分ける。
3. callerに部分graph/安いprofile/偽paid flagを与えても受付できず、全owner共謀・再充填下の一人経路とF02のno-slash復旧を実nodeで通す。
4. 元の期限・他claim責任・資格・現在署名権限・第三者relayを保存し、停止Minerの私鍵なしで承認された処理だけが進む。
5. t12同一性とRFC-0026 T01–T15を同一候補で通す。実モデルdoorと飽和・費用の測定は別途必要で、toy/会計例で代替しない。

実装・検証結果は未記録。A/Bの選択は詳細仕様化・隔離実装に限り、本番採択・旧claim移行・pre統合・武装・配備を含まない。

#### 2026-10-21 ユーザー選択 — Aの詳細仕様化・隔離実装

ユーザーは「A：実UTXOで裏付けた独立escrow」を選択した。上の選択待ち表記は選択前の履歴である。実outpoint・支出・保存則と役割別同意を束縛する詳細仕様化・隔離実装に進む。金額・新料金・draw上限・保持期間・本番採択・pre統合・武装・配備を含まない。実UTXO/決済・全費用/資本・原子的original受付の接続と検証は未完であり、この選択だけから資金確保済み・G14完了を導かない。

#### 6. Aの実装境界調査 — 未接続のUTXO保護

`crypto/txscript/src/lib.rs`の現engineは未知のscriptPublicKey versionを即座に成功とする。したがって、未対応versionのmarkerを書くだけでlockされたと扱ってはならない。`OP_TRUE`等の通常は自由に支出できる出力へ金額を書いただけでも原資保護は成立しない。

既存PALWのobject受付拒否とordinary carrier transactionのUTXO受付は別の段階である。新contractが不成立のcarrierについて、専用資金だけが無保護で残る実装を作らない。資金outpointのnativeなspend拘束を原子的に成立させるか、不成立時もpayerだけが回収できる通常署名のfallbackを維持する必要がある。Miner停止後のrelayには、payerの将来の秘密鍵操作を必要とせず、元の署名済み契約が明示した決定論的な支出だけをnativeに認可する経路が別途必要である。通常inputの署名検査を任意のpayloadや`paid`値で省略しない。

既存bondのUTXO保護は`UtxoProcessingContext`/`BondSpendFilter`、selected-parent stateのlock/burn集合に加え、同mergesetで新規に宣言されたbondも保護する。escrowにもown-bodyだけでなく同block・mergeset・mempool・IBD/reorgに一貫した保護が必要である。単にparentのescrow mapを読むと、資金を作った同block内で横取りできる可能性が残る。

これらの実UTXO拘束・適格支出・不成立時回収が未実装なら、rooted金額や資金DTOを作っても「実escrow確保済み」にはしない。新script/opcodeの番号、engine規則や本番forkはこの調査記録で採択しない。

#### 7. 次の隔離実装で必要なnative adapter

以下はAを実装するための検査境界であり、実装済みの支払経路・新script番号・本番forkの採択ではない。

1. **実carrierとのpairing。** 新契約は同carrierのfunding output index・実額・payer/script・元claim/source・canonical graph/価格規則・返還先を署名する。txidをそのtx自身の署名payloadへ自己参照させず、最終carrierの実txidとindexからoutpointをnative側で確定する。callerが別outpointを指定して資金を作れたことにしない。accepted transaction extractionから実output factsを引き継ぎ、通常のUTXO validationと一致させる。
2. **不成立時の資金保護。** 新しいPALW contractが不成立でもcarrierがordinary transactionとして残る場合がある。新outputを無条件支出可能なmarkerにせず、payerだけが回収可能なfallback、またはtransaction自体を拒否する完全なUTXO連携を必須にする。失敗したoriginalの金額をrooted残高へ加えない。
3. **同block/mergeset lock。** parentの確定escrowに加え、新規資金/継続changeを同mergesetでも保護する。未成立contractを根拠に他人の既存outpointをlockできるようにしない。raw carrierによる暫定保護を使う場合は、そのcarrier自身が作る該当outputに限定する。新しい資金はparentで検証可能になるまで支出を拒否する保守的な方法も、元期限内で処理できるかを検証する。
4. **停止Minerに依存しない適格支出。** 元payerの契約同意と確定済み履行・fee receipt・精算規則から、その実transactionのescrow input、支払先/額、burn、残高/changeをnativeに検証する。将来のpayer署名を待つだけの経路では第三者relay要件を満たさない。通常inputの署名を省略する汎用flagは設けない。特殊な支出認可を実装するなら、唯一の実escrow outpoint・確定contract・同じnative stateに束縛された私有の認可結果だけを使い、通常inputや別contractへ流用しない。有効化前はこの認可経路を一切受理しない。
5. **保存則と一回性。** escrow inputの実値を、契約で許可した実payee出力・実burn・実返還・新しい実change出力へchecked arithmeticで割り当てる。同じactual feeを複数operationへ全額ずつ償還しない。relay自身が払った通常feeと、escrowが負担する承認済み費用を分け、callerの過大なfee選択で予約を抜き取れない上限を契約で検査する。数値未決なら受付拒否を維持する。
6. **同じ原子遷移。** 新規funding・支払・changeのUTXO結果と、original/作業receipt/一回限りの精算状態を同じaccepted transaction集合から更新する。契約のescrow作用についてUTXOだけ成功・stateだけ成功の結果を許さず、二重支出、受理順変更、再起動、reorg/undo/replay、pruned IBDで再現する。未成立contractのordinary payer fallback outputは、その契約のfunded escrow成功に数えない。mempool、block構築、mergeset検証で別々の許可条件にしない。

このadapterの原資検査だけができても、全合法分岐・役割別同意・依存horizon・容量予約・実供給・金融/保持効果が閉じなければ、originalの完全受付とは認定しない。

##### 7.1 同じcarrierの実feeの一回割当

新adapterは `F = Σ実input UTXO額 − Σ実output額` をchecked arithmeticで一度だけ求める。専用escrowの新output額はoutput側に既に含まれるため、同じ額をfee原資にも加えない。旧bond burn・旧rent burn・新escrowの実burn/承認feeを同carrierで併用する場合、それぞれが単独で`F ≥ 自分の額`を満たすだけでは不足する。すべての割当の合計を`F`と照合し、miner collectibleから差し引く実burn総額、実支出、rooted escrow debitを一致させる。`palw_bond_burn_paid`等の既存単独検査を新複合会計の保存則に流用しない。

#### 8. Aの隔離実装の第一段階

新しい`TFO1` transportは旧`TFP1`のliteral bytesを保持し、新payer consentを別署名domainで束縛する。全wire幅は旧TFP1のouter ceiling以内とし、新metadataを理由に上限を増やさない。署名は元claim/job/execution/source、canonical相対graphと各profileの識別子、受付interval、必要material horizon、実資金output index/amount、payerと返還鍵、nonceを束縛する。最終carrier txidを自己参照で署名せず、実transactionからoutpointを取得する。

実出力との照合は既存version0の厳密なpayer P2PKH-MLDSA87 fallbackだけを認める。未知script version、OP_TRUE、別出力、異なる額/返還鍵を準備成功としない。payer署名とnative expected scopeを検査したopaque preparationも、UTXO検証済み・lock済み・全graph導出済み・actor同意済み・funded original受付済みではない。expected scopeのDTOをpeer入力から作るだけではnative導出にならない。

**2026-10-22 native回帰での実装修正:** `payer_key_id`（既存unkeyed `mldsa87_key_id`）とnative P2PKH address payload（domain付き `blake2b_512_address_payload`）は異なる。前者を出力先と同一視すると、同じpayer鍵でもfallbackを支出できない。鍵ID・wire field・署名domainは変更せず、実payer公開鍵から鍵IDとnative addressを別々に導出し、署名済み`refund_payload`と実funding outputを後者へ一致させる。他者の返還先、旧unkeyed payloadの出力はpayer自身が署名しても認めない。これは上記payer回収可能性を満たすbugfixであり、別返還先や新金融方式の採択ではない。過去のhelper成功だけでは実script支出を検査できていなかった。失敗と修正の検証状況は[保存記録](../evidence/palw/g14-completion-2026-10-22/material-and-native-candidate.md)を参照する。

同carrierのpopulated input/outputからchecked differenceを求め、一つのfee budget上でallocation identityの重複と合計超過を拒否する。これは実UTXO viewの供給と通常script検証、nativeでのallocation適格性、burn/collectibleの更新を代替しない。個別helperを呼び直して別budgetを作る経路で保存則を迂回させないことが、将来coordinatorの責務として残る。

accepted transactionのobject extractionでは、既存history/TIR fenceとは独立してTFO1を拒否し、内包TFP1を旧originalとして受理しない。通常carrierのUTXO/feeは残り得るが、rooted escrow残高を作らない。新rooted row、同mergeset lock、停止Minerに依存しない適格支出、全分岐の原子的受付・精算は未接続である。第一段階の署名・codec・実出力検査を、Aの実escrow確保やG14完了とは扱わない。

#### 9. Native feeとsource graphの接続 — 2026-10-22

既存carrierでも、実fee `F`に対するbond burn `B`とrent `R`を独立に`F>=B`・`F>=R`と判定すると、同じfeeを二重に充当できる。native accepted transactionのraw feeはそのまま保持し、同じ検証済みcarrierとburn obligationからrent利用可能額`F-B`を別に導出して価格検査へ渡す。finality分類も実collectibleの範囲内とし、burnされた額をcollectibleとして分類せず、他carrierの通常feeも誤ってfinality枠へ分類しない。既存の金額・価格・fence条件を変更するものではない。

source資料のv3候補は全7query familyから導出するopaqueなSourceMaterialOnly graphである。相対logical IDと実受付allocationに束縛したpacket IDを分離し、source全packet集合・辺・元時計の内側の予約を検査する。短bodyの未使用tail、他signerが供給した認証済み資料を使う応答、FullIds共有を維持する。semantic・数値裁定・parameter・役割別同意・資本・実費まで閉じた全original graphではなく、旧v2のcodecやrooted schemaも変更しない。

実escrow支出の次段階では、ordinary UTXO受付より前に、同じselected-chain state/accepting pointに束縛した決定論的なUTXO/金融state planを検査しなければならない。後段のPALW object dropへ会計を付加するだけでは、実支出とstate更新が分離する。認可結果は同じ実input entries・全transaction bytes・contract/pointへ束縛し、txidだけで署名検査を省略するcacheに流用しない。

[実装範囲・検証記録・残りの接続](../evidence/palw/g14-completion-2026-10-22/native-fee-and-source-graph.md)を参照。これらを実escrowの確保、公開有料入口、G14全体の完了とは扱わない。


##### 9.1 同branchの事前plan・commit・復旧の接続条件

実装調査では `consensus/src/pipeline/virtual_processor/processor.rs` の既存 `PalwUtxoCommitV2` / `commit_utxo_state` が、rooted PALW delta/rootとblockのUTXO diff・multiset・AcceptanceData・statusを同じWriteBatchで保存する。独立したfunded storeへ残高だけを先書きせず、将来のescrow rowも同じrooted deltaのold/newへ含める。virtual tipのcacheを先に書いて原資を認証したことにしない。

以下の事前認可表は実escrow inputを消費する経路を対象とする。新規funding carrierの通常受付との区別は後述する。

| 段階 | 必要な拘束 | 不一致時 |
| --- | --- | --- |
| branch準備 | retained PALW stateとcomposed selected-parent UTXO viewを同じbranchから解決。root・parent block・実accepting pointをplanへ拘束 | 別tip/cacheの認可を流用せず拒否 |
| UTXO受付前 | 実accepted順で、全transaction bytes・実input entries・出力・fee・契約・一回限りの適格履行を検査。成功候補だけをtransaction-local scratch planへ置く | txのUTXO diffへ加える前に拒否 |
| UTXO検証 | 唯一の該当escrow inputだけに限定した認可と通常inputの既存検査を両立。selected-parent script省略経路も金融条件を省略しない | payload上の成功やobjectの後段dropで代替しない |
| PALW final fold | 事前planの全消費と最終rooted old/new、実outpoint変更の完全一致を確認 | 未消費plan・別tx再利用・異なる金融deltaを拒否 |
| block commit | 同じWriteBatchへPALW delta/rootとUTXO結果を保存 | 片側のみの成功を公開しない |
| undo/replay | retained PALW deltaのreverse/forward検査が成功してから対応するUTXO diffを進める。残高・current outpoint・once状態・署名scopeを両方向で復元 | 古い状態や破損deltaに対して進行しない |

`calculate_utxo_state_relatively` の既存復旧はretained branchから再導出する。tip cacheの更新とflat virtual UTXOの更新はblock journalの原子的保存とは別段階なので、再起動時もblock単位の同一deltaを基準とする。template・mempool・mergeset・selected-parent shortcut・IBDも同じ支出契約へ接続する必要がある。

これは実装接続の条件であり、現時点で支出認可・rooted escrow・原子的original受付が稼働した記録ではない。private preflightがexact transactionを束縛しても、caller提供のscope/profileをnative全graph由来の認可へ昇格させない。将来のqualified authorizationは専用inputに限定し、通常の署名cacheへ認可成功を格納しない。


新規funding originalと実escrow支出を区別する。新規funding carrierの通常署名・UTXO/fee検証を先に通し、その後のpaired FP checkpoint内で重い全graph検査を行うことは可能である。不成立ならFP/予約/frame/funding rowをまとめて戻すが、通常carrierとpayer fallback outputは残せる。同mergesetの暫定lockは、そのraw carrier自身が作る実outputだけに限定する。他人の既存outpointを占有しない。これを実escrow支出へ広げてはならず、後者にはUTXO diffへ追加する前の適格認可が必要である。

既存の書込前seamは`apply_object_with_pair_v1`のpaired checkpoint内、`prepare_paired_original_v1`の後・最初の`reserve_for_claim`の前である。新coordinatorは既存private `prepare_original_v1`のframe/source導出を共有し、`original_plan_v1`と実native extrasのrules/court/demand、同時点のbuilder stateの現在署名権限を使う。parent snapshotだけで同block内の先行object更新を無視しない。全検査後、`install_original_v1`と同checkpointでfundingを更新する。これらの接続は現時点では未実装である。

##### 9.2 数値geometryと実native policyの照合

source identityはheaderの同一性を示すが、court/demandの採択を示さない。全branch coordinatorでは、実native params・受付点・stateからallocation rulesを解決し、`check_tir_public_allocation_native_v1`のexpected/carried完全一致検査を通した同じcourtを数値geometryへ渡す。frameのrules IDだけから別のcaller courtを正当化しない。demandも元の登録DAAとnative fence履歴から解決し、class recordやsource digestに保持されていない規則をあるものと扱わない。

geometryの全path列挙とsource graphの組合せは、同じsourceへの拘束を強める隔離部品である。署名済みroot値・役割別の提出/開示/応答・parameter/prompt取得・終了効果・実費/原資の全閉包を含まない。RFC-0030 §3の終了理由と完成不整合を費用と効果へ接続する必要があり、既存archive試験の件数を合法終了grammarの数に読み替えない。

---

<a id="source-v1-0030-L00001"></a>

### 旧RFC-0030 / 原文1–170行

出典: [0030-court-end-effects-and-pending-liability-archive.md](../rfc/0030-court-end-effects-and-pending-liability-archive.md)。配置理由: 主題別統合。

### RFC-0030 — court終了効果と未解決責任の保存

Status: **Draft / Proposed / Review Required** (2026-10-21)。利用者はAの詳細仕様化・隔離実装を指示。本番採択・全条件の実装済み・有効化を意味しない。

正本は[RFC-0026 §4.3](../rfc/0026-court-policy-master-contract.md)、[RFC-0014](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC-0023](../rfc/0023-unresolved-claim-liability-and-independent-progress.md)。[ADR-0197](../adr/0197-court-end-effects-and-pending-liability-archive.md)で新候補の方式を比較する。問題調査のsource基準は隔離候補`c54ba38fd`、§8の隔離実装の親は`494361118`。既存の署名責任・料金・wireを変更する採択ではない。

#### 1. 問題と実コードの境界

現候補は、未解決serviceの通常court deadline/close組立sweepを保留する。しかし、`objects.rs::CourtClosed`、`convict_close_declarer_v1`、`void_claim`の中立close、`retire_claim`等は`TransitionBuilder::write_court(id, None)`へ集約する。このwriterは、court/sessionの索引とchallenger exposureを外し、残存close groupのdepositを`slash_bond`で徴収し、groupとdissectionを削除する。客観的な反証が受理されても、別の未完groupまで「未提出だった」という理由で同じ処理へ入る。自動期限の保留だけでF02/F08は閉じない。

**現depositは前払escrowではない。** `palw_close_assembly_deposit_v1`の説明と実装では、declaration時に送金せず、宣言者のbond outpointと額を記録し、後で残存collateralから徴収する。`forfeit_close_deposit_v1`は`slash_bond`を呼び、その徴収は残存collateralにclampされる。「保存したdeposit額」が、その額の資金を別勘定に確保したことにはならない。終了後も資金が必ず残ると仮定せず、他claimの最大重複責任・exit・各経済入口へ接続しなければならない。新Account Bondの実装や最低10万による十分性を借りない。

`offence_attribution_active`時の`void_claim`の「中立に他courtを終了する」経路にも共通writerのgroup徴収がある。fence下のvoidが常に全sessionを即削除するという意味ではない。TIRの直接/分割fraud裁定自体を止めてこの相互作用を避けない。既存streamed-fraud試験は、通常courtの残存groupを伴う終了全体の証明ではない。

#### 2. 終了効果を分離するfield

候補は次の効果を別々に表す。単一の`closed`/`success`/`resolved`で代用しない。

| field | 意味と禁止する推論 |
| --- | --- |
| `claim_credit` | claimの非credit/void、または別の認証条件を満たすまで保留。courtの棄却だけで`SecurityCertified`、有効Final、一致したlocal replayによる合意creditを生成しない |
| `court_adjudication` | 当該争点の裁定結果。別の宣言・供給・支払義務の履行を意味しない |
| `active_session` | active slotを終了するか、hard内で復旧するか、hard後の未解決保存へ移すか |
| `established_liability` | 既存の検証済み証拠により当事者・対象・理由・額が確定した責任。offence markerで二重徴収しない |
| `pending_liability` | 未決のgroup、署名者義務、保持・費用の責任。無応答・quota・時刻だけで有責/無責へ変えない |
| `collateral_encumbrance` | 役割別・他claimを含む実資本の拘束。archived額を残高と数えず、同じ担保を複数回使わない |
| `material` | 本物の終了証拠、未完prefix、元契約・署名・phaseの保持先。部分prefixを完成証拠全体と呼ばない |
| `shared_frontier` | 確定済み知識と未処理義務を区別して保持。同じrootの別の合法争点を消さない |
| `pruning` | 全未決責任の解消、採択した保持期間、他claimのholdを検査して初めて許可。terminal phaseだけでは許可しない |

#### 3. 終了理由ごとの効果表

以下は新service ruleset用の候補で、旧claimの経済規則を後付けで変えない。検証済みの客観的事実と、供給/配送/収録前提の下でしか確定できない欠席を区別する。

| reason | claim credit | court/復旧 | 確定責任・担保 | 未決責任・証拠 | frontier/pruning |
| --- | --- | --- | --- | --- | --- |
| 計算不正を客観的証拠で確定 | 当該claimは非credit/void。計算不正を別の署名者へ自動帰責しない | 当該裁定を成立させ、不要になったactive sessionを終了 | 対象となるproducer等の既存確定責任を一度だけ適用 | 他方の未完groupは自動defaultせず、元契約とともにpending保存。false-Validは別の適格証拠が要る | 確定反証と未決義務を別に保持。未決を含む全条件までpruning不可 |
| 合法な申立てを証拠で棄却 | claimの計算正当性・合意creditは未認証のままなら保留 | 当該争点を終了。他の合法争点を妨げない | 当該rulesetで確定するchallenger負担だけ適用。不正入力を受理して無関係な鍵へ課金しない | 別groupや依存義務の解消を推定しない | 元の共有知識・別争点を保持。判決だけで全資料を消さない |
| 開示・応答義務違反 | 計算不正とは別のdefault reason。適格性がない現候補では非credit保留 | 実供給、元の署名者義務、期限、配送/収録前提を全て判定した場合のみ違反終了。それ以外は復旧/未決 | quotaや不在だけでslashしない。将来の採択済みdefault validatorの結果が必要 | pending義務・資料を保持。boolや呼出者申告を許可証にしない | hard外へ時計をresetしない。pruningは全解消後 |
| Panelの放棄 | producerの計算不正・署名者のfalse-Validとは推定しない。非credit保留 | 予約済み合法graph内の復旧。未予約の代替Panelや署名を捏造しない | 客観的に確定した責任以外はpending | 放棄時点の契約・既存応答と残義務を保持 | stage1の共有frontierを保持。stage2独立進行を既実装としない |
| service前提の崩壊 | 非credit保留 | hard内なら合意状態から有料・有界に再割当。hard後は明示的復旧契約待ち | 機会を与えられなかった者へ無応答slashをしない | 未決担保・証拠を保持。供給不足を無責任な自動返還へ変えない | 障害中の`L_class`内前進や無条件の返還期限を保証しない |
| hard deadline到達 | 有効Final、自動無罪、計算不正へ変えない | activeな旧時計を延長せず、未決状態を保存。新復旧は別の採択済み有料契約 | 時刻だけではslash/返還しない。確定済み責任は一度だけ保持/精算 | 新しい外部配送義務を旧署名へ後付けしない。state内保持と提供者の約束を区別 | 全未決を残し、必要な保持とpruning条件を明示 |
| 認証済みの完成assemblyが元の宣言と不整合 | 計算不正の証拠と同一視しない | 完成bytes/宣言scope/rulesetに対する客観的不整合だけを裁く | 当該宣言者の適格な確定責任だけ適用 | 他方の未完groupは別pending。完成していないことを不整合の証拠にしない | 元の完成bytesと不整合箇所を保持。再徴収不可 |

#### 4. 推奨A:裁定とpending保存を原子的に分離する

別のversioned namespace `court-end-archive/v1`を候補とする。§8の隔離実装では末尾key 20 / row 26を使用するが、公開schemaの採択ではない。旧key、row、tag230、X3 container、profile identityを読み替えない。

1. 通常native verifierが証拠を検証し、実際のaccepting block/DAAと当事者/claim/original/rulesetを確定する。呼出者が渡した`guilty`や`can_default`だけでarchiveや徴収を作らない。
2. active sessionを削除する前に、元session、embedded phaseと別tableのTIR phase、両sideの残存groupと受理済みchunks、宣言者/outpoint/deposit、署名済み元契約、確定した終了証拠の実bytesまたは同じ保持保証のある認証済み参照を捕捉する。digestだけでは資料の取得可能性を満たさない。
3. 別々の`adjudication`と`pending obligations`を保存し、確定責任のmarker/徴収、archive作成、必要な資本holdの移行、active索引/slot解除を同一checkpointで適用する。archiveや資本の確保が不成立なら全体rollback。ただし受付後に正しいfraudを容量不足で止めないため、次節のoriginal事前予約が必須。
4. 未決の他方groupを保存しただけでdepositをburn/返還したり、amountをescrowと呼んだりしない。既に適格証拠で確定した同一sideの責任は、pendingに戻して二重処理しない。
5. 終了記録は再開可能なactive courtではない。同じsession/side/revisionを上書き・再利用して未決を消さない。新しいsession IDの大量作成や「終了して枠が空いた」ことによるarchive無限増殖を、originalで予約した全合法graphと消費済みidentityで防ぐ。
6. snapshot/restart/pruned IBDは、元claim/frame/service binding、終了point、phase/chunk/署名のscope、確定/未決の排他性、全金融holdと索引を再構築する。undoは元active状態と資本へ、replayは同じarchive/rootへ戻す。受理済みblock全体の証拠bytesを参照するなら、その保存/取得保証もprofileに含める。
7. pruning/closeを、裁定終了・金融精算・外部提供期限・state資料保持の別条件で検査する。全条件を満たすまでarchive削除やbond exitをしない。既存署名の約束より長い外部提供/課金を遡及導入しない。

以上は目標契約である。§8の部分実装はarchive rowと名目exposure/exit holdを接続するが、完全な金融予約移行、public coordinator、解除validatorは未実装である。

#### 5. original受付と資本・容量の条件

Aは「後で保管先を探す」案ではない。新rulesetのoriginal受付で、許容する全敵対的分岐、各Court/side/phase、完了・棄却・中立終了・供給不足・hard後未決、重複統合/並行性を含め、activeとarchiveが重なる原子遷移の最大bytes/work/IO/memory/byte-DAA、署名検証、journal、root計算、snapshot/reorgを積算する。

各役割の責任額・最終保持条件・資本holdを新しい署名契約と同じprofileに束縛し、既存のclaim予約資本・fee立替・他claim責任・新Account Bond案の責任を混同しない。各金融入口で同じ資本を再利用できないことが要る。未校正の費用・期間・branch数を0にせず、original受付/武装を拒否する。signature期限が足りなければ新しい適格契約が必要で、旧pledgeの上書きでは補わない。

**保持期間の未被覆を隠さない。** service/復旧が永久に成立しない条件では、hard=10,000からarchiveの有限な解放時刻や有限のbyte-DAA総量は導けない。有限の上限を使うprofileは、その成立に必要な配送・収録・復旧の前提と、前提崩壊時の保持/資金/全体受付制限を別に示す。archiveの未決予約枠をactive session終了だけで再利用し、未決行を無限に積み上げない。保持が無期限になり得る条件を有限額で全費用支払済みと扱わず、未被覆のままなら新original受付/武装を拒否する。10,000を実測の返還時刻や自動pruning時刻へ読み替えない。

ローカルのmemory/IO不足と、合意上の有料容量上限違反も区別する。validな既受理証拠を、あるnodeのローカル資源不足だけで不正と裁定・absence slashする規則にしない。元profileの費用とnative resource policyが未校正なら、その条件を未被覆として残す。

この完了前のprivate adapterは、完全な有料受付やG14を達成したものとしない。上限引上げ、モデルの除外、二階層の再審、禁止した計算証明、新fee/返還期限の仮採択で成立させない。

#### 6. 比較案と採択境界

Bはdeclaration時の実費前払と違反罰を別料金にする案。実在する支払/償還財源、全対話費用、署名者の同意、保存費用・未決金融責任、二重課金/rollbackを新規に設計する。既存のdepositを単に「費用だった」と呼んで常時徴収する変更ではない。値は未決で、このRFCは設定しない。

A/Bとも新ruleset/profileの候補で、旧claimの経済規則を変更しない。詳細仕様化・隔離実装の判断、本番採択、pre統合、有効化、配備を別々に記録する。採択まで現行候補の拒否gateを維持する。

#### 7. 検証条件（全条件の合格は未達）

以下は既存T01–T15を置換しないF08の追加条件である。

- E01: 全6終了理由と完成不整合のfield別効果。客観的fraudが成立しても無関係な未完groupへabsence slashしない。
- E02: 両side、embedded/別TIR phase、最大合法chunk prefix、完成証拠全体を実native verifier/有料受付からarchiveへ移す。
- E03: 判決/徴収/hold/active解除/archiveの各失敗点が全体rollback。完成sideの責任・他sideの未決・offence markerを二重適用しない。
- E04: 同一originalの全合法branch・同root重複・再開/再投稿・新session IDによるarchive枠の無限生成を防ぎ、正直な外部者の枠を先着者へ渡さない。
- E05: producer/challenger/宣言者/Panel/custodian/relayを区別したexitと他claim重複資本。未知額・短い署名・欠けた資金は拒否。
- E06: hard前後、notice境界、最後の適格応答とsweep順、配送/収録前提崩壊、既存契約の期限超過を区別する。
- E07: reorg/restart/pruned IBDから同じ金融・資料・索引/rootへ復元。改竄/欠落/孤立/archive消去・再利用を拒否。
- E08: 元complete証拠が取得不能なのにdigest/部分prefixだけで保存済みとしない。全保持/pruning条件とtyped heap/wire preflightを同時検査。
- E09: serviceなしの旧claimは既存のW10/ADR-0152等に従い、t12 identity/preset/codecをrepinしない。
- E10: 同じrelease buildでT01–T15・実モデルdoor≤3,120・完全有料graphの条件を満たすまでG14完了にしない。

#### 8. A案の隔離実装 — 2026-10-21

親`494361118`から、service bindingのあるcourt削除にprivate archive adapterを加える。新key 20 / row 26を既存enumの末尾へ追加し、`court-end-archive/v1`と元service profileからarchive identityを導く。既存ordinal、X3 key 11 / row 17、service key 12 / row 18、tag230、preset/identity pinは変更しない。このidentityは料金・容量が校正されたprofileの認証ではない。

共通`write_court(None)`は、元session・embedded/別table phase・両sideの残存groupと受理済みchunkを捕捉してから、active索引とphase/groupを除きarchiveを作る。全体をcheckpointで戻せる。正常なnative `CourtClosed`は実objectと受理pointを、直接/分割TIR convictionは最後のfragmentだけでなく検証された全decoded evidenceとpointを保持する。元宣言の全raw署名は旧group rowに保存されておらず、このadapterが署名契約全体の保存を達成したとはしない。

完成した不整合assemblyでは、最終chunkも原子的に保存してから、当該sideの既存徴収を一度適用する。`applied_deposit_debits`は残存collateralでclampされた**実際のdeposit徴収額**であり、名目額の全額確保や他の既存罰の総額ではない。反対sideの未完groupを同時に徴収しない。正常に完成してnative closeを通ったgroupもraw bytesを残し、`fulfilled_groups`で未決depositから区別する。chunk全体のhashと宣言scopeを検査し、typed objectの再encodeが同じbytesになることを新しい受理条件にはしない。旧readerが許す表現を勝手に狭めない。

終了causeをまだ捕捉していない内部経路は`Uncaptured`を明示し、未知の判決・受理pointを補わない。この場合と、当該courtの裁定まで確定しない関連native causeでは、元claimの実reserved court stakeを名目holdへ残す。archiveのread-only dispositionはdefault・credit・解除の許可証ではない。snapshot検査もnative adjudicatorの再実行全体を代替しない。

archiveの未決group名目額を既存accuser exposureへ加え、該当bondのexitを止める。これは同額のescrow口座を新設・入金したことでも、全金融入口の資本保存則を満たしたことでもない。元claim/frame/service bindingと最低material期限をcontext検査し、旧hard/material TTLを過ぎても保持する。未決archiveの上書き・削除・型置換、同じsession IDの再利用を拒否する。料金、署名済み期限、hard、上限値を変更しない。

追加5試験は、小型のverified-half native fixtureで、実際のlocal legacy v2 adjudicatorによるCourtClosed、完成不整合chunk、正常完成groupのraw保存、Uncapturedのstake保持、分割TIR反証の全証拠保存を検査する。nodeはv5でheld context/各fork fence等も検査しており、この小型fixtureがその公開受付を通ったとはしない。将来のarchive resolverも元の適用規則を束縛し、v2への降格で置換しない。各経路でcarriage復元、typed heap/wire reader、期限後保持、元rootへの全undoとreplayを確認する。fixtureのservice cost・carrier署名・元declaration設置は実資金original受付の証拠ではなく、E01–E10全体や最大合法phase群の成功を主張しない。

**次の未完:** 終了後の反対sideが、元宣言に束縛された完成bytesを提示し未決を解消するvalidator/経路が必要。現adapterではactive sessionがないため旧CourtCloseChunkへ出せず、holdが続く。元rulesetのCourtParams/fee/署名scopeを取得できなければ、現在のdefault paramsや仮値で再裁定しない。全金融入口、全合法分岐の予約、実署名の保存、資料提供と保持費用、qualified settlement/pruning、実供給・公開ingress、最大family/phase、T01–T15と実doorは未完。無期限保持を完成と呼ばず、未校正の保持期間/費用を0や10,000で置き換えない。


| 条件 | 現在の検証範囲 | 残る条件 |
| --- | --- | --- |
| E01 | direct close、完成不整合、streamed TIR fraudの小型native fixture | 全6理由と全actorのfield別効果 |
| E02 | 両groupと完成raw bytes、full decoded TIR evidence | 実署名・実資金受付、最大prefix、embedded/別TIR phase全family |
| E03 | archive改変/削除/再利用拒否、重複chunkのrollback | 各書込・金融移行点の資源失敗を注入した原子性 |
| E04 | 同じsession IDの再利用拒否 | 全合法branch/並行性/重複統合の予約、無限archive生成対策 |
| E05 | pending nominalのexposureとexit hold、実deposit debit | 全役割/他claim/全金融入口と資金保存則 |
| E06 | hard/material後の保持とTTL sweep | notice/最終応答/配送収録/元契約境界、適格な解除 |
| E07 | 小型fixtureのcarriage復元・全undo/replay/root一致 | 実DB restart/pruned IBDのarchive経路、各孤立・欠損の否定試験 |
| E08 | 実raw chunk/全decoded evidence、typed heapとwire scanner | 完全な元署名契約、全原資/最大資源/保持・取得保証 |
| E09 | 既存ordinary courtとt12同一性の回帰対象 | 公開service受付導入時にも旧claim分離を検証 |
| E10 | 有効化拒否を維持 | T01–T15、実door、全有料graphの元の完了条件 |

「全family/全合法分岐」は当面有効な経路の範囲であり、延期中のRFC-0003画像・動画とRFC-0004モデル改善を有効化・迂回開放する意味ではない。


#### 9. 宣言時の署名・native裁定入力の保存候補

親`e2e367e98`。終了後の未決解消validatorに先立ち、新しいprivate `CourtCloseReceipt`（末尾key 21 / row 27）でnative受付時の契約資料を保持する。archive key 20 / row 26を含む既存ordinalは変更しない。Aの隔離実装の続きであり、公開schemaや終了後受付の本番採択ではない。

serviceに束縛された`CourtCloseDeclared`が通常のnative署名検査を通る経路で、完全な署名済みobject、宣言者の当時の公開鍵、実point、元session、embedded/別table phase、original/service scope、最低material horizonを保存する。receiptは署名のdomainを既存のdeclaration contextで再確認し、元groupのcount・各digest・whole digest・verdict・時刻・当事者と一致させる。active courtとarchiveのいずれか一方に対応するgroupが必要で、別のnative処理や任意row置換で過去のreceiptを捏造しない。receiptは前向き上書き・削除・retypeを許さず、service未解決の間はTTL後も保持する。

native processorは、実際のv5呼出しが読むCourtParams、leaf ladder、prompt形、held regime、offence-attribution、FP decode/cell closure/memory routingのfork境界と、TIR/各proof-familyの有効条件を、その受理pointとnetwork domainからextrasへ渡す。carrierの自己申告を使わない。元の証拠を異なる条件で後から裁定しないための履歴入力であり、この記録だけで新しい配送義務・無応答default・遡及罰・返還を発生させない。旧nodeの「後日のblockで証拠を検査する」規則を新しい固定条件へ置換する採択でもない。

native入力がないfixture/内部経路は`rules=None`を明示し、current/default paramsへfallbackしない。旧groupに署名や条件を後付けし、完全なreceiptとして扱う経路は作らない。条件の保存と、その条件を使う権限ある終了後validator、実費予約・決済、pruningは別の段階である。元宣言資料の保存に加え、将来のresolverでphase・class/claim binding・native feature gate・v5裁定・全金融効果を一体で検査する必要がある。

追加の署名再検査、署名/公開鍵/session/phaseのcloneとserialized保存、root/journal、snapshot/reorg、保持byte-DAAは全分岐profileの追加費用で、0としない。料金・上限・署名期限・preset/identity pinを変更せず、全original予約/公開ingress・E01–E10全条件・T01–T15・実doorが閉じるまで有効化保留を維持する。

署名が認証するのは既存declarationのfieldであり、保存したnative rulesは合意上の受理履歴である。宣言者が料金・fork条件・終了後の新しい外部義務へ追加署名したと見なさない。gen/improvement等の条件を記録しても、延期中の機能を有効化する効果はない。


#### 10. 保存した入力によるprivate v5再検査候補

親`c153ecc02`。`verify_archived_close_v1`は、元archiveの未決side、署名済みreceiptとnative入力が存在する場合だけ完成groupを検査する。count、各digest、whole digest、宣言者、outpoint、verdict、deposit、元の期限を変更できず、既に受理したchunk prefixを消去・差替えできない。完成bitmap/全index・各chunkの既存サイズ上限を確認する。native入力があるreceiptには、既存nodeと同じ宣言chunk数の上限検査も追加する。上限の変更ではない。

完全なraw bytesを署名済みhashと照合し、native wire readerでheap shapeを調べてからdecodeする。typed objectの再encode一致を新しい条件にはしない。元のscopeとverdict、TIR/各familyの有効条件を検査し、宣言時のsessionとembedded/別table phaseを一時viewへ戻して、保存した全入力で**既存のadjudicate_court_close_v5**を呼ぶ。v2へfallbackせず、現在のextrasで欠けた入力を補わない。返った裁定が宣言と異なる場合は拒否する。

一時viewはlocal inspection用で、active/archiveが共存し得る検査scratchである。新しい合意状態としてcommit・journal化せず、元state、担保、claim credit、service/frontier、責任の未決/確定状態を変更しない。結果はprivate constructorだけを持つ非Borsh・非Cloneの型で、元state root、session/side、raw group、decoded close、実v5結果を束縛する。将来の金融writerはstate rootの再確認に加え、独立した有料契約・実費/原資・一度だけの終了効果を検査しなければならない。この型や一致したlocal replayだけでSecurityCertified・credit・default・解除を許可しない。

メッセージ/組立buffer・group/session/phase cloneとBTree membership、decoded proof、全stateの検査用copyを事前quoteし、native trackingが有効な場合は既存のconstruction ownerへadmitする。実readerのCPU予算超過・shape算術/メモリ不足は`NativeResource`として伝播し、証拠不正やabsence slashへ変えない。syntax/truncationは構造上の拒否である。RSS/allocator/全familyの物理上限や実CPU費用が校正されたという主張ではなく、全有料profileへの費用追加が必要である。

これは宣言時の条件による**privateな履歴再検査**であり、旧公開nodeが後日の検証blockの条件を読む規則を置換する採択ではない。終了後の公開受付で適用する時点/規則、遡及しない署名責任、精算・pruning・資本経路は未決のまま残す。wire、schema ordinal、cap、fee、署名期限、preset/identityを変えず、武装/統合/配備を行わない。


#### 11. 借用recordを使う共通v5 kernel

親`54c0081d6`。§10の初版で検査用に全stateをcopyした部分を、session/別table phaseの借用へ置換する。通常の公開`adjudicate_court_close_v5`入口は既存のcost gateとsession/claim解決を維持する。private借用入口も同じcost gateを先に実行し、session/phaseのscopeとclaimを確認する。両入口は一つのv5裁定本体へ進み、TIR/Gen/Eval dissectionの3箇所だけ、phaseの取得先を引数へ置き換える。embedded attention phaseは元sessionから読む。延期中のfamilyを有効化する変更ではない。

archive adapterは、署名済みRooted receiptを認証した後にそのsession/phaseを借用し、全state copy・一時active行・session/phase cloneを作らない。宣言raw bytes、受理済みprefix、元のnative条件、Rootに束縛した結果と無金融効果の条件は維持する。借用入口自体はadmissionや署名責任の認証器ではなく、呼出元がRooted recordのscopeを検査する。callerの任意recordを金融権限へ変えない。

初版のv5本体と現共通kernelを、3箇所のphase lookupを戻してbyte単位で照合するsource auditを残す。これは全familyの実行試験や物理容量の代替ではない。group/メッセージ/組立/decodeの事前quoteは残し、Root hashing、署名/IO、内部のlegacy v5 allocationとCPUの全費用は依然として校正・全分岐予約へ含める必要がある。金融解除、公開late規則の適用時点、pruning、有料original全graphを採択したものではない。
#### 12. Private append of precommitted archive material

Parent `2fcbcac6a`. A private adapter accepts only a missing chunk whose index, size and digest match a retained declaration. It rechecks the rooted original declaration signature, retains every previously accepted byte, and adds the new bytes and presence bit through the existing journal writer. No new wire tag, schema field, public dispatcher, sender entitlement or deadline is introduced. Any future public relay path still needs funded admission and explicit delivery/inclusion rules.

Forward archive writes can extend only group material. Version, scope, original session/phase, service/allocation/source, evidence, court stake, deadlines, nominal deposits, applied debits and fulfilled status stay unchanged. Removal, retyping, changed contracts, deleted/replaced prefixes and duplicate chunk submission are refused. Loaded carriage rechecks all retained per-chunk digests and the existing archive context. The writer does not grant fulfilment merely because the bitmap becomes complete.

Even a complete group whose decoded object or declared verdict does not adjudicate is retained as signed material; storing it is not a new misconduct verdict or charge. The separate private v5 verifier continues to distinguish acceptable and unacceptable close evidence. Completion alone neither discharges nominal exposure nor changes collateral, claim credit, original clocks, service or signer obligations. In particular, late material retention is not a finding that an old response obligation was met on time, and does not choose declaration-time versus accepting-block public settlement rules.

The adapter admits the actual archive copy, Box/Arc envelopes, new chunk bytes and BTree membership, and declaration message buffers before allocating them when native construction tracking is active. The standard writer separately quotes map/journal/index work before mutation. Signature/hash CPU, repeated prefix-copy cost, retained byte-DAA, full branching and physical capacity remain costs to derive and calibrate. This adapter is not a streaming capacity claim or a funded public endpoint.

The existing eight tiny signed v5 archive cases exercise missing material append after the minimum material TTL, unchanged nominal exposure/collateral/claim, refusal of duplicate/invalid inputs and contract/hold changes, subsequent private v5 results, carriage restoration and complete undo/replay. Original funding and service remain fixture assumptions; public node ingress, all failure-injection points, qualified financial settlement/release/pruning, E01-E10/T01-T15 and real-model door remain incomplete.

---

<a id="source-v1-0034-L00001"></a>

### 旧RFC-0034 / 原文1–99行

出典: [0034-archived-close-qualified-financial-discharge.md](../rfc/0034-archived-close-qualified-financial-discharge.md)。配置理由: 主題別統合。

### RFC-0034 — archived closeの実受付と金融責任の適格解消

Status: **Proposed / Review Required** (2026-10-21)。[ADR-0201](../adr/0201-archived-close-qualified-financial-discharge.md)のS1を推奨する。詳細仕様化・隔離実装の判断待ち。本番採択、旧claim移行、料金・保持期間、有効化、pre統合・武装・配備を含まない。

正本は[RFC-0026](../rfc/0026-court-policy-master-contract.md)。[RFC-0030](../rfc/0030-court-end-effects-and-pending-liability-archive.md)の裁定と未決責任の分離、および[ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md)の実供給とno-slashを維持する。本RFCはRFC-0030 §12が保留した、終了後materialの実受付と義務履行の区別を具体化する。

#### 1. 問題と現在の根拠

現private `append_archived_close_chunk_v1`は、宣言済みdigestに一致する不足chunkを保存する。引数はsession/side/index/bytesであり、accepting blockのpointを受け取らず、各chunkの受理時刻も保存しない。`last_point`はブロック全体commit前には親pointであるため、これを完成時刻と見なせない。後日完成したbitmapや現在tipから、元assembly期限内の完成を逆算しない。

元期限は既に確定できる。署名済み`CourtCloseReceiptV1`は宣言object、宣言point、元session/phase/native v5入力を持つ。対応groupの`declared_daa`と宣言pointを照合し、既存`palw_close_assembly_daa_v1(count)`による`assembly_deadline_daa`、元session deadline、original hardを検査する。新しい返還期限や延長値は作らない。既存active chunk受付と同じく、assembly期限の等号を含む。

`verify_archived_close_v1`は、元署名と原契約・既受理prefix・全chunk/whole digestを照合し、保存native v5入力によって裁定する。返す非serialized tokenはstate rootに束縛されるが、現在は金融状態を変えない。妥当な計算裁定資料であることだけでは、元の時限義務を履行した証拠にはならない。

#### 2. S1の区別

| 根拠 | 効果 |
| --- | --- |
| 正しい全bytesが元assembly期限内の実native accepting pointで受理され、元署名/native v5に適合 | 当該sideの提出義務についてpositive fulfillmentを認証できる。未投稿txの配送・収録を仮定する必要はない |
| 元期限後に正しい全bytesを取得し、private v5で一致 | 裁定資料の妥当性だけを確認。自動履行・自動返還・absence違反にはしない |
| 期限までのmaterialが欠け、F02の実供給・元義務・配送/収録前提を全て確認 | 別の適格absence判定を要する。計算不正と混同しない |
| 供給不足、前提崩壊、受理時刻不明、署名/native入力欠落 | no-slash・資料/未決責任の保持。hard到達だけで返還・pruningしない |

positive fulfillmentは当該declarationに限る。claim credit、計算不正に基づく既確定徴収、別side、同bondの他claim、別のcourt stakeや提供/保持義務を消さない。別判決でclaimが既にvoidとなっていても正しい裁定を取り消さない。

#### 3. 実受付と権限の構成

1. 新originalの全合法graphで、終了後の不足material受付、署名・digest・v5、金融writer、journal/root、reorg、retentionまで予約する。既存depositを前払escrowと数えない。
2. 公開入口はnative accepting contextからpointを得る。callerが渡した`paid`、`on_time`、`fulfilled`、現在tip、私有cacheは根拠にしない。公開受付のdispatchと実fee決済が未接続なら入口を開かない。
3. 元declarationに束縛した不足chunkだけを受理する。継続prefixを削除・変更せず、既受理のduplicateを新完成として再使用しない。body完成を理由に新しいsession/clock/priorityを得ない。
4. 実native遷移内で、元assembly期限以下の完成point、original/allocation/source/profile/session/side、declaration digest、完成raw bytesのdigestを束縛する。rootedな受理記録、または同一原子遷移だけで消費できるprivate capabilityを用いる。保存済み旧appendから時刻記録を捏造しない。
5. 元署名、元native v5、完全group、同一state rootを再照合する。proof妥当性tokenとpositive completion tokenのどちらか一つでは金融writerを許可しない。途中でstateが変わったtokenは再検証する。
6. 当該sideのpending declarationだけを一度履行済みに移す。raw bytes、元契約、実受理point、根拠を残す。元court終了causeの証拠とは別記録とし、異なる妥当な裁定資料を一つのcauseへ上書きしない。
7. 金融marker/名目hold解消/資料保存を同一checkpointで適用する。失敗時はbytes追加も含めrollback。undo/replay・snapshot読込時にも同じ根拠を検査する。

新しいrooted記録を導入する場合、既存archive/receiptのBorsh意味を変更せず、別version/namespaceと末尾ordinalを使用する。具体ordinalは実装時に競合を確認して記録する。本RFCはordinalを予約しない。

#### 4. 金銭と保持

既存assembly depositは将来の名目徴収額である。positive fulfillmentによる当該名目責任の解消は、預かり金の送金や新規Panel収入ではない。実徴収済み額を戻さず、徴収・fee・払戻を仮残高で作らない。

公開material受付・native検証・署名・保存・伝送・金融/root更新には実費と実在原資が必要である。新originalの署名済み責任範囲と資本拘束、実native transactionの通常fee、profile内の作業料/retention原資を区別する。未採択の料金・期間・全分岐費用を0で埋めず、資本不足は受付時に拒否する。既受理の正しいfraudを、後から資本不足で妨げる実装にはしない。

一つのdeclaration履行後もarchive自体を即削除しない。全side、元stake、F02/提供義務、他claim責任、保持契約、金融once-only markerを確認するまではbond exit/pruningを許可しない。全解消後の公開pruning方式は別途完全な契約と検証を必要とする。有限の保持完了を元hardから仮定しない。

#### 5. 隔離実装範囲と受入条件

最初の実装は、上記positive fulfillmentを実際の私有遷移で一度適用し、名目pending/exit判定へ接続する。公開有料coordinatorが未完なら、その私有遷移の検証結果を公開受付完了と呼ばない。absence徴収、期限後completionの返還、旧資料の時刻補完は実装しない。

- 元assembly期限の直前・等号でのみpositive completionを認証し、直後を拒否する。元session/original hardの外へ延長しない。
- 元署名/native v5、scope/root/side/contract、prefix、全digest、実受理pointの改竄・欠落を拒否する。
- 期限不明の旧append、late valid body、未完成bitmap、cache一致だけではholdを解消しない。
- positive completionは追加された当該sideの名目pendingだけを解消する。既確定徴収・反対side・他claim・元stake・claim creditはそのまま。
- 二重適用、state変更後token、途中失敗を拒否し、complete undo/replayとcarriage復元で同じroot/金銭/holdとなる。
- 実原資、全合法graph、署名/IO/retention費用、公開native受付・実node試験、E01–E10/T01–T15が未完なら、G14完了とはしない。

実装・試験結果は未記録。数値・費用・測定の成功は本RFCによって追加されない。

#### 6. 採択非依存のprivate native event検査候補

`palw_court_archive_verify_v1.rs`の`inspect_archived_native_chunk_v1`は、既存native wrapperが捕捉するpoint/objectの組からだけ検査入力を得る。caller引数の時刻を許可せず、元group・署名receipt・不足chunk・元assembly/session/original hardの境界を検査し、state rootに束縛した非serialized/非Cloneのinspectionを返す。親tipを実受付pointに読み替えない。これは現在native eventの検査であり、block全体commit済みのreceiptや金融権限ではない。

公開dispatcherは引き続きactive groupを必要とし、終了後archiveへの公開受付は開かない。既存private appendも変更せず、資料の妥当性・fulfilled status・名目/実残高・exit・保持を変更しない。S1採択後の実原資・v5・once-only金融transitionへの接続は未実装である。既存の実署名8fixtureへ境界直前/等号/直後、native event欠落/別object、state-root変化、公開入口拒否、状態不変の検査を追加する候補であり、試験結果はまだ未記録。

#### 7. S1のprivate実装初稿

以下は§6の検査のみの段階から進めた初稿であり、試験結果は未記録。新key25 / row31に`CourtArchiveProgress(session, side)`を末尾追加し、既存archive/receiptのBorsh・署名・旧ordinalを変えない。

`ArchiveDeclarationProgressV1`は、元scope、実archive生成point、active時に受理された初期prefix mask、終了後のnative chunkごとの実point、完成時の実pointとnative close全体を保持する。初期rowのforward writerは、同checkpointでのactive session/group除去、全く同じ初期groupを含む新archive作成、実終了causeとの一致を要求する。保存receipt/native入力や終了pointが欠ける既存経路は新rowを作らず、未決holdを残す。過去のarchiveへ後から初期prefixの出所を付与しない。

`apply_native_archived_chunk_v1`は公開dispatchから呼ばれないprivate transactionである。既存native wrapperが捕捉するpoint/objectと元署名期限を検査し、必要なら完全候補を同一state rootのnative v5で裁定する。原子的な不足bytes保存と別rowの根拠付きpositive fulfillmentにより、当該declarationの名目pendingだけを一度解消する。旧archiveの終了cause/fulfilled集合/実徴収/stakeは変更せず、実残高の返還・支払・claim creditを作らない。失敗時はbytes・point・金融markerの全体をcheckpointへ戻す。

時刻なしprivate appendはnative資格maskを増やさない。従って、期限内の最後chunkを追加して全bytesが揃っても、以前のchunkの資格が不明なら責任を解消しない。native v5/署名の検査を通るだけでもこの不足は埋まらない。new row単独の削除・retype・根拠履歴の上書き・completedの取消をforward writerで拒否する。金融readerはstrict scope/全chunk資格と完成記録を確認し、forward writerとloadは元v5とraw bytesを再検証する。

既存8署名fixtureへのS1境界・二重適用・不正verdict・undo/replay/carriage・旧row遡及拒否と、3chunkのうち中間だけ時刻不明の場合/全数nativeの場合を比較する試験を追加した。元期限直後の拒否、反対sideのgroup/exposure、実担保不変も対象にする。別claimを含む全金融入口・全failure injection・公開node受付の網羅は未検証である。

これは公開有料original/relay受付、実原資/料金、全分岐容量、全責任の解消・pruningを完成させない。実装のnative heap/wire事前検査は費用の物理校正やfee充足の測定ではない。G14全体の完了条件は維持する。

#### 8. 次候補のresource分類と期限内の新たな実受理

§7初稿の次候補では、loaded verifierも`PalwStateV2Error`を保ち、実wire scannerのCPU/arithmetic・fallible assembly allocationの不足を`NativeResource`として返す。署名不適合・truncated/malformedな資料とは区別する。loaded pathは実署名messageのclone/serializerとassemblyを構築前に予約し、実streamのshapeから再decode heapを予約して検査完了までleaseを保持する。forward writerは既存native v5 verifierを再利用する。retained closeをcloneしたcapacityを再decodeの上限と見なさない。

既存declaration messageのhash vector cloneと`borsh::to_vec`の初期serializer容量は、固定256 byteのscratchでは覆えない。archive系のquoteを、実countのhash cloneと既存`encoded()`による実fieldのBorsh grow要求へ直す。message本文・domain・料金/容量上限/presetは変更しない。count 1/2/32・両side/両verdictで、quote用borrowed sliceと元Vec messageの実bytes一致、clone/serializerの実capacity充足を検査する。

**未資格の同値chunkを新たに期限内受理することと、過去への時刻補完を区別する。** 既にprivate保存した正しいbytesが提出を塞がないよう、元署名window内の本物のnative eventで同一bytesをもう一度届けた場合、その現在pointだけを新資格として記録できる。条件はexact bytes/digest一致、当該indexが初期native maskにも実point mapにも未登録、completed未成立である。bytesと元causeは変更しない。全chunk資格と同root native v5が揃って初めて当該declarationだけをonce解消する。

期限直後、偽bytes、既qualified indexの重複、caller申告時刻は拒否する。旧bytesを保存しただけでは依然として資格を追加しない。このprivate経路を公開有料受付へ接続したものとはしない。3chunk試験は、時刻不明の中間資料を残した時点ではholdし、その後の実期限内native再観測による解消を、bytes/cause/反対side/実残高不変とともに検査する候補である。次候補の試験結果は未記録である。

#### 2026-10-21 ユーザー選択 — S1の詳細仕様化・隔離実装

ユーザーは「S1：期限内の実履行だけ責任解消」を選択した。元の期限内に実際に受理・完成した正しい応答だけ当該sideの未決責任を一度解消し、期限後のproof-valid資料の妥当性と金融上の履行を分離する。上の未選択表記は選択前の履歴である。

この選択は詳細仕様化・隔離実装の承認であり、新料金・本番採択・pre統合・武装・配備を含まない。公開有料入口・実原資・同一rootのnative v5・全必要materialの時限資格・once-only処理・他責任保持の接続と試験は別途必要である。実装・試験の成功をこの選択だけから導かない。

#### 2026-10-21 隔離候補の検証checkpoint

S1のprivate native writer・別rooted provenance（key25/row31）・同一root/native v5・当該declaration名目責任の一回解消まで接続した。nativeの期限内再提出で既存同値bytesの現在pointを新規記録でき、過去時刻は遡及しない。wire/OOMのNativeResource分類、実serializer/decode容量の事前予約も修正した。caps・元deadline・旧wire/署名domainは変えていない。

関連84 unique selected pass（重複除外、archive filter11とnative frame42は7件重複）、core全tests型検査・非test consensus lib検査はexit0。これはprivate native fixtureであり、実原資・公開有料入口・実返還/徴収・全exit/pruning・別claim金融composition・実node全T01–T15・G14全体の完成ではない。[正確な範囲・raw log・source/binary hash](../evidence/palw/g14-completion-2026-10-21/s1-and-original-cover.md)。文書検査は既存の参照欠損によりFAILのまま。
