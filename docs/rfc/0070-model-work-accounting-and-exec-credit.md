# RFC-0070 — MWA1：モデル別仕事量認定・FOW1報酬・EXEC credit

- 日付: 2026-11-26（Asia/Tokyo）
- 状態: **PROPOSED_DESIGN_NOT_ACTIVATED**。ユーザー指示によるRFC追加。共通仕事量認定の方向を記録し、具体的な価格式・パラメーター・移行はレビュー／採択／実装／検証を経る。
- 判断案: [ADR-0238](../adr/0238-model-work-accounting-and-exec-credit.md)
- 実装仕様案: [MWA1 implementation](../design/palw/economics/mwa1-implementation.md)
- 基準: 提供された `LLM-chain-MOE3-20261126(1).zip`（以下「基準tree」）。未取得branchの状態を主張しない。
- 継承: [FOW1 RFC0062](0062-palw-function-output-and-work-admissibility.md)、[TB1 RFC0061](0061-palw-internal-threshold-beacon.md)、[QF1 RFC0066](0066-qwen3-fow1-execution-and-readout.md)、[MAD2 RFC0067](0067-model-admission-kernel-auto-and-bft-decision.md)、[MOE3 RFC0068](0068-fractional-position-miner-rewards.md)、[BMR1 RFC0069](0069-bft-model-retirement-and-role-bonds.md)、[EXEC RFC0048](0048-exec-credit-schedule-and-one-bps.md)、[共通carrier RFC0055](0055-global-admission-court-calendar-and-one-bps.md)。

## 0. 目的と採用範囲

一回のpromptや一回の出力を同額にしない。小型Denseと大型Dense、MoEでは実行される演算、文脈長、出力長が異なる。全モデルに同じ規範仕事量単位を適用し、同じ適格性・価格版・資本制約下で、新規に認定された仕事量に比例する期待報酬と将来EXEC生成機会を与える。モデル登録／採用時に経済計算を可能にする仕事量profileを固定する。モデル名・総parameter数・実測秒数・leaf数・物理GPU数を直接報酬倍率にしない。

**本書はMOE3を基準とする。** 全モデル（元genesisを含む）係数1.0、一意市場、小数8桁POSITION、共通MAD2管理、BMR1恒久廃止を継承する。旧core直接MISAKA報酬、元genesis BFT免除、fork特別区分、追加model係数0.3、MOE2の30/20/50を復活させない。genesisのcreator不在と通常売買feeの例外もそのままとする。既存市場のresetは禁止。

モデル採否、仕事量の経済的認定、実際の計算の正当性、採掘適格性、台帳の決済確定は別の判定である。BFTの承認を計算の証明にしない。暗号学的なモデル計算証明は一切導入しない（[PRINCIPLES](../PRINCIPLES.md)）。Panel、Court、独立Gate Cを維持する。QF1 REVIEW_HOLD、実装されていないkernel、既存安全停止をこのRFCで解除しない。

## 1. 現行コードの再利用と差分

| 基準treeのコード | 現状の責務 | MWA1の差分 |
|---|---|---|
| `consensus/core/src/palw_canonical_work_v1.rs` | descriptor・execution factsからwork vectorを導く | FOW1用facts、版付きmeter、origin dedupを接続 |
| `consensus/core/src/palw_tir_work_v1.rs` | TIR programの構造から分類・shapeを導く。active expert・history軸・Select armを扱う | QF1 stageおよびcanonical分岐に合うmeter、実行長と資源上界を分離 |
| `consensus/core/src/palw_economic_compute_v1.rs` | 旧graphのMAC相当cost table、較正・shadow経済集計 | 規範価格版の共通係数を導出する候補。shadow実装そのものを新合意へ直結しない |
| `consensus/core/src/palw_model_registry_v1.rs` | `PalwModelWorkV1`（verification_ccu、economic_ccu_per_claim、artifact_bytes、working_set_bytes、ops_supported） | 新版ModelWorkProfileを追加。旧fieldの意味は変更しない |
| `consensus/core/src/palw_execution_quanta_v1.rs` | Attempt Finalから有限execution quantum、未来round、一回消費 | FOW1のcanonical work key・成熟・共通scheduleへ接続。旧単位・IDをそのまま再利用しない |
| `consensus/core/src/palw_work_target_v1.rs` | 旧compute laneのtarget・経済境界 | 旧targetやFinal後抽選をFOW1へ混入させない。共通事前snapshotと予算診断を実装 |
| `consensus/core/src/palw_state_v2/objects.rs` | class登録、構造からwork算出 | 登録時estimate、採用generation、native stateとの認証を追加 |

