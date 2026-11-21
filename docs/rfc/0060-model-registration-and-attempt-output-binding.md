# RFC-0060 — モデル登録時の試行別出力束縛と暗号学的非重複性

<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。MOR1の認証出力非重複性は補助保証であり、MAS1の短絡検査・G-USE・FOW1費用定理を代替しない。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未知の費用定理の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

<!-- FOW1-TB1-20261120-BEGIN -->
## 現行の適用 — 認証出力と推論関数の採掘値を分離する

2026-11-20のユーザー明示指示による新規採掘の設計は、[RFC-0062 PALW-FOW1](0062-palw-function-output-and-work-admissibility.md)／[ADR-0230](../adr/0230-palw-function-output-and-work-admissibility.md)を正本とする。乱数生成は[RFC-0061 PALW-TB1](0061-palw-internal-threshold-beacon.md)／[ADR-0229](../adr/0229-palw-internal-threshold-beacon.md)に固定する。新方針は**ADOPTED_DESIGN_NOT_ACTIVATED**であり、以下のMOR1独立草案の状態・数学的論証は旧版の記録として保持する。

MOR1の`O=(y,s,rho)`と§8のhash非衝突性だけでは、ユーザーが要求した**LLM関数自体の採掘出力**の非衝突性・新規計算費用を満たさない。FOW1はコア推論`(y,z,tau)`、zの関数非衝突性、複数試行費用、全checker coverageを登録・採掘の必須gateにする。自然回答yの一致を許す条項は維持するが、それをコア採掘出力zの衝突を無条件に許す条項として使わない。

FOW1では補助envelopeを`(y,z,s,rho)`とし、四Root TicketとTB1の独立EXEC/PANELを使う。旧三Root Ticket、未指定beacon、receiptのみの適合から新資格を得ることは禁止する。0062のactivation A以後の新規採掘はFOW1必須とし、旧MOR1をfallbackにしない。既存署名・旧claimのdrainと未完成の具体定理・本番実装は0062 §9/§10に従う。
<!-- FOW1-TB1-20261120-END -->

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **DRAFT_NOT_ACTIVATED**
- 提案系列: **PALW-MOR1**（Model Output Registration、設計上の名称。native wire IDの割当ではない）
- 対応ADR: [ADR-0228](../adr/0228-model-registration-and-attempt-output-binding.md)
- 依存する実行方式: [RFC-0059 PALW-AN1](0059-palw-an1-input-binding-and-panel-court.md)
- 現行の担当: [モデル登録03](../rfc-v2/0003-model-onboarding-and-artifacts.md)、[入出力01](../rfc-v2/0001-inference-and-generative-jobs.md)、[元claim16](../rfc-v2/0016-miner-execution-commit-and-material-supply.md)、[Final20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)
- 改訂記録: [README](../maintenance/revisions/2026-11-20-model-output-binding/README.md)、[検証](../maintenance/revisions/2026-11-20-model-output-binding/VALIDATION.md)

## 0. 要求と適用状態

本節以下はMOR1初版が保証した範囲を記録する。現在の新規採掘に必要な関数出力・費用条件は冒頭の0062に追加されており、本書の構造適合だけを新しい採掘適格性にしてはならない。

**対象profileのモデル登録時に、別の採掘試行の認証出力が天文学的に小さい確率でしか重複しない構成を要求する。** この要件を、自然文の多様性に対する経験的な審査ではなく、固定入力adapter・決定論的実行・canonical trace・暗号学的な出力処理の共通構造として定義する。

以下の「必須」「拒否」「してはならない」は、初版が提案したMOR1対応の登録／profileに対する規範を記録する。初版への指示は設計記述の追加であり、その時点でnative実装、既存採択の上書き、稼働activation、費用定理の成立を認可したものではなかった。依存する旧AN1のDRAFT記録も保持する。今回明示された新規採掘の方針変更と旧受付の切替は、冒頭と0062 §9の採択記録に従う。

