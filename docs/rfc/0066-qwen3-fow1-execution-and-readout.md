# RFC-0066 — QF1：Qwen3-8Bを用いるFOW1の具体的実行・Readout

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**（具体化した設計。Qwenの採掘資格・実測・native有効化は未成立）
- 論理profile: **QF1-Q3-8B-A16-8K-v1**。native class ID、wire tag、activation値は未割当。
- 対応ADR: [ADR-0234](../adr/0234-qwen3-fow1-execution-and-readout.md)
- 上位規範: [FOW1](0062-palw-function-output-and-work-admissibility.md)、[MAS1](0065-model-admission-screening-and-bft-stop.md)、[TB1](0061-palw-internal-threshold-beacon.md)
- 機械可読契約: [qf1-qwen3-8b-profile.json](../design/palw/models/qf1-qwen3-8b-profile.json)
- 実行位置・符号化の小規模参照: [qf1_reference.py](../design/palw/models/qf1/qf1_reference.py)。LLM実装・本番codecではない。

## 0. 何を今回固定するか

Qwenの「任意の内部状態」から採掘値を取るという未指定部分を、本profileについて閉じる。対象をQwen3-8Bのdense decoder graphに限定し、整数版の同一推論から有用回答`y`と採掘値`z`を得る。新しいモデルの系列を選ぶ、Readoutの層を変える、数値型を変える、generationを変える場合は別profileとして審査する。

**選択する状態は、回答の各tokenを予測する位置における全36 blockの第二残差加算後の4096要素と、最終RMSNorm・gain・A16 narrowing後の4096要素である。** これら37本の整数vectorを順序どおりにhashする。最終hidden state一本だけ、8層おきのsample、全MACのprefix trace、外付けnonceだけのdigestへ置換しない。

この選択は、どの関数を実行・再現・反証するかを確定する。**新しい正規Ticketを安く作れるかは、固定した仕事・経路・費用条件で繰り返し測る（G-COST）。異なる試行の数値状態が重なる確率には、別途G-COLLの適合条件を適用する。** G-EXEC/G-COLL/G-COST/G-USEとMAS1の全条件を残す。Qwen用のG-COLLの適合TheoremRuleが未完成の間は、非報酬評価だけを許し、採掘、credit、weight、受賞権利を発生させない。

### 0.1 採用した範囲と旧条項の限定置換

| 旧規範 | QF1で具体化・限定置換する内容 |
| --- | --- |
| RFC0062の抽象Readout | §5〜§7の37 vector、token位置、整数bytes、hash処理に固定 |
| RFC0059 §3.1の35-token byte frame／RFC0062 §3の継承 | **QF1だけ**は§3のHEX64 chat frameを使う。旧AN1と同じinput rootにはならない |
| 未指定のQwen実行精度・kernel | §2/§4のPALW整数A16 graphと、登録済みの全scale/table/program bytesで固定 |
| FOW1のhash/木形をprofileで固定する要求 | §6/§8の論理codecで固定。既存wireの意味は変更しない |
| MAS1モデル追加・UNKNOWN停止 | 維持。QF1名、公式重み、有限testの成功からPASSを推定しない |

旧claimは受付時の規則でdrainする。TB1のEXEC/PANEL/MODEL_AUDITの独立性、MOE2の経済条件、burn停止と安全停止の独立性、モデル非保有Node、Panel＋Court、暗号学的な計算validity proofを使わない方針を維持する。QF1はモデル改善・画像動画の延期を解除しない。

## 1. 対象architectureと不変artifact

### 1.1 Qwen3-8Bの形

本節の形は公式Qwen3-8Bのconfigと、TransformersのQwen3実装を参照した。末尾の一次資料を参照する。ユーザー説明中の32層は例示であり、本profileの対象は**36層**である。

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
5. §5のStateMap、全commit/tileのlayout、Readout/End checker、全資源上限、FOW1/MAS1 descriptor・policy・TheoremRuleへの参照。

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

