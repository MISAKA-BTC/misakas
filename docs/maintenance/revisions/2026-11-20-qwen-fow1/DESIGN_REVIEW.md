# QF1設計レビュー

これは本改訂内の仕様点検であり、外部監査、native実行、数学的費用証明ではない。

## 確認した具体性

| 問い | 規範上の答え |
| --- | --- |
| どのQwenか | Qwen3-8B dense、36層。instanceは正確なartifact/program/tokenizer rootsが必須 |
| どの瞬間か | 各回答tokenの予測位置、第二残差のClamp後、次block norm前。final normは別の37本目 |
| どのtokenか | 0番目はprompt末尾。j番目はL−1+j。最後のselected tokenを追加forwardしない |
| 数値差はどうするか | 固定整数TIR/A16と全登録scale。上流BF16との一致を合意条件にしない |
| yの品質をどう扱うか | 実際の整数/HEX64/greedy/trace構成をG-USEへ通す。未実測は採掘不可 |
| nonceだけhashしていないか | zの直接入力は固定formatと数値vectorだけ。nonceは実入力を経由 |
| Readoutが偽ならどうするか | 元TIR値→canonical bytes→4096-byte hash chunk→z→R_z/Endを局所反証 |
| 同じ値を別の層から出せるか | StateMapから期待layer/node/tileを導出。任意source指定を拒否 |
| 全modelをNodeへ渡すか | いいえ。元traceと登録modelの対象index opening、対象tile/chunkを検査 |
| 通常推論への追加 | stream hashと認証記録。採掘用layer、MAC prefix、EOS後dummyを追加しない |
| 旧モデルやeconomicsへの影響 | 新profileだけ。MOE2/TB1/旧claimは維持 |

## 設計レビューで反映した点

1. 仮定の32層を公式8Bの36層へ修正し、FFN幅12288、QK norm・untied headを明記した。
2. 既存TIRが整数であるため、BF16の任意GPU tensorをそのままhashする仕様にせず、整数派生profileと品質gateを明示した。
3. layer出力とTransformersのhidden_states配列の意味を分離し、最終normの前後を固定した。
4. 最後の出力tokenを処理しない位置規則、EOSと上限の優先、prefillで不要なheadを仕事に数えない規則を加えた。
5. hash圧縮blockごとに巨大な常設証拠を作らず、4096-byte chunkごとのchain recordと最大32 compressionのCourtを選んだ。
6. zへ試行IDを足して数値衝突を隠さず、数値状態衝突とhash衝突の上界を分けた。
7. 根拠のないc_min、品質score、性能上限の実測値、実model rootを作らず、instance未作成・未認定をJSONへ記録した。

## 実装時に残る確認

TIR entry/stage runner、全scale/table、実tokenizer segment、real artifact、bounded source mapping/multiproofの実carriage、完全trace、実Panel/Court、MAS1停止consumer、再起動/reorgの整合が必要である。小規模参照にはこれらを実装していない。

特に整数化と入力frame/greedyの品質、8K contextのメモリとthroughput、Readout/保持/Panel費用は未実測。37 vectorを使うことは有用推論の省略不能性の定理ではない。全descriptorが揃っても、適合するG-COLL/G-COSTがなければ採掘は閉じたままである。

## 安全性の限定

モデルが正しい数値状態を安く作れるなら、それだけを理由にMinerを算術詐欺としてslashしない。その算法が費用契約を反証する場合はMAS1の採掘停止へ進む。BFT署名は未知の算法の不存在を証明しない。

一人正直のPanelが資料・全検査・反証包含を行える前提、Nodeの局所checkerのsoundness、元commitmentのbinding、native合意・活性は別途必要。QF1のcodec固定はDAG全体の安全性を証明しない。