モデル登録の自由は維持する。要件は、全対象登録へ同じ規則で適用する構造・形式・実行対応の検査であり、モデル品質、人気、保有人数、公開可用性、ownerの裁量による資格審査・昇格制度を復活させない。[RFC-0053](0053-additional-model-burn-pause.md)の追加モデルburn停止・解除、genesis／年次指定との区別、既存1.0/0.3等の経済条件を変更しない。HF取得検査・定期可用性認定の復活でもない。

検証方式はPanelの完全再実行とモデル非保有Nodeの有界Courtである。ここでいう「非重複性の証明」は出力構成に対する数学的論証であり、Groth16、全推論のvalidity SNARK、新しい本番proof VMを導入しない。

## 1. 三つの性質を区別する

以下の表の「出力」は旧MOR1のenvelopeである。FOW1ではこれに加えて、**コア推論の出力z**について0062 §4の非衝突性の定理と反復費用検査を要求する。yの自然文が同じ場合もzを検証する。

| 性質 | 要求 | 保証しないこと |
| --- | --- | --- |
| 同一試行の再現性 | 同じC、実beacon、model、profileなら、MinerとPanelが同じy／trace／認証出力を得る | 実行するたびに違う結果を返す一度限りの秘密処理 |
| 別試行の非重複性 | 正規文脈が異なる試行の認証出力が、明示したhashモデルの下で極めて衝突しにくい | 自然文yそのものが必ず違うこと、全中間値が違うこと |
| 消費の一回性 | 同じslot／s／規定の仕事キーから追加creditを得ない | receiptが違うだけで新しい有用仕事・新しい実費と認定すること |

正しい中間値と出力を安く生成する方法の不存在、通常LLMと同じGPU時間・FLOPs・電力・金銭費用、個人の物理的実行は、いずれの性質からも導かない。費用の扱いは§10に分ける。

## 2. 登録対象を実行profile全体へ結び付ける

### 2.1 不変な登録binding

登録するのはモデル名だけではなく、既存のimmutable Model ID／artifact identityと、次の規則の組である。

| 要素 | 必須内容 |
| --- | --- |
| モデル資料 | 実weights、architecture/config、tokenizer、chat template、adapter、quantization資料のcanonical root／固定版 |
| 実入力規則 | AN1のByteToken表、BOS/POW/SEP、job token化、frame位置、長さ、attention mask、position、cache初期化 |
| 実行意味 | TIR／kernel版、整数・浮動／量子化の規範、丸め、演算順序、非線形処理、生成・停止、sampling |
| 証拠規則 | program、Init/Step/End、全必須関係、trace schema、三Root、Merkle木、local checker、証拠／計算上限 |
| 出力束縛規則 | OutputRuleの版、hash suiteとdomain、完全context codec、回答yとenvelopeの境界、rho生成位置 |
| 運用接続 | 元claimの署名対象、Panel比較、Nodeの再導出、受付・Final・会計の既存profileへの接続 |

各要素は固定長IDまたはcanonical manifestへの認証済みdigestで参照する。任意のself-reported `verified=true`、未認証のURL、可変な`main`名、外部serviceの返答だけで適合としない。実bytesのroot照合は、モデル資料同一性の検査であり、取得可否を投票する合意制度ではない。

新しい入力adapter、出力規則、実行意味を追加する場合は新しい登録／profile identityを使う。既存Model IDに後付けでMOR1 flagを立て、既存weights/tokenizer/specの意味を変えてはならない。[登録の永久不変性](../adr/0175-registered-models-are-permanently-immutable.md)を維持する。

### 2.2 OutputBindingDescriptorの論理項目

次は規範上必要な論理項目であり、新しいwire tagやbinary codecを割当済みという意味ではない。

```text
OutputBindingDescriptor = (
    descriptor_version,
    execution_family_and_version,
    input_adapter_rule_id,
    context_codec_id,
    answer_encoding_and_root_rule_id,
    execution_trace_and_root_rule_id,
    receipt_hash_suite_id,
    receipt_domain_and_encoding_rule_id,
    output_envelope_rule_id,
    original_claim_binding_rule_id,
    resource_bounds_rule_id
)
```

