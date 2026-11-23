# 文書番号の台帳

新規文書は [RFC](../rfc/README.md) と [ADR](../adr/README.md) の既存番号系列に追加します。以下の統合冊数と割当は各改訂時点の履歴です。

## 2026-11-08 EXEC・1 BPSの追加採択

新規記録は[RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md)と
[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)。
今回の提供workspace内で未使用を確認した範囲の割当であり、未取得branchの一意性は保証しない。
現行20冊／統合ADR50冊を増やさず、後続採択記録として管理する。
旧番号系列の実ファイルはRFC49・ADR212となる。旧原文・衝突パスは保持する。
以下の記録にある冊数・次番号は各時点の履歴である。

## 2026-11-08 の統合と採択の記録

当時の統合RFCは **RFC-v2-0001〜RFC-v2-0020 の20本**として整理する。[RFC 一覧](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/README.md)。新規の0016〜0020はこの名前空間で割り当て、旧 `RFC-0016`〜`RFC-0020` の意味・主文書パスを変更しない。統合 ADR は **ADR-v2-0001〜ADR-v2-0050 の50本**を維持する。

新しい採択記録は [ADR-0215 — Model-free Node, Noninteractive Positive and Adversarial Availability](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md)。添付 `docs(2).zip` 内で同番号の既存主文書がないことを確認した範囲の割当であり、他 branch・未取得 remote・将来予約まで含めた一意性は確認していない。ADR-v2 50本とは別の post-consolidation 文書として識別する。

2026-11-08 の明示採択は設計の採択であり、wire ID・fence・有効化高さの割当ではない。[改訂記録](revisions/2026-11-08/README.md)。以下の「次番号」や文書数は各日時の記録であり、現在の未使用番号を保証しない。既存衝突は完全パスで識別し、解消のために過去番号を再利用・再採番しない。

## 初回整理時の記録

既存主文書は再採番していません。このセットの次番号は **ADR-0185 / RFC-0023**。新規0181/0021は設計凍結、0182/0022は画像・動画・改善の延期です。他ブランチの予約は未確認です。[機械可読版](number-registry.json)。

## 既存の衝突

### ADR-0026

- [0026-current-palw-testnet11-architecture.md](../adr/0026-current-palw-testnet11-architecture.md)
- [0026-palw-v2-runtime-separated-verification.md](../adr/0026-palw-v2-runtime-separated-verification.md)

### ADR-0173

- [0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md](../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md)
- [0173-public-verifier-dispute-completeness-is-misaka-purpose.md](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)

## 欠番

**ADR:** 0048, 0104, 0113, 0156, 0157, 0158, 0159, 0164。内容を捏造して補いません。

**RFC:** なし。内容を捏造して補いません。

## 補助文書・仕様章

RFC-0010/0012のdormant implementationは主RFCではなくdesignへ移しました。旧新パスは [対応表](path-map.md)。Specの18番は以下の別章が共有するため、完全パスで区別します。

- [18-inference-surface.md](../spec/palw/18-inference-surface.md)
- [18-layer-sharded-panels.md](../spec/palw/18-layer-sharded-panels.md)
- [18-verification-certificates.md](../spec/palw/18-verification-certificates.md)

## 2026-10-20 X3 worktree の追加予約

[ADR-0193](../adr/0193-sequential-witness-filing-clock.md) を Proposed として予約。ADR の次ローカル番号は 0194。0189–0192 は別 branch の割当で、この branch に主文書を捏造・複製しない。上の ZIP 時点の次番号は履歴であり、全 branch の未使用番号を保証しない。RFC の別 branch 予約も引き続き区別する。


## 2026-10-21 終了効果候補の追加予約

[ADR-0197](../adr/0197-court-end-effects-and-pending-liability-archive.md) / [RFC-0030](../rfc/0030-court-end-effects-and-pending-liability-archive.md)をProposed/Draftとして予約した。利用者はAの詳細仕様化・隔離実装を指示し、本番採択・有効化とは区別する。ローカルの822 unique ref headとactive worktreeを調査し、同番号の別文書は見つからなかった。[調査範囲](../evidence/palw/g14-integration-candidate-2026-10-20/court-end-effects-number-audit.json)。未fetchのremoteや将来branchの未使用保証ではない。次のローカル番号はADR-0198 / RFC-0031。上の過去時点の次番号は履歴である。