層ごとの量子化scaleを単一の架空値に決めると回答品質を壊し得るため、**学習済み重みと同じく、全scaleを不変なmodel parameterとして登録する**。ただし「実装既定値」「auto」「実行時に校正」「好きなtokenizer」は許さない。各MatMul出力、norm gain、残差加算前の共通scale、SiLU入力、attention score/value、最終logitsまで、operand型、scale、丸め、overflowをLoweringManifestに列挙する。

embeddingのlift、normのepsilon/gain、Q/K per-head norm、RoPE table生成とQ24化、attentionの`1/sqrt(128)`、SiLU前後とgate×upのscale、LM headのlogit scaleも同じinventoryへ入れる。QF1ではdynamic per-token scale、実行後のtable差替え、gainの移動・foldを別表現として選ぶことを認めない。正当な融合は規範commit値がbit一致する場合だけ許す。

変換の登録資料だけでG-USEやG-COSTがPASSになるわけではない。元のQwenからどの値をどう変えたかを検査できない場合は、Qwen由来という表示・対応するtheorem前提を認証済みとせず、MAS1のUNKNOWNを使う。整数化済みartifact自体についての全実行検証は必須である。

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

64個のchallenge tokenは固定長・単射である。連結後に再BPE・文字正規化・chat templateの再適用をしない。JobTokensは登録tokenizerのordinary-text規則で正規UTF-8の利用者文をtoken化する。追加special-token解釈、任意system/tool message、画像・audio token、soft prompt、embedding直入力、job中からtemplateを差し替える解釈はこの版では拒否する。JobTokensと元UTF-8の束縛・上限検査は[登録tokenizer契約](0051-funded-preclaim-tokenizer-registration.md)を使う。

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

本版は**non-thinking frame＋greedy**に固定する。`next = min{v | logits[v] = max(logits)}`。temperature/top-p/top-k/repetition penalty・外部乱数・自由seed・speculative draft・beam選別は使わない。これは再現性のために選んだ本profileの仕様であり、Qwen公式の推奨sampling設定や同等品質の保証ではない。G-USEで不十分なら不合格とし、samplingを入れる別profileで再審査する。greedy選択だけを変えて同一試行を再抽選しない。

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