descriptorとその参照先の内容を、変更不能な実行profileへ結び付ける。登録ID／profile digestを導出するmanifest自身へ、その導出結果を再び埋め込む循環を作らない。OutputRuleは独立に固定された規則のIDであり、将来のC、実beacon、s、rho、claim署名を登録manifestへ入れない。実行ごとの値はclaimのインスタンスで束縛する。

同一descriptorを許可されていない旧profileに貼り付けるだけで新しい適格経路を作れない。family／版／codec／kernel／checkerの対応表をnative実装とtoolingで一致させる。

## 3. AN1の試行と実入力を維持する

FOW1でこの実入力契約を継承する場合、`sigma`はTB1 EXECの認証済み値に限定し、PANELは別の秘密・RoundRef・freezeから得る。旧seal-reveal、block hash、BLS候補、任意schemeを新規用途に選べない。詳細な型・公開gate・曝露規則は0061に従う。

### 3.1 C・登録・freeze

`C`はRFC-0059 §2.2の完全な正規登録文脈とする。chain、family/version、epoch、登録anchor、schedule、owner、beneficiary、model/profile/job、nonce、候補集合識別、beacon、target、資源・期限、会計・資料義務等を、そのprofileの型・順序・長さで固定する。receiptだけの都合でCを短縮しない。

既存の次の関係を維持する。

$$\mathrm{RegKey}=H_{\mathrm{reg}}(\operatorname{Enc}(C)),\qquad s=H_{\mathrm{attempt}}(\operatorname{Enc}(C,\sigma)).$$

RegKeyごとの一意なslot割当、実beaconへの攻撃者の最初のアクセスより前の候補閉鎖、元claimをPanel選出より前に固定する二段階freezeに従う。slotや閉鎖集合rootをCへ戻さない。MOR1初版は具体的な乱数源を追加していなかったが、今回の新規対象では0061のTB1を使用する。BFT台帳finality又は第三のfuture Ticket乱数は追加しない。

### 3.2 実tokenへの結合

MOR1の採掘経路では、AN1の次の入力を必須とする。

$$x=[\mathrm{BOS},\mathrm{POW}]\Vert[\mathrm{ByteToken}[s_0],\ldots,\mathrm{ByteToken}[s_{31}]]\Vert[\mathrm{SEP}]\Vert\mathrm{JobTokens}_P(J).$$

ByteTokenは有効token IDへの256要素の単射とし、特殊tokenとの区別もAN1に従う。正規化、UNK置換、template、truncation、context shift、外部cacheによってchallengeを削除・同一化してはならない。入力・position・mask・cache由来を、対応する初期状態と全実行関係へ結び付ける。

同じtoken IDが別のembedding意味を持つ変更、専用token／adapterの学習、量子化、RoPE等の変更は、新しい不変bindingとして登録する。token IDの単射性はembedding全体の非衝突性や、challengeが全後続値を必ず変えることの証明ではない。

`y=Qwen(J)`を一度計算し、後からsだけを付ける経路は、この実入力条件を満たさない。seedだけを生成samplingへ入れ、prefill入力を変えない経路もAN1の代替にしない。全実行の新規費用を保証するという意味ではなく、規定された別入力に対する実行を要求する条件である。

## 4. 回答と認証出力

### 4.1 実行対象

以下は旧MOR1の`y,tau`とenvelopeの定義である。FOW1の規範実行には0062のzとReadout/End関係を追加し、関数出力`(y,z)`と補助receiptを分ける。旧MOR1式を新familyの完全な実行schemaと表示しない。

$$\operatorname{Exec}_{M,P}(x)=(y,\tau).$$

`y`は登録profileが定めるcanonical回答token列、`tau`はその回答までの規定された完全な実行traceである。Ryは引き続きyをcommitする。自然文が同じであることを、不正または衝突と判定しない。