`palw_tir_model_work_v2` は登録された標準jobのverificationと旧Attempt drawを別に算出し、artifact bytesをworking set推定にも用いている。この標準job値はFOW1の実promptの計算量ではなく、実機peak RAMでもない。旧CCU値をFOW1の一回の自由生成へそのまま請求しない。実装状態は静的調査であり、実行成功・性能・native接続を証明するものではない。

## 2. ModelWorkProfile と登録・採用

### 2.1 不変の仕事量契約

`ModelWorkProfileV1` は論理schema名。新wire ID、domain bytes、state table番号は別途割当する。全fieldは固定精度・canonical codecで署名／認証し、未知版・余剰field・過大長を拒否する。

| 項目 | 内容 |
|---|---|
| identity | network/genesis、model ID、artifact root、tokenizer root、execution descriptor hash、kernel/rule/Readout版 |
| meter | common meter ID、cost table digest、graph/shape digest、stage map、branch/expert規則、数値型、prefix origin規則 |
| reference jobs | 共通token geometryと入力fixture、実行mode、reference work vector、各実行の上界、測定可能な場合の再現記録 |
| limits | 最大input/output/context、許容expert/branch、最大新規仕事量、最大検証仕事量、資料・trace・メモリ・I/O・carrier上界 |
| calibration | hardware別測定bundle digest又は明示的UNKNOWN、測定環境、対象job、精度と範囲（非合意の運用資料） |
| version | profile generation、適用epoch、前profile ID、共通policy ID、採用根拠、署名対象hash |

登録時にNodeが認証済みdescriptorから構造、共通meter、各標準jobの推定値および上界を導出する。標準jobの入力長・出力長は評価suiteで指定し、異なるtokenizerには同一token geometry試験と同一テキスト試験を分ける。無理な長さ・EOS制限を本番profileへ導入しない。標準jobが実行できない場合はUnsupported/NotApplicableを明記し、比較母集団を偽らない。変換の意味・検証可能性が不明であれば技術互換を満たさない。

### 2.2 MAD2への接続

1. immutable model登録とMOE3市場原子生成を維持する。市場が存在しても採掘資格は生じない。
2. 共通meterが対応する有限graphからModelWorkProfileを導出し、宣言値と一致するかNodeが検査する。未知kernel/数値規則、上界不明、虚偽shapeは両経路で拒否する。
3. KERNEL_AUTOは既存MAD2のG-EXEC/G-COLL/G-COST/G-USEと共通技術条件を維持し、同じmeterを使う。
4. BFT_DECISIONはdescriptor、WorkProfileId、policy、admission generation、roster、epoch、期限を同じproposalへ束縛し、既存quorumでYES/NOを判断する。BFTは**計算量を事前推定して確認するが、任意の倍率・単価・宣言CCUを設定できない**。Nodeは同じ導出値を検査する。測定がUNKNOWNでも既存MAD2のBFT採用判断を許す（数学的定理・実機費用証拠を新たな必須条件にしない）。構造から得る規範仕事量と安全上界は必須であり、未知kernelをYESで許可しない。
5. 既存modelは認証された移行／新profile生成で同じ規則へ移す。元genesisに対してもMOE3の共通管理を維持し、creatorを捏造しない。

BFTが既存のG-COLL/G-COST/G-USE結果をPASSへ書き換えること、登録時の測定値だけで個別claimを承認すること、model/policy/価格版を票の後で差し替えることを禁止する。停止・再審・廃止はBMR1の独立経路で処理する。

## 3. CanonicalWork meter

### 3.1 三種類の尺度

- `W_econ`: 認定された新規の規範仕事量（CCU整数）。採掘報酬とEXEC creditの共通基準。
- `W_verify_upper`: 適格な検証・再実行の保守的上界。Panel/Gate Cの役務予約・受付可否に使い、Minerの仕事へ加算しない。
- `ResourceVector`: Nodeの局所Court検査、material、hash、署名、帯域、RAM/VRAM、disk、live stock、carrierを別次元で予約する。W_econが小さくても資源負担が大きいjobを無条件受付しない。

