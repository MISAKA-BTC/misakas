# 同一TFP1のnative Trace量と予約・供給の接続

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前ターンは`26d79b9ef`の局所undoをコミットしたprogress。今回はclaim固有資料のTrace量をnative geometryから計算し、共通preclaim予約、publisher、Panel replayへ接続した。モデル取得証明・HF可用性の認定は実装しない。Studio同一claimの公開G14受付・Final・当選権利消費・対応blockは **NOT_PROVEN**。

## 計算とconsumer

[StepSpace](../../../../consensus/core/src/palw_tir_step_v1.rs)に、全位置のraw値数を閉形式で求める処理を追加した。post開始位置、同じblockの複数出現、Hウィンドウ、checkpointとhistoryの全instanceを計上する。nativeの要素数のu64飽和も同じ位置で反映し、合計はu128で飽和する。Nodeは値の生成・全trace列挙・モデルダウンロードを行わず、既に検査されたprogramの規模に依存する走査と固定stack配列を使う。

[Source](../../../../consensus/core/src/palw_tir_public_source_v1.rs)は、4-byte v1値の総量をOption<u64>として保持する。u64に入らない場合も旧Sourceの生成を新たに拒否せず、新しいpublic Trace量consumerで有限予算の不足として扱う。Source ID、旧wire、commitmentは変更しない。型付きSource構築の既存サイズ見積もりは追加された固定fieldを含む。

[通常TFP1 projection](../../../../consensus/core/src/palw_court_shared_material_intake_v1/batch/plain_output/projection.rs)は、値の総量と136-byte indexを合算する。[共通preclaim予約](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/preclaim.rs)はこれをdrawn bondごとの送信・保持枠の下限として、graph構築前に照合する。unsigned quoteとsigned intakeの予約入口が同じ処理を通る。これはTraceの下限であり、他の資料、opening、carrier、実fee、全枝のwork・期限を計上する完全plannerの代替ではない。

[remote replay](../../../../misaka/palw/remote/src/positive_panel_sources/legacy_tfp1_replay.rs)と[publisher](../../../../misaka/palw/remote/src/positive_panel_sources/legacy_tfp1_trace_publish.rs)も、manifestのTrace宣言がnative量と完全一致することをprovider read・capture sizing・予約より前に検査する。過少申告と過大申告の両方を拒否する。モデル重み配信を追加せず、対象は元claimのTraceのみ。

## 回帰コード

- 閉形式と位置ごとの計数、実際のleaf値を比較する。tiny、fixed/history、dense multi-layer、job末尾を超えたprefixを含める。
- 0、1、u64::MAX/2、u64::MAXの要素数でnative飽和との一致を検査する。内部寸法を書き換える境界fixtureを有効な実モデルとは扱わない。
- 旧Source生成と型付き生成で、leafごとの値数から得た総量が一致することを検査する。
- Traceが収まらない予約枠ではgraph構築・book変更が起きないことを検査する。
- 正しく再署名したmanifestでもTrace宣言が4 bytes短い/長い場合に拒否する。provider/capture/reservationへ到達すると失敗するfixtureを使う。

すべて **WRITTEN_NOT_RUN**。[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従いRustの実行は通し実装後。静的レビュー・整形・文書検査の結果はvalidation.jsonに分離する。

## 最新のユーザー優先順位

1. 原子的な受付保存・巻戻しを完成する。三行の原子性に加えclaim/frame/book/escrow全体を同じ受付単位へ接続する。
2. G14の費用・資源予約・期限計算を受付本経路へ接続する。現drawn予約だけで全枝完成としない。
3. FpTokenizer route不足を解消する。既存route predicateはfenceだけでなく、そのclassのtokenizer資料も確認する。HF取得や今回のTrace量計算では代替しない。
4. 公開T12でquote成功・署名・受付を実証する。
5. 同一claimでPanel検証・Finalを実証する。
6. 当選権利の消費・対応ブロック生成を確認する。

モデル取得証明は作業対象外。公開gateの変更、配備、署名送信、公開probeは今回行っていない。