正式なMOR1認証出力を次とする。

$$\mathcal O=(y,s,\rho).$$

consumerにC／profile／元claimが必要な場合は、認証されたnative参照をenvelopeへ持たせる。参照文字列、表示用JSON、回答のdetokenize表現を新しい抽選自由度にしない。人間向けにyだけを表示しても、検証・保存・再送・精算では元の認証文脈を失わない。

### 4.2 256bitの実行識別値

MOR1参照hash suiteはSHA-256、出力長は32 byteとする。receipt-domainの固定byte列を`PALW-MOR1/receipt/v1`のASCII bytesと終端NUL一つの連結とする。別用途のdomainと重複させない。これを本番へ採用するには、対応するprofileとcodecを明示的に有効化する必要がある。

$$\boxed{\rho=\operatorname{SHA256}\left(D_{\mathrm{receipt}}\Vert\operatorname{Enc}_{P}(C,\sigma,s,R_x,R_y,R_\tau)\right).}$$

各Rootとsはprofileの32 byte値として符号化する。C・sigmaのscheme/長さを含む符号化はcontext codecが一意に決める。固定fieldの順序、型、長さ、canonical整数、未知fieldと末尾byteの拒否を登録時に固定し、曖昧な連結、暗黙型変換、miner選択の別codecを認めない。

**完全なCを含める。** Cのdigestだけ、sだけ、ownerの表示名だけへ置換しない。異なる正規文脈が異なるreceipt入力になることを、sの衝突前提と分離して論証するためである。Cの他の認証Root自体の同一性には、その登録方式の暗号前提が残る。

署名bytes、Panel ID、PanelのPositiveNonce／ActionNonce、Court path、資料URL、ACK、自由salt、コメント、受信順序、再送回数をrhoへ追加しない。MOR1のrhoは識別値であり、Panelの承認receiptやDA受領証とは別の型とする。

### 4.3 循環を作らない計算順序

| 段階 | その時点で固定されるもの |
| --- | --- |
| 登録 | Cと不変なmodel／profile／入力・出力規則 |
| beacon後 | sigma、s、x、Rx |
| 規定実行後 | y、tau、Ry、Rtau |
| 出力処理 | 固定済みの値からrhoとOutput envelopeを導出 |
| 元claim固定 | 新profileの署名対象として元claimを固定 |

Ryへrho入りenvelopeをcommitしてはならない。Rtauにもrho／Ticket／それらのhash入力を計算する後処理を循環的に含めない。rhoは固定された三Rootからnativeが再導出できる後処理であり、実行traceの末尾へ追加してRtauを作り直す対象ではない。

OriginalIdまたは署名がrhoを含む新profileでも、そのOriginalIdや署名をrhoへ戻さない。同じ条件を、登録ID／profile digest／descriptorの導出にも適用する。

## 5. Ticketと会計を変更しない

MOR1初版でrhoを追加した際は、RFC-0059 §5のAN1 Ticket式を置換しなかった。以下はその旧式を記録する。現在の新規FOW1は0062 §5の四Root Ticketへ置換し、旧三Root式を新profileに適用しない。

$$T=H_{\mathrm{ticket}}(\operatorname{Enc}(s,R_x,R_y,R_\tau)).$$

`T=H(rho)`への切替、二段目の独立抽選、モデル別の追加D、receipt生成回数に対する仕事加算を行わない。既存D／thetaの境界、target snapshot、同一RegKey、slot、AttemptUse[s]、ComputationalWorkKeyと用途別BaseWorkKeyの一回性を維持する。

別rho・別owner・別nonce・別表示形式を、新しい有用需要や新たな通常推論実費の証明とみなさない。rhoの非重複性は会計キーの意味的重複排除を代替しない。fork/reorgにおける消費・発行の原子性もAN1と既存native規則に従う。

## 6. 登録時の決定論的な適合検査

