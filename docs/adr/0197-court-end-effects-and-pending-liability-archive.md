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