`W_econ` は物理的な実行時間・電力や普遍的な計算費用下界ではない。共通の算術MAC-equivalentで価格を付ける規範量であり、正規計算を同じ結果で高速化することは許容する。FOW1のG-COST検査は別契約として維持する。

### 3.2 位置、演算、文脈

規範graphとFOW1 instanceから、実際に成立したcanonical positionとstageの集合 `S(c)` を導出する。QF1で入力P、生成Dなら、原則処理位置は0..P+D-2（P+D-1個）、最後のprefill位置から最初の予測を行い、最後の生成tokenをもう一度評価したとみなさない。LM head/Readoutの実施位置はQF1の規定stage（PrefillBody/DecodeDecision）で決まる。D=1、P=1、EOS、生成上限、context境界を試験する。他profileにQF1のoff-by-one規則を押し付けず、そのstage mapを使う。

各位置で次のvectorを計算する。既存TIR parser/shape plannerの分類を再利用し、graphの論理nodeが複数の物理kernelに展開されても一回だけ計上する。

| 次元 | 計算例（正確な式は共通meter版） |
|---|---|
| dense_matmul | 入出力dimensionの積、batch/position、正規dtype係数。router/shared expert/LM headを漏れなく含む |
| routed_expert_matmul | 正規TopK/Gatherで選んだactive expertのshapeの和。全expertの総parameterではない |
| attention_prefill / decode | 正規のquery heads × head dimension × 実有効KV範囲（QKとAV）、GQA、sliding window、mask等を規範どおり |
| recurrence | Gated DeltaNet等の正規状態更新と読み出し |
| normalization / other_verified_ops | versioned cost table、graph構造に基づくnorm、activation、RoPE、routing、reduction等 |
| weight_traffic_bytes / kv_read_bytes / kv_write_bytes | 正規論理量を独立記録。実際のSSD/GPU転送量を新たな採掘仕事として足さない |

MWA1の初版は既存 `arithmetic_mac_eq` と同じく算術7次元を共通scalarにする。既存表のdtypeによる価格付けを含む版は、その意味を明記する。同じweight byteをもう一度加算して二重価格にしない。memory trafficを新たに価格化する場合は全モデル共通の公開cost table改定と資源・公平性の再評価を要し、BFT個別票やbackendの実測値で変更しない。

```
V_new(c) = sum_{k in canonical newly eligible logical operations} MeterCost_v(k)
W(c)     = checked_sum(V_new.arithmetic_dimensions)
0 < W(c) <= W_max(reserved job)
```

Selectの両armを実行したとして二重に請求しない。canonical branchが検証された場合はそのbranchを用い、分岐に依存する計算量が完全には認証できないなら共通meterの安全な下側（正規全branchの最小）を使う版を明示するか、対応不能として拒否する。受付用上界には最大branchを使う。後から値が大きいarmを選び直せない。MoE expert ID、top-k、tie-break、重複ID、実行shapeは正規trace・kernelに拘束する。

### 3.3 新規性、再利用、計算水増し

prefix/KV/resultが別の採掘仕事として既に認定されている場合、認証済みorigin/依存範囲を特定し、その範囲を `V_new` から除く。実行入力frame、モデル、状態、position、rule、生成済み範囲が合わない再利用は拒否する。申告だけでcache missや新規仕事とは認定しない。ローカルのキャッシュヒット回数、同じ仕事の別包装、再署名、Panel再実行、trace再生、Merkle hash、padding、未使用branch、不要な繰り返し計算、FOW1採掘専用枝を加点しない。

FOW1の入力拘束・G-COST・一回性を維持する。合法的なbatch、GPU fusion、weight共有、実際の高速化を物理演算回数によってslashしない。検出不能な隠れcacheや新しい等価アルゴリズムを完全に識別できると主張しない。経済的に安価な正規Ticket経路の発見は採用/評価版を見直す理由になり得るが、個別の算術不正の証拠とは別である。

### 3.4 Nodeをmodel-freeに保つ