以下のMOR1構造検査はFOW1では必要条件の一部である。0062 §2/§4のTheoremRule、全適合predicate、G-EXEC/G-COLL/G-COSTを満たさない登録から新しい採掘資格を発行してはならない。一般モデル登録とFOW1採掘適格性は別状態とする。

### 6.1 検査する内容

MOR1対応登録の技術的受入処理は、少なくとも次を確認する。

1. descriptorと全参照先がcanonicalかつ認証され、許可されたfamily/versionの規則に一致する。
2. ByteTokenの256個が相異なる有効indexで、特殊token、長さ、frame、job導出が固定される。
3. template・mask・position・cache・停止規則がchallengeを落とす別経路を作らず、その全経路に対応する検証関係がある。
4. 実行意味論・生成規則が再現可能な形で固定される。未指定のsampling RNG、draw順、丸め、動的kernel選択を残さない。
5. Init/Step/Endと入力・重みindex・状態・出力を覆うTIR／verification planおよび有界Courtが登録規則に対応する。
6. OutputRuleが完全C・sigma・s・三Rootを使用し、別名codec、自由field、自己参照を許さない。
7. 実行資料の実bytesからRootを照合でき、自己申告属性や別実行cacheを代替にしない。
8. 新profileの元claim・Panel比較・native出力検査・保存／復旧consumerが同じbinding版を使う。

有限個の試験で出力衝突がなかったこと、hashの見た目、文章の多様性、モデル名、登録者の署名だけを合格根拠にしない。登録者が任意の出力関数を提出して非衝突を自己申告する方式ではなく、合意で固定された構成の適合性を検査する。

### 6.2 登録境界と責務

Nodeはモデル全体を取得・実行することを要求されない。Nodeは既存登録経路の認証済みbinding、宣言形式、対応profile、資源上限を検査する。必要な全資料を使うconverter／loader／Panelは実bytesを認証して評価し、claim時の検証機会と不正排除は既存の公開検証・Courtへ接続する。

登録時に一度Panelが署名しただけで全将来claimを免除しない。本RFCは、任意モデルのgraphの意味的影響や計算下界をNodeが自動判定できる登録証明を導入しない。

MOR1の技術的必須項目が未対応の場合は、当該新profileの適合登録として受理しない。旧profileへ自動fallbackして同じ新profile名で採掘を続ける経路を作らない。既存登録の存否、追加モデルのPAUSED／ACTIVE、claimの成立、報酬・合意用途の適格性はそれぞれの既存状態に従う。

## 7. claim・Panel・Nodeへの接続

### 7.1 元claim

新profileの元claimは、MOR1のOutputRule版、登録descriptorとのbinding、s、三Root、rhoまたはその一意なnative導出規則を、同じ署名subjectへ結び付ける。rhoを搬送する版では、native再計算結果と不一致なら元claim受付で拒否する。省略可能とする別版を設ける場合も、認証済みprofileから一意に復元する規則を事前固定し、minerが都合で選択しない。

本草案の参照envelopeはrho32を明示搬送する。nativeの具体的なfield tag・domain・protocol activationは別の版付けで確定する。旧署名の検証byte列へ無断でrhoを追加しない。

### 7.2 Panel

PanelはC、実beacon、s、正規入力、model、program/profile、元trace、出力を認証して全体再実行する。正しい三Rootとrhoを再導出して元claimへ照合する。rho一致だけ、回答文一致だけ、他Panelの承認、少数の正しい局所Courtを完全検証の代替にしない。

root不一致だけでMinerの有罪を決めない。元claimの認証済み局所矛盾を示すか、取得不能を適格な資料caseと成功停止へ接続する。未検証・未解決を成功に変えない。

### 7.3 Nodeとconsumer

Nodeは認証済みnative文脈と元claimからrhoを再計算し、full modelなしで出力処理の等式を検査する。これは三Rootの中身全体の正当性を検証したことではない。計算不正は既存Court、元claimへの署名偽造・形式不一致は既存認証・受付規則で扱う。