QF1には[FOW1の非対話検証と直接反証](0062-palw-function-output-and-work-admissibility.md#fow1-panel-dialogue)を適用する。正常なclaimの照合は公開済み資料と固定したinstanceから完結する。不一致時は元traceの該当record、opening、必要な依存値を使い、Minerへの追加質問なしで局所Courtを開始・完了できることを求める。対話による局所化も選択できる。

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

長さはu32範囲内かつ既存の事前上限以下。未来値をCへ逆流させない。TB1 sessionの選択、最初の正直shareより前のfreeze、固定targetはRFC0061/0062のままである。

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

正しいrootであっても、新しいTicketを安く生成できる経路の確認には別途G-COSTの反復検査が必要である。偽rootをhashして当選した候補だけ提出する行為自体をhash式だけで防いだと主張しない。不正実行のFinal排除はPanel＋Court、資料・包含・活性条件、MAS1 gateに依存する。

## 9. cache・batch・正当な高速化

同じ試行のprefill/KVを保持してdecodeするのは通常推論であり許す。物理batch・tensor parallel・kernel融合は、全canonical commitとtoken結果が一致する場合に許す。規範上のexact演算で再結合可能な範囲を固定し、浮動小数点の近似一致で代替しない。

cache keyは少なくともnumeric artifact/program/profile、実入力prefix全体のcommitmentと長さ、position、mask、KV型/scale、layer/headを束縛する。hashだけ一致したと宣言するcacheを認証済みとしない。別sで変わったchallenge以降のKVを、そのまま正しいと扱うことはできない。

一方、challengeより前の固定prefix、weights前処理、同じ意味の計算の再利用、数学的簡約が可能であることを隠さない。Readoutの全vectorが正しければ、それを作った高速な手順自体は算術不正ではない。G-COSTは、これらを許す攻撃者と複数候補を対象に、新しい正しいTicketの初回照会以前の費用を評価しなければならない。

2026-11-21の非報酬評価では、同じ回答を返した署名済み6試行で二つの経路を調べた。別試行の数値Readout `z` をそのまま流用した場合、全15組で値が異なった。[Readoutの比較記録](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-readout-reuse.json)

先頭試行の回答を固定し、残り5件それぞれの試行IDでcommitmentを計算し直すと、回答の9 tokenと回答Rootは5件すべて元の値に一致した。同じ方法で先頭試行の数値Readoutを使って作ったReadout Rootは0件一致だった。この入力集合では回答と停止位置を予測できたが、それだけで正規Ticketの全入力は揃わない。[回答キャッシュの比較記録](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-cached-answer.json)

同じ6試行を3組に分け、各組で共通prefixの数値計算を共有した場合は、全試行の回答・数値Readout・四Root・Ticketが独立実行と一致した。3組の合計経過時間は、独立実行475.835秒、共有実行447.899秒で、5.87%短縮した。どちらの計測にも生成の停止位置を求める実行と、元のTicketに照合するtrace生成の2回が含まれる。初回Ticket照会の費用を判定するには、停止位置の予測で最初の実行を省ける場合も調べ、予測を外した試行の費用も計上する。各組1回ずつの測定であり、採択済みの費用閾値と誤差範囲は未設定のため、G-COSTはUNKNOWNを維持する。[測定結果と対象入力](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-shared-prefix.json)

2026-11-22に同じ6件で停止位置の予測を実測した。最初の1件を完全実行して回答長9を固定した。残り5件では、予測した長さで全traceを1回生成すると、回答・数値Readout・四Root・Ticketがすべて一致した。5件合計は停止位置を求めてからtraceを生成する基準実行が396.068秒、予測を使う実行が205.998秒（47.99%短縮）。最初の1件とモデル準備を両経路に算入した6件合計は、それぞれ506.340秒と316.270秒だった。同じ対象で長さ8を指定した対照ではtraceが拒否され、完全な再実行でTicketを再現するまで120.020秒かかった。

停止位置を調べるための追加実行を、正規Ticketの生成に必ず必要な仕事として計上しない。次の費用検査では、予測と再試行の両方を計上する。同じ仕事を反復できる場合の「2 + 2?」を回帰対象に残し、異なる仕事と停止位置での結果も仕事別に記録する。この5件は同じ回答を返した過去の標本で、一般の仕事における予測成功率と採択済み費用閾値は未確定である。QF1のG-COSTはUNKNOWNを維持する。[元のTicketと計時の照合](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-predicted-length.json)

別に、抽選前に固定した3問をそれぞれ新しいEXECとPANELで実行した。元の回答長は2（EOS）、2（EOS）、16（上限）であり、固定予測9は3件とも外れた。予測経路で失敗したtraceと完全な再試行を計上すると3件で368.101秒、基準の2回実行は244.377秒であった。全て元の四RootとTicketに一致した。同じ仕事を反復した5件と、異なる仕事の各1件とは費用判断を分ける。事前閾値と誤差の扱いが未採択のため、現世代は引き続きUNKNOWNである。[仕事別の費用と再試行](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-varied-jobs.json)

次のQF1費用検査では、基準実装が停止位置を求めるために何回推論したかを記録する。正規の停止条件を評価しながら同じ実行で四RootとTicketを生成できる経路も比較対象に含める。2回実行との時間差だけで、通常の1回の推論をどれだけ省略できたかは判定しない。安価と判定した経路は、その入力、停止条件、正規Ticketを修正後の回帰試験に残し、[RFC-0062 §4.3](0062-palw-function-output-and-work-admissibility.md#g-cost-review)に従って新しい入力でも再検査する。

2026-11-22、回答長を先に与えずに1回の推論で停止位置を確定し、保持した記録から正規Ticketを生成する経路を実測した。回答長9・2・2・16の4件すべてで署名済みOriginalと照合できた。モデル準備を除く4件合計で、同一バイナリの2回実行323.475秒に対し169.752秒（47.52%短縮）。今後このprofileでさらなる短絡を調べるときは、この1回推論の経路を基準比較に含め、記録保持のメモリも報告する。各条件1回の計時であり、費用閾値・誤差の扱いが未採択のためG-COSTはUNKNOWNを維持する。[仕事別の計時と元Ticket](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-online-stop.json)

QF1の修正後にG-COSTを再判定するときは、旧世代で閾値を下回った仕事・経路ごとに、元のTicketを再現した記録と修正後の同条件での測定を対応付ける。その上で、抽選前に固定した計画に従い、新しい入力でも1回推論を基準に比較する。判定記録には確認できた範囲と未検査の経路を明記する。[RFC-0062 §4.3](0062-palw-function-output-and-work-admissibility.md#g-cost-review)

## 10. MAS1登録・有用性・停止への接続

### 10.1 有限の自動検査

| 検査 | QF1で具体的に調べること | 合格だけでは示さないこと |
| --- | --- | --- |
| shape/lowering | 36層、QK norm位置、GQA、head/FFN形、同一weights、未対応op、range | denseだから高コストという定理 |
| input | 64 tokenの単射、実input root、切詰めなし、challengeが実attentionに入る | challengeを無視できないという定理 |
| StateMap | 36の第二残差＋final norm、正しい位置、全4096 lanes、順序 | 数値のentropy・非衝突性 |
| shortcut | zero/定数枝、採掘専用のunused重枝、消えるscale/gain、局所的相殺、既知low-rank/lookup/cache attack | 全未知算法の不存在 |
| Court | 全演算＋Readout＋Endの有界checker、最大case bytes/CPU、資料取得 | 永続検閲の解消 |
| G-USE | この整数・HEX64・greedy実行の回答と資源 | G-COSTの代替 |

省略されたlayer、readoutだけ別計算、ゼロscale等の**立証された違反**はFAIL。有効で安い短絡を発見した場合は該当WorkCostContractの不合格事例として採掘適格性を止める。単なる疎性・ランク・経験的速さだけから個々のMinerをslashしない。

### 10.2 実用性を保つ評価

PracticalSuiteは少なくとも、指示応答、QA/推論、日本語等の対象言語、実jobの長さ帯を含む。正確な公開dataset版、採点、正の閾値、適用タスク、被比較baseline、固定hardware/測定手順、最大latency・メモリ・Readout/trace overheadの許容値をMAS1 policyに**評価前に**設定する。未設定・空suiteはUNKNOWNであり通過させない。

上流BF16、QF1整数＋同じchat/greedy、QF1＋Readout/trace、の差を分けて測る。実用性として通過すべき対象は最後の実際に提供する構成である。Readoutを止めた測定だけを性能保証にしない。登録時校正資料をtestの全体に使って一般化を主張しない。

本RFCは測定値・架空の品質score・Qwen合格を提供しない。閾値未設定又は未実測なら一般登録状態を保ったまま非報酬評価とBFT risk reviewへ進む。市場作成を延期したり、停止中のtrialへ後から報酬を付けたりしない。

### 10.3 UNKNOWNからの経路

QF1 instanceのG-COLLの定理が未確認、又は[RFC-0062 §4.3の反復費用検査](0062-palw-function-output-and-work-admissibility.md#g-cost-review)が未完なら`REVIEW_HOLD`。MAS1のModelRiskCaseと限定BFT MODEL_STOP/RESOLVEへつなぐ。BFTは費用検査の欠落を多数決で埋めず、全必須PASS・正規停止解除・native inclusionなしに採掘を開始しない。自動検査で未判定のまま「まず報酬付きで動かして後からslash」はしない。

停止scopeはartifact/profile又はrule version。通常の固定prefix reuseや正しい速い算法を、過去の計算詐欺へ遡及再分類しない。既確定の決済、burn解除、BFTの台帳権限についてRFC0065を変更しない。

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
| QF1-09 eligibility | G-COLLの定理未確認、費用検査の未完・不合格、停止中、評価専用のclaimを全成功consumerで拒否 |
| QF1-10 end-to-end | 対話経路を無効または未実装とし、非報酬評価で元資料を取得してPanelが完全再実行する。一致時の承認、不一致時の直接反証と局所Court、資料欠落時の成功停止を個別に確認する。MAS1判定、同時停止・再起動/reorg/履歴復元も確認 |

同梱の小規模Python参照の対象は、QF1-04/05/07の論理codecの一部と否定例である。

2026-11-21の隔離・非報酬試験では、同一バイナリの3ピアでQF1-10の3ケースを個別に確認した。正常時は選出2席がQwen3-8Bの整数実行を再現して承認した。署名済みReadoutとEndが矛盾する場合は追加質問を経ずに局所裁定した。1席の資料が欠けた場合は承認が1件にとどまり、元期限で新規受付が終了した。資料義務違反の裁定、合意状態での未検証終了、Final・決済を含む一貫検証は残る。これらの試験によるG-COST判定は行わない。[正常時](../evidence/palw/mas1-qf1-native-2026-11-21/qf1-undisputed-replay.json)・[Endの直接裁定](../evidence/palw/mas1-qf1-native-2026-11-21/qf1-replay-deadline-refuted.json)・[資料不足と元期限](../evidence/palw/mas1-qf1-native-2026-11-21/qf1-missing-replay-material.json)

2026-11-22の隔離3ピアでは、元期限までの選択鎖を走査し、完全承認の格納先がない場合に未検証終了を記録した。各ピアはDBを開き直して同じ記録を検証した。期限内に承認が入った対照では、その格納ブロックで走査が終了を保留した。この経路では判別できないcommitmentがある場合も保留する。公開合意の状態遷移と資料義務違反の処理は残る。[未検証終了](../evidence/palw/mas1-qf1-native-2026-11-21/native-unverified-expiry.json)・[期限内承認の保護](../evidence/palw/mas1-qf1-native-2026-11-21/native-unverified-expiry-positive.json)

2026-11-22の隔離試験では、登録済みQwen3候補の非報酬MODEL_AUDITで、3 holderが元の監査closeに署名した。3件の公開行から復元したseedと、固定した1候補・2ケースのchallengeは、公開したpeerと、別々の新しいDBで再生した2 peerの結果に一致した。3 peerの受理後の状態Rootも一致した。ここで確認したのは監査の抽選までである。PracticalSuiteの採点、新規採掘資格、受賞は引き続き未確認である。[署名者・certificate・3 peerの照合結果](../evidence/palw/mas1-qf1-native-2026-11-21/fresh-audit-readback.json)

有効化までには、実artifactとtokenizer/変換/StateMapの作成、TIR lowererのstage schedule、Readout checker、Court/資源sizing、二実装、実用性、G-COLLの適合定理、G-COSTの反復費用検査、MAS1停止、native仕様・移行が全て必要である。これらが未完の間は**具体的設計済み・採掘未適格・公開未有効化**と表示する。

## 13. 一次資料と今回の確認範囲

- [Qwen3-8B公式config](https://huggingface.co/Qwen/Qwen3-8B/blob/main/config.json): architectureの形を2026-11-20に確認。main URLをruntime identityとして使う承認ではない。
- [Qwen3-8B公式model card](https://huggingface.co/Qwen/Qwen3-8B): thinking/non-thinkingと利用方法。QF1整数版の品質保証ではない。
- [Transformers v4.51.3 Qwen3実装](https://github.com/huggingface/transformers/blob/v4.51.3/src/transformers/models/qwen3/modeling_qwen3.py): QK norm、2回の残差、final norm、hidden_states APIの境界を確認。native数値意味は本RFCと固定TIRが所有する。
- [RFC7693](https://www.rfc-editor.org/rfc/rfc7693): BLAKE2bの規定算法。hashの性質だけからQwenの費用定理を導かない。

公式configの事実と、今回選んだA16・8K・HEX64・greedy・37-vector Readoutは区別する。後者はMISAKA向けの設計判断であり、上流Qwenが採用している採掘仕様ではない。
