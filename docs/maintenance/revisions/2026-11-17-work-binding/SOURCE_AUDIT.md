# RFC-0058 改訂のソース監査 — work binding / ticket / 経済機会

## 0. 範囲と証拠の境界

- 監査日: 2026-11-17。
- 読み取り対象: `MISAKA-BTC/misakas` の commit `2d8212b634880b0967fbf24b00b41936c2322c14`。以下の GitHub リンクはすべてこの commit に固定する。
- 今回の追加入力は `docs(3).zip` と貼付テキストである。引用された15ページの「PALW: LLM Inference as Consensus Work（2026年11月12日版）」PDF原本は今回の提供資料に存在せず、原本のページ・数式・証明は未確認である。本監査はユーザー提示の監査文と、実際に読んだ固定ソースを区別する。
- 方式の前提は、規範的な整数演算によって実行結果が完全一致すること、モデル非保有Node、無審査での多数モデル登録を維持することとする。整数での決定性を、演算費用の下界の証明へ読み替えない。
- 今回はソースの読み取り監査であり、Nodeの実行、負荷測定、悪用コードの実行、mainnetの設定・activation確認を行っていない。既存ソース内のtestを引用する場合も、この監査がそのtestを実行したという意味ではない。
- `Confirmed` は掲載した関数・分岐・preimageをソース上で確認したこと、`Candidate` は複数の経路を組み合わせた攻撃候補、`Open obligation` は証明・実測・統合試験が必要な性質を表す。ソースコメントの主張そのものを安全性の証明にしない。
- 本書は改訂の根拠資料であり、RFC/ADRの採択、runtime実装、検証完了、公開有効化を宣言しない。既存のhistory、wire、activation fence、統合snapshotを変更しない。ZK等のモデル計算の暗号学的正当性証明は導入しない。

## 1. 総合判定

**Confirmed:** 現コードの `execution_commitment_v3` は、attemptの `challenge` だけをゼロ化し、それ以外の全unsigned fieldをhashする。`trace_root`、`output_root`、`execution_root` はこの入力に含まれる。したがって「実装のticketには出力もtraceもrootもなく、位置情報だけで有効な当選を先読みできる」という断定は訂正する必要がある。

**Open obligation:** rootがhashされることは、正しいrootを得るための実計算費用、資料可用性、完全な局所反証可能性、検証前のheader処理費用を保証しない。任意のroot値を安価に選んでticketを計算することと、そのrootへ正しい計算資料を後から開示できることも別である。

改訂は「欠落したrootをhashへ追加する」だけの変更として扱わず、既存のbindingを保った上で、Original固定、公開乱数の時点、経済機会の一回性、失敗・破棄・再試行、検証段階ごとの合意効果を閉じる。

| 監査命題 | 判定 | この監査で言える範囲 |
|---|---|---|
| 固定unsigned attemptのticket入力にtrace/output/execution rootが入る | Confirmed | §2の実関数で確認 |
| 同一anchor・同一実行のwithin-bucket nonce変更がclass ticketを増やさない | Confirmed | challengeをゼロ化し、署名を入力にしない |
| 任意rootを安価にhashできない | 成立しない | hash関数は任意の適切な型のbytesを計算できる。正当性は別の検査 |
| 有効なrootを得るには必ず指定回数の通常LLM推論が必要 | Open obligation | bindingの実装だけから計算下界は導けない |
| 既存コードにwork identityやfuture beaconがない | 不正確 | §6と§7の既存保護がある |
| 未検証headerが全fenceで同じDAA効果を持つ | 不正確 | §8のsingle lottery / clock等で異なる |
| 当選executionのheader variantによるbeacon選別 | Candidate | §9の依存経路を確認。end-to-end悪用は未実行 |
| k個のblock hashをfoldすれば偏りが一般にp^kへ下がる | 未証明 | last contributor / withholdingを含む追加条件が必要 |

## 2. C_attempt^0 とclass ticketの正確な依存関係

### 2.1 unsigned attempt全体が対象

`PalwAttemptUnsignedV2` は次の15 fieldsを持つ。