## 2026-10-22 認証付き state map の追加予約

[ADR-0203](../adr/0203-authenticated-palw-state-map-and-row-access.md) / [RFC-0036](../rfc/0036-authenticated-palw-state-map-and-row-access.md)をProposed / Review Requiredとして予約した（30 GB pruned node 方針の state lane）。基準 b1eb33960 の tree には ADR-0202 / RFC-0035 までの主文書があり、0198–0202 / 0031–0035 は他 lane の割当で本台帳には未記録。storage 作業の他 lane（IBD）が consensus 可視の形式を起案する場合は ADR-0204 / RFC-0037 以降を使う。次のローカル番号は ADR-0204 / RFC-0037。未 fetch の remote や将来 branch の未使用保証ではない。

## 2026-11-02 並行割当衝突

CourtSupplierConsent と AllModelG14PanelOne は並行 branch で同じ RFC-0042 / ADR-0210 を割り当てた別主文書である。両方の元番号・完全パス・採択状態・wire/署名domainを保持し、再採番・採択内容の統合を行わない。これは衝突の明示記録であり、新たな採択・実装検証・有効化の記録ではない。既存機械検査の exact-path duplicate registry に以下の二組を記録する。

- [RFC-0042 (AllModelG14PanelOne)](../rfc/0042-all-model-g14-panel-one.md)
- [RFC-0042 (CourtSupplierConsent)](../rfc/0042-court-family-supplier-consent-and-native-reservation.md)
- [ADR-0210 (AllModelG14PanelOne)](../adr/0210-all-model-g14-panel-one.md)
- [ADR-0210 (CourtSupplierConsent)](../adr/0210-court-family-supplier-consent-and-native-reservation.md)

## 2026-11-03 先行relay資料の採択

[RFC-0046](../rfc/0046-positive-native-funded-relay.md) / [ADR-0214](../adr/0214-positive-native-funded-relay.md)を、ユーザーの「両案を採択し、限定実装を進める」により同時採択。ローカル全refの履歴で同番号の主文書が無いことを確認した。先行署名資料・有限custody・条件付き部分精算を実装する。本番有効化・旧原資移行は別承認。未fetch remoteや将来branchの未使用を保証しない。

## 2026-11-07 Batch court time contract

[RFC-0047](../rfc/0047-batch-court-time-contract.md) は、Panel 発行の期限証明を受理経路にする案を不採用としたあとの再検討である。上位は [公開裁定の方針 v0.1](../design/palw/consensus/public-adjudication-policy-v0.1.md) である。Batch の新しい版だけが、一つの時間契約、収集と保持の分離、6,880 DAA の公開予算を始める。完了 receipt から 10,000 を始めない。306,108 DAA の局所化は代替経路ではない。G14 の batch 予算は測定したモデルではない。`DISPUTE_WINDOW_DAA` は 10,000 のまま。`NOTICE_WINDOW_DAA` は 3,120 のまま。P0–P3 には試験がある。debug の局所 door 27 は class の受理ではない。条件 2–4 を完了とは書かない。出荷 preset は武装しない。main へは push していない。未 fetch remote や将来 branch の未使用は保証しない。

<!-- CAD1-NUMBERING-BEGIN -->
## 2026-11-17 受付難易度の独立RFC

[RFC-0056](../rfc/0056-global-claim-admission-difficulty.md) / [ADR-0224](../adr/0224-global-claim-admission-difficulty.md) はPROPOSED_DESIGN_NOT_ACTIVATEDとして登録する。先行の[RFC-0055](../rfc/0055-global-admission-court-calendar-and-one-bps.md) / [ADR-0223](../adr/0223-global-admission-court-calendar-and-one-bps.md)は同会話で提供した資源保護の設計記録を併録する。

今回のmainに0051〜0054/0219〜0222がないことを未使用の根拠にしない。先行pre側の割当と0055/0223提供を保持し、番号を再利用しない。照合対象はpinned main2d8212b6と会話内の先行成果物であり、全remote refの一意性保証ではない。次候補はRFC-0057 / ADR-0225。runtime・wire IDを今回割り当てない。
<!-- CAD1-NUMBERING-END -->

<!-- CPN1-NUMBERING-BEGIN -->
## 2026-11-17 Court nonceの独立RFC

