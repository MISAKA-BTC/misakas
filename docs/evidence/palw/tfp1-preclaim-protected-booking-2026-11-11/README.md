# TFP1 preclaim と選出枠予約の接続

状態: **IMPLEMENTATION_IN_PROGRESS / RUST_VALIDATION_NOT_RUN / NOT_PUBLICLY_ACTIVATED**。

2026-11-11 の作業。旧 allocation/frame が存在する前に、受理済み親の tokenizer 登録と元 TFP1/Source から資料予約の相対 graph をコンパイルする。署名前の companion と署名後の companion は同じ物理予約 kernel を使い、OriginalId と正規化した draw rank に束縛した別々の packet namespace を予約する。後続の受理済み draw/署名要求から復元した case にも同じ予約 scope を返す。

## 今回の実装

- DAA と blue score の両方を prospective terms と照合し、受理時には再導出を必要とする。
- 全 rescue 配分（選出者数、各選出枠、第三者枠、防御枠）を service resource identity に束縛する。今回、物理 graph を構成するのは選出枠だけであり、完全 profile を意味しない。
- 署名前後で manifest 容量、保持期限、総予算、notice/case schedule を共通検査する。
- 先頭 graph の幅と選出枠数から合計 packet 上限を確認してから、残りの graph を構築する。各枠の FullIds を別に予約し、別枠の完了へ依存させない。
- 全枠の候補予約ができた後、各枠の予算・資料 Finish・検査・control・終端/回復期限を確認し、成功時だけ共通 book を確定する。後の修復で動かせる packet window の最終時刻も同じ期限内に制限する。
- claim固有資料と局所openingの範囲を維持し、Minerによるモデル重み全量配信やHF取得認定を追加しない。

## 静的レビューで修正した点

当初は companion schedule と予約 graph が独立して検査されていた。その場合、資料期限が notice+1 でも Open/Begin/Part/Finish の予約が成功し得た。全枠の確定前に両者を照合するよう修正した。

従来の tokenizer fixture の executor key は32 bytesの仮値であり、実 companion principal を導くための入力には使えなかった。旧 fixture は維持し、capture/hash より前に本物の ML-DSA key を固定する別 constructor を追加した。これはFPの実署名・支払・公開受理を成立させるものではない。

## 検証状態と残件

追加した回帰コードは、2枠の独立性・総上限・予算/期限不足時のbook不変、および同じ実署名companionの署名前後でquote/graph/bookが一致することを対象とする。

実装計画の通し実装順序に従い、この候補の Rust build/test は未実行。追加した回帰コードを PASS と報告しない。rustfmt と git diff --check のみ成功。文書検査ログは [docs](docs-check.log)、[RFC](rfc-integrity.log)、[ADR](adr-integrity.log) に保存する。

Selection、claim資料の供給失敗/defaultと回復、第三者/防御枠、完全版のnative planner、versioned public quote、TFO3同時受理/実支払/保存、遠隔PanelとFinalへの接続は未完。既存op248や旧署名profileの意味を変更せず、完全profile未設定の拒否を維持する。公開T12への配備・送信・quote照会は今回実施していない。対象Studio claimの公開quote成功・受付・Final・main pushの証拠ではない。

## 目標の追加確認

ユーザーが追加した「当選した場合の権利消費・ブロック生成」も未完の達成条件とする。同じclaimのFinal/必要認証、当選判定、成熟・有効期限・未消費権利、実際の消費と対応する生成blockを読み戻す。carrierの収録と権利消費による生成を区別する。非当選は別結果とし、他claimの生成で代替しない。

文書検査は既知不整合で非zero終了した。docsのエラー973件の集合digestは `f47503d7ffeb46801258b72a5535bb2e0dbf74f62b5776057ca1cd3e8873e577` で前回と同一。RFC検査の7件のログは前回とbyte一致、ADRのerrors配列87件も同一であり、baselineは更新していない。
