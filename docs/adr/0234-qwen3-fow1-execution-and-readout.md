# ADR-0234 — Qwen3の整数実行と37-vector ReadoutをFOW1の具体profileとして採択する

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — 二経路モデル採用 MAD2

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](0235-model-admission-kernel-auto-and-bft-decision.md)／[適用・移行](../governance/model-admission-dual-route-transition.md)を、MAD2対象のモデル採用について優先する。KERNEL_AUTOは対応kernelへの自動分解と採択済み審査、BFT_DECISIONは証明提出を要求しない所定quorumのYES/NOである。

G-COLLの定理・G-COST/G-USEの証拠が未完成でも、共通の技術互換とq YESがあればBFT承認を記録できる。元のUNKNOWN/FAILを数学的PASSへ改変しない。未知kernel、壊れた実行仕様、個別claimの不正はYESで免除しない。Panel/Court、Final、独立Gate C、Ticket、一回会計、TB1、MOE2は維持する。

以下の旧「UNKNOWNは一律採掘不可」「BFTは停止専用」「全モデルに定理・費用審査を必須」とする条項は、旧profileの履歴及びMAD2のKERNEL_AUTO側として読む。BFT_DECISIONへ再適用しない。実投票・native実装・公開activationの完了を意味せず、既存QF1のREVIEW_HOLDを文書だけで解除しない。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

<!-- MAD2-ADOPTED-20261123-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 対応RFC: [RFC-0066 QF1](../rfc/0066-qwen3-fow1-execution-and-readout.md)
- 根拠: ユーザーの「Qwenの設計を想定してどうFOW1を行うのかRFCで具体的に記述する」という明示指示。
- 上位: [FOW1](../rfc/0062-palw-function-output-and-work-admissibility.md)／[MAS1](../rfc/0065-model-admission-screening-and-bft-stop.md)。経済・TB1・旧claimを変更しない。

## 背景

FOW1は同一の有用推論からコア採掘値zを得ることを要求するが、Qwenのどのlayer、token位置、数値、stopまでを読むかは未固定だった。モデル非保有Nodeが局所反証を裁くには、これらを一つの決定論的program、trace、元claimへ結合する必要がある。単にhidden_statesをhashするAPI呼出では、最終normの前後、prefill/decodeの位置、演算精度、EOS後の余剰実行が曖昧になる。

## 判断

1. **対象をQwen3-8B denseに絞る。** 36層、幅4096、FFN12288、Q/KV head32/8、head幅128、QK norm、untied output headを固定する。全Qwen系列・MoEへ自動一般化しない。
2. **PALW-TIRの整数A16を使う。** 既存整数primitive・C1 loweringを基礎に、全scale、rounding、gain、tableを登録artifactで固定する。上流BF16と同じ数値出力であるとは表示しない。未知のlibm/GPU reductionを同じprofileへ混ぜない。
3. **QF1専用HEX64 chat frameを使う。** 固定CとTB1 EXECから得たsを64個の固定hex tokenとして実入力へ入れる。これはAN1の35-token frameのQF1限定置換であり、旧claimを再解釈しない。通常の有用jobを同じdecoderへ続ける。
4. **non-thinking frameとgreedyを固定する。** 最小token IDで同点処理し、EOS又は事前生成上限で止める。Qwen公式samplingと同等品質という保証はせず、実構成をG-USEへ通す。
5. **37本のstate vectorを選ぶ。** 各回答tokenを決める位置で全36 blockの第二残差後と、final norm/gain/narrowing後を読む。各4096 i16。最初はprompt末尾、最後に選んだtokenの追加forwardはしない。
6. **実値を順番にstream hashする。** zはBLAKE2b-512の64 byte。試行ID・owner・nonceをzへ直付けせず、実数値の試行依存性とmetadataの非重複性を混同しない。hashを内部stateへ戻すXORや追加の重い採掘層を置かない。
7. **Readoutも局所反証対象にする。** 元TIR producerと数値bytesの対応、固定4096-byte hash chunkの連鎖、Stop/End、四Rootまで認証する。1 chunkは最大32 compressionであり、全readoutを一回のCourtへ渡さない。全bodyの検査はPanel＋既存TIR Courtで行う。
8. **四Root・Ticketの論理codecを一つにする。** 木形、field幅、順序、domain、empty/padding、target境界をRFC0066で固定する。native IDは別の実装gateで割り当てる。
9. **費用は反復検査、非衝突性はG-COLLの定理で評価する。** G-COLL/G-COSTはこの固定関数・入力分布・前処理/cache/batchを対象に必要。公式Qwen名、37 vector、実行一致、BFT署名を定理の代替にしない。
10. **MAS1の閉鎖を保つ。** 未対応・未検証・未証明は非報酬評価とUNKNOWN/BFT停止の経路へ接続し、仮採掘を許さない。モデル追加時のMOE2市場生成は独立して維持する。

## 選択しなかった構成

| 構成 | 本版で採らない理由 |
| --- | --- |
| 最終hidden stateだけ | どの計算へ依存するかを明確に調べるため、全blockの有用stateを指定する。ただし37本でも下界は未証明 |
| 全MACのrunning prefix | 巨大な書込み・hashを仕事根拠にして、実用推論との関係を弱めるため |
| BF16の未固定GPU出力をhash | backend差を算術不正にする危険があり、現在の整数TIRに暗黙移植できないため |
| nonce付きreceiptをコアzと呼ぶ | 数値状態が一定でもmetadataだけで重複を避けられ、FOW1の対象を変えるため |
| greedy以外の自由sampling | seed/選択法の自由度と再現性を増やすため。この版は一つに固定し、必要なら別profileで審査する |
| Qwenの知名度又はBFTで採掘PASS | G-COLL/G-COST/G-USEが未完でも通る入口になるため |

## 影響と限界

追加Readoutは1生成decisionにつき303104 byteの数値をhashする。T=1024で296 MiBの累積数値読出しとなり、trace・hash chunk記録・供給・Panel再実行は別費用である。実際のlatency、memory、品質を測定していない。QF1整数化又はHEX64/greedyで品質が落ちれば、G-USEで採掘未適格のままとする。

QF1は37 vectorを使った関数を具体化するが、状態のentropyや評価下界を証明したものではない。数値状態が同じなら同じTのzは同じである。未知の速い算法を漏れなく自動排除できるとも主張しない。BFT停止は危険の拡大を制限する別の条件付き手段であり、過去の不正を発見した証拠ではない。

## 移行・検証

旧RFC/ADRの歴史payloadと過去実測は保存する。現行FOW1/MAS1、TIR/登録/元claim/Panel/Courtの入口に本profileへの参照を追加する。20冊RFC-v2／50冊ADR-v2の構成を変えない。

native lowerer、hash chunk checker、実8B artifact、tokenizer binding、二実装差分、G-USE、反復費用検査、公開activationは別の完了gateである。同梱参照コードの合格は小さなcodec・位置規則の検査に限る。公開設定・配備・支払を変更しない。
