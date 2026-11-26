# RFC-0002 — PALW-TIR・kernel・推論ランタイム

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0002](../adr/0002-deterministic-kernels-and-runtime.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 実行契約

PALW-TIRは型・shape・位置・状態遷移・数値演算を固定する有界tensor programである。Model IDは重みだけでなく、program、tokenizer、数値profile、入力adapter、Readout、trace/checker契約と結び付く。モデル名やURLを実行契約にしない。

Nodeは登録されたprimitiveの型、shape、上界、証拠形式を解釈する。MinerとPanelは同じ規範実行を再現する。任意のnative binaryや汎用VMをモデル登録からNodeへロードさせない。

## 2. TIRの意味論

| 項目 | 固定する条件 |
| --- | --- |
| tensor | dtype、rank、dimension、layout、要素順、padding、scale、zero-point、範囲 |
| arithmetic | 積・和の幅、縮約順、丸め、飽和又はoverflow拒否、定数表 |
| control | 有界scan、position、初期state、更新state、停止、実行範囲 |
| data access | weight index、Gather/TopK、分岐、expert、KV・外部入力の認証元 |
| output | 正規token、採掘Readout、終了関係、canonical commitment |
| local checker | 各primitiveの必須入力・出力・境界・weight openingと最大資源 |

全局所関係の成立が規範実行へ結合するよう、Init/Step/Readout/Endのcoverageを定義する。部分的に対応したgraphを完全対応として登録しない。浮動小数点の近似一致やGPU固有の演算順を合意値にしない。

integer accumulationの再配列は、範囲とexact性を保つことが確認できる部分だけに許す。Saturating add、再quantize、近似norm等の境界をまたぐfusionは同値性を要する。広いlogit表現はdtype・scale・codecごとに固定し、飽和した旧型へ黙って戻さない。

## 3. kernelとモデル承認を分離する

既知primitiveへの自動lowering、shape推論、weight変換、frontend pack、生成kernelは技術準備として利用する。その成功はBFT採用票を代替しない。

新primitive、未知codec、非対応checkerが必要なら、先にNodeの明示upgradeと有効化を行う。BFTのモデルYESだけで実行命令の意味を追加しない。既知primitiveの高速実装は、同じcanonical値・状態・observerイベントを返すホスト実装として扱う。

## 4. artifact、変換、runtime pack

manifestに全ファイルの正規path、長さ、digest、tensor区間、config、tokenizer、converter版を固定する。ヘッダーだけのpreflightでは型・shape・必要kernel・上界を調べ、全重みの性質を確認したとは表示しない。

変換はshard/tensor/tile単位で読み、必要な範囲だけをmaterializeする。入力packのdigest、量子化descriptor、統計抽出、丸め、書込順を固定して再現可能にする。量子化形式はpacking・scale・group・virtual tensorの規範記述を持つ。未対応形式を名前の類似で推定しない。

runtime packは高速ロード用の派生物である。別packでもcanonical tensorが一致すれば同じ実行とし、正規identityをpackの非本質的metadataへ依存させない。model/artifact/checkerの認証をpack cacheで省略しない。

## 5. GPUバックエンド

CPU参照とGPUの両方が同じ実行planと数値意味を読む。GPUではGEMM、attention、norm、activation、RoPE、KV更新を対象にし、対応単位ごとにexact同値を検査する。backend/device/driver名、block/tile分割、fusion、非同期scheduleをTicketの自由fieldにしない。

非対応演算は同じ意味のCPUへfallbackできる。転送待ち、CPU作業領域、二重bufferも共通予算へ計上する。精度削減、非規定の近似softmax、文脈の省略をメモリ不足の回避にしない。採掘値の一致だけで不正な中間実行を見逃さない。

GPU providerの具体選定・性能目標は実装計画で決める。ここでは数値一致・資源上限・FOW1との接続を必須とし、未測定のtokens/sやRAM下界を保証しない。

## 6. 小さいRAMでの実行

model全量を同時RAM/VRAMへ置くことを必須にしない。resident、部分resident、streamedの各配置は同じ計算を返す。

- weightは認証済みtile単位で読み、必要な次tileを有限数だけ先読みする。
- KV cacheとcarryはpositionを保持したpage単位で退避・復元する。sliding windowを使わないprofileでは過去KVを勝手に捨てない。
- attentionは規範の縮約・丸めを守る有界tile処理とし、全系列score行列の常時保持を要求しない。
- 共有RAM/VRAM、pinned buffer、spill、trace、Merkle処理、並列jobを一つの予算台帳で予約する。共有領域の別名を二重計上せず、実別領域を計上漏れしない。
- event/fence完了前のbufferを解放・再利用しない。cancel・OOM・再起動で予約と実領域の所有がずれないようにする。

GPUを使うことと小RAMで動くことは別の測定項目である。disk帯域・page fault・転送量・peak resident・latencyを分けて報告する。

## 7. FOW1 observerとtrace

QF1等のprofileが指定するcanonical位置・演算後の値をobserverへ渡す。fusionでobserver境界を消さず、正しい値を再構成する。前処理、PrefillBody、DecodeDecision、Stop、ReadoutEndを区別する。

最終生成数をleaf hashの前置fieldへ含む形式では、生成中に最終rootを確定したふりをしない。有限disk spoolへcanonical recordを書き、終了数確定後に正規順でsealする等の方式を使う。同時RAMに全traceを保持する必要はないが、書込・再読・保持費用を予約する。

## 8. 長文脈と資源契約

register時の最大context、input、output、branch、expert、trace、Court、cold replay、資料供給を同じdescriptorから導く。計算グラフの上界、実行時上界、Nodeの証拠検査上界を混同しない。

共有weightやprefixの再利用は許すが、元identityと状態依存を認証する。並列batchが存在しても、各jobのinput、Stop、Readout、仕事会計を分離する。RAM不足は型付きlocal資源拒否であり、不正claim認定ではない。

## 9. 適合試験

同一入力でCPU/GPU、resident/streamed、batch単独/並列、cold/warm、再起動のcanonical出力と四Rootを比較する。境界値、overflow、丸め、GQA、routing、EOS、context上限、破損tile、途中cancel、disk不足を含む。

モデル変換・技術互換の試験成功と、BFT採用、claim Final、受賞を別結果として記録する。GPUや低RAM実装の存在だけで新たな採掘資格を作らない。

## 10. 規範演算とkernel契約

演算の結果を数学関数名だけで定義しない。exp/log/norm/RoPE/activationは、適用profileが指定する整数algorithm、定数表、範囲縮小、丸め、飽和又は拒否を同じ順で実行する。残差加算の前後、積和の縮約境界、再量子化siteをprogramで明示する。既存にない狭い中間型をbackendが挿入しない。

A16、広いlogit、held-context等のprofileは、型・scale・state幅・contextをdescriptorで区別する。同じ名前のweightでも別の数値profileへ置換しない。embeddingの行取得、KV/履歴、recurrent stateの各readは元tensor/rootと期待indexに認証する。modelが要求する全dependencyを実行・Court両方で解決する。

kernelの論理descriptorは、semantics、grammar、primitive、arithmetic、memory、commitment、checker、challenge policy、Court、resource scheduleを束縛する。モデル側bindingはdescriptor、program/plan、artifact、入力schema/tokenizer、出力schema、state/context方針を束縛する。論理項目の記述は新wire型・tagの割当ではない。

既知演算のcompositionは、有限graph・型・資源・検査coverageの範囲内で許可する。未知演算・不明な状態効果・任意script/VMは登録時に拒否する。kernel更新の影響を受ける署名・trace・cache・Courtは同じdescriptorへ接続する。モデルのBFT YESはkernel更新の代用にしない。

weight inventoryは有界なleaf stream、runtimeは認証済みmmap/tile readerと有限cacheを使用できる。cold load、複数job共有、pressure、cancel、spill復元を同じ資源勘定で扱う。必要RAMをartifact比だけの固定値で保証しない。
