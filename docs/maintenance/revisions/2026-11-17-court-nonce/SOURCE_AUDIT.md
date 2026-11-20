# RFC-0057 — Court nonceのソース監査

日付: 2026-11-17（Asia/Tokyo）  
状態: **SOURCE REVIEW / PROPOSED_DESIGN_NOT_ACTIVATED**  
対象提案: [RFC-0057](../../../rfc/0057-court-nonce-and-panel-challenges.md) / [ADR-0225](../../../adr/0225-court-nonce-and-panel-challenges.md)

## 1. 対象と証拠の範囲

添付 `misakas-main(1).zip` から展開した `MISAKA-BTC/misakas` の `main@2d8212b634880b0967fbf24b00b41936c2322c14` を静的に読んだ。以下の外部リンクはすべて同じ固定commitを指す。直前のRFC-0056成果物に追加した提案本文を、既存mainの実装証拠として用いていない。

今回の監査は、採択済みのpositive/negative設計、既存Court署名と手番、共有資料、positive contract、assignment nonce、beacon、既存の虚偽Valid責任を対象にした。全runtime・全入口・全network presetの網羅監査、Rust試験、実Node試験、公開activation確認は行っていない。sourceに型や関数があることと、新RFCの一体経路が実装・有効化済みであることを分ける。

## 2. 採択済みの契約で維持する点

### 2.1 元rootへのbindingと独立実行の限界

[RFC-v2-0017](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/docs/rfc-v2/0017-noninteractive-positive-verification.md) §2・§2.1・§6は、局所positive、完全再実行の責任付き承認、成功Finalを別の結果として扱う。承認はchain、claim、元commit、model/program/input/output/execution roots、profile/ruleset、選出bond・署名者・期限へ束縛される。一方、正しい資料・署名・openingからMiner/Panel本人のCPU/GPU使用、非共謀、独立実行の物理履歴を証明したとはしない。

RFC-0057が補うのは、このscopeに対する主体別のnonce導出、局所challengeとの結合、受理・効果の一回性である。nonceだけで全体再実行の履行を観測できるという意味を追加しない。

### 2.2 共有する証拠と、重ねて数えないcoverage

同じ[0017 §5・§8](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/docs/rfc-v2/0017-noninteractive-positive-verification.md)は、canonical対象集合の被覆を要求する。同じ対象の証拠を複数者が提出しても、別の対象を検査したことにはならない。同一資料の共有・重複排除、再実行した記録・Merkle tree・認証済み重みの再利用は許される。

したがって、世界共通の `CourtHash` を一度使ったら以後すべて拒否する方式は、この共有契約に適合しない。資料identity、検査対象identity、提出者の義務identity、承認の一回性、精算の一回性を分離する必要がある。

### 2.3 一人の反証と、不署名の多数

[RFC-v2-0017 §2.2・§9](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/docs/rfc-v2/0017-noninteractive-positive-verification.md)と[0020 §4・§5](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/docs/rfc-v2/0020-verification-gated-final-and-adversarial-models.md)は、PublicOneHonestの最低完全承認数を1件とし、残りの不署名だけで正常claimを止めない条件を規定する。担当者の全体検証責務と、正直者一人の取得・検査・提出・包含の前提は残る。

Panel固有challengeを全選出者分の必須署名へ変換すると、悪意多数の不提出で正常完了が止まる。新nonce規則は、当該Panelの肯定承認を数えるための条件と、共通coverage・未解決case・Finalの既存条件を区別しなければならない。

また、適格な単独反証は、肯定多数や局所positiveで覆せない。異議申立人の反証箇所を、その人に無作為割当されたpositive challenge箇所だけへ制限してはならない。

### 2.4 個別caseと原時計

[RFC-v2-0018 §2.2・§5.2・§10](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/docs/rfc-v2/0018-interactive-negative-court-and-defaults.md)は、元claim、canonicalな申立人、争点identityからCaseIdを導き、relay・再送nonce・重複drawで別予算を作らない。共有bytesを他者の役務履行・他者のdefaultへ変換せず、不正packetで被害者のcaseや手番を進めない。