登録時に有限graph/shapeを解析して不変の集計表を認証し、線形・区分和のclosed formを使う。claim受付時はbounded execution facts、token数、Stop、prefix origin、正規stage/branch集計のcommitmentを読む。Nodeはモデル重み全量取得やLLM完全再実行を行わない。元claimに束縛されたwork factsについてPanel/Gate Cが独立再実行で照合し、個別positionのshape・branch・origin・count・合計不一致を既存Courtの有界局所反証で裁けるようにする。分解の完全性が未接続なら成功させない。

受付時は `W_est` と `W_max` のみで、成功Final・Gate Cと正規Ticketを経るまで経済creditに使う `W_cert` を確定しない。Producerの実測値、RPC estimate、登録者の自己申告は合意値でない。署名済みfactsだけで計算が正しいとみなさない。

## 4. FOW1の一試行一Ticketと公平な報酬

### 4.1 初版の方式B（共通target、可変報酬原資）

標準job固定なら事前targetを仕事量比例にする方式Aも理論上可能だが、本RFCは自由な生成長とFOW1の事前固定targetに整合させるため、初版は**全モデル共通の事前target、認定仕事量比例の当選原資**を提案する。方式Aや可変targetは別profileのレビュー対象とし、本版へ混在させない。

FOW1の256bit Ticket `T=H_FOW1-ticket(Enc_P(s,R_x,R_y,R_z,R_tau))`、正規TB1、四Root、`T < theta` を変えない。epochの `theta` は0..2^256、試行固定前に確定する。`theta=0` は新規経済試行停止、除算による報酬quoteを発行しない。`theta=2^256` は全当選の境界として厳密に扱う。新規FOW1へ旧128bit targetやquantum複数draw、Final後の再抽選、自由saltを導入しない。

`k_e=a_e/b_e`（MISAKA最小単位/CCU、a>=0,b>0）は**当選時の**共通価格。受付snapshotで固定し、モデル別にBFTが変更できない。予算sourceの割当は事前の共通発行schedule・model cap・claim cap・bond/責任上限に従う。

```
R_quote(W) = floor(a_e * W / b_e)
R_max      = R_quote(W_max)
R_actual   = R_quote(W_cert)   // 正規当選・Final・Gate C・成熟その他全gate成立後
```

`R_actual` はMOE3の配分対象原資Cのうち当該仕事へ予約された総額であり、Minerの直接MISAKA受取ではない。受付時に `R_max` を全額確保できなければ新規試行を許さない（又は試行前に小さいjob envelopeを新規予約する）。事後的な倍率／cap clipping／勝者間の減額で公平性を変更しない。実行長に応じた未使用予約の解放は、同一sourceの期限・終端に従う。`R_quote=0` は新たなmintがない場合として明示し、resourceや既存の検証義務を免除しない。

理想化された古典ROにおいて、固定した有効仕事Wと同じeligible・資金・認証条件なら `p=theta/2^256`、`E[C_i | W]=p floor(k_e W)`。床による誤差は一試行の期待値でp未満の最小通貨単位。従って10倍のWは、丸めとcapに阻まれない範囲で10倍の期待原資。実行後の条件、申請選別、認証率、モデルcap、費用、market liquidity、POSITION売却価格は異なり得るので、同じ実費利益や換金額の保証ではない。winnerを見て試行のWやStopを選び直さない。同じ試行の比較対象や失敗率の母集団を分けて測る。

`theta` と `k_e` は、共通発行計画・適格仕事供給・正規受付/終了・認証・資源制限・保留責任を用いたepoch policyから決定する。初期実装では、activationに結合された共通固定 `theta,k_e` と資源予約による受付制御を用い、動的retargetを自動的に開放しない。後続の動的更新を採用する場合、具体的な決定論的更新則・上限・初期値はactivation前の較正と承認を要する。観測された当選claimの仕事だけを全試行仕事の実測として扱わない。`rho_e=p*k_e` を監視する。pが小さいだけで無条件にk_eを逆数倍にせず、許容報酬上限・担保・予算内で整合しなければ受付を制限する。旧経済floorが効いたまま無意味なDAA待ちを続けない。空epoch、供給0、認証停滞、cap枯渇は理由コードを返し、無限単価・上限なしtarget・過去snapshot変更を禁止する。

### 4.2 予算とmodel別資本上限

MOE3 §2.1の `c_m=1`、7日相当平均bondの既存 `A_m=mean_bond_m^(3/2)`、共通正規化、資本上限・成熟・liability hold・export capを保持する。共通計算量meterに `A_m` を掛けてさらに報酬倍率にしない。