| fields | ticketにおける扱い | pinの主な根拠 |
|---|---|---|
| `version`, `network_domain` | hashされる | wire/networkの一致 |
| `challenge` | execution commitmentではゼロ化 | header positionへの一致を別途検査 |
| `class_id`, `executor_bond` | hashされ、anchorにも入る | 登録class/bondとjobの識別 |
| `executor_pubkey`, `operator_id`, `artifact_root` | hashされる | stateful equality |
| `trace_root`, `output_root`, `execution_root` | hashされる | replay / courtでの実行整合性。hashだけでは正当性を認証しない |
| `pwu` | hashされる | fenceに応じたcanonical derivation / class rule |
| `trace_manifest_root`, `trace_chunk_count`, `trace_retention_daa` | hashされる | DA形状、class/profile、DAAによる導出・一致 |

署名は `PalwAttemptEnvelopeV2` の別fieldであり、unsigned identityやexecution ticketの入力ではない。[構造体とrootの宣言](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_attempt_v2.rs#L351-L415)

`execution_commitment_v3` の実際の処理は次の通りである。

```text
view = clone(attempt)
view.challenge = zero64
bytes = Borsh(view)
E = BLAKE2b-512[key = execution_commitment_domain](
      execution_anchor || LE64(len(bytes)) || bytes)
t = LE128(first_16_bytes(
      BLAKE2b-512[key = class_ticket_domain](E)))
```

したがって、式の `C_attempt^0` を実装へ対応させるなら、`CanonicalBorsh(attempt[challenge := zero64])` である。位置challenge単体という読み方ではない。実際のdomainは `misaka-palw/attempt-v2/execution-commitment/v3` と `misaka-palw/attempt-v2/class-ticket/v3` である。[domain・commitment・ticket関数](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_attempt_v2.rs#L500-L587)

ここから導けるのは「これらのbytesが固定ならticketは一意」という性質である。次の性質は別の条件を必要とする。

1. 正規の実行に対応するrootが一意であること。
2. 許される別encoding・metadata・identityによって、一つの実行が別の経済機会へ化けないこと。
3. 有効なrootを得る費用が計上workに対応すること。
4. 偽rootの当選から実際のFinal・weight・credit等を得られないこと。

### 2.2 anchor・nonce・template

anchorの実preimageは `network_domain, pre_pow_hash, class_id, bond.transaction_id, bond.index, nonce_bucket` である。`nonce_bucket = nonce >> 22`。同じbucket内ではnonceを変えてもanchorは変わらず、challengeのゼロ化と合わせてclass ticketが増えない。[anchorの実装](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_attempt_v2.rs#L182-L234)

`pre_pow_hash_64` はnonceとtimestampをゼロ化する一方、parents、bits、DAA score、UTXO関連commitments等を含む。そのため、DAA scoreの変更を単に同じanchorの無料metadata変更として数えることもできない。[header preimage](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/hashing/header.rs#L52-L119) [pre_powの定義](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/hashing/header.rs#L208-L225)

bucket上限は `bucket <= 1 << 24` であり、許可個数は `2^24 + 1` である。コード自身がこれをhardeningと位置付け、templateや未検証root等の問題の根本修正と区別している。[上限とその制約](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_attempt_v2.rs#L237-L267)

「別anchorなので別job」という構造上の差は確認できる。しかし、別jobごとに必ず通常実装と同じ費用の推論が必要であることは、この構造だけからは導けない。

## 3. execution_rootが実際に拘束する内容

`PalwStepBindingV2` は `job_context`、shape/checkpoint profiles、state chunk map、full logits trace root、activation leg root、step leaf count/root、checkpoint count/root、`committed_execution_root` を持つ。[binding型](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_step_leg.rs#L2164-L2183)

`verify_binding` はcontext形状、宣言shape IDとprofileの一致、state chunk mapの一致を検査し、step/checkpoint rootsを再計算してcomposite rootと比較する。composite rootはjob context、full logits、activation、checkpoint、stepを結合する。[binding検証](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_step_leg.rs#L2274-L2329) [composite root](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_step_leg.rs#L1839-L1857)

job contextにはmodel/runtime/shape/trace/CU/tokenizerのidentities、prompt commitment、token counts、seed等が入り、明示的なcanonical encodingでhashされる。[contextとそのhash](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_v2.rs#L509-L527) [canonical context encoding](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_v2.rs#L626-L648)

**境界:** `verify_binding` の成功は、反証のbindingが同じcommitmentに属することを検査する。すべての演算が正しいこと、全資料が期限内に取得できること、init/step/endの反証完全性、Panelが実際に全計算をしたことまで、この関数だけで証明するものではない。全モデルfamilyの全経路に対する局所反証完全性を、本監査では証明していない。

## 4. admission順序と軽量受付gateの接続点

`check_palw_attempt_admission_full_v2_with_bootstrap` は、以下の順序を実行する。

1. `validate_stateless_v2`。
2. `validate_signature_v2`。
3. DA・retention・witness pins。
4. stateful admission: bond/class/work/budget/exposure/duplicate等。
5. anchorをheaderから導出。
6. native material ticket、またはclass lotteryを評価。

[composed admissionの実分岐](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_admission_v2.rs#L1142-L1186)

`pwu` のcanonical-work fence以後の唯一値はchainから導出される。DA pinsも単なる「fieldがhashに入る」という処置に留まらず、形状やretentionとの一致を検査する。witness/court capacity等のfenceによる枝があるため、旧one-chunkコメントだけを全設定の規則としない。[pwuの導出判定](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_admission_v2.rs#L523-L559) [DA pinsのfence分岐](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_admission_v2.rs#L988-L1040)

**改訂への含意:** 受付難易度を既存composed admissionの末尾へ追加するだけでは、それ以前の署名・stateful検査・native custody検査の到着負荷を制限できない。RFC-0055/0056/0057/0058の接続では、bounded framing、cheap ticket、重複抑制、計算資源予約、重い検証の順序を全入口で指定する必要がある。これは、すでに受理したclaimの異議・反証の保護された帯域を新規受付抽選へ戻すという意味ではない。

## 5. PAV3に既にあるcustodyとfree-field対策

runtimeでは、PAV3 carriageを検出すると通常admissionより前に `verify_prior_with_recovery_typed_v4` を実行し、その `class_ticket` を後段へ渡す。非対応fenceや除外BASE classではPAV3形式を拒否し、custody必須の場合の旧形式も拒否する。[runtimeのPAV3分岐](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/pipeline/virtual_processor/processor/palw.rs#L10924-L10955)

`priced_execution` はbase execution commitment、session、consent、provider roster、execution/trace roots、chunk countを結合する。provider署名、seal、測定bytes/contentを追加のticket entropyにしない。`verify_prior` はcanonical eligible sessionをstateから選び、session/consent/rosterの差替え、execution keyの差替えを拒否し、署名を検証する。[price projection](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_attempt_material_v3.rs#L188-L224) [prior session検証](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_attempt_material_v3.rs#L257-L303) [session選択](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_attempt_material_v3.rs#L306-L324)

この既存対策を「署名変更のたびに新ticketが取れる」という批判で無視してはいけない。一方、custody署名の存在をモデル計算全体の正当性・実費の証明へ昇格させてもいけない。新しいOriginal/Opportunityの規則とsession recoveryがどう対応するかは、明示的な移行・再試行規則が必要である。

## 6. 既存のwork identityと二重計上防止

### 6.1 attempt

`apply_attempt` はclaim IDの重複を拒否する。その上で、deep audit fence以後は同transition内の `seen_exec`、2026-09-23 audit fence以後はrooted `work_ids` を参照し、同じ `origin.execution_key` からの別claimを拒否する。後者はblockを跨ぐwithin-bucket nonce sibling対策である。[実際の三段階dedup](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2.rs#L23030-L23065)

したがって「claim IDを変更すれば常に同じ実行のweightを二重取得できる」は不正確である。ただし、既存keyのscope、claim retirement後の保持、reorgによる復元、新しいOriginal/Opportunityとの対応は別途明示する。

### 6.2 FP

FPの `fp_work_id_v1` は `(class_id, prompt_token_ids_hash, executor_bond)` である。`job_nonce`、decode length、sampler seedはkeyから外れ、同じpromptのprefixを多数claim化する過去の問題を防ぐ。stateful foldはlive `work_ids` との重複を拒否する。[FP work IDと理由](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_freeprompt_v3.rs#L632-L665) [FP foldの拒否](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L4285-L4298)

このkeyはbondを含み、live claimのscopeを持つ。これを、異なるbondを含む全世界・全期間で同一実計算が一度しか起こり得ないことの証明に読み替えない。

### 6.3 EXEC V2

`palw_exec_job_work_id_v1` は `(class, canonical_job_id, input_root)` をhashする。root declarationのoutput、cuts、extent、verification planやcarrierから独立したjob keyであり、foldはcurrent jobsとretained job usesを照合する。[EXEC job identity](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_work_slice_v2.rs#L212-L252) [current/history gate](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_exec_v2_fold.rs#L981-L989)

**改訂への含意:** WorkKeyの概念が存在しないとして全置換しない。新しいEconomicOpportunity/Original固定は、既存の各work keyをどう継承・正規化し、何が再試行になり、どのoutcomeをどこまで保持するかを指定する。

## 7. FP receiptのfuture beacon・maturity・一回消費

`receipt_items_1_to_5` は、claimの存在とFreePrompt種別、`Final`、必要なprofileでは `SecurityCertified`、未消費quantum、所定slotのbeacon、使用期限、ticketを順に検査する。slotはFinalまたは指定certification clockにmaturityを加えて求める。spendが自分で選んだbeaconを持ち込むとstate-derived factとの一致で拒否する。[receipt受理の順序](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_fp_admission_v3.rs#L187-L237)

`fp_quantum_ticket_v3` の入力は `network_domain, beacon_block, claim_id, quantum_index` である。spend nonceや署名は新ticketを作らない。最初の16 bytesをbig-endian u128にする点はattemptのlittle-endian ticketと異なり、後続RFCでwireの暗黙変更をしてはならない。[FP ticket・maturity・use window](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_freeprompt_v3.rs#L1226-L1250)

candidateのselected-parent chainからbeaconを導出する既存処理もある。新RFCは「未来beaconがこれまでなかった」と記述せず、既存の時間順序・scope・maturityを保った上で、乱数の予測不能性・偏り・再試行の不足を補う。[candidate chain由来のbeacon](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/pipeline/virtual_processor/processor/palw.rs#L11360-L11410)

## 8. 未admit header、難易度row、fence別の限界

ソースの `an_unadmitted_attempt_header_is_still_a_priced_difficulty_row` は、任意のtrace/output/execution roots、pwu、bucket、templateが安価な候補空間になり得るというC-1の制約を記録している。これは「それらのbytesを変更すればhashが変わる」ことを確認するtestを含むが、live networkの悪用を実行した証拠としては扱わない。[C-1の記録とtest](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_attempt_v2.rs#L1186-L1263)

header段階では、carrier形状、署名、DA pins、bucket ceiling等を検査する。一方、bond/class等のstateful admissionは後段にあり、通常の拒否は `StatusDisqualifiedFromChain` になる。BASE recovery等の別枝もある。[header段階の実検査](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs#L461-L538) [chain admissionと拒否](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/pipeline/virtual_processor/processor.rs#L3081-L3125)

ただし、旧C-1コメントを全fenceへ一般化してはいけない。single lottery有効後の `algo_id_is_priced_by_bits_v3` はattemptもbits-priced rowから除外する。`palw_lane_advances_daa_v1` はanchor clock、single lottery、receipt rowsのfenceで分岐し、difficulty managerにはさらにclock cursor、floor、real clock tickの設定がある。[priced-rowのv3](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/pow_layer0.rs#L323-L334) [DAA分岐](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/processes/difficulty.rs#L743-L763) [clock関連の追加設定](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/processes/difficulty.rs#L206-L233)

**改訂への含意:** 以下を別々に定義・検査する。

| 効果 | 必要な設計の区別 |
|---|---|
| header受付、保管、relay、signature CPU | 原本計算の正当性確定前にも発生し得る有限資源 |
| bits retarget用row | laneとfenceを解決して算入 |
| DAA / clock tick | retarget用rowと同一とは限らない |
| Provisional/Final weight | 対象claimのphaseと別規則 |
| EXEC credit / receipt / reward | 一回消費、maturity、certification、取消規則 |

「Finalにしない」だけで、前段の有限資源を保護できたとはしない。逆に、未admit headerの存在だけから報酬・credit・Final weightまで得られると断定しない。

## 9. 追加監査候補: header variantによる未来beacon選別

### 9.1 ソース上確認した組合せ

1. attemptのclass ticketはchallengeを除外し、同bucket内のnonce/time位置変更を新drawにしない（§2）。
2. header identityのhashは実nonce・timestampを含む。unsigned attemptのsignatureはidentityとは別でも、header carrierの表現に関する検査は別に存在する。
3. receipt beaconのpipelineはchain上のfull `block` IDを渡す。`FirstKDescending::fold` はk=1ならそのID、k>1ならそのblock ID列をhashする。[block IDを供給するpipeline](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/pipeline/virtual_processor/processor/palw.rs#L11399-L11410) [IDのfold](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_fp_beacon_v3.rs#L105-L126)
4. single lottery有効時、attemptのLayer-0 digestは計算されるがbitsとの比較なしで通過する。[実際の分岐](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/pow/src/lib.rs#L586-L613)

### 9.2 Candidateの内容

当選済みの正しいexecutionを持つproducerが、許可されるnonce/time/正規署名付きheader表現を複数生成し、future receipt ticketへ有利なblock hashを選んでから一つだけ公開できるかを検証する必要がある。既存work dedupが複数claimの採用を拒否しても、公開前に候補を比較して一つだけ選ぶ行為が自動的に防げるわけではない。

この経路は、同じclass ticketから複数の採掘報酬を直ちに得られるという主張ではない。future beaconの偏りと、その経済効果を別の攻撃対象としている。timestampの合法範囲、各headerの署名費用、native session binding、header/admission/chain選択の全条件、reorg・withholding等を組み込んだend-to-end試験は未実施である。

ソースコメントにある「block hashはexecution commitmentの下流だから、beaconの再抽選は毎回一推論を要する」という文だけではこの候補を排除できない。bindingとfresh costを同一視しない。[該当コメント](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_fp_beacon_v3.rs#L1-L22)

### 9.3 k個foldとp^kの留保

上記コメントはk個のattempt blockをfoldすると全kを支配しなければ操作できず、確率が `p^k` になると記述する。しかし、既知の先行寄与に対する最終寄与者の候補選別・withholdingを認めるモデルでは、全kを支配しなくても分布に偏りを与え得る。単なるhash foldを偏りの一般的証明にしない。

RFC-0058で乱数契約を新たに定義する場合、対象集合の固定、公開前の予測不能性、一意な結果、公開時のwithholding、遅延・再試行、header表現からの独立、停止時の処理を別々に規定する。BFTの合意成立、Panel中の正直者の存在、乱数の無偏性は互いに代用しない。

## 10. RFCへ反映する監査対応表

| 問題 | 既存機構を維持する点 | 改訂で必要な契約・検証 |
|---|---|---|
| ticket inputの曖昧さ | full unsigned attemptとroot binding | `C_attempt^0`、wire、domain、byte orderを明記 |
| 原本固定前の結果選別 | attempt/FPの署名・commitment | Original固定とticket乱数公開の厳密な前後関係 |
| 同じ仕事の再抽選 | attempt、FP、EXECのwork IDs | EconomicOpportunityとの一意対応、losing/expired/aborted outcome、保持・reorg |
| rootの差替え | step bindingとMerkle authentication | 正規encoding、init/step/end、profile/rulesetとの全経路照合 |
| 偽rootの安価な候補化 | header pins、後段admission | pre-gateと予約、検証前の各合意効果の制限 |
| 大量claimによる前段負荷 | 既存budget/court capacity | cheap gateより前後のCPU/bytes/state上限、全入口の迂回禁止 |
| Courtのコピーと再提出 | claim/scope/署名・root binding | Panel challengeとsemantic dedup。ただし独立全再計算を証明した扱いにしない |
| beaconの候補選別 | candidate chain-derived beaconとmaturity | header variant、last contributor、withholding、乱数再試行の受入試験 |
| 高速化・償却・退化モデル | 正確な整数結果の検証 | 最適化された費用とcredited workの評価。普遍的下界がなければ未証明と記録 |
| Final後の反証とDAG | branch-scoped state、既存一回会計 | 安全な確定境界、credit消費、取消、weight、reorgの別定理・試験 |

本書により閉じるのは「現コードを誤読してroot欠落と断定すること」である。PoWとしての計算下界、全モデルの非償却性、敵対的ネットワークでの全case終結、DAG全体の安全性は、この読み取り監査で証明済みにならない。

## 11. 実施した確認と未実施の確認

実施したのは、固定snapshotの型・hash preimage・domain・byte order・branch・call order・dedup key・beacon導出の読み取りである。引用した各関数について、コメントだけでなく実際のfield access、比較、hash入力、return branchを確認した。

未実施なのは、runtimeへのパッチ、Node統合試験、未認証payload洪水、悪意あるsigned header洪水、PAV3 custodyとの組合せ、header variantからのreceipt beacon選別、性能測定、任意モデルに対する計算下界の証明、白書PDF原本の査読である。後続のcontract modelや文書整合性checkが通っても、これらの実行結果として報告しない。