新nonceでcase、予約、期限を再生成することは許されない。既存のnegative・資料要求・応答については、確定済みcaseのactor/phase/stepを一意に束縛し、追加の乱数待ちで反証を遅らせない構成が必要である。

## 3. 既存native Courtの署名・手番

| source | 確認した事実 | RFC-0057との境界 |
| --- | --- | --- |
| [palw_tir_public_carriage_v1.rs L350–397](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_carriage_v1.rs#L350-L397) | actionとtarget、literal signer、expected_turn等の型・役割整合を確認する | 新しい乱数challengeの有無とは別に、既存の主体/手番拘束がある |
| [同 L433–462](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_carriage_v1.rs#L433-L462) | network、claim、allocation、source、target、expected_turn、rent、literal signer、actionを署名messageへ含め、native bondの鍵で検証する | carrierの送信者をCourtの署名者に置換しない。新scopeを旧signature domainへ黙って追加できない |
| [同 L464–480](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_carriage_v1.rs#L464-L480) | accepted owner、予約時刻、allocation、source、現在turnを照合する | nonce文字列を比較するだけでは、このrootedな期待値解決を代替しない |
| [palw_tir_public_state_v1.rs L1432–1450](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_state_v1.rs#L1432-L1450) | 現在のaccepted demand/turnと元clockに照らしてstale source/turn/期限外を拒否する | 別roundへコピーして時計・予算を回復する新経路を作らない |
| [palw_tir_public_fold_v1.rs L1540–1564](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_fold_v1.rs#L1540-L1564) | rootedな元frameを解決し、実署名を検査してからverified foldへ入り、失敗時はcheckpointを復元する | 新nonce/once marker/経済効果も同じ受理世代に原子的に接続する必要がある |

関連する過去の[signer/relay監査](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/docs/design/palw/integration/rfc26-court-signer-relay-audit-2026-10-20.md)は、署名済みobjectの第三者搬送を許し、relayerへ元の義務を移転しないことを記録している。今回この過去試験を再実行していない。

## 4. 共有資料のnative経路

[palw_court_shared_material_intake_v1.rs L1–22](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_court_shared_material_intake_v1.rs#L1-L22)は、適格主体による分割搬送、submitterに結び付いたdraft、最初の完全認証bodyをcanonical rowとする共有を説明する。認証はデータ同一性であり、計算証明・役務完了・custody・creditではない。

実foldの[palw_court_shared_material_intake_fold_v1.rs L180–235](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_court_shared_material_intake_fold_v1.rs#L180-L235)には、active route、元hard区間、scope、Active/floor bond、署名の検査がある。同じ受理済みdraftは `Retried`、完成済み対象は `AlreadyComplete` となる。

[palw_court_shared_tir_frontier_v1.rs L133–167](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_court_shared_tir_frontier_v1.rs#L133-L167)は、重複queryを `Coalesced` にし、認証済みunitを共有する。同じ対象への代替openingが既に認証済みならfault/new workとせず、共有から署名者のcredit/default効果を導かない。ただし同fileのconstructorは、単独で完全なpaid reservation/native ingressになるとは述べていない。

**帰結:** 正しい証拠bodyの再利用と、別人の承認・役務・報酬を得ることは別である。新RFCの重複拒否は、共有資料storeを破壊せず、個別義務とonce ledgerで実施する必要がある。

## 5. 既存positive contractのscopeと一回性

[positive-contract/messages.rs L8–68](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/positive-contract/src/messages.rs#L8-L68)の `MinerResultCommit` は、旧契約上はstep tree/trace rootを含まない。CaseIdはscope/job/execution/minerへ束縛し、出力・witness・Panel・relayを変えてcase予算を更新させない。

同fileの[PanelWitnessCommit L81–128](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/positive-contract/src/messages.rs#L81-L128)には、case/result、author、attempt、profile、plan、execution record、candidate output等の署名bindingがある。shape検査は `ExactStreamV1` を扱う。これは新RFC-v2-0017の元Miner traceを対象とする完全再実行承認への接続を示す証拠ではない。

旧 [palw_positive_court_v2.rs L1–5](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_positive_court_v2.rs#L1-L5) は、Panel所有の証拠による隔離されたEXACT_STREAM prototype、Gate C creditの発生源ではないことを明示する。実際の[admit L178–221](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_positive_court_v2.rs#L178-L221)でも、policy/job/artifact等とMiner/Panelの署名を検査している。旧Panel所有rootの意味を新しい元Miner rootの意味へ読み替えない。

### 5.1 SourceRequestとServiceReceiptは主体別nonceで増殖させない

[messages.rs L185–224](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/positive-contract/src/messages.rs#L185-L224)では、SourceRequestのidentityからauthor/relay/request nonceを意図的に除き、全Panel・全attemptにわたる重複unitのunionを計量する。request署名はrequesterを別途束縛する。

[native.rs L21–43](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/positive-contract/src/native.rs#L21-L43)のServiceReceipt keyも、case/source kind/source/unitを使い、payer/recipient/Panel/relay/attemptを除いている。新しい主体別Court nonceをこの共有役務identityへ加え、同じunitを別の新仕事として二重支払する構成にしてはならない。

旧prototype内の[positive_court_v2/service.rs L108–170](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_positive_court_v2/service.rs#L108-L170)も、canonical unionからPanel/relay/nonceを除きquota resetを防ぎ、再応答のtransmission/workを計量しつつ完了unitの支払を一回にする。この局所service実装は新profileのnative入口・実資金・全Node負荷の完成証拠ではなく、未認証packetで他者の保護予約を消費させる許可でもない。

### 5.2 既存assignment nonceは参考になるが、新Court nonceではない

[positive-contract/assignment.rs L252–269](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/positive-contract/src/assignment.rs#L252-L269)には、`next` と `exhausted` を持つ永続canonical-lineage counterがある。厳密な次nonceを受理し、u64 overflowではwrapせず以後を拒否する。

[palw_state_v2/positive_assignment.rs L703–721](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/positive_assignment.rs#L703-L721)は、実stateのposter nonceを読み、資金・assignment・role等と同checkpointで書く。資料retentionと分離した一回性の先例として参照できるが、Panel固有の局所challengeや全Court共通nonceが既にあることを意味しない。

## 6. 公開乱数の実装で確認した境界

[challenge/beacon.rs L1–20](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/challenge/src/beacon.rs#L1-L20)は、canonical PALW履歴の将来の適格workを集約するWorkBeaconを定義し、committeeが発行・lockする仕組みではないと明記する。対象subjectに依存するwork、古いcommitの再添付、順序差を排除し、branch相対のlockはreorgで再計算する。資料不足時の `BEACON_UNAVAILABLE` をfraud/passとせず、block hash・署名・local randomness等へのfallbackを認めない。

同fileは、workを多く混ぜることだけではbias resistanceの証明にならず、適応的な最後の寄与、withholding、並べ替え、reorgの評価が外部review gateとして残ることも記載する。

[challenge/seed.rs L29–47](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/challenge/src/seed.rs#L29-L47)のseed導出は、beacon context、challenge policy、subject kind/root、chain/rulesetを照合する。

**帰結:** 引用文にある「BFTの公開乱数」を現行実装の保証として記述しない。RFC-0057のnonceは適用profileの認証済みpost-commit乱数とanchorへ接続し、その取得順序・偏り・期限を実装/有効化gateとして扱う。ここで参照したhash/乱数機能を、計算正当性の暗号学的証明の採用根拠にしない。

## 7. 既存の虚偽肯定責任と二重徴収防止

[palw_offence_attribution_v1.rs L650–668](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_offence_attribution_v1.rs#L650-L668)のfalse-Valid offenceは、一つのseat/claimに対して一つである。反証bytes、receipt形式、reporter slotを変えても同じoffenceであり、二度目の証拠で二重に徴収・報酬を発生させない。

[同 L1000–1054](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_offence_attribution_v1.rs#L1000-L1054)は、署名receiptが実際にattestした範囲と、反証が読んだ全範囲を照合する。full receiptと部分scopeを区別し、責任範囲外の反証をその署名者の虚偽Validとして扱わない。[同 L2175–2207](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_offence_attribution_v1.rs#L2175-L2207)も、対象identity、署名scope、nativeに認められた反証を必要とし、単なるexecutorの沈黙を計算不正へ読み替えない。

TIRの[apply_conviction L1935–1965](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_fold_v1.rs#L1935-L1965)は元のcollection door、producer/execution rootを確認し、既に消費されたconvictionを再受理しない。[L2061–2101](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_fold_v1.rs#L2061-L2101)ではcredited Valid custodianとproducerについて、元のexit hold、未消費offence、実lockを確認して既存の制裁経路を適用する。選出されただけで未承認のseatを全員有罪にする処理ではない。

delivery defaultも[同 L1693–1763](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_fold_v1.rs#L1693-L1763)で、供給機会・当人の元義務・clock・実担保を別に確認する。service不供給はNoSlashHoldとなる経路がある。

**帰結:** 新しい完全承認をこの責任経路へ接続するには、承認scope、署名者、元subject、反証対象、実lock、once keyの明示adapterが必要である。既存の一部receiptがslash対象であることから、新FullReplay承認への接続済みを推定しない。nonce不一致、不正packet、資料不足、観測不能な「実際には再計算しなかった」ことを一律の計算有罪へ変換しない。

## 8. 検索で確認できなかったこと

監査時点の `consensus/core/src`、`consensus/src`、`misaka/palw` について、`FullCanonicalReplayExact`、`PublicOneHonest`、`minimum_full_replay_approvals`、`ComputationRefuted`、`CertificationReady` の文字列検索に一致はなかった。これは仕様名そのままのnative実装を確認できなかったという結果であり、対応する意味のすべてがsourceに存在しないという証明ではない。

今回提案する統一scope、主体別nonce、個別positive challenge、共有materialと分離したonce ledgerが、全対象入口へ一体接続されているとは確認していない。その不足を[実装接続表](../../../design/palw/integration/court-nonce-implementation-plan-2026-11-17.md)に分解する。

## 9. 追加監査ファイルのGitHub tree照合

GitHub mainが同じ基準commitであることを再確認し、再取得したtree `d99e8148554f04b7f53a19d4717a1f9b4796d3f3` に対して、添付sourceの追加6ファイルのGit blob SHA-1がすべて一致した。[照合記録](source-provenance.json)に保存した。前回のdocs/consensus等3,582 blobと追加RPC 1 blobの記録も保持している。hash一致は実装の正しさ・稼働を保証しない。

| path | Git blob SHA-1 |
| --- | --- |
| misaka/palw/positive-contract/src/messages.rs | c4d5ce6db183470931a841ebbd443caa5bc57a1b |
| misaka/palw/positive-contract/src/native.rs | a89d0b316575e7dc744b1a04d4f8357e3d6b45ae |
| misaka/palw/positive-contract/src/assignment.rs | ce39630a822c134eb3111b522733bd9ff74ee91c |
| misaka/palw/challenge/src/seed.rs | fbd0059cf3026edbba471e7f0c48017e1fbaecac |
| misaka/palw/challenge/src/beacon.rs | b324f0b92fb8c409a04b3a63e630e726f860d704 |
| misaka/palw/challenge/src/hash.rs | 45c3b01721bf40820d74622021812313812d6038 |

追加確認した[challenge/hash.rs](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/challenge/src/hash.rs)のnative `Digest` は64 bytesである。新nonceの導出でこの既存seedを32 bytesへ切り詰めず、版とnative型を認証して全bytesを取り込む必要がある。

## 10. 監査結論

既存設計・実装には、署名scope、手番、元clock、共有資料の重複排除、assignment nonce、作用/制裁の一回性という部品がある。新RFCはこれらを壊さず、元claim・選出者・round・元root・認証済み乱数から導くnonceと個別challengeを、新しい版の承認条件へ明示的に接続する必要がある。

この監査は静的確認であり、既存のscope型へnonce fieldを追加しただけで接続・復旧・経済効果が完成すると判定していない。完全再実行の能動的な正直者、公開資料、提出包含、期限内裁定という安全性前提も継続する。