[RFC-0057](../rfc/0057-court-nonce-and-panel-challenges.md) / [ADR-0225](../adr/0225-court-nonce-and-panel-challenges.md) をPROPOSED_DESIGN_NOT_ACTIVATEDとして追加する。既存0055/0056/0223/0224および別枝の過去割当を再利用しない。新wire IDは割り当てない。今回の照合範囲は固定mainと会話内成果物。次候補はRFC-0058 / ADR-0226。
<!-- CPN1-NUMBERING-END -->

<!-- SWC1-NUMBERING-BEGIN -->
## 2026-11-17 監査対応の提案番号

[RFC-0058](../rfc/0058-staged-work-commitments-and-consensus-eligibility.md) と [ADR-0226](../adr/0226-staged-work-commitments-and-consensus-eligibility.md) を、添付docsと前回0055〜0057の後続提案として登録する。今回の範囲で衝突はなく、未取得の全remote refまでの空き番号を保証しない。0056／0057は同じ主pathでSWC1追補を記し、元bytesと過去試験は新revisionの記録で区別する。

旧next_localは当時の割当snapshotであり現行の未使用番号案内ではない。次の提案は少なくとも0059／0227以降について新たに番号監査する。20 RFC／50 ADRと旧番号・wireは維持する。
<!-- SWC1-NUMBERING-END -->

## 2026-11-18 AN1草案のローカル割当

[RFC-0059](../rfc/0059-palw-an1-input-binding-and-panel-court.md) / [ADR-0227](../adr/0227-palw-an1-input-binding-and-panel-court.md)。添付AuditRevision内で未使用を確認した番号であり、未取得remote/branchの予約まで保証しない。状態はDRAFT_NOT_ACTIVATED。現行20 RFC・統合50 ADRを維持し、旧next_localは当時の履歴として書き換えない。

## 2026-11-20 MOR1草案のローカル割当

- RFC-0060: [モデル登録時の試行別出力束縛と暗号学的非重複性](../rfc/0060-model-registration-and-attempt-output-binding.md)。
- ADR-0228: [モデル登録時の試行別出力束縛](../adr/0228-model-registration-and-attempt-output-binding.md)。
- 状態は`DRAFT_NOT_ACTIVATED`。添付の最新docsで未使用の番号を使用した。別branchとの番号競合が存在しないことや、本番採択・有効化を証明する割当ではない。
- 現行の閲覧冊数20 RFC／50 ADRは維持し、両registryのpost_consolidation_documentsに草案を登録する。[変更記録](revisions/2026-11-20-model-output-binding/README.md)。

<!-- TB1-FOW1-NUMBERING-20261120 -->
## 2026-11-20 の後続割当

既存RFC0060/ADR0228はMOR1として保持する。添付TB1の仮RFC0060は衝突を避け、[RFC0061](../rfc/0061-palw-internal-threshold-beacon.md)と[ADR0229](../adr/0229-palw-internal-threshold-beacon.md)へ統合した。関数由来のコア採掘値・費用適格性は[RFC0062](../rfc/0062-palw-function-output-and-work-admissibility.md)と[ADR0230](../adr/0230-palw-function-output-and-work-admissibility.md)。いずれもADOPTED_DESIGN_NOT_ACTIVATEDであり、wire値・activation heightの割当ではない。

[number-registry.json](number-registry.json)の後続割当に追加し、過去のnext_localや旧重複・欠番の履歴は書き換えない。現行の統合冊数20 RFC-v2/50 ADR-v2は維持する。

## 2026-11-20 MOE1後続割当

[RFC0063](../rfc/0063-additional-model-option-economics.md)／[ADR0231](../adr/0231-additional-model-option-economics.md)を新経済契約へ割り当てる。現行20 RFC-v2/50 ADR-v2と旧欠番・重複番号は維持する。設計採択でありnative ID・有効化高さではない。

## 2026-11-20 MOE2追加採択

[RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md)と[ADR0232](../adr/0232-reward-buy-and-permanent-liquidity.md)を未使用番号として追加した。原子的市場登録と追加モデル限定を引き継ぎ、30%購入・20%永久LP・残余50%へ改訂する。既存番号、旧next_local、wire IDは再利用しない。設計採択済み・未有効化。
