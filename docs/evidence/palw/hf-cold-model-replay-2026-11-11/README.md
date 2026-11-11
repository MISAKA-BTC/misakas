# HF固定版の新規取得・同じStudio候補の再計算 — 2026-11-11

後続の[RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)により、取得可否のnative認定は廃止された。本記録はoff-chain取得・同一性・再計算の証跡であり、追加モデルのburn停止・解除を許可する証明ではない。

[RFC-0052](../../../rfc/0052-hugging-face-model-acquisition-and-claim-materials.md)の取得経路について、モデルファイルを置いていない新規directoryへ匿名で実取得し、固定recipeで変換したモデルを使って同じStudio候補を再計算した。**ローカル取得・再計算の成功であり、公開T12受理・選出Panelの承認・Finalではない。** host全体には既存モデルが別directoryに存在した。新しいprocessが読むモデルは今回の取得・変換先に固定した。

## 結果

- `HuggingFaceTB/SmolLM2-135M@93efa2f097d58c2a74874c7e644dbc9b0cee75a2`、9 files、272,443,805 bytesを取得。全size/hashを含む[source manifest](source-manifest.json)は[元の固定manifest](original-source-manifest.json)と一致した。保存したHub APIは同じrepo/commitについて`private=false`、`gated=false`を明示している。
- 固定stats/calibration・量子化recipeから変換し、class `7a2616b4…36c625`、inventory `8e31580d…8eec4`が元対象と一致した。[変換ログ](convert.log)、[layoutログ](declare.log)を保存した。
- 候補`7645bc8b…a3711`の元worker request/result、commitment、captureと照合し、独立に計算した全header、execution root、output root、生成token `[28]`が一致した。5154 leaves、再計算1.348秒。[実行ログ](replay.log)、[probe source](replay-probe.rs)、[実行script](replay-probe.sh)を保存した。
- native資源見積もりはRAM 2,480,262,208 bytes、disk 660,288 bytes。最初のローカルprobeの仮置き2 GiB上限では停止した。[初回拒否](replay-first-resource-refusal.log)を残し、32 GiB hostで10 GiB以上の空きRAMを確認後、probe上限を3 GiBへ変更した。**Node/profile/合意の資源値は変更していない。** script全体の最大RSSは981,499,904 bytesで、見積もりと実測は別である。

## 実装

- `0c41b0307`: live accepted TFP1の適格replayでbackendがない場合、既存operator指定root-fetchへexact class/rootを渡す。SDKによるinventory再計算とclass照合を通ったartifactだけをロードする。別tickの完了回収で元wantを保持し、以前別rootとしてscanされたfileを必要時に再評価する。daemon checkと回帰5件PASS。
- `55a9a7ffd`: fetcherに`--require-public-ungated`と`--expected-source-manifest`を追加。strict時は固定Hub APIの正規URL・repo・SHAとliteral falseのprivate/gatedを必要とし、全file集合・size・独立SHA-256が一致するまでcompleteを書かない。modelsテスト15件PASS。

初回downloadと再計算は追加flag実装前の既存fetcherで実行した。その後、両flagを付けたCLIで別の新規directoryへ9 filesを実取得し、元の固定manifestと一致することと全SHA-256を確認した。[strict liveログ](strict-live-fetch.log)と[manifest](strict-live-source-manifest.json)を保存した。再計算に使用したのは初回取得であり、二回目の再計算を実行したとは扱わない。二回目の重複weightsと最初の中間lowered artifactは、照合・証跡保存後にこのtaskの一時ファイルとして削除した。

## 限界と残件

変換probeは固定stats/calibrationを既存packから受け取った。これらのrecipe依存もPanelへ認証して供給する必要がある。HFの重みだけから自動的に揃ったとは扱わない。既存rlibへ直接linkしたprobeであり、現HEAD全体の新規release buildではない。元入力・tool・linked rlibのSHA-256は[summary](summary.json)に記録した。

legacy PALWTIR1全fileのdigestは元と異なる。native headerのmeta以外は同一で、JSON provenanceの`logits_scale`の末尾表記が`5.960464477539064e-08`から`5.960464477539063e-08`へ変わっていた。native inventory/classと実再計算は一致するが、byte-identical fileやPALWART3再pack一致を主張しない。

遠隔HTTPからのclaim資料取得、選出Panelの承認、公開T12のquote/受付/Finalは本検査では成立していない。先行設計の公開HF契約・定期検査・有効期限・取得不能裁定は後続採択で廃止し、その実装を残件として続けない。次の公開阻害条件は最後に観測した`G14_ROUTE_ABSENT(FpTokenizer)`の修復である。
