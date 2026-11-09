# 文書番号の台帳

## 2026-11-08 EXEC・1 BPSの追加採択

新規記録は[RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md)と
[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)。
今回の提供workspace内で未使用を確認した範囲の割当であり、未取得branchの一意性は保証しない。
現行20冊／統合ADR50冊を増やさず、後続採択記録として管理する。
旧番号系列の実ファイルはRFC49・ADR212となる。旧原文・衝突パスは保持する。
以下の記録にある冊数・次番号は各時点の履歴である。

## 2026-11-08 の現行入口と採択記録

現在の RFC は **RFC-v2-0001〜RFC-v2-0020 の20本**として整理する。[RFC 一覧](../rfc-v2/README.md)。新規の0016〜0020はこの名前空間で割り当て、旧 `RFC-0016`〜`RFC-0020` の意味・主文書パスを変更しない。統合 ADR は **ADR-v2-0001〜ADR-v2-0050 の50本**を維持する。

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