- グローバル発行scheduleが定める各source/epochの総上限と、その内側のmodel別認証済みcapを予約台帳に固定。
- `reserved + consumed + closed_unissued <= authorized` を全source・model・bondのscopeで検査する。検証役務、BFT、市場原資、旧pending、責任予約を二重使用しない。
- Wに応じたR_maxと別に、ADR-0176等の**実際の支払可能額**・担保予約・利用可能枠をネイティブ関数で事前検算する。資本上限不足はR_max不足として受付前に拒否。claimの宣言だけで予算を予約したとみなさない。
- model shareは経済機会の上限であり、仕事がないmodelへの無条件mintを生まない。未使用枠の戻し/失効は元sourceの有限規則に従う。winnerを見て枠を取り直すことを禁止。

### 4.3 MOE3のsourceと結合按分

共通source単位を事前policyで固定する。利用者の同一仕事の別包装、任意のsource分割、再送で丸めや費用の再計算を誘発しない。真に異なる適格試行のfloor残差は異なり得るため、これを完全な分割不変性とは主張しない。MWA1は認定仕事量に基づく予約を、同じsourceの正規支払い対象額へ結合する。具体的には、事前固定されたsource closeの正規対象仕事の `R_actual` をmodel別 `r_i` として合算し、`C=sum(r_i)` をそのsourceの許可予算内に閉じる。MOE3 §4.3の認証済みmodel配分重みとして、この新版では **実際にそのsourceへ認められた仕事の予約消費額 `r_i`** を使う案とする。これは従来の資本shareを二重乗算することではなく、前段のmodel capを維持したまま、後段の支払対象を決める限定改定である。

全leg按分後の実額がmodel/bond責任cap内となることを丸め余裕込みで予約・検算する。r_i=0はno-op。capに入らないsourceを成功として部分mintせず、規定の有限未成立/失効へ進める。独立に成立済みP/F役務権は市場失敗と分離する。sourceの確定時点、close期限、quote鮮度、原期限は先に固定し、同じ仕事を複数sourceへ持ち越してmintしない。評価版を混ぜない。

## 5. MOE3・BMR1への会計接続

対象Cを、既存RFC0068の同一分母で一度だけ分割する（全モデル共通）。新しい配分率を追加しない。

```
F = floor(50*C/1000)      // BFT 5%, MISAKA
P = floor(50*C/1000)      // Panel + Gate C 5%, MISAKA
A = floor(200*C/1000)     // LP MISAKA
J = floor(200*C/1000)     // LP POSITION購入原価
D = C - F - P - A - J    // Miner購入原価50% + 端数
B = D + J                // 一括購入原価70%
```

複数modelはRFC0068 §4.3の結合按分を**そのまま**使う：`E=D-2J`、`J_i=apportion(J,r)`、`A_i=J_i`、`E_i=apportion(E,r)`、`D_i=2J_i+E_i`。最大剰余、canonical model IDでtie-break。`B_i=D_i+J_i`。各modelで5%を再控除しない。丸めにより各modelの個別配分が名目50/20/20と完全一致するとは限らない。`C`は全sourceの上限内、P/F合計は共通原資で一回。

市場はRFC0068の `X=V+r`、`q=floor(B_i U/(X+B_i))`、`L=floor(q J_i/B_i)`、`M=q-L`、`r'=r+B_i+A_i`、`U'=U-M`、holder `H'=H+M` を使用する。V・seedの既存制約、POSITION atoms、小額dust、quote、期限、一回消費、checked広幅整数を維持。名目2/7は実購入予算比と一致する場合だけ。旧MOE2のreserve比LPや余剰burn、外部LP引出し、virtual reserve換金を追加しない。MをMiner側に配る際は同model内の認証済み採掘配分plan・責任hold・成熟に従う。市場失敗に伴うP/Fの横取り禁止、役務費用不足は別途診断・受付制御する。5%枠がGate C費用を賄えることは実測前に保証しない。

BMR1の個別Court slashは実回収額Sの累積に対する `floor(490*S/1000)` の差分を有効なPanel/Gate C発見・立証者へ一度、残差をburn。BFT恒久廃止の100%burn/失効と混ぜない。全検証bondはBFT廃止から保護し、個別責任は維持。恒久廃止は対象marketの現存real reserveと用途別lot等に限定する。新規未発行の取消と既発行資産burnを二重計数しない。