receipt-onlyの不一致を、別modelの不正計算や他人の責任へ自動的に転嫁しない。未認証packetはInvalidEvidenceまたは受付拒否、元claimへ認証された実行矛盾はComputationRefuted、資料未充足は資料caseとして分離する。

validator、Panel、miner、RPC／SDK、回答API、state storage、snapshot／IBD、import／repair、reorg undo、報酬／credit consumerが同じprofile判定を使う。回答APIの既存本文形式を自動変更せず、MOR1の機械向けenvelope版で必要な認証情報を保持する。rhoを捨てた表示用回答を、別試行の成功claimへ変換してはならない。

## 8. 非重複性と再現性の証明

この証明の対象は補助receiptのhashと旧MOR1のenvelopeである。FOW1のコア出力z又は複数試行費用の定理として転載しない。0062 §4の未完了gateを本節で代替しない。

### 8.1 前提

- context codecは対象文脈上で単射であり、型・長さ・field順を含めcanonicalである。
- 異なる試行とは、nativeに登録されたCまたは認証された実beaconが異なる試行をいう。同一C／sigmaの再送は新試行ではない。
- model/profileと三Rootの意味は認証され、同一試行の規定実行は決定論的である。
- 確率論証ではreceipt-domainのhashを古典ランダムオラクルとして扱う。具体的SHA-256の安全性は暗号学的前提であり、無条件定理ではない。

### 8.2 同一試行の一意な認証出力

C／sigma／model／profileが固定されると、sとxが定まり、規定実行からy／tauが一意に定まる。canonicalな三Rootとrhoも一意である。したがって同一試行を正しく再計算したMinerとPanelは同じOutputを得る。再署名・再送・Panel交替は別の正しいrhoを作らない。

同一試行の重複提出は、この決定性だけで拒否されるわけではない。追加creditの拒否は、AN1の原子的な一回消費を使う。

### 8.3 別試行の識別値の衝突上界

相異なるC／sigmaを完全にrhoの入力へ含め、単射のcodecを使うので、別試行は異なるreceipt hash入力を与える。仮にsが衝突しても、完全C／sigmaの相違を出力処理が失わない。

receipt-domainの**相異なる入力への総照会数の確定上限**をQとする。攻撃者の事前照会、協力者、未提出・破棄された候補、同一試行の不正Root選別、検証者が初めて行う照会も含める。公表された成功件数だけをQとしてはならない。Qは安全性分析の計算予算であり、nativeが全攻撃者の非公開照会を観測できるという仮定ではない。

新規照会iの返値が、それまでのi−1個の返値と一致する確率は高々(i−1)/2^256である。union boundにより、

$$\boxed{\Pr[\exists i\ne j:\rho_i=\rho_j]\le\min\left(1,\frac{Q(Q-1)}{2^{257}}\right).}$$

| 総照会上限Q | 衝突確率の上界 |
| --- | --- |
| 10^12 | 約4.318084277543 × 10^-54 |
| 2^40 | 約5.220243574394 × 10^-54 |
| 2^64 | 約1.469367938528 × 10^-39 |

この数値例は新しいnative調整値・登録枠・実測件数ではない。256bit digestの一般的な古典衝突探索強度は128bitであり、256bitの衝突安全性と表示しない。自然文yのentropyや語彙数からこの上界を導いていない。

Output=(y,s,rho)全体の衝突はrhoの衝突を必要とするため、同じ上界で抑えられる。sの非衝突から実入力が異なるというAN1の主張は、別のattempt hash前提である。receiptとattemptの両方の失敗を同時に扱う場合は、照会予算とdomainごとの失敗項を明示してunion boundを取り、二つを無条件に一つのepsilonへ隠さない。

### 8.4 この定理に含めない性質

rhoは誤ったRootでも生成できる。上の定理は、正当性、資料取得、反証包含、費用、本人の労働の証明ではない。SHA-256へ単に長い出力を選ばせれば、これら全てが成立するという推論は禁止する。

## 9. Qwen等への適用とモデル設計

