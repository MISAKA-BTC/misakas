# ADR-0220 — Hugging Face取得とclaim固有の資料供給を分離する

**後続決定が優先:** [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)により、以下の独立取得検査・定期可用性record・期限・HF取得不能の裁定をburn停止・解除へ置換する。本文は採択時の履歴として残す。HF取得とclaim資料の分離は継続するが、取得可否認定の実装を続けない。

Status: **Accepted / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-11
RFC: [RFC-0052](../rfc/0052-hugging-face-model-acquisition-and-claim-materials.md)

## 決定と承認

ユーザーは2026-11-11、Hugging Faceの固定版取得・hash照合を活用し、モデル重みの独自全量配信を必須としない方針で実装を進め、RFCも修正するよう明示指示した。この指示を本RFC/ADRの採択として記録する。元の設計凍結規約による再確認を求めない。公開有効化・性能認定の承認とは分ける。

モデルの内容は登録済みclass/artifact/tokenizer/program/containerのidentityで固定する。新HF profileの唯一の必須公開元をHugging Faceとし、public/ungated・匿名全量取得、immutable revisionと全file hashを固定し、必要な変換を再現してnative rootを照合する。既存のoff-chain配布原則を新しいPublicOneHonestにも明示して適用する。

Minerのprompt、実行trace、中間値、opening、初期依存、有限のclaim固有供給は維持する。モデル未保有の独立選出Panelが取得・検証できない場合の資料不足case、成功停止、原期限、未検証Abortを維持する。任意のcache/mirrorは許すが、全量独自再配布を正常系の完了条件にしない。

追加指示としてHF公開契約の登録時独立全量取得検査、定期可用性検査、有効期限、Finalでの公開条件と取得手続き確認を採択する。登録検査者の信頼前提をclaim Panelの一人前提と分離する。新BFT・validator・DA委員会は導入しない。モデルclassの無審査登録とtierを変えず、HF公開契約は別recordとする。

## 変更する範囲

- PRINCIPLES §9の公開状況・取得可能性をチェーンで審査しない一般規則は、新HF公開契約のcanonical確認に限り、後続のユーザー指示によって明示的に改定する。別recordという名称だけで旧規則との衝突を隠さない。品質審査、class登録、モデル階層の不変条件は維持する。
- PRINCIPLES §13.3、RFC16の公開供給とRFC17のcold検証に、固定HF取得によるモデル資料供給の選択を明記する。
- RFC18/20の取得不能と条件付き正常完了へ、外部取得失敗と計算有罪を混同しない扱いを明記する。
- RFC-0050 §3の全資料供給を、モデル静的資料とclaim固有資料の別経路として具体化する。全Nodeへモデルを配布しない原則は不変。
- RFC-0051には上記との境界を追記する。TKR1によるtokenizerのnative登録は置換しない。
- 原文で取り込んだ2026-11-10実装計画は改変せず、適用する追補を作成する。

## レビューと残余

既存ADR-0067はHugging Face等のoff-chain配布を許し、既存fetcherはcommit/size/hash照合を実装している。一方、取得済みcacheの存在と、未保有Panelの受理済みTFP1 missからの接続は異なる。この調査を実cold検証成功として扱わない。独立レビュー結果と現在の実装状況は[実装追補](../design/palw/integration/hugging-face-cold-panel-2026-11-11.md)に記録する。

全PanelのACK、HTTPの合意入力化、外部サイトの永続可用性、登録時のモデル品質審査、HF receiptによる計算承認の代替は採用しない。資料不足caseの客観的終結、全関係検証、実資源測定と公開T12のquote以降は未完なら未完と報告する。番号は現在checkoutと取得済みlocal refsに同番号の記録がないことを確認して割り当てた。未取得remote branchの予約までは主張しない。
