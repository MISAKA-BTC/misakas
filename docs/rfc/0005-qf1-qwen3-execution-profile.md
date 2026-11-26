# RFC-0005 — QF1：Qwen3の具体的実行・Readout

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**ADOPTED_DESIGN_NOT_ACTIVATED**。QF1の整数実行・codec・Readoutを継承し、モデル採用をBFTのみに統一する。
- 判断理由：[ADR-0005](../adr/0005-qwen3-execution-and-readout.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 対象architectureと不変artifact

### 1.1 Qwen3-8Bの形

本profileは公式Qwen3-8BのconfigとTransformersのQwen3実装を参照し、以下の**36層**構成を固定する。

| 項目 | QF1で固定する値 |
| --- | --- |
| decoder blocks | 36、層番号`l=0..35` |
| residual width `D` | 4096 |
| FFN width `F` | 12288 |
| query heads / KV heads / head width | 32 / 8 / 128 |
| GQA対応 | query head `h`はKV head `floor(h/4)`を参照 |
| Q/K norm | 各headの128要素ごとのRMSNorm＋learned gain、RoPEの前 |
| activation | SwiGLU、gate側にSiLU |
| RoPE | half-splitの`(i,i+64)`、base 1,000,000、scalingなし |
| bias / dropout / sliding window | なし / 0 / なし |
| vocabulary | 151936 IDs、`0 <= id < 151936` |
| embedding / output head | 別tensor。`tie_word_embeddings=false` |
| RMS epsilonの元意味 | `1/1,000,000`。整数scaleへの変換をartifactで固定 |
| QF1最大処理位置数 | 8192。上流モデルの最大contextの主張とは別の本profile上限 |
| `max_new_tokens` | jobで`1..1024`の整数を事前固定 |

行列は数学上`[out,in]`、row-majorで登録する。embeddingとheadは`[151936,4096]`、各層のQ/Oは`[4096,4096]`、K/Vは`[1024,4096]`、gate/upは`[12288,4096]`、downは`[4096,12288]`。各blockのnorm gainは`[4096]`、Q/K norm gainはそれぞれ`[128]`を全headへbroadcastし、headごとに別gainを持たせない。最終norm gainは`[4096]`。configから省略してよい演算を推測しない。

### 1.2 名前ではなくinstanceを登録する

`Qwen/Qwen3-8B`というrepo名又はmutableな`main`はModel IDではない。登録時に以下を欠けなく固定する。

1. 入手元のimmutable revision、全safetensors/config/tokenizerファイルのpath・長さ・content digest、上流artifact root。
2. QF1整数artifactの全weights、gain、scale、epsilon、RoPE table、変換manifestのinventory root。推論とCourtが使うのはこちらのroot。
3. Canonical TIR program bytesとroot、primitive-set版、lowerer版、変換recipe・calibration資料のdigest、各数値parameterの意味。
4. Tokenizerの登録済みprogram/rootと§3の固定token列、16個のHexToken表、job token化規則。
5. §5のStateMap、全commit/tileのlayout、Readout/End checker、全資源上限、FOW1 descriptor・policy・AdmissionBasis、BFT proposal/quorumへの参照。定理提出を採用の必須条件にしない。

同一numeric graph/weightsの再包装、別URL、別owner、追加metadataだけで別仕事を作らない。FOW1のComputationalWorkKeyと登録alias規則を継承する。新しいscaleや変換recipeは別関数であり、固定EXECを見た後に選び直してはならない。

今回の文書には実重み全量、変換済み8B artifact、実program rootを同梱していない。JSONの`instance_bound=false`はそれを明示する。実登録では必須digestの欠落・placeholder・未支持primitiveを拒否する。これは仕様の自由欄ではなく、登録されるモデルの実bytesから埋めるinstanceデータである。

## 2. 数値実行 — PALW整数版Qwenとして定義する

### 2.1 BF16をそのまま合意値にしない

上流BF16実行、GPUごとのautocast、cuBLASの任意reduction、libmの近似値を本profileの規範値にしない。[PALW-TIR §2/§6/§11](../spec/palw/04b-tensor-ir.md)と[整数architecture分解C1](../design/palw/tir/corpus-v1.md)を用いる。**対象はQwen3-8BをPALW整数表現へ変換した別の決定論的実行であり、上流BF16と全tokenが一致するとは主張しない。** 実用性と劣化はこの実行について測る。

| 値・演算 | 規範 |
| --- | --- |
| 主projection weights | `i8`、per-output-channel scaleを別tensorに保持 |
| residual / QKV / MLP code / final norm code | `i16`、範囲`[-32767,32767]`。`-32768`は不正値 |
| logits・norm unit・SiLU/softmax固定小数点 | `i32`。Q24の意味を持つ箇所をprogramに明示 |
| 積和 | TIRのexact `i64` accumulator、必要なscale積は内部`i128`。wrap禁止 |
| narrowing | TIRのA16 `N(acc;m,s,z)`、HalfAwayFromZero、明示Clamp。全m/s/zを登録時に固定 |
| norm / exp / SiLU / softmax | TIRのIntRsqrt・IntExp等の固定整数算法と合成graph |
| lossy演算 | 固定されたDivの丸め・Clampだけ。hidden stateを後から任意桁へ丸めてReadoutしない |
| 初期状態 | 全KV historyが空。未認証の既存cacheから開始しない |

`N`は具体的に`clamp_out(sat_i64(HAFZ(acc*m/2^s)) + z)`であり、`m:i64`、`s`は`0..62`、`z:i64`、加算は拡幅してからout rangeへClampする。HAFZは絶対値の半端を0から遠い方へ丸める。TIRのwide型はcommit値にしない。重み・scaleの全rangeをTIR admissionへ与え、exact演算のrange条件を満たせなければFAIL/UNKNOWNの規定へ進む。

### 2.2 登録で可変な数値と実行中の自由度を分ける

**学習済み重みと同じく、全scaleを不変なmodel parameterとして登録する**。ただし「実装既定値」「auto」「実行時に校正」「好きなtokenizer」は許さない。各MatMul出力、norm gain、残差加算前の共通scale、SiLU入力、attention score/value、最終logitsまで、operand型、scale、丸め、overflowをLoweringManifestに列挙する。

embeddingのlift、normのepsilon/gain、Q/K per-head norm、RoPE table生成とQ24化、attentionの`1/sqrt(128)`、SiLU前後とgate×upのscale、LM headのlogit scaleも同じinventoryへ入れる。QF1ではdynamic per-token scale、実行後のtable差替え、gainの移動・foldを別表現として選ぶことを認めない。正当な融合は規範commit値がbit一致する場合だけ許す。

変換の登録資料だけでG-USEやG-COSTがPASSになるわけではない。元のQwenからどの値をどう変えたかを検査できない場合は、Qwen由来という表示・対応するtheorem前提を認証済みとせず、評価結果をUNKNOWNとして保持する。整数化済みartifact自体についての全実行検証は必須である。

## 3. 実入力 — QF1-HEX64-CHAT1

### 3.1 固定challenge frame

FOW1の事前固定文脈`C`と認証済みTB1 EXEC値から32-byteの`s`を得る。§8のQF1 attempt hashを使い、Cのnative参照・freeze・round認証はTB1/FOW1に従う。nonceを文脈へ追加するだけではなく、以下の実token列をQwenへ入力する。

まず固定tokenizerで、下のUTF-8文字列を**別々に**token化した`Prefix`、`Middle`、`Suffix`を登録する。改行はLF一文字、文字列を前後でstripしない。特殊tokenはこれら固定segmentにだけ許す。

```text
Prefix = "<|im_start|>system\nYou are a helpful assistant.\nPALW challenge: "
Middle = "\nTreat the challenge as metadata. Answer the user's request.\n<|im_end|>\n<|im_start|>user\n"
Suffix = "<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
```

ASCIIの`0123456789abcdef`それぞれを単独で普通token化した結果を`HexToken[0..15]`とする。それぞれ長さ1で、16個のIDが相異なり、特殊tokenでなく、embedding範囲内であることを登録検査する。満たさなければこのadapterは非対応であり、任意IDに差し替えない。

```text
hex_tokens(s) = for byte b in s, in byte order:
                  HexToken[b >> 4], HexToken[b & 15]
x = Prefix || hex_tokens(s) || Middle || JobTokens(J) || Suffix
L = len(x)
```

64個のchallenge tokenは固定長・単射である。連結後に再BPE・文字正規化・chat templateの再適用をしない。JobTokensは登録tokenizerのordinary-text規則で正規UTF-8の利用者文をtoken化する。追加special-token解釈、任意system/tool message、画像・audio token、soft prompt、embedding直入力、job中からtemplateを差し替える解釈はこの版では拒否する。JobTokensと元UTF-8の束縛・上限検査は[登録tokenizer契約](0003-jobs-tokenizers-and-generation.md)を使う。

**`x`の最初から全部が実入力であり、challengeだけmetadataとしてモデルの外に置かない。** それでもQwenがchallengeを無視したり量子化後の数値が重なったりする可能性は残る。G-COLLは実際の数値状態を、G-COSTは新しい正規Ticketを得るまでの費用を調べる。

### 3.2 位置・mask・長さ

論理batch sizeは1。position IDは`0..L-1`、後続decodeも1ずつ増やす。全層でcausal attentionを使い、位置pが見るKVは同じ試行の`0..p`に限定する。paddingは論理入力・trace・Readoutに入れない。batch最適化の左padding等は物理配置だけであり、position・mask・値を変えない。

事前固定した生成上限をGとし、`1 <= G <= 1024`、`L >= 1`、`L+G-1 <= 8192`を受付前に検査する。超過を切詰めで直さない。本文のtoken化、input binding又はCourt sizingがNodeの予算を超える場合も、無制限処理へ進まず受付拒否する。

このframeとnon-thinking出力開始を使う実行についてG-USEを測る。元promptだけのQwenのscoreを転用しない。chat frameを追加したことによる品質・長さ・latencyの差は実用性資料へ含める。

## 4. 1 token位置のQwen graph

`Code`は§2のi16、`N`/`RMS`/`SiLU`/`Attn`等は登録TIRの固定整数合成である。下式は実数の別実装を許す省略記法ではなく、C1 loweringで次の順序・shapeを要求する。

```text
pre(p, token):
    h = EmbeddingLift(Embedding[token])                 # Code[4096]

for l = 0..35:
    u = RMS_Gain_N(h, input_norm[l])                    # Code[4096]
    q = Linear_N(u, Wq[l])                             # Code[32,128]
    k = Linear_N(u, Wk[l])                             # Code[8,128]
    v = Linear_N(u, Wv[l])                             # Code[8,128]
    q = PerHead_RMS_Gain_N(q, q_norm[l])
    k = PerHead_RMS_Gain_N(k, k_norm[l])
    q, k = RoPE_HalfSplit(q, k, p, registered_tables)
    K[l].append(k); V[l].append(v)
    a = CausalGQA(q, K[l][0:p+1], V[l][0:p+1])           # Code[32,128]
    a = Linear_N(flatten(a), Wo[l])                     # Code[4096]
    r = Clamp_Code(Add_same_scale(h, a))
    u2 = RMS_Gain_N(r, post_attention_norm[l])           # Code[4096]
    g = Linear_to_Q24(u2, Wgate[l])                     # i32[12288]
    v2 = Linear_N(u2, Wup[l])                           # Code[12288]
    m = Narrow_Mul(SiLU(g), v2)                         # Code[12288]
    d = Linear_N(m, Wdown[l])                           # Code[4096]
    h = Clamp_Code(Add_same_scale(r, d))
    commit POST_BLOCK[l,p] = h                         # Readout指定点

n = RMS_Gain_N(h, final_norm)                           # Code[4096]
commit FINAL_NORM[p] = n                                # Readout指定点
logits = Linear_to_i32(n, Wlm)                           # i32[151936]
```

Attention scoreはQ/Kのhead幅128にわたるexact積和と登録scale、maxを引く整数softmax、probabilityの固定narrowing、全historyへの値積和で作る。head mappingは§1のGQAとし、headのtile順序・repeat順序を実装任せにしない。RoPEはQ/K norm後、KVに保存するKはRoPE後、VはV projection後である。

### 4.1 生成規則

本版は**non-thinking frame＋greedy**に固定する。`next = min{v | logits[v] = max(logits)}`。temperature/top-p/top-k/repetition penalty・外部乱数・自由seed・speculative draft・beam選別は使わない。本profileのgreedy仕様は、Qwen公式の推奨sampling設定又は同等品質の保証を意味しない。G-USEで不十分なら不合格とし、samplingを入れる別profileで再審査する。greedy選択だけを変えて同一試行を再抽選しない。

EOS集合は`{151643,151645}`に固定する。選択されたEOSもcanonical `y`に1 tokenとして含める。UIは制御tokenを非表示にできるが、R_y・trace・token数を変えない。EOS以外の停止文字列、利用者cancel、timeout、実行エラーは成功終端とせず、元契約に従う未成功終端とする。

### 4.2 prefillのLM head

入力位置`0..L-2`では全36 blockとKVを計算するが、使われないLM headの全vocabulary出力やfinal normを採掘のためだけに強制しない。位置`L-1`以降の、実際に次の回答tokenを選ぶ位置でfinal normとheadを評価する。既存のTIR runnerが全prefill位置でpost headを生成する場合、その余分な実装処理をQF1の有用仕事・費用下界へ足さない。

この**PrefillBody/DecodeDecision**の固定stage scheduleと、それぞれの実行・trace位置をlowerer/runnerへ明示的に結合する。未対応のrunnerを既にQF1実装済みと扱わない。stageごとに任意の別weightsを選ばず、同一layer graph・KV履歴を継続する。

これはTIR v1の各DAGへ任意branchを追加する指定ではない。登録manifestで共通Bodyと固定postのentryを区別し、job長Lにより決まる外側のscheduleが前者又は前者＋後者を選ぶ。両entryのBody、parameter参照、KV型は同一で、異なる重みの別実行を合流させない。このrunner結合をnativeに実装するまでQF1対応とはしない。

## 5. Readoutの正確な対象とtoken時系列

生成されたcanonical token数をTとする。`1 <= T <= G`。回答tokenを0始まりの`y[j]`で表すと、予測位置は

$$p_j=L-1+j\quad(0\le j<T).$$

| j | 実際に処理する入力 | その状態から選ぶ出力 |
| --- | --- | --- |
| 0 | promptの最後のtoken、位置`L-1` | `y[0]` |
| 1 | `y[0]`、位置`L` | `y[1]` |
| T−1 | `T=1`ならprompt末尾、それ以外は`y[T−2]` | `y[T−1]` |

**最後に選んだ`y[T−1]`を、Readoutのためだけに追加forwardしない。** 実際にモデルが処理する位置数は`N_pos=L+T-1`。EOSを選んだ時点で停止する。G番目とEOSが同時ならstop reasonはEOSを優先する。T<Gかつ末尾がEOSでない成功は不正である。

`j`ごとのReadout材料を次へ固定する。

$$V_j=(h_{0,p_j}^{post},h_{1,p_j}^{post},\ldots,h_{35,p_j}^{post},n_{p_j}).$$

各vectorは4096要素のi16。`h^{post}`は**attention、第一残差、post-attention norm、MLP、第二残差加算とClampの後、次のblockのinput normより前**。`n`は第36block後のfinal RMSNorm・gain・narrowingの後で、LM headの前である。

`V_j`は`[37,4096]`、全Readoutは論理shape`[T,37,4096]`。prefillのそれ以外の位置、embeddingだけ、Q/K/Vそのもの、attention score、全logits、量子化前BF16、LM回答の表示文字列は**zの直接の材料にはしない**。それらのうち推論に必要な値・関係は§7の完全検査から除外しない。

Transformersの`output_hidden_states`の配列番号をそのまま採用しない。同APIはlayerに入る前の状態やfinal norm後の状態を返すため、「最後の配列要素＝最終block残差」と解釈すると異なる値になる。登録のStateMapは`(stage,layer,node,role,shape,dtype,scale_ref)`を固定し、TIRの実producerへ結び付ける。commit tileの配置はStateMapから一意に導出する。

## 6. zのbytesとstreaming Readout

### 6.1 一つの符号化

`B2b_n(m)`はRFC7693のunkeyed sequential BLAKE2b、digest length n byte、keyなし、salt/personalizationなし、tree modeなし。digest length32は64-byte digestの切詰めではない。新profileの論理仕様であり既存Hash64のaliasと推定しない。

`LE16s`は2-byte little-endian二の補数、`LE32`は4-byte unsigned little-endianとする。bool、float、範囲外整数、別endian、余剰bytesを拒否する。i16でも`-32768`はCode範囲外として拒否する。

```text
DOM_Z = ASCII("misaka/palw/qf1/readout/v1") || 0x00
Step[j] = 0x01 || LE32(j)
          || concat for l=0..35, then FINAL_NORM:
                 concat for c=0..4095: LE16s(V[j,l,c])
End = 0xff || LE32(T)
z = B2b_64(DOM_Z || Step[0] || ... || Step[T-1] || End)
```

Readoutは64 byte固定。layer数・width・role順は本profileで固定なので任意headerを入れない。`s`、owner、nonce、epoch、job名、signature、R_tau、R_z、Ticket、receipt、selected token IDはzのhash入力へ直接加えない。試行固有性は**実入力を通して実際の数値状態へ届くこと**に依存する。j/Tの構造bytesの違いだけをG-COLLの成功例と数えない。

Step[j]の長さは`5+37*4096*2=303109` byte。全message長は`len(DOM_Z)+303109*T+5`で一意。4096を別のwidthにしたり、都合のよいlayerだけを選んだり、vectorを並べ替えたりしない。

### 6.2 実装してよいstream処理

各選択位置の各layerが値を出した時点で、producer bufferから決定論的順序のstreamへ渡す。全T分を同時RAM保持する必要はない。layer先行の並列prefillでは、最後のprompt位置のvectorを正しい順でemitする。microbatch/device順序をcanonical順に混ぜない。

hashはReadoutだけに使い、その途中stateをQwenのhidden stateへXORしたり、後続層へ戻したりしない。追加の重いlayer・行列積・dummy生成・EOS後の追加tokenを作らない。BLAKE2b chain state、byte counter、最終block flag、paddingは規定算法で一意とし、4096-byte chunk境界に記録する。§7の局所検査対象とする。

### 6.3 非衝突性の意味

同じTの二試行について数値列`V`が同じなら、zも必ず同じである。hashを使っただけでQwenの状態衝突を消せない。構造bytesを除いたVを評価し、有限回の観測だけでentropyや安全性を認定しない。

仮に、許された入力分布・適応的候補選別・固定済み重みについて数値列衝突の上界`epsilon_state`が別途示せ、hash衝突の上界を`epsilon_hash`とすれば、同じTの範囲で

$$\Pr[z_s=z_{s'}]\le\epsilon_{state}+\epsilon_{hash}$$

と分離できる。この式は**未証明のepsilon_stateを小さく設定してよい規則ではない**。異なるTではmessageが異なるが、それも通常推論費用を証明しない。QF1の現在のG-COLL/G-COSTはUNKNOWNであり、BFTや公式モデル名でPASSへ変更しない。

## 7. trace、Readout関係、Panel、Court

### 7.1 traceが含むもの

traceはReadoutの37 vectorだけではない。Init、全prefill/bodyとdecodeのTIR必須commit、KV append/history、全予測位置のfinal norm・head・greedy選択、Stop、Readout、Endを含む。全scalar MACのrunning prefixを新たに常設することは要求しない。大きいdot/attentionは既存TIRのtileとhistory dissectionで有界化する。

以下をLogicalTraceManifestへ固定する。

| 関係 | 必須の認証と規則 |
| --- | --- |
| Init | 正規C/TB1/s、adapterから得た入力、model/program、空KV |
| BodyStep | 位置、layer/node、typed operands、登録重みの期待index、TIRの演算・range |
| KV | 同じ試行の同layer/head・位置のRoPE後KとV、欠落/並替えなし |
| DecodeDecision | final norm、全head logitsの正しさ、max値と最小ID tie-break |
| StateMap | Readoutの各rangeが元trace内の正しいproducer/tileを参照 |
| HashChunk | 規定Readout messageの4096-byte区間、直前chain state、byte counter、最終flag、最大32回のBLAKE2b圧縮 |
| Stop/End | T、L+T−1位置、EOS又はG到達、R_y/R_zとの一致、余剰実行・余剰recordなし |

zはhash終了時点で確定し、その後にReadout/End recordを含むR_tauを計算する。R_tau/R_zをzへ戻す循環は作らない。R_tauのMerkle内部node自身はtraceの実行recordに再帰的に入れない。

### 7.2 bounded Readout checker

hashをlibraryへ丸投げし、zだけがtraceから浮く形を禁止する。論理checker`QF1_READOUT_CHUNK_V1`は、messageを先頭から**4096 byteずつ**区切った一つのchunkを検査する（native tagは未割当）。128-byteのBLAKE2b圧縮blockとは区別する。各圧縮blockのchain stateを常設traceへ記録する必要はない。

1. 元claimのtraceから問題chunkのindex・出力chain stateと、直前chunkの出力を認証する。区間と全message長はTと固定formatから再計算する。隣接関係をPanelに選ばせない。
2. 当該区間がどの固定bytes又はStateMapのi16要素に対応するかをNodeが導出する。任意のsource範囲を採用しない。
3. source要素を元trace rootの正しいtyped tileへopeningし、LE16sを再構成する。要素がchunk境界で分割されても同じ元値を使う。
4. 先頭は規定IV/parameter、途中は直前chunkの出力から開始する。非最終chunkは4096 byteで、32 block全てのfinal flagは0。最終chunkの長さは1..4096で、最後の圧縮blockだけfinal flagを立てる。最後が128 byte丁度でも追加の空blockを作らない。
5. counterはその圧縮までに消費した実message bytesの総数とする。最後の128-byte blockの未使用部分だけをゼロ埋めし、そのpaddingをcounterに足さない。高64 bitのcounterは0である。
6. 高々32圧縮を再計算し、認証された出力との差をComputationRefutedにする。最後のchain stateのLE64 word列とz全64 byteを結合する。

一chunkは数値要素を高々2050個参照し、64-laneのsource tileを高々34個開く。固定header/Endはモデル値を必要としない。この範囲計算・path数・bytes・最大32圧縮を独立にsizingする。hash-stateは通常TIRのwide tensorを偽装せず、固定幅のprotocol recordとする。chunk単位の記録により、全128-byte圧縮ごとの記録と重複raw messageを避ける。

`ReadoutData`のために全matrixや全modelをNodeへ渡さない。一方、局所hashが正しいことは、そのsource tensorの算術が正しい証明ではない。sourceの誤りはTIR関係へ遡って別の局所反証で判定する。

### 7.3 trace leafと順序

本profileは§8のtreeへ、以下の順で有限recordを入れる。BodyStepは位置昇順、pre→layer0..35→必要なpost、programのnode順、row-major tile順。wide非commit値はleafにしない。

1. Init record（context/instance/input長/生成上限の束縛）。
2. 全処理位置のcanonical TIR commit recordと、予測位置でのDecodeDecision。
3. Stop record（EOS優先又はLIMIT、T、最終位置）。
4. Readout hash chunk recordをchunk index昇順。
5. End record（R_x、R_y、R_z、body位置数、各record数、z）。

TIR commitの値tile長は64 lanes、history dissection chunkは32行を本profileの開始値とし、全shapeで既存のcarrier・CPU・path・保持上限に入ることを実admissionが確認する。適合しなければ本profileを受け入れない。上限を隠して巨大Courtを受け取らない。別tileを採用するにはlayout版を更新して審査する。

record bytesは`kind:u8 || payload_length:u32le || payload`とし、TIR payloadは`position:u32le, stage:u8, layer:u16le, node:u32le, tile_start:u32le, lane_count:u16le, dtype:u8, values`の順。layerを持たないpre/postは`65535`、dtype/値はTIRのcanonical型。全場面でlogical schema・field幅・rangeを一つにし、余剰bytesを拒否する。Init/Decision/Stop/HashChunk/Endの詳細field表は[trace schema](../design/palw/models/qf1/trace-schema.md)で固定する。native包絡の割当を実装前に済ませ、未対応consumerへこのrecordを流さない。

### 7.4 完全再実行と局所裁定

QF1には[FOW1の非対話検証と直接反証](0004-fow1-execution-and-ticket.md)を適用する。正常なclaimの照合は公開済み資料と固定したinstanceから完結する。不一致時は元traceの該当record、opening、必要な依存値を使い、Minerへの追加質問なしで局所Courtを開始・完了できることを求める。対話による局所化も選択できる。

Panelは元claimの同じinstanceを使い、全入力から、prefill、KV、36層、head、token選択、Readout、四Rootまで再現する。37 vectorだけの比較、他Panelのpositiveコピー、上流BF16の近似一致を完全検査にしない。計算再利用をした場合も、同じcanonical実行の全関係が検査対象である。

Nodeは元claim、固定model/program/instance、正しい重みindexとopening、過去値、演算規則を認証し、対象tile又はhash chunkだけを再計算する。別モデル・別層・別試行のopeningは拒否する。モデル全体を保持せず、Merkle proofがglobalな費用下界を証明すると扱わない。

元rootとの差だけではMiner有罪にしない。必要資料が取得できなければ既存の資料caseを成功停止へつなぐ。InvalidEvidence/LocalHolds/ComputationRefuted/資料未充足を区別し、正直者の反証をFinal前に包含・処理できる条件を維持する。Panel全員悪意・永続検閲の解決を本profileが新たに与えたとはしない。

## 8. 四RootとTicket — QF1の論理codec

以下は本profileの**唯一の論理符号化**である。nativeの型・class・signature包絡の割当は別gateであり、旧AN1等のwireを解釈変更しない。識別・署名の既存hash幅を一括変更しない。

### 8.1 attempt

`C_bytes`はFOW1で先に固定し認証したcanonical登録文脈そのもの、`sigma_bytes`は認証済みTB1 EXECの正規公開値のbytes（certificateの署名集合ではない）とする。TB1 codecを受け入れられないNodeは受付拒否する。

```text
D_a = ASCII("misaka/palw/qf1/attempt/v1") || 0x00
s = B2b_32(D_a || LE32(len(C_bytes)) || C_bytes
              || LE32(len(sigma_bytes)) || sigma_bytes)
```

長さはu32範囲内かつ既存の事前上限以下。未来値をCへ逆流させない。TB1 sessionの選択、最初の正直shareより前のfreeze、固定targetはRFC-0006/0062のままである。

### 8.2 root

tree kindはX=1、Y=2、Z=3、TRACE=4。X/Yはtokenごとの`LE32(token)`、Zはz全64 byteを唯一のleaf、TRACEは§7.3のrecordをleafとする。すべてlogical leaf数D>=1、`N=2^ceil(log2 D)`、全digestは64 byte。empty/wildcard leafを許さない。

```text
D_m = ASCII("misaka/palw/qf1/merkle/v1") || 0x00
base = s || U8(kind) || LE64(D)
real_leaf(i,b) = B2b_64(D_m || 0x00 || base || LE64(i) || LE32(len(b)) || b)
pad_leaf(i)    = B2b_64(D_m || 0x01 || base || LE64(i))   # D <= i < N
parent(h,j,L,R)= B2b_64(D_m || 0x02 || base || LE32(h) || LE64(j) || L || R)
```

leaf levelは高さ0、最初のparentはh=1、そのlevelの左からj=0..。D=1ならreal_leafがroot。各rootは`(kind,D,digest)`で保持し、期待Dをinput/Stop/trace grammarから検査する。任意のpadding、木形、並び、saltで正しいrootを複数作れない。tree kind・D・期待indexをCourtが元claimから導出する。

### 8.3 Ticket

```text
D_t = ASCII("misaka/palw/qf1/ticket/v1") || 0x00
TicketInput = D_t || s
              || LE64(D_x) || R_x
              || LE64(D_y) || R_y
              || LE64(D_z) || R_z
              || LE64(D_tau) || R_tau
ticket_bytes = B2b_32(TicketInput)
T_ticket = unsigned_big_endian(ticket_bytes)
win = T_ticket < theta
```

`D_z`はZ treeのleaf数1、§6の`DOM_Z`は固定domain bytesである。thetaはFOW1で事前固定した`0..2^256`の整数。nonce/owner/job/model/profileはC→s→実入力と元claimに結合済みであり、自由にTicketへ足し直さない。署名、Panel/Court、slot再包装、receiptはTicket入力に入らない。

正しいrootであっても、新しいTicketを安く生成できる経路の確認には別途G-COSTの反復検査が必要である。偽rootをhashして当選した候補だけ提出する行為自体をhash式だけで防いだと主張しない。不正実行のFinal排除はPanel＋Court、資料・包含・活性条件、個別claimの認証条件に依存する。

## 9. cache・batch・正当な高速化

同じ試行のprefill/KVを保持してdecodeするのは通常推論であり許す。物理batch・tensor parallel・kernel融合は、全canonical commitとtoken結果が一致する場合に許す。規範上のexact演算で再結合可能な範囲を固定し、浮動小数点の近似一致で代替しない。

cache keyは少なくともnumeric artifact/program/profile、実入力prefix全体のcommitmentと長さ、position、mask、KV型/scale、layer/headを束縛する。hashだけ一致したと宣言するcacheを認証済みとしない。別sで変わったchallenge以降のKVを、そのまま正しいと扱うことはできない。

一方、challengeより前の固定prefix、weights前処理、同じ意味の計算の再利用、数学的簡約が可能であることを隠さない。Readoutの全vectorが正しければ、それを作った高速な手順自体は算術不正ではない。G-COSTは、これらを許す攻撃者と複数候補を対象に、新しい正しいTicketの初回照会以前の費用を評価しなければならない。

費用評価では、回答cache、停止位置の予測、共通prefix、batch、前処理、誤予測後の再実行を分けて記録する。通常の1回推論と正規の停止判定・trace生成を基準に含める。基準実装が停止位置の取得とtrace生成を別々に実行した場合、その2回分を不可欠なLLM費用とみなさない。

必要なtraceが最終長Dをleaf prefixに含む場合は、オンライン推論で正規recordを有界spoolへ保存し、停止確定後に規定のDでMerkleをsealする。再計算・spill・保持の費用を別に測り、モデルを2回実行することを規範要件にしない。

未知の高速算法の不存在は証明されたと扱わない。測定を実施した場合も対象入力・機材・予算・未検査経路を明示し、G-COSTの評価とBFT採用を区別する。

## 10. BFT採用と評価

技術互換を満たすこの不変instanceを、RFC-0007のBFT YES/NOだけで追加・採用する。kernel解析、G-COLL、G-COST、G-USEの評価値は判断資料であり、自動採用の入口にしない。数学的定理や実機費用証拠の提出を票の有効条件にしない。

未知演算、未定義codec、数値規則違反、有界checkerの欠落はYESでも免除しない。q YES、現在のpolicy/generation、停止なし、対象profileの有効化を確認する。既存の保留を文書更新だけで解除しない。評価専用実行のtraceに後から採掘報酬を付けない。

全モデル共通の市場・係数1.0・BFT管理を適用する。genesis免除、fork特別経済、自動承認の経路を設けない。

## 11. 追加量と有用仕事の会計

Readoutの数値bytesは1生成decisionあたり`37*4096*2=303104` byte（296 KiB）。T=128で37 MiB、T=1024で296 MiB。これは**読む／hashする量**であり、同時RAM下界、実測latency又はLLM費用の下界ではない。固定header等とtrace/Merkle/署名/保持の量は別に加わる。

選択した値は元の回答生成にも使う。final norm vectorをtraceへcommitする追加、stream hash、hash関係の記録とCourtには実費がある。BLAKE2bのchain recordは4096-byte chunkごとに保持し、そのbytesも生じる。既存TIRの全body資料、Panelの再実行、DA費用をこの296 KiBだけで代表しない。

有用推論費用、認証・readout費用、保持・供給費用をWorkCostContractで別々に計数する。追加hashやtrace書込みの下界をQwen GEMMの下界へ読み替えない。費用のためだけの追加GEMMや全MAC prefixを採用しない。性能上限を超えた場合はgateを下げず、別profileでReadoutの設計・安全性を再審査する。

仕事量を算出する際は、検証された実処理位置`L+T−1`と実際のT回のhead/選択を使う。上限Gを実生成数とみなしたり、未実行のprefill head、EOS後のdummy token、再送した同じtraceを追加仕事へ数えたりしない。資源予約の最悪上限と、認定可能な実仕事は別である。型・規範演算数の集計だけを攻撃者の物理費用下界にしない。

## 12. 受入試験と完成判定

以下は実装の必須試験であり、表の列挙だけでは実行済みではない。

| 群 | 合格条件 |
| --- | --- |
| QF1-01 geometry | 36/4096/12288/32/8/128、gain共有・QK norm・untied headの相違を拒否 |
| QF1-02 input | 256値全byte→64 hex token、segment KAT、再token化・切詰め・position不一致を拒否 |
| QF1-03 numeric | 独立2実装で各commit値一致。丸め、saturation、RoPE、全head tie-break境界を含む |
| QF1-04 schedule | L=1/T=1、EOS初回、G到達、EOSとG同時、最後のselected token非forward、余剰post拒否 |
| QF1-05 readout | 全37 vector、層/要素/step順、LE16s、-32768拒否、stream分割非依存、z同一 |
| QF1-06 trace/court | 別重みindex、別layer/position、KV差替え、hash counter/padding/chain、End長とrootの違反を局所反証 |
| QF1-07 roots/ticket | 木形、padding、kind、leaf数、D=1、target境界、再署名/別carrierで再抽選なし |
| QF1-08 useful | 実artifact・実job・実hardwareでG-USEとoverhead上限。前処理・batch/cacheの条件を公開 |
| QF1-09 eligibility | 技術互換＋正規BFT YESのみで採用。未知評価は保持。自動PASSのみ、停止中、未知kernel、評価専用claimの報酬利用を拒否 |
| QF1-10 end-to-end | 対話経路を無効または未実装とし、非報酬評価で元資料を取得してPanelが完全再実行する。一致時の承認、不一致時の直接反証と局所Court、資料欠落時の成功停止を個別に確認する。BFT採用・停止状態、同時停止・再起動/reorg/履歴復元も確認 |

同梱の小規模Python参照の対象は、QF1-04/05/07の論理codecの一部と否定例である。

有効化までには実artifact、tokenizer/StateMap、lowerer、全実行checker、資源条件、native移行と通常claim経路を完成させる。採用根拠は共通技術互換とBFTの正規YESだけである。実装・採用・有効化を別に表示し、BFT経路へ定理と測定の完成を再要求しない。