以下は旧MOR1への接続・研究候補であり、Qwen等がFOW1Admissibleであるという認定ではない。FOW1では適合する具体的関数系列と全定理が必要である。名前又はnonce入力だけでzの非衝突性と費用を保証した扱いにしない。

既存Qwen等の特定の固定版を、共通MOR1入出力構造へ接続する。学習済み自然文headに暗号学的な非衝突性を期待しない。参照例のモデル名をallowlistへの自動登録、実行対応済み、性能認定済みの意味にしない。

第一段階では、正確なweights/config/tokenizer/templateを固定し、AN1 challenge frameと規範的実行・trace・rho後処理を接続する。既存重みを維持しても、challenge追加により元job単独の回答と品質が変わり得る。新しい実入力について品質・資源・再現性を評価する。

生成にsamplingを使う場合は、sから導出する固定seedだけでなく、PRNG、draw順、確率表現、丸め、選択・同点・停止をprofileに固定する。同じseedという理由だけで異なるhardware／backendのbit一致を仮定しない。greedyへ変更しても自動的に品質を保持するとは表示しない。

第二段階で専用token、複数層adapter、中間状態に依存する後続challenge等を研究する場合は、変更後の新model/profileとして扱う。計算graphの経路の存在だけで、全値の変化や新規費用下界を認定しない。ゼロ、相殺、飽和、量子化、cache、batch、代替アルゴリズムを検討対象にする。既知のXOR maskの可逆性を、一方向性や評価困難性の証明へ読み替えない。

これらの研究は、延期された自動モデル改善・line/head更新経路の有効化ではない。固定artifactの新規登録という範囲を超える機能は、既存の延期・変更管理に従う。

## 10. 計算費用・PoW適格性との境界

