# ADR-v2-0046 — Court終了効果・未解決責任archive・金融解除

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 2本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

Courtが終了した後も残る義務を、資料・会計・期限のどの状態として保存するかを整理する。

## 2. 統合した判断の読み方

### 2.1

裁定終了と他の未解決義務の終了は同じではない。archiveへ責任を書くことと、将来支払の資金を確保することを分ける。[旧ADR-0197](../adr/0197-court-end-effects-and-pending-liability-archive.md)

### 2.2

後から正しいbodyを保存しただけでは、元のassembly期限内に受理した証明にならない。実受付pointと期限内完成を確認してqualified dischargeを判断する案を保持する。[旧ADR-0201](../adr/0201-archived-close-qualified-financial-discharge.md)

### 2.3

名目exposureの解除は新規送金でも売上でもなく、他claim責任や既確定徴収を自動返還する根拠ではない。[旧ADR-0201](../adr/0201-archived-close-qualified-financial-discharge.md)

## 3. 維持する境界・後続改定

private adapterの実装状況と、本番金融規則の採択を別に記録する。終了したCourtを口実に既存署名責任を無断で縮めない。

## 4. 隣接する判断との分担

実原資契約は[ADR-v2-0047](0047-funded-original-and-role-authorization.md)、feeだけの有限終了は[ADR-v2-0037](0037-fee-escrow-and-role-consent.md)、依存claimは[ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0197-court-end-effects-and-pending-liability-archive.md](../adr/0197-court-end-effects-and-pending-liability-archive.md) | L1–L60 | L3–L3 |
| [0201-archived-close-qualified-financial-discharge.md](../adr/0201-archived-close-qualified-financial-discharge.md) | L1–L42 | L3–L3 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0197-court-end-effects-and-pending-liability-archive"></a>

<details>
<summary>0197-court-end-effects-and-pending-liability-archive.md — 原文 L1–L60</summary>

[原文を開く](../adr/0197-court-end-effects-and-pending-liability-archive.md) · SHA-256: `84dd7bb01e852f02e25125f61e741638432806d5120a5f60e59e170b9eea1c7b`

<!-- ADR_SOURCE_BEGIN:0197-court-end-effects-and-pending-liability-archive.md -->
~~~~~~~~markdown
# ADR-0197 — court終了効果と未解決責任の保存方式

Status: **Proposed / Review Required** (2026-10-21)。利用者はAの詳細仕様化・隔離実装を指示。RFC-0030 §8のprivate adapterのみ部分実装し、全金融予約・qualified settlementは未実装。本番規則の採択・統合・有効化ではない。

Source: 隔離候補`c54ba38fd`。[RFC-0030](../rfc/0030-court-end-effects-and-pending-liability-archive.md)で、field別終了効果、具体的な原子移行、資本・容量・署名条件、追加検証を示す。正本は[RFC-0026 §4.3](../rfc/0026-court-policy-master-contract.md)。

## 必要な判断

`write_court(None)`は客観的裁定やclaimの中立voidでも残存close groupを徴収/削除する。現serviceの自動期限guardはこの経路を通る全終了効果を閉じない。既存depositは別残高へ移したescrowではなく、宣言者の残存collateralに対する将来の徴収額である。「archiveに額を書けば資金確保済み」とは扱えない。

ADR-0192 A/Aはserviceと共有frontierの詳細仕様化、ADR-0194 AはX3/serviceの隔離統合に対する指示である。本ADRは、そのAを再確認する質問ではなく、**客観的裁定と未決の別義務が同時に残る場合の終了/金融/資料の方式**を具体的に選ぶ。

## 選択肢