## 6. EXEC credit（1 BPSと10分確定目標）

### 6.1 仕事から生成権へ

本版では**同じFOW1正規当選・成功Final・独立Gate C・成熟・依存・一回会計・現在の適格性を満たす仕事**から有限creditを発行する。落選仕事を別入口で合意credit化しない。Ticketは一枚のみ、EXEC quantumは当選抽選券ではなく既に認定された生成権の数量である。Coinbaseの予約・支払いとEXECの発行は別purposeの一回性を持ち、同じ仕事を新しい仕事として重ねない。

共通 `Q_exec>0`（CCU/生成権）と上限を試行前のprofileに固定し、 `n=floor(W_cert/Q_exec)`、`r=W_cert mod Q_exec` とする。端数はRFC0048のfreeze→独立TB1 SCHEDULE seedの規範で `Pr(extra=1)=r/Q_exec` とする方式を提案する。実装は有限かつ偏りのない範囲サンプリング（固定widthの rejection sampling、試行上限と未成立時の保留/失効を規定）または等価な厳密整数規則を使う。旧 `lead_u64 % quantum` を任意u128範囲へ無条件流用しない。上限での失敗・丸め誤差を報告し、別seedによる引き直しを禁止。`n+extra` の最大値が予約上限に収まらないなら試行前に拒否する。候補集合・W・権利ownerはseed以前に固定し、生成権不足を無認証発行で埋めない。

`WorkKey / CreditPurpose / CreditIndex` をcanonical identityへ束縛し、旧family/alias/再署名/別modelで再発行しない。creditは予約、成熟、schedule割当、消費、期限切れ、取消の状態を持つ。scheduleのauthority、署名journal、canonical undo、公開済みTB1曝露状態を区別する。モデル廃止/有罪時は既存取消・保全規則に従い、未消費の権利と依存状態を処理する。既に確定した無関係履歴を勝手に巻き戻さない。

条件付きの理想期待発行数は `p*W/Q_exec`（cap、成熟、予約、端数処理失敗前）。credit在庫と実際に生成するブロック数は異なる。共通slot数や保護枠の不足で未割当・失効が起き得る。大型modelに毎秒複数生成を約束しない。固定されたmodel資本上限・共通公平scheduleの中で供給に応じた機会を与える。

### 6.2 共通carrierの1 BPS

RFC0055の全kind共通global 1000ms slot、最大1正規外側carrier/slotを維持する。10分区間には名目最大600枠であり、Court・認証・資料・制御・bootstrap・復旧の保護枠があるため600通常EXEC_TXを常に保証しない。EXEC_TXとEXEC_SLICEを混ぜない。scheduleと生成権の事前公開・署名・閉鎖・消費、既存の正規carrier取り込みを維持。claim/反証/Gate C/新scheduleは既発行creditだけに依存させず、循環deadlockを作らない。

### 6.3 600秒の台帳確定目標

台帳のsafe/settled frontier進行を通常健全時の**平均600秒の設計目標**とする。これは個別claimのFinal期限・Gate C期限・TB1 round・DAA・120秒のRFC0055計画/difficulty epoch・1秒slotを一括変更するものではない。既存の合意安全条件、保護期間、異議、成熟が成立した範囲だけ確定する。600秒で強制Final、空の証明、未認証weight、依存のある未確定UTXOの確定を認めない。実際のfinality latencyとfrontier進行間隔を別々に測り、達成不能なら原因と性能目標の再設計を報告する。BFTへ台帳finalityやfork choiceの権限を与えない。

EXEC生成数からcoinbase発行を独立させる。既存発行scheduleのsourceと認証済みcredit/報酬予算を使い、heartbeatや空EXECを発行源にしない。DAAと実時間の変換は既存ClockBridge、canonical clockに従う。短時間試験用profileは公開規則を変えない。

## 7. 公平性と較正

比較例（仮想規範値、実測ではない）：共通p=0.1、当選時単価1最小通貨単位/CCU、同一cap・適格性。小型2B dense=100 CCU、大型MoEのactive work=300、大型dense=1000なら、当選原資100/300/1000、期待原資10/30/100。Q_exec=100なら当選時のcredit期待1/3/10、全試行あたり0.1/0.3/1。実際のPOSITION受取・売却額はAMMで変動する。この数字をQwenやFlash Nextの計算量、採用値、公開targetとして用いない。