新規FOW1の費用判定には、対象instanceでの反復検査を適用する。検査不足は採掘保留とする。[RFC-0062 §4.3](0062-palw-function-output-and-work-admissibility.md#g-cost-review) 本節の「通常推論費用を保証しない」という正確な限界は、未適格モデルを新profileで受理する許可ではない。

MOR1は新しいモデル別の裁量的費用審査や品質資格を導入しない。登録構造適合、claimの実行検証、運用状態、報酬／合意用途の適格性を既存の担当規則に従って区別する。

新しい認証出力があることだけを理由に、通常Qwen推論のFLOPsや料金を「強制された仕事量」として発行しない。適用profileについて保証された費用を主張する場合は、前処理、cache、batch、外注、入力・モデル選別を含む計算モデルと、複数試行の費用下界を別に示す。未証明の下界を登録flagで成立させない。

同じ正しい観測記録を安く生成する最適化を、rhoが変わることを根拠にslashしない。誤計算、認証偽造、二重消費、既存の責任付き義務違反は、それぞれの証拠・責任規則で扱う。

このprofileの出力非重複性が成立しても、AN1の費用下界やDAG全体の安全性を完成済みにしない。PoWで必要な既知入力の評価費用と、出力から入力を逆算する一方向性は異なる。

## 11. 受入試験と反例

| 試験群 | 期待結果 |
| --- | --- |
| 同一C／sigmaの独立再実行 | 同一s、token入力、三Root、rho、Ticket |
| owner／nonce／epoch／model／profile変更 | 変更後のCで別試行を正規登録する。model／profileの内容変更は新しい不変IDを要求し、nonce変更だけでモデル再登録を要求しない。旧claim／openingの直接転用を拒否 |
| 同じ自然文yを返す二試行 | 正常に許容。認証出力は異なる。文章一致を不正扱いしない |
| ByteToken重複、特殊token衝突、無効index | 対象profileの構造適合を拒否 |
| challenge切詰め、template除去、別cache起源 | 正規入力／Init関係との不一致として拒否または既存caseへ接続 |
| 完全C省略、field順変更、未知field、型alias | canonical codec／OutputRule違反として拒否 |
| Ryをrho入りenvelopeで構成、rhoをtraceへ循環追加 | 登録設計を拒否 |
| rhoだけを改竄 | 元claimのbinding／native再導出で拒否 |
| 表示・転送metadataだけ変更 | 認証対象の値は変わらず、別receipt・Ticket・creditを与えない。許された表示変更自体を誤計算にしない |
| 同じsに異なる未認証Rootを与える | hash生成はできるが正しい実行の承認にはならない |
| 正しいopeningを持つ局所矛盾 | 既存Courtで反証。偽openingはInvalidEvidence |
| receipt一致のみ、部分traceのみで承認 | 完全検証条件を満たさない |
| 再署名、再送、別slotで同じRegKey、別envelope | AN1の一回消費／別名排除を維持 |
| PAUSED、未解決資料、既受理negative、同batch Final | 既存の現在state／negative優先／成功停止を維持 |
| 旧claim、旧署名、旧API | 遡及変更せず、MOR1成功経路への暗黙fallbackを拒否 |
| 正しいtraceを安価に生成できる対照モデル | 非重複性を満たしても、費用下界の証明にならないことを確認 |

大量のランダム試行で衝突ゼロを観測することは、§8の確率定理の代替にならない。正常経路の試験だけでなく、canonical constructorを迂回した偽Root・不正符号化・過大資料も既存AN1の境界条件で検査する。今回の文書整合検査を、これら全受入試験の実装完了と表示しない。

## 12. 互換性・移行・activation

以下はMOR1初版の互換条件である。新規採掘の置換・旧受付終了・legacy drainは0062 §9を優先し、旧MOR1だけを満たす新規claimを同じ報酬/credit/weight経路へ流さない。旧署名・不変ID・元期限は保持する。

1. MOR1は新しい不変なmodel/profile bindingとして選択する。既存登録ID、AN1草案の意味、旧claim、旧署名を上書きしない。
2. 対象の登録epoch／family／codec／OutputRule／全consumerを明示してから有効化する。文書番号や日付だけで優先・activationを決めない。
3. rho後処理と新envelopeを導入しても、AN1のTicket式、difficulty、二段階freeze、Panel＋Court、元期限、negative優先、資料hold、一回消費を維持する。
4. 新規登録の適合性と既存modelのburn PAUSED／ACTIVEを混同しない。廃止された取得証明・定期認定・モデル品質審査を導入しない。
5. runtime、署名codec、native検査、全consumer、cold資料、最悪case、reorg、性能・費用の検証が揃うまで、DRAFTを本番対応済みに昇格させない。
6. 有効化失敗時は既存の停止・保留・移行手順に従う。不適合なMOR1 claimを旧profileへ付け替えて成功させない。

## 13. 変更範囲と参考

以下はMOR1初版の来歴である。今回の0061/0062による追加採択・優先・移行範囲は冒頭に明記したとおりであり、「既存原則を変更しない」という初版記録を新規採掘の置換拒否に使わない。

本RFCとADRは、添付`misaka_docs_latest.zip`のモデル登録経路とAN1に対する設計追記である。現行の閲覧冊数20 RFC／50 ADR、過去の原文・実測・digest、既存原則、runtimeを変更しない。現行冊には本草案への適用範囲付き参照を追加する。

外部資料は登録・暗号・数値再現性の背景であり、PALWの費用下界を証明する資料ではない。

- [NIST Hash Functions](https://csrc.nist.gov/Projects/Hash-Functions) — SHA-256の出力長と衝突強度。
- [Qwen3-8B model card](https://huggingface.co/Qwen/Qwen3-8B)／[config](https://huggingface.co/Qwen/Qwen3-8B/blob/main/config.json) — 適用例。実登録では可変mainでなく実artifactの固定版・rootを使用する。
- [PyTorch Reproducibility](https://docs.pytorch.org/docs/stable/notes/randomness.html) — seedだけで異なる実行環境の完全一致を保証しないという注意。

§8の定理と数値例は本RFCの定義・仮定から導く。Qwen実機、nativeモデル登録、実ネットワークでの測定を行ったとは記載しない。