| 案 | 具体的な挙動 | 必須条件と未解決 |
| --- | --- | --- |
| A: 裁定を成立させ、未決責任を別archiveへ原子的に移す（推奨） | 確定した責任だけ一度適用。active session/slotを終了し、他sideの未完group・phase・本物の終了証拠・残責任を別versioned namespaceへ移す | originalの全合法branchで資本/資料/移行の最大費用を事前予約。金融holdの実移行、署名済み責任範囲、欠落のない証拠保存、reorg/IBD/pruningが必要。保存した数字をescrowと呼ばない |
| B: 新rulesetで実費前払と違反責任を分離する | 実費を実在財源からdeclaration等で先に支払い、無応答罰は適格な機会があったときだけ別途確定 | 新しい料金・償還・保存・署名同意・資本を全て積算。未決責任のarchive自体も必要になり得る。旧depositの改名や仮単価で済ませない |
| C: 方式選択を保留し、先にoriginal全分岐有料graphを閉じる | 現候補の未解決guardを維持し、archiveのwire/金融移行は実装しない | F08未完を明示。無期限holdをG14完成にしない。客観的fraudを拒否する追加guardを入れない |

**推奨:** Aを新ruleset候補として詳細仕様化・隔離実装する。既存claimを移行せず、unknown cost/期間/責任を0で埋めず、全graphが閉じるまでproduction writer/武装を拒否する。Aは新Account Bondの実装や金額採択ではない。

## 今回のレビュー対象

RFC-0030 §2–5が具体案である。特に「裁定終了と責任解消を別fieldにする」「archiveより前に新originalで金融/容量/署名を予約する」「署名済み旧期限を延長しない」「終了証拠はdigestだけでなく実bytesまたは同じ保持保証を持つ参照」「全未決解消までpruning/exitを許可しない」を一体で検討する。金額・duration・公開wire ordinalは採択していない。隔離実装の末尾ordinalはRFC-0030 §8に記録する。

現コードのソース調査はF08の必要性を示す証拠であり、新archiveの動作・資金保存則・node検証の成功ではない。E01–E10全条件の合格は未達、T01–T15も既存の完了条件のままとする。

## 判断と採択の範囲

求めるのは候補方式の詳細仕様化・隔離実装の判断であり、旧W10等の変更・新数値/公開schemaの採択・pre統合・武装・配備ではない。[docs/AGENTS.md](../AGENTS.md)は「重大な問題の場合のみ、新規 RFC と新規 ADR の両方、レビュー、設計責任者の明示的採択によって変更する」としているため、既存の経済規則をこの実装判断だけで置換しない。


## 利用者の判断 — 2026-10-21

利用者回答「A：裁定と未決責任のarchiveを分離（推奨）」により、Aの詳細仕様化・隔離実装を進める。旧claimの移行、価格/期間/wireの本番採択、pre統合・武装・配備の承認を含まない。上の未回答を前提とする選択肢説明は判断前の経緯である。資本とarchiveの実装が未完であることは、この指示によって完了へ変わらない。

## 隔離実装の進捗 — 2026-10-21