比較suiteでは正確なHF repository/revision、artifact digest、tokenizer、量子化、入力frame、context、output/Stop、実行contractを固定する。ユーザーの例であるQwen2BとFlash Nextは正確なモデルIDが未指定なので、測定対象の同定を先に行う。実測では稼働expert数と実行shapeを保存し、同じtoken geometryと同一テキストを別に試験する。既存の異なる標準jobのCCU表から報酬倍率を決定しない。

PGM1等のGPU/低RAM runtimeは同じFOW1数値・trace・work factsを生成しなければならない。RAM/VRAM/SSD配置、batch/fusionは経済値を変えない。性能較正ではtoken/s、prefill/decode、claim-ready latency、Panel/Gate Cの独立再実行、初回取得、spool、peak RAM/VRAM、SSD・帯域、電力（測定可否を明示）を記録する。実機時間をNodeの合意値にしない。較正結果から未知アルゴリズムの費用下界や同等の実費収益を保証しない。

## 8. 更新、移行、障害

WorkProfileの変更は新generation・新policy IDで行う。共通meter/cost tableの改定は全モデルに同じ公開変更手続きで適用し、BFTの個別票で特定モデルだけを高くしない。model artifact/実行契約が変われば不変model ID規則に従う。高速化の発見はG-COSTの評価・停止判断の材料となるが、正当な旧署名・確定済み報酬を遡及変更しない。登録者不在genesisの経済・BFT管理はMOE3を維持。

activation recordにnetwork/genesis、fence、source/clock、meter/price/slot policy、旧cut、drain期限、責任、旧新work aliasを固定する。未知版・未設定・不足budgetはfail closed。公開ネットワークへ有効化しない。旧sourceと新版の予約を二重に使わず、歴史readerを新規採掘入口にしない。

reorgはcanonical branchのjournalをundoし、source残高、model cap、予約、work消費、credit、market、P/F役務、state rootを同じ順序で復元する。再起動では署名済みattempt・曝露・schedule journalを保持して再抽選しない。pruningは未決・有効権利・責任期間・未決済・reorgに必要なfacts、origin、証拠・dedupを保護する。保存期限終了後の要約／tombstoneとPruned IBDで一回性を維持する。ローカルI/O失敗はclaim不正ではない。

## 9. 有効化前の受入条件

1. 同一FOW1 instanceのCPU/GPU/streamed executionでwork vector・W・四Root・Ticket・正規エラーが一致する。
2. 固定小型Dense、大型Dense、active MoE、Attention長、EOS、QF1 stage、prefix、分岐の手計算とnative meterが一致する。宣言膨張・leaf粒度・inactive expert・再包装・偽originに増額しない。
3. BFT YES/NOとAUTOは同じmeterを使用し、BFTの任意係数や未知kernelを拒否。測定UNKNOWNをPASSにしない。既存停止・QF1 hold維持。
4. FOW1の唯一target、境界・0/2^256、事後変更拒否、落選・Final前・Gate C前のmint拒否。予算不足を試行前に検出。整数floorとモデル間条件付き期待原資を参照式で確認。
5. MOE3同一Cの5leg、複数model結合按分・実予算比LP・8桁POSITION・dust・quote・P/F保護、BMR1の独立49/51と100%burnが原子的に一致。
6. EXEC creditの一回発行・同一用途二重消費拒否、future seed freeze、丸め・上限、global 1 BPS/保護枠、落選仕事のcredit不可、再起動/reorgで状態一致。
7. 既存のコードと短時間fixtureで全経路を先に接続。その後実LLMの一貫したclaimでFinal/Gate C/当選・報酬・EXECを3ピアで確認。Pruned Node/Archival Node一致、IBD、物理保存容量、連続1 BPS供給条件と600秒frontier目標を長時間試験で測る。

実装／試験／評価／採択／activationは区別する。設計の追加を公開済み、実機公平性の達成、全経路の実装済みとはしない。具体的な実装順序・diagnostics・例示fixtureは[詳細設計](../design/palw/economics/mwa1-implementation.md)を参照する。
