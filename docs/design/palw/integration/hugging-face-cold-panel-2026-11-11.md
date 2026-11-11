# HF必須profileとcold Panel — 2026-11-11 実装追補

**後続指示を適用:** [RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../../../adr/0221-additional-model-burn-pause.md)により、HF取得可否の認定・登録検査・定期期限は廃止する。追加モデルだけを誰でもburnで停止、登録者burnで解除する。genesis・年次fork指定は対象外。以下の実取得・hash・再計算の成果はoff-chain機能として維持する。

採択: [RFC-0052](../../../rfc/0052-hugging-face-model-acquisition-and-claim-materials.md)／[ADR-0220](../../../adr/0220-hugging-face-model-acquisition-and-claim-materials.md)。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

[2026-11-10原文計画](public-one-honest-vertical-implementation-plan-2026-11-10.md)はユーザーの原文保持指示に従い変更しない。以下はそのI0/I1/I3/I5とcold検査への明示的な追補である。モデルの静的資料とclaim固有資料を分離し、モデル重み内部DAの全面完成をT12受付修復の前提にしない。

## 実装順

1. 最初の公開拒否`G14_ROUTE_ABSENT(FpTokenizer)`について、既存TKR1登録・G14共通quote/admissionの資源・費用・期限を修復する。
2. HF固定commit・全file hash・conversion pins・native rootの照合をPanel取得へ接続する。取得証明のnative認定は作らず、追加モデルのburn停止・登録者burn解除を実装する。
3. 既存HTTP providerで同じclaimのprompt・入力・初期依存・trace・openingを供給する。モデル重みの独自再配布を必須にしない。
4. 公開T12で同じ期限内候補のquote、署名送信、受付、選出Panelの全体再実行・認証・Finalを確認する。
5. off-chain取得失敗時に再計算承認を偽造しないこと、burn停止/解除の認証・支払・競合・年次指定境界・再起動/reorgを検証する。取得可能性の第三者裁定は行わない。

## 確認した既存経路

- `scripts/models/fetch_hf_checkpoint.py`: 完全40桁commit、予定全fileサイズ上限、実取得のexact length、LFS SHA-256またはGit blob hashを照合する。実モデルPython/pickleを実行しない。
- `misaka/palw/tir/lower/src/convert.rs`: 固定stats/calibration/quant/mathからnative artifactを再現変換する。
- `kaspad/src/palw_root_fetch.rs`とPanel hook: operatorが設定した取得jobとexact class/rootのSDK取込みがある。accepted TFP1のbackend miss接続を`0c41b0307`で実装し、daemon check・回帰5件を通過した。
- `55a9a7ffd`: fetcherに明示的public/ungated確認と独立した期待source manifest照合の任意flagを追加。modelsテスト15件と、新規directoryでの両flagを使ったlive取得を通過した。これはoff-chain取得オプションであり、burn停止/解除の条件ではない。
- [claim固有Trace publisher](../../../evidence/palw/ordinary-tfp1-trace-publication-2026-11-11/README.md): native Source認証・単位ごとの資源許可・保持予約・index-lastを実装。これはHF重み配布ではなく、引き続き必要なclaim資料供給である。

## 今回の同一Studio対象

対象候補は`7645bc8b…a3711`。source manifestは`HuggingFaceTB/SmolLM2-135M`、commit `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`を固定している。モデル重み269,060,552 bytesを含む9 fileを独立した空directoryへHFから匿名取得し、保存済みsource manifestと全項目が一致した。実checkpoint取得の結果であり、公開契約登録や選出Panelの全体検証成功ではない。

変換には固定packのstatsとcalibrationが必要であり、これらの補助recipeも取得可能にする必要がある。取得した重みを変換した結果、元のclass/inventory rootが一致し、同じ候補の独立再計算でexecution/output/headerが一致した。[完全ID・ログ・probe・限界](../../../evidence/palw/hf-cold-model-replay-2026-11-11/README.md)を保存した。hostには元モデルも別directoryに存在しており、未保有の遠隔Panelでの検証とは分ける。PALWART3再packは未確認である。

## 未完条件

burn停止/解除の署名・実支払・native保存と全受付/Final/reward gate、経済数値と公開activation、対象TFP1のquote/受付/Final、main pushは未達である。HFModelReferenceのnative可用性認定・定期検査・期限とHF取得不能の裁定は、後続採択により実装残件から削除した。外部HTTPをNode合意入力へ入れるfallbackは作らない。

## レビュー

既存担当エージェントが、必須HF匿名公開、独立登録検査・定期期限、network/class/root/recipeの束縛、旧tierとの分離、他者receiptを本人到達性に使わない点をレビューした。登録/更新のpurpose・accepted parent・clock、canonical後継、private/gatedの明示判定を含めて提案をRFCへ反映した。静的レビューをruntime成立やpublic activationへ読み替えない。