[RFC-0030 §8](../rfc/0030-court-end-effects-and-pending-liability-archive.md#8-a案の隔離実装--2026-10-21)に部分実装を記録した。rooted archive、実終了object/全TIR evidence、未完groupの保存、完成sideの実deposit徴収とfulfilled raw groupの区別、名目exposure/exit holdを追加する。これは本番の資金確保・全署名契約の保存・有料original graphの完了ではない。Uncaptured/関連causeだけの経路では元court stakeを残し、未知の判決を作らない。

終了後の未完groupを完成させる経路、元rulesetに従うqualified settlementとpruningはまだない。元署名scopeとCourtParamsを確定できない経路を、仮paramsで裁定・自動返還しない。5件の追加native fixtureは動作の部分検証であり、全E01–E10/T01–T15・実容量・実doorや公開採択の代替ではない。


## 宣言時資料の保存候補

親`e2e367e98`から、[RFC-0030 §9](../rfc/0030-court-end-effects-and-pending-liability-archive.md#9-宣言時の署名native裁定入力の保存候補)のprivate receiptを追加する。旧groupにない署名全文・公開鍵・元phaseとnative v5入力を宣言時に捕捉する。資料の保存は終了後の担保解除権限ではなく、旧宣言へ新しい義務/期限を後付けしない。過去のgroupに欠けた入力を仮値で補わず、`None`は未被覆として残す。新key 21 / row 27は隔離候補だけの末尾ordinalであり、本番採択を含まない。


## Private v5再検査

[RFC-0030 §10](../rfc/0030-court-end-effects-and-pending-liability-archive.md#10-保存した入力によるprivate-v5再検査候補)に、保存した署名/chunk/session/phase/native入力で既存v5を呼ぶ候補を追加する。state rootに束縛した非serialized結果を返すだけで、元の金融・credit・責任状態を変更しない。公開受付の適用時点や金融解除を、このprivate再検査によって採択しない。局所の予算/OOM不足はNativeResourceであり、不正/defaultの根拠にしない。全有料graphとqualified settlementは未完である。


## 共通v5 kernelと検査用state copyの除去

[RFC-0030 §11](../rfc/0030-court-end-effects-and-pending-liability-archive.md#11-借用recordを使う共通v5-kernel)は、同じ裁定本体へ通常sessionまたは認証済みretained session/phaseを渡す実装上の整理である。公開入口のcost gate・裁定規則、旧preset/identity、金融権限を変えず、archiveの全state copyを除く。新しいprofileの単価・期限・公開late規則の選択ではない。全資金/物理費用と有料original graphは未完である。
## Private archive material append

[RFC-0030 section12](../rfc/0030-court-end-effects-and-pending-liability-archive.md#12-private-append-of-precommitted-archive-material) adds a private missing-chunk adapter under A. Forward archive updates may extend only signed precommitted material; every original contract, clock, disposition and nominal hold remains fixed. Actual signature and per-chunk digests are checked. Complete material can then be inspected by the existing v5 verifier, but is neither automatically fulfilled nor financially settled. This does not adopt public late-submission timing, new prices or old-claim migration. Funded public ingress, financial qualification/release/pruning and the complete original graph remain outstanding.
~~~~~~~~
<!-- ADR_SOURCE_END:0197-court-end-effects-and-pending-liability-archive.md -->

</details>

<a id="source-0201-archived-close-qualified-financial-discharge"></a>

<details>
<summary>0201-archived-close-qualified-financial-discharge.md — 原文 L1–L42</summary>

[原文を開く](../adr/0201-archived-close-qualified-financial-discharge.md) · SHA-256: `01e9e93987e74b16ee46b6e11c6d54d766776f7f761e9a9db3ff5903ce204697`

<!-- ADR_SOURCE_BEGIN:0201-archived-close-qualified-financial-discharge.md -->
~~~~~~~~markdown
# ADR-0201 — archived closeの期限内実受付と責任解消

Status: **Proposed / Review Required** (2026-10-21)。詳細仕様化・隔離実装の選択待ち。[RFC-0034](../rfc/0034-archived-close-qualified-financial-discharge.md)に根拠、時限・署名・原資・金融writer・保持と受入条件を具体化した。

## 問題

[RFC-0030 §12](../rfc/0030-court-end-effects-and-pending-liability-archive.md#12-private-append-of-precommitted-archive-material)のprivate appendは不足bytesを保存するが、実受付pointを保存しない。後からprivate v5が正しいと判定しても元のassembly期限内に提出したとは限らない。これを履行と同一視すると、署名済み期限とdepositの金融効果を黙って変更する。一方、court終了後でも**元期限内に実際に受理・完成した**正しいbodyは、未投稿応答の配送・収録仮定なしにpositive fulfillmentを確認できる。

既存depositは前払escrowではなく、履行による名目責任の解消から新しい送金やPanel売上は発生しない。既確定徴収や他claim責任の返還を導出しない。

## 選択肢

| 候補 | 内容 | 境界 |
| --- | --- | --- |
| S1（推奨） | 元期限内の実native completionと元署名/native v5を束縛し、当該sideだけpositive fulfillmentとして一度解消。late valid資料は妥当性と時限履行を分離し、absenceはF02適格判定を別途要求 | 実受付point・同一state-root・原資/有料入口が要る。不明はno-slash/hold。hard自動返還なし |
| S2 | 正しいlate completeを提出義務完了として、元期限後でもdeclarationの名目holdを解消 | 元署名の期限効果を緩和する新しい経済規則。既存F02/Aの単なる接続ではなく、追加の変更採択が必要 |

S1を推奨する。実際に受理された期限内の正しい履行を解消しながら、遅延資料や欠落した供給根拠から違反・返還を推定しない。S2を暫定動作にしない。

## 採択範囲

求める判断はS1による詳細仕様化・隔離実装である。既存承認Aの裁定と未決責任分離を維持する。新しい料金・保持期間、公開late受付の本番採択、旧claim移行、pre統合・武装・配備を含まない。

未回答の間は経済入口の既存hold監査と、方式に依存しない全分岐/実供給実装を続ける。採択前にarchiveのfulfilled statusを変更したり、実費未接続の公開受付を開いたりしない。元署名deadline、元session/original hardを上げない。

実装・検証結果は未記録。G14全体の受入条件、RFC-0030 E01–E10、T01–T15を縮小しない。

## S1 private実装初稿

[RFC-0034 §7](../rfc/0034-archived-close-qualified-financial-discharge.md#7-s1のprivate実装初稿)に、同checkpointのactive prefix出所、native chunk全数の元時限資格、元署名/v5と同root、once-only名目責任解消を接続する候補を記録した。新key25 / row31で既存wireを保持する。旧時刻なしappendや出所不明prefixは、最後のchunkが元期限内でもholdを維持する。実徴収・返還・他side・元stake・claim creditは変更しない。試験結果はまだ未記録で、公開有料入口、全原資、全責任解消/pruning、G14全体の完了とはしない。

## 2026-10-21 ユーザー選択 — S1の詳細仕様化・隔離実装

ユーザーは「S1：期限内の実履行だけ責任解消」を選択した。元の期限内に実際に受理・完成した正しい応答だけ当該sideの未決責任を一度解消し、期限後のproof-valid資料の妥当性と金融上の履行を分離する。上の未選択表記は選択前の履歴である。

この選択は詳細仕様化・隔離実装の承認であり、新料金・本番採択・pre統合・武装・配備を含まない。公開有料入口・実原資・同一rootのnative v5・全必要materialの時限資格・once-only処理・他責任保持の接続と試験は別途必要である。実装・試験の成功をこの選択だけから導かない。

## 2026-10-21 隔離候補の検証checkpoint

S1のprivate native writer・別rooted provenance（key25/row31）・同一root/native v5・当該declaration名目責任の一回解消まで接続した。nativeの期限内再提出で既存同値bytesの現在pointを新規記録でき、過去時刻は遡及しない。wire/OOMのNativeResource分類、実serializer/decode容量の事前予約も修正した。caps・元deadline・旧wire/署名domainは変えていない。

関連84 unique selected pass（重複除外、archive filter11とnative frame42は7件重複）、core全tests型検査・非test consensus lib検査はexit0。これはprivate native fixtureであり、実原資・公開有料入口・実返還/徴収・全exit/pruning・別claim金融composition・実node全T01–T15・G14全体の完成ではない。[正確な範囲・raw log・source/binary hash](../evidence/palw/g14-completion-2026-10-21/s1-and-original-cover.md)。文書検査は既存の参照欠損によりFAILのまま。
~~~~~~~~
<!-- ADR_SOURCE_END:0201-archived-close-qualified-financial-discharge.md -->

</details>

