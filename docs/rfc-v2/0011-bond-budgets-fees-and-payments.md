# RFC-v2-0011 — Bond予算・配分・Fee同意・決済会計

## 2026-11-09公開資料・一人前提の追加境界

[採択RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)により、独立選出されたPanelの中の一人が、モデルを事前保有せずに公開資料を取得して全体検証できる新profileを採択する。cold取得・全再実行・最大合法caseの費用と支払原資を既存の有限契約へ結ぶ。多数不署名だけを成功阻害条件にしない。登録tier縮小分の発行・再配分の既存不整合は未解決gateとして維持し、今回のk_min=1から経済値変更を導かない。 成功の規範所有者は[20](0020-verification-gated-final-and-adversarial-models.md)、公開方針は[v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)。以下の先行採択・基礎契約と衝突する箇所は、この新profileに限って本境界と16〜20を適用する。旧claim・歴史付録は保持し、状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。claim受理時に一人の合法な救済経路の有限費用と予約を確保し、算術不正・供給default・未検証終了の帰責を分ける。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

既存のbond、finite fee、原資区分、署名済み同意・予約・返還権を維持する。[16](0016-miner-execution-commit-and-material-supply.md)の供給、[17](0017-noninteractive-positive-verification.md)の検証、[18](0018-interactive-negative-court-and-defaults.md)の公開反証の費用を混ぜず、[20](0020-verification-gated-final-and-adversarial-models.md)の未検証Abortには成功仕事報酬・creditを与えない。実行済み役務費と仕事報酬を別に精算する。

**優先関係:** 2026-11-08の先行採択はADR-0215と公開裁定v0.2を根拠とする。今回の公開資料・一人前提の新profileには冒頭の0050／0218・RFC16〜20・[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)を優先する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**原文収録元:** [旧0014](../rfc/0014-panel-independent-fraud-prosecution.md)、[旧0015](../rfc/0015-panel-free-permissionless-verification.md)、[旧0016](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)、[旧0039](../rfc/0039-finite-fee-consent-and-provider-payment-bounds.md)。

**関連RFC:** [04 モデル二階層・年次カタログ](0004-model-tiers-and-annual-catalog.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [07 Claim・EXEC・仕事量・回答会計](0007-claims-exec-and-answer-accounting.md) / [08 Claim依存・Prefix生成元・未解決責任](0008-claim-dependencies-and-liability.md) / [10 DAG合意・Gate C・Native EVM決済](0010-dag-consensus-and-native-settlement.md) / [12 認証State・保存・Pruning・IBD](0012-authenticated-state-storage-and-ibd.md) / [14 Court統合契約](0014-court-master-contract.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. 共通会計の正本

旧0016のV+／弾力的Final容量、旧0039の採択済みfee consentを統合する。旧0015に書かれていた採用済み共通bond上限・配分規則もここへ移す。Panel=0の休眠によって、共通会計まで停止したことにはしない。

旧0027の新Account Bond案、および旧0039のprovider役務有限化A/Bは未採択案として15に置く。旧0027の表題にある額を現行本番値に採用しない。

## 2. 四つの上限と一回性

claim容量、報酬対象ブロック、報酬総額、Final weightを同じ拘束資本・規則・共通期間に対応付ける。計算速度、class、claim分割、別支払名目、bond再登録によって共通上限を増やさない。

予約、未払権利、queue、物理支払は同じ権利の状態変化であり、別々の新規報酬ではない。発行枠の回復と、slash可能な担保の解放を区別する。全関係の責任が終わる前に担保を二重使用しない。

## 3. 配分とパラメーター

二階層係数、超線形資本配分、V+による物理ブロック機会とcoinbaseへの接続を維持する。既存の採択記録ではα=1.5、平均期間は7日相当DAA（旧30日から改定）であり、原文中の30日を現行値として復活させない。

κ等の値・分母・縮小分の扱いはパラメーター台帳とADRの採択範囲を照合する。台帳自身が記録する不整合は本編集で勝手に解決しない。値の再承認、実測値、推定値、有効化recordを区別する。

## 4. 通常VerifierのFee

新しい署名済み同意によりfee全額を有限の責任期限まで保留する。期限内に客観的有罪が対象Clean attestationへ結び付く場合は元posterへ全額返す。それ以外は期限後にverifierの支払権利を解除する。ServiceHoldを理由にfee期限を無断延長しない。

このfee解除はclaimのFinal、Gate C、producer報酬の解除ではない。遅いverdictで解放済みfeeを遡及回収せず、既存bond liabilityは別に維持する。

## 5. ProviderのFee-only契約

採択対象はprovider自身の前払feeだけで、V1のservice、worker権利、replacement、材料保持、finalityは変更しない。providerと元sponsorが同意し、Hは元の`enrollment_until_daa`へ明示的に結び付ける。

対象providerのdefaultが選択chain上で実際にH以内に処理された場合だけ返金し、それ以外はH後に支払う。challenge開始時刻とdefault処理時刻を混同しない。default記録をfee解除より先に評価する。旧cohort、既払fee、後任providerへ遡及しない。

## 6. Courtとの境界と受入

court固有の受付・役割同意・original escrow・終端効果は14が所有する。本RFCは共通money engine、保存則、二重支払防止を所有する。同じwireや判定を両方で別定義しない。

正常支払、期限内返金、期限後default、H/H+1、無rootのenrollment、replacement、overflow、資金不足、支払と返金の競合、reorg／restart／IBD後の一致を確認する。

---

## EXEC creditの追加採択と経済上限

[07](0007-claims-exec-and-answer-accounting.md)のExecCreditは有限の生成資格であり、
取引可能な新coinや新たな仕事報酬ではない。既存のwork量、成熟、共通資本・bond上限、
失効・依存・署名済みの原資と責任を継承する。
予定表への予約と使用を別の発行として数えず、1 BPSを理由にcredit倍率や寿命を無断で増やさない。
端数抽選の一回性と、無償の再試行・nonce変更による量の増加禁止を維持する。

EXECの取引手数料、PALW仕事報酬、courtの役務費、未使用予約の返還は別の会計である。
仕事が失効した場合の残permitの無効化と、すでに本体が受理した取引の状態を区別し、
本改訂から新しい補償・自動rollback・settlement確定性を導かない。
既存WorkerPaymentのB=F=R=0とfunded relayの権利を保持する。
courtとEXECが共有する物理容量の予約は[05](0005-bounded-resources-and-long-context.md)、
slot割当は[10](0010-dag-consensus-and-native-settlement.md)を正本とする。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0014 | 747–898 | [16. モデル入手への不介入と拘束bond連動coinbase配分（2026-10-10後続改定）](#source-v1-0014-L00747) |
| 旧0014 | 905–913 | [Bond予算・総影響保存の改定 — 2026-10-10](#source-v1-0014-L00905) |
| 旧0015 | 380–524 | [8.3 Class-Independent Bond Production, Reward and Final Weight Caps — 2026-10-10追加決定](#source-v1-0015-L00380) |
| 旧0015 | 544–679 | [8.4 Claim容量の倍率とbond当たり総影響の保存](#source-v1-0015-L00544) |
| 旧0015 | 954–968 | [ユーザー方針 2026-10-15 第4回 — §8.5.5 の配分式に階層係数](#source-v1-0015-L00954) |
| 旧0016 | 1–111 | [RFC-0016 — bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する](#source-v1-0016-L00001) |
| 旧0016 | 157–674 | [3. 拘束期間・発行能力の調整とmode切替（D-H、ADR-0176と整合）](#source-v1-0016-L00157) |
| 旧0039 | 1–23 | [RFC-0039 — 通常 verifier の fee consent と provider の有限支払・保持契約](#source-v1-0039-L00001) |
| 旧0039 | 51–92 | [4. 旧 cohort と新 wire](#source-v1-0039-L00051) |

---

<a id="source-v1-0014-L00747"></a>

### 旧RFC-0014 / 原文747–898行

出典: [0014-panel-independent-fraud-prosecution.md](../rfc/0014-panel-independent-fraud-prosecution.md)。配置理由: 共通経済規則を会計RFCへ。

#### 16. モデル入手への不介入と拘束bond連動coinbase配分（2026-10-10後続改定）

##### 16.1 採択方針と旧方針の撤回

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を正とする。
**チェーンはモデルを入手できるかどうかに一切干渉しない。**
MISAKA Torrentの採用・統合、専用Bonded SeederとSeeder報酬を廃止する。
必須Torrent、独立Bonded Seeder群、PoR、Full Fetch、funded availability lease、TRDC、FPR、
モデル全量の強制公開費用と取得不履行に基づくreward/weight停止を撤回する。
旧7/5コピー、Seeder 15%、監査3-of-5、符号化4-of-7も採択しない。
旧文書チェックのPASSは旧設計の整合記録であり、この後続設計の経済・実装証拠ではない。

##### 16.2 配布と合意の境界

一般的なP2P配布・DHT/PEX・mirror・保存/共有契約はoff-chainの任意運用である。
MISAKA固有のTorrent採用や専用Seeder制度・報酬は設けない。
モデル保有者、miner、Panel、一般peerへ公開・Seeder義務を課さず、chainはpeer数・独立性・速度を追跡しない。
全peer停止、公開拒否、ローカル取得失敗だけではclaim受理、報酬、Final weightや担保責任を変えない。
配布の成功/失敗を証言する新委員会・availability oracleを導入しない。

##### 16.3 固定モデルとの同一性

登録時にModel ID、Canonical Artifact Root、canonical manifest、全weights、tokenizer/config/実行仕様、
artifact sizeを固定し、変更は別Model IDとして登録する。path/order/encoding/file扱い等のcanonical化を定義する。
claimは固定Model ID/root/specをbindする。取得者はpiece/shardを認証し、全量取得/復元後のrootを登録rootと照合する。
任意のTorrent descriptor/infohashを登録するなら、その固定rootとの対応をimmutable bindingにする。
infohashとartifact rootは別domainであり、infohashの必須登録・Torrent接続を資格条件にしない。
異なるデータを正規モデルとして使用せず、root一致だけでminerの実計算を証明したと扱わない。

##### 16.4 モデル資格とclaim証拠のscope

旧六項`ModelRewardEligible`の可用性gateは撤回する。新routeの設計predicateは次とする。
型名は仮称で、合意snapshotから導き、swarmやノードの取得結果を入力にしない。

~~~text
ModelRewardEligible(model, epoch)
    = ImmutableIdentityBindingValid
      AND SemanticConstraintCourtPlanAdmitted
      AND ApplicableKernelModeAndRulesetActive

ClaimRewardEligible(claim)
    = ModelRewardEligible(claim.model, claim.accepted_epoch)
      AND RootBoundClaimCommitmentAndAuthorizationValid
      AND NetworkModelAndBondBudgetsReserved
      AND ClaimSpecificEvidenceCourtAndFinalConditionsSatisfied
      AND UniqueWorkAndNoApplicableConviction
~~~

型付きのclaim固有state/trace/output/witnessのcommitment・保持・客観裁定は維持する。
モデル取得目的のweight-file/range、全量列挙/反復要求をこのcourtに持ち込まない。
許可unit・累積scopeと必要model operandの認証をADR177 D2に従って確定する。
取得できないという申告だけでは適格なcourt開始、void、Slash、反証期間延長にならない。

**実装（lane DA16、2026-10-10、休眠 `palw_provider_court_v1`）:** 全 route の demand／opening／challenge unit を
`consensus-core palw_court_scope_v1` の inventory（`palw_court_unit_scope_v1`）として claim 固有（input/trace/state/output/witness hash）と
model bytes（weights／その複写・関数／file range）に分類し、compel できる unit は claim 固有のものだけとする（`palw_court_demand_allowed_v1`）。
累積 scope は、モデル bytes 開示が要求回数によらず claim・model ごとに 0、claim 固有 unit は claim ごとに 1 回、(claim, 要求 operator) ごとに
16 unit まで（他の要求者の枠を消費しないため G14 の starvation-freedom を保つ）。kernel route では fence 以上で snapshot slice と登録 `M0` の
pre-state を拒否し、retrieval claim 自身の entry（stage `0xC0 + s`）を claim 固有の demand unit とした（malformed／junk leaf に対しても
court が全域で裁定する、G14C GAP-52）。verifier が自分の copy から出す model operand は登録 root（kernel class の `ParamCommitmentsV1` root、
V2 `artifact_root`）に対して `palw_verify_verifier_model_operand_v1` で認証する。node 単位の model-bytes mask（K2S が v4/v5 の owed set で
呼ぶ）、活性値から関係を復元しうる残余（Level 2 は利用者判断）、t12 で武装済みのモデル bytes を compel する route（held `StepLeaf`、
dissection の root claim、readiness）と新しい休眠 fence の提案、各 unit class の G14 到達性は
[DA16 design](../design/palw/verification/da16-transport-and-provider-court.md) §7。状態は implemented（armable ではない）。

##### 16.5 重複しないモデル別miner元本

受理済み配分snapshotにおいて、各bondのmodel別適格拘束元本を`C_{b,m}(t)`とする。

~~~text
sum_m C_{b,m}(t) <= C_b_effective_locked(t)
S_m(t) = sum_b C_{b,m}(t)
~~~

claim予約額や件数を単純合算しない。rho・再送・root/slice/rider・key分割で同じ元本を増幅しない。
owner seed/AMM/Position/Panel担保も自動算入しない。slash、退出、責任控除、model移動、期間境界の
資本帰属とsnapshot/積分方式を固定し、短期借入と二重配分を評価する。7日間は未採用の説明例である。

##### 16.6 モデル別coinbase allocation

採択済み総PALW発行予算の中で、非負・単調、`f(0)=0`の配分関数を用いる。

~~~text
A_m(t) = f(S_m(t))
R_m(t) = R_PALW(t) * A_m(t) / sum_j A_j(t)   [positive denominator]
sum_m R_m(t) <= R_PALW(t)
~~~

他モデル資本を固定した場合、model資本の増加は配分を非減少にする。
全modelが同時に増資する場合の絶対増額や既存minerの収入増は保証しない。
分母0ならmodel配分0。丸め・未配分/未使用残高、model内leg、old/new会計の処理を規範化し、追加mintをしない。
元本を預けただけでは支払わず、当該modelの有効な計算claimとFinal・予算条件を要する。

##### 16.7 個別bond予算との同時制約

ネットワークcoinbase予算、model配分、ADR176の個別Q/B/R/F capを同時に検査する。
公開参加者の資本追加で集合の配分が増えても、既存minerの最大枠と共通`d + W`時計を増やさない。
受理snapshotと最大配分権を固定し、未払escrow・成熟・Final・reversal/readerで同じ帰属を使う。
可変model配分から過去claimへ未予約の追加creditを発生させない。配分だけでblock数・DAA・難易度を変えない。

##### 16.8 独立受入matrix

ADR177 §3のnon-interference、identity/court scope、distinct capital、allocation conservation、
same-bond opportunity、open-versus-closed economics、recovery/activationを独立に試験する。
同一chain/proofでpeer停止とlocal取得結果を変えても合意結果が同じになることを確認する。
モデルのavailability監査をactivation gateへ戻さず、実効監視・経済評価の仮定を明示する。

##### 16.9 資料と記録の位置づけ

Torrent/PoRの外部仕様は任意transportの資料であり、報酬資格・全量公開・外部所有者識別の証明ではない。
現行subsidy carveは`palw_reward_v2.rs`、DAA emission budgetはADR167を参照する。
本章の`S_m`/`f`配分は未実装で、従来のt12・Torrent/FPR文書PASSを新配分の証拠に流用しない。

##### 16.10 規則Eとfork-choiceへの接続

claim固有の適格court/Final条件、暫定creditと不可逆settlementの区別、ADR176のFinal weight上限を維持する。
root/sliceと全writer/reader/undo、partition/reorg/restart/IBDをversioned規則で検証する。
モデル配布不能だけで新たなweight停止を起動せず、閉鎖時の未検出不正を無条件に防げるとは主張しない。
escrow・個別cap・model配分だけでRule E統合が完了したとしない。EXEC_TX/EXEC_SLICEのweight/DAAは0を維持する。

##### 16.11 ECON・MEAS: 公開参加の経済優位

公開による外部参加資本の増加が、閉鎖自己資本より圧倒的に有利となる倍率曲線を目標にする。
同額の`S_m`では自己資本と外部資本は同じ配分であり、ウォレット数を独立資本認定に使わない。
`f(S)=S`だけで既存minerの利益増を保証せず、superlinearな曲線の集中/Sybil/予算飽和も評価する。
既存miner/ownerのcap、配布/資本/計算/検証費、競合model、自己増資、借入、全資本自前、
閉鎖による検出率低下、49% reporter還流、共有担保と外部利得を含めて比較する。
優位の数値基準・適用市場/資本範囲を事前定義し、成立しない条件と未立証事項を報告する。
正しいモデルを得られないverifierの実効`p`は0にもなり、`p(R_risk+L_net)>C_saved`を無条件に成立としない。

**2026-10-10改定（bond集約優位）:** 本節の目標はADR-0177の改定に従い、「公開参加の経済優位」から「実拘束bondを多く集めたモデルの優遇」へ変わる。
「同額自己増資より公開が常に有利」の証明は要件から外す。候補`A_m = S_m^α`（`α > 1`、比較`α = 2`）を、個別cap・費用・モデル内配分・資本集中を含めて評価する。
大口資本の優位は残余リスクとして受容する。`p=0`の計算省略は独立の安全性gateとして残す。

##### 16.12 未決パラメータとactivation

`f`の形・上限/逓増範囲、model内配分、適格元本、配分時計/snapshot、ADR176の共通`W`との接続、
整数精度・残余、claim固有court scope、検出/包含/徴収仮定、経済優位の閾値、versioned移行を固定する。
wire/fingerprint・全実装・独立試験・別の協調activationまで新modeを有効化しない。
model配分で採掘機会まで変更する追加設計は本coinbase改定から自動導入しない。

<a id="forced-public-retrieval"></a>
##### 16.13 Forced Public Retrievalの撤回と検証保証の変更

旧FPR三段階、全量公開義務、モデル配布用重大Slash/上限付きdefault、代替Seeder・公開費予約と
availability起因CHALLENGED weight制約を撤回する。modelを公開しないだけでminer bondを失わせない。
既存claim固有の客観的計算反証・適格court/defaultはそれぞれのrulesetで維持する。
**全所有者が拒否してもpublic verifierが全modelを取得できるという保証は撤回する。**
G14の検証は正しい固定modelを保有/取得できた条件付きとなる。モデル未取得だけではchainは介入しない。
旧TRDC/FPRは未実装・未有効化だった設計であり、現在のG14/activation要件として引用しない。
過去の文書チェックと撤回sectionはADR177の整合記録に履歴として保持する。

---

<a id="source-v1-0014-L00905"></a>

### 旧RFC-0014 / 原文905–913行

出典: [0014-panel-independent-fraud-prosecution.md](../rfc/0014-panel-independent-fraud-prosecution.md)。配置理由: 共通経済規則を会計RFCへ。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

確率的検出とpublic non-seat prosecutionに、producer bondが時間内のclaim機会とblock/reward/Final weightの総量を制限する前提を追加する。計算省略で枠を増やさず、客観裁定後は報酬失効と徴収可能担保への責任を執行する。
public challengerの別責任・資源枠をproducer採掘capと同一視しない。正当な追及を包含できる能力、実効検出/徴収率、共有担保枯渇と自己共謀還流を評価し、capだけでG14/Rule E/ECON/MEASを満たしたと扱わない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

---

<a id="source-v1-0015-L00380"></a>

### 旧RFC-0015 / 原文380–524行

出典: [0015-panel-free-permissionless-verification.md](../rfc/0015-panel-free-permissionless-verification.md)。配置理由: Panel=0に依存しない共通bond・配分の採用規則を分離。

##### 8.3 Class-Independent Bond Production, Reward and Final Weight Caps — 2026-10-10追加決定

###### 8.3.1 目的・claim容量・三つの総量上限

**一定額のbondが一定DAA期間に得られる報酬対象ブロック数と報酬総額の上限を、
モデルや計算量とは独立に固定し、Final後の確定weightにも同じ資本・期間による総上限を課す。**
経済的な発行能力の単位は個別claimの計算速度ではなく、
拘束されたbond資本と合意上の共通DAA期間である。
同じclass・仕事量のclaimだけを比較する規則ではなく、異なるclass間にも共通の上限を適用する。

同一rulesetの下で、producer bondを`b`、固定された適格担保額を`C`、
合意上の集計期間を`I`、その長さを`W` DAAとして、次をMUST満たす。

~~~text
N_claims(b, I) <= Q_max(C, W, rho) // 共通倍率に対応するclaim発行上限
N_blocks(b, I) <= N_max(C, W)     // Block Cap: 報酬対象ブロック数
R_total(b, I)  <= R_max(C, W)     // Reward Cap: 帰属し得る報酬総額
FinalWeight(b,I) <= F_max(C, W)   // Final Weight Cap: 確定consensus credit
~~~

`N_max`、`R_max`、`F_max`は共通rulesetの下で担保額と期間だけから導出する。
`Q_max`は共通のclaim容量倍率`rho`にも対応するが、倍率増加で三つの総量上限を増やさない。
同額bond・同期間・同倍率なら、正直なminerと高速に偽造するminerの発行上限は同じである。
Model ID、class ID、PWU、実行時間、推論token数、主張した計算量、計算速度で
claim容量とこの三つの総量上限を増やしてはならない（MUST NOT）。
class/仕事量は実行仕様・検証手順・検証資源・必要な損失担保を決めるために使用する。
個別classのadmission、chain全体の予算、未解決責任によって実際に使える機会が減ることはあり得るが、
それらをbondの共通上限の加算枠にしてはならない。

Block Capだけでは高額rewardの経路を選ぶ迂回を防げず、Reward Capだけでは
大量のclaim/blockによる検証負荷やfork-choice操作を制限できない。
三つの総量上限に加え、未決済claim数・weight・最大責任額の別capをMUST維持する。
無報酬claimによるspamをBlock Capだけで防いだと扱わない。

これは同額bondの**最大獲得機会の上限**を共通にする規則である。
実際の報酬額、上限まで採掘できること、同じ実終了時刻を保証しない。
報酬は必要な計算・適用modeの検証・Final条件を満たしたclaimにのみ発生し、
計算を省略した者へ同額の報酬を無条件に支払う規則ではない。

###### 8.3.2 全報酬・確定weight経路の予約と清算

Block Cap / Reward Cap / Final Weight Capは全てのproducer報酬対象経路にMUST一貫して適用する。
Attempt、Free Prompt、claim-backed block、receipt block、対象となるwork slice/rider/batch、
未払escrow、将来の報酬請求権を、単一claimの発行数だけで代用してはならない。
一つのclaimから複数の報酬対象blockや支払が派生する場合も、全てを同じbond会計へ束縛する。
root/sliceの同じ仕事へ別rewardを新設せず、既存の発行・escrow資金制約も維持する。

1. claim受理時に、帰属するproducer bond、共通ruleset、受理DAA `d`、
   claim発行枠、派生blockの最大数、最大報酬責任とFinal weight配分を合意状態へ予約する（MUST）。
   将来の権利行使数・報酬額を有界に予約できないclaimを報酬対象へadmitしてはならない。
2. block生成・権利行使・報酬付与/成熟・Final weight付与の各経路でも同じ上限と予約残高をMUST再検査する。
   claimだけを制限して、後続receiptや別支払経路で上限を超えることを禁止する。
3. 未払escrowと将来請求権は潜在的な報酬としてMUST予約に含める。
   支払時には同じ予約を消費済みへ移し、予約と支払を別々の新規rewardとして二重計上しない。
   payoutを後の期間へ遅らせて古い権利と新しい発行枠を無制限に合算してはならない。
4. claim・block・reward・Final weightの帰属、集計期間、権利の発行/行使/支払時点の対応をMUST固定する。
   rolling/epoch方式、期間境界、繰越、担保増減の扱いは§14で定めるが、
   どの方式でも同じ資本を同時に重複計上して共通上限を迂回してはならない。

これらは個別bondへの追加制約であり、総subsidyを増やさない。model別配分は§8.5/ADR177で改定するが、
可変配分から個別capを超える報酬や未予約の権利を発生させない。
Tx feeその他の支払を含む`R_total`の構成と帰属もrelease前に列挙し、
producerの計算報酬を別の支払名目へ移して除外することを禁止する。

###### 8.3.3 共通DAA拘束と責任担保の分離

受理DAA `d`で消費した経済的発行枠の通常の最早回復時刻を、
class/仕事量によらない共通期間`W`を用いて`reuse_not_before = d + W`に固定する（MUST）。
秒・日への換算を合意時計と同一視しない。

早期の検証成功、任意audit/Valid報告、早いFinal、claim取消、void/conviction、
再送、別session、同じ仕事の分割によって、`d + W`より早く発行枠を返してはならない（MUST NOT）。
失効した報酬の予約を新claimのReward Capとして即時再利用することも禁止する。
量産した不正claimは同じ経済的発行枠を消費し、計算省略によって枠の回復を早めない。

**発行枠の回復と、slash可能な担保・未払報酬の解放は別の条件である。**
challenge、court、DA、retention/liabilityの責任が残る限り、必要な担保と未払報酬をMUST保持する。
`d + W`到達だけで既存のslash責任を消滅させず、同じ担保を新旧claimの損失吸収へ重複使用しない。
発行枠が回復しても、実際の新claim受理には残存責任控除後のheadroomを要求する。
slashされた資本を元の担保額`C`のまま再利用可能として数えてはならない。

§6のChallengeable開始時刻・base window・hard deadlineは公開反証の時計であり、
`d + W`の発行枠回復時計と別に検査する。`W`経過だけではFinal・報酬成熟を認めない。
claim固有の適格裁定/証拠義務が未解決なら、Final/担保解放は当該規則で処理する。
model取得不能だけではFinal/defaultを起動せず、実効検出不足を経済評価の未解決リスクとして扱う。
本RFCの「事後反証」はclaim受理後の公開反証を指し、不可逆な経済Finalと
責任担保の解放後に無条件で取り消せることを意味しない。

###### 8.3.4 同額資本の分割・再登録・別経路による迂回禁止

同額の拘束資本を複数bondへ分割しても、Block Cap / Reward Cap / Final Weight Capの合計を
一つのbondへ集約した場合より増やしてはならない（MUST NOT）。
最小枠・丸め・burstを含む導出式についてこの性質を検証する。
bond退出・再登録・鍵変更・担保移転によって既存の消費履歴、`reuse_not_before`、
未解決責任を消して満額の初期枠を再付与してはならない。
追加の独立した資本には規則上の追加枠を認め得るが、同じ資本の再表示を追加資本と数えない。

複数class、Free Prompt、receipt権利の譲渡/償還、slice分割、複数の支払経路でも
同じproducer帰属と集約上限をMUST維持する。
reorg/restart/IBDではcanonicalな予約・消費・回復時計・責任を決定的に復元し、
同じcanonical workの二重支払や時計の初期化を禁止する。
未検証claimの不可逆なfork-choice利益、報酬、EVM/state決済利益は§9で別途制限し、
これらのcapだけで防止済みとは扱わない。

###### 8.3.5 現行issuance slotsとの関係

現行[`palw_issuance_slots_v1`](../../consensus/core/src/palw_issuance_slots_v1.rs)は
bond額からslot/token容量を導出する既存のqueue/DoS制限であり、経済安全性全体の証明ではない。
新規Attemptが主対象で、counted `ReceiptLicensed`等でslotを早期解放し、
この機能単独では全報酬経路のBlock Cap / Reward Cap / Final Weight Capを保証しない。
tokenのDAA補充と、phase依存のslot解放を同じ時計と呼ばない。

2026-10-10の[pre/t12コード確認](../audit/palw/design-reviews/t12-bond-reuse-audit-2026-10-10.md)では、
`palw_t12_shipped_params()`のF-SはDAA 1,700から有効となる設定である。
過去のlaunch/dormant fixtureやsource冒頭の休眠説明を、現在の組立設定と同一視しない。
これは稼働fleetのbinary/tipを確認した記録ではない。
同確認記録のclass/work依存の拘束期間という提案は、本節のclass非依存の共通`W`へ改定する。
既存のslot/bucketを利用する場合も、本節の全経路・共通拘束・報酬会計を実装し、
§13の試験と別の協調upgradeを経てから新modeへ適用する。文書追記で旧claimの規則を変更しない。

###### 8.3.6 経済安全性の判定と未確定値

不正反証はPanelの承認を要さず、適格な独立public verifierが公開証拠で開始できなければならない。
期限内の有効な客観証拠には§8.2の裁定・void・slash・報酬失効を適用する。
計算省略で同額bondのブロック数・報酬総額の上限が増えないことを先に確認し、
その上で正直な計算の期待利益が不正を上回ることを経済分析する。
claim/戦略ごとの単純化した比較条件は次である。

~~~text
p * (R_risk + L_collectible_net) > C_saved
~~~

`p`は責任期限内の実効検出・裁定・徴収確率、`R_risk`は検出時に実際に失効させられる報酬、
`L_collectible_net`は実際に回収できる担保損失から攻撃者側への還流を控除した額、
`C_saved`は偽造により節約できる計算費用である。
producerとreporterが共謀する場合は、現行PALW reporter bountyの49%規則を引き継ぐ場合等の実際の還流を
控除し、nominal/gross Slashをそのまま攻撃者の損失と数えない。
同時claimが同じbondを共有する場合も、同じ担保損失をclaimごとに重複して計上しない。

Reward Capは報酬経路の最大不正利得を有界にするが、`p > 0`や十分な期待損失を保証しない。
fork-choice/外部市場等の追加利得が残る場合は上式だけで判定せず、その利得も評価する。
Block Cap、Reward Cap、`W`、責任期間、検出率、徴収可能額は現行規則の読み取り、
実測、敵対的経済分析から固定する。10,000 BILIや100件/7日を既定値にしない。
現行t12のproducer最小bond 13,000 BILIを、この追記で変更しない。

---

<a id="source-v1-0015-L00544"></a>

### 旧RFC-0015 / 原文544–679行

出典: [0015-panel-free-permissionless-verification.md](../rfc/0015-panel-free-permissionless-verification.md)。配置理由: Panel=0に依存しない共通bond・配分の採用規則を分離。

##### 8.4 Claim容量の倍率とbond当たり総影響の保存

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を共通前提として適用する。
Bondが採掘速度と報酬総額の上限を決め、モデル計算はその枠を使用するために必要な正当な仕事となる。
不正をしても枠は増えず、公開反証・客観裁定で有罪になれば報酬失効と適用規則上の担保損失を負う。

claim容量を`m`倍にする場合、一件の経済的権利を細分化するか同じ予算から配分することで、
同額bond・同期間のブロック権利・報酬・確定weightの合計を維持する（MUST）。
同じ大きさの権利なら`N → mN`に対して一件の配分は`A → A/m`、合計は`N × A`のままである。
これは説明用の関係であり、全claimの報酬額を同じにする、実行量/PWUを偽装する、
物理blockを小数個発行する、上限までの実採掘を保証する規則ではない。
block権利の端数は合意上の整数/fixed-point台帳で集約し、実際のblock発生時にも総上限を検査する。
端数切上げ・最小credit・一claimからの複数支払・期間境界で総量が増えることを禁止する。

Final前の合計weight上限だけでは本原則を満たさない。Final時の付与、retiredへの移動、
conviction/reversal、fork-choice/DAAとsettlementの全writer/readerで、同じ予約済み配分と上限をMUST使う。
現行の`palw_weight_final_safe_v1`が原則full contributionを加算する経路は、新規則へ移行する際に改定する。
旧claimは旧rulesetのままとし、旧責任を残したupgradeで新しい満額枠を重複取得させない。
EXEC_TX/EXEC_SLICEのweight/DAAは0を維持し、この原則だけで時計や乱数sourceを増やさない。

一件の権利・予約を小さくし、正直なminerが数十万BILIの担保なしに実用的なclaim数を扱える方向を選ぶ。
必要な計算・公開検証・court/DA費用・徴収可能な損失担保は維持し、§8.3.6–8.3.7の実測と期待利益評価を要求する。
共通上限は確率的検査・外部監視・exact courtの代用品ではなく、全条件のANDでPALWの成立を評価する。

##### 8.5 モデル別bond連動coinbase allocationと入手への不介入

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)をPanel=0でも適用する。
model配布・Seeder・取得可否はchainが管理せず、旧availability/TRDC/FPR gateを撤回する。
固定Model ID/root/specとclaim固有の有限証拠/court責任は維持する。

###### 8.5.1 集計と配分

modelへ予約/拘束されたminer元本を、受理済みsnapshotで重複なく集計する。

~~~text
sum_m C_{b,m}(t) <= C_b_effective_locked(t)
S_m(t) = sum_b C_{b,m}(t)
A_m(t) = f(S_m(t)), f >= 0, f(0) = 0
R_m(t) = R_PALW(t) * A_m(t) / sum_j A_j(t)  [positive denominator]
sum_m R_m(t) <= R_PALW(t)
~~~

claim件数/予約額、rho、root/slice/rider、鍵分割、model複製で同じ元本を増幅しない。
市場seed/AMM/Position/Panel/配布契約担保は自動算入しない。責任控除・slash/退出・model移動、
snapshot/積分・期間境界を規範化し、短期借入や同時多model割当による二重配分を防ぐ。
7日は説明例であり、配分epochと共通`W`の対応/具体値は未決である。
**2026-10-14改定:** 配分の平均期間は30日相当DAA、共通`W`は7日相当DAAとする（§8.5.5）。

総coinbase/subsidy・DAA発行予算を維持し、分母0ならmodel配分0とする。
整数精度/丸め残余、未配分/未使用枠、model内のproducer/owner等のlegを確定する。
資本預託だけで支払わず、有効な計算claim・Final・固有workを要する。

###### 8.5.2 個別bond capとの同時制約

受理時にmodel、配分snapshotと最大権利を固定し、network/model/bondの各予算を同時に予約・検査する。
coinbase、未払escrow、成熟、Final、retired/reversal、reader/undoで同じ帰属を使う。
外部資本でmodel配分が増えても既存bondのQ/B/R/F capと最早回復`d+W`は増やさない。
model倍率変動や移動で枠を早く回復せず、過去claimへ無予約の追加権利を遡及付与しない。
未使用model枠の再配分も個別cap以下とし、同じsubsidyを複数legで二重支払しない。
coinbase配分からblock頻度/ticket/DAA/難易度を自動変更せず、変更には別の明示的upgradeを要する。
**2026-10-14改定:** モデル間のブロック生成機会は§8.5.5の`P_m`で配る。network全体のblock頻度・DAA・難易度・fork-choice・Final weightは変えない。

###### 8.5.3 公開対閉鎖の経済評価

公開により第三者の参加資本を集めた方が、閉鎖自己資本より圧倒的に有利となることを設計目標にする。
同額`S_m`なら自己/外部資本は同じ配分であり、ウォレット/IP数を所有者の独立性証明に使わない。
線形`f`だけで既存minerの利益増を保証せず、superlinearな曲線の集中・Sybil/自己増資・予算飽和を審査する。
他model競争、model内取り分、既存bond cap、資本/配布/検証費、閉鎖による`p`低下・49%還流も含める。
全資本自前の閉鎖攻撃者と同額自己増資も比較し、優位の数値閾値/範囲を事前定義する。
正しいmodelを得られないverifierの`p`は0にもなり、capや資本増加だけで不正の期待損失を証明しない。
達成できない条件と未立証事項を報告し、未確定の倍率式を安全性保証として使わない。

**2026-10-10改定（bond集約優位）:** ADR-0177の改定により、評価対象は「公開対閉鎖の優位」から「bond集約による配分優位」へ変わる。
`A_m = S_m^α`（`α > 1`）を候補とする。個別bond上限・総予算・共通期間は不変である。大口資本の優位は残余リスクとし、`p=0`は独立gateとする。

**2026-10-14改定（最終Goal、[PRINCIPLES §0](../PRINCIPLES.md)）:** 最終Goalは「モデルの非公開を経済的に不利にすること」で、不正の完全排除は目標にしない。`p=0`は独立gateではなく、§8.3のbond単位の上限で有界化する残余リスクとする。モデルが1人にでも共有され正しいPanel（正直な検証者）を建てられれば、claim固有の公開証拠から客観的courtで不正を立証しslashできることを要件とする。評価はADR-0177の二段（モデル全体／所有者個人）で行う。

###### 8.5.4 completion / activation

同一chain/claim/proofでpeer/取得結果を変えても合意結果が変わらない試験、claim固有court scope、
資本重複排除、整数/zero-denominator/未使用枠、三層予算、同額bondの正直/偽造機会、
公開対閉鎖の経済比較、reorg/restart/IBD/移行を独立に検証する（ADR177 §3）。
現行subsidy carve/rider/emission budgetのPASSは本配分式の実装証拠ではない。
倍率式・会計/裁定実装・条件付きG14・経済評価と別の協調activationまで新modeを有効化しない。

###### 8.5.5 モデル配分の確定 — 2026-10-14決定（α = 1.5、Miner＋Verifier元本、30日平均、ブロック機会にも適用）

> **整合性注記（2026-10-22、lane rfcgap-b）:** 下の「30日平均」「`N_30`」は2026-10-16のユーザー決定（10-22再確認）で**7日相当DAA**に置き換わった（[PRINCIPLES](../PRINCIPLES.md) §0・§9、コード `consensus/core/src/palw_bond_budget_v1/allocation_v3.rs`）。本文は履歴として残す。

ユーザーの決定により、[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)の配分を次のとおり確定する。
2026-10-12改定の`S_m · μ(S_m)`（1.3〜2倍）は採用せず、比較試験用に保持する。数値の本番定数化・実装・activationは未了である。

~~~text
S_m(k)  = C_miner_claim(m, k) + C_verifier_claim(m, k)        [資金単位で重複なし、DAA kの受理済みsnapshot]
S̄_m(d)  = (1 / N_30) * sum_{k = d - N_30 + 1}^{d} S_m(k)      [N_30 = 30日相当のDAA数]
P_m(d)  = S̄_m(d)^1.5 / sum_j S̄_j(d)^1.5                        [分母0なら全model P_m = 0]

モデル別ブロック生成機会:  E[PALW block share of m] = P_m(d)
モデル別coinbase予算:       R_m(d) = R_PALW(d) * P_m(d)
~~~

* **モデル資本はMiner＋Verifierの実拘束元本。** 2026-10-12改定の「Panel bond」を、Panel=0に合わせて
  **claimに実際に紐付いて拘束され、検証上の責任を負うpermissionless verifierの元本**へ一般化する。
  固定Panel seatは要らず、誰でも検証者として参加できる。待機しているだけの検証者bond、登録料、AMM/Position、未払の予定報酬は数えない。
  同じ元本をMinerとVerifierで二重計上しない。Verifier元本はモデルの`S_m`にだけ加え、Miner個人のQ/B/R/F capの元本にはしない。
* **同じ比率を一度だけ使う。** `P_m`をブロック生成機会とcoinbase予算の共通目標にし、1 block当たりの報酬に再び掛けない。
  network全体のblock頻度・DAA・難易度・beacon・fork-choice・Final weightは変えない。
* **個別Minerの上限は7日相当DAA。** 共通拘束期間`W`を7日相当DAAとし、各bondのQ/B/R/F capはモデル配分が増えても増えない。
  モデル枠を実際に使うには、それに見合うMinerと計算能力が要る。資本預託だけでは支払わず、有効claim・Final・固有workを要する。
* **マイナー報酬はモデル登録者へ直接分配しない。** モデル別のcoinbase（`R_m`）とブロック報酬は、claimを生成したMinerと、
  検証責任を果たしたVerifierにだけ帰属する。登録者・ownerへのcoinbase legは置かない（[ADR-0091](../adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)の
  「報酬からのowner service feeはNo」を維持）。登録者の収入は**ModelPositionの売買手数料**である。売買ごとのBILI額の5 %がownerへ入り、5 %がburnされる
  （[ADR-0114](../adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md)。fence `palw_model_leg_v2`で[ADR-0087](../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md)の1 %から引き上げ）。
  モデルが使われてPositionの売買が増えるほど登録者の収入が増える。報酬の5 %によるPosition買い上げ（ADR-0091）はlegもburnもなく、価格を上げるだけで登録者へ払わない。
  登録者が自らMiner/Verifierとして参加した場合は、その役割の報酬を他の参加者と同じ規則で受ける。
* **Verifier元本の未決事項。** Panel=0での検証者の参加手順、検証責任の範囲、拘束期間、報酬条件、bondを預けるだけで実際には検査しない問題への対策は、
  release前に明示的に定義する。Verifier元本の合算は「複数の検証者がいないモデルは無効」という規則を含まない（検証attestation gateは別の決定、readiness §3f）。
* **必須の受入試験。** `α = 1.5`は資本集中を強める。新規モデルが採掘機会を失いすぎないこと（新規参入時の`P_m`、上位モデルの資本比と配分比、
  30日平均による立ち上がりの遅れ）を、有効化前の必須評価とする。

組み合わせの全体像:

| 項目 | 決定 |
|---|---|
| 配分曲線 | `α = 1.5`（`μ`案は比較試験用に保持） |
| 平均期間 | 30日相当DAA |
| モデル資本 | Miner＋claimへ実際に拘束されたVerifier元本 |
| 固定Panel | 0 |
| ブロック生成機会 | `P_m`に比例 |
| coinbase | 同じ`P_m`に比例、二重乗算なし |
| 個別Minerの上限 | 7日相当DAAのQ/B/R/F |
| モデル登録者の収入 | ModelPositionの売買手数料（5 %、ADR-0114）のみ。マイナー報酬は直接分配しない |
| モデルの取得・共有 | オフチェーン |
| 「不正の期待利益が負」 | 最終Goal・gateにしない（PRINCIPLES §0） |
| 非公開モデルの残余リスク | 明示的に許容し、bond上限と公開反証で対応 |

---

<a id="source-v1-0015-L00954"></a>

### 旧RFC-0015 / 原文954–968行

出典: [0015-panel-free-permissionless-verification.md](../rfc/0015-panel-free-permissionless-verification.md)。配置理由: Panel=0に依存しない共通bond・配分の採用規則を分離。

#### ユーザー方針 2026-10-15 第4回 — §8.5.5 の配分式に階層係数

> **統治する前提:** [PRINCIPLES §9](../PRINCIPLES.md)(凍結前提)。規則は [ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) が定める。

§8.5.5 の `P_m(d) = S̄_m(d)^1.5 / Σ_j S̄_j(d)^1.5` を、次のように読む。

~~~text
P_m(d) = c_m · S̄_m(d)^1.5 / Σ_j c_j · S̄_j(d)^1.5
c_m    = 1.0  (m ∈ ApprovedModels(h): genesis と年次フォークの指定、追加のみ)
       = 0.3  (後から登録したモデル、登録時点から固定。モデル単位の審査はない)
~~~

* 係数は一度だけ掛け、ブロック生成機会と coinbase 配分の両方に使う。
* 登録 tier 全体の発行上限 `κ` を加える。
* `α = 1.5`、Miner + Verifier の 30 日平均は変えない。

---

<a id="source-v1-0016-L00001"></a>

### 旧RFC-0016 / 原文1–111行

出典: [0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)。配置理由: 主題別統合。

### RFC-0016 — bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **設計前提:** [PRINCIPLES](../PRINCIPLES.md) §0（2026-10-14、モデルの非公開を経済的に不利にする。不正は排除ではなく有界化）を最上位とする。
> [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)（bond単位のQ/B/R/F上限と共通拘束）、
> [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)（モデル入手への合意不介入とモデル別配分）、
> [RFC-0015 §8.3–8.5](../rfc/0015-panel-free-permissionless-verification.md)（2026-10-14確定: `α = 1.5`、30日平均、Miner＋Verifier元本、ブロック機会にも適用）を前提とし、変更しない。
> 本RFCは、bondの多いモデルが**物理的に**多くのブロック生成機会を得る設計と、人気モデルが検証容量の不足でFinal待ちにならない設計を扱う。

> **改訂(2026-10-16 ユーザー決定)— 本文中の「30 日平均」「Panel=0 は 7 日、Panel=1 は 1 日」は、この改訂で置き換える。**
>
> 1. **平均期間を 30 日から 7 日に変更する。** モデル別配分 `A_m = c_m · S̄_m^1.5` の `S̄_m` は、Miner と claim に拘束された Verifier の元本の 7 日平均とする(コード `PALW_ALLOCATION_AVERAGE_DAYS_V1 = 7`)。
> 2. **拘束期間は Panel=0・Panel=1 とも 1 日に固定する**(コード `PALW_BOND_HOLD_PANEL_ZERO_DAYS_V1 = PALW_BOND_HOLD_PANEL_ONE_DAYS_V1 = 1`)。mode による差は無くなり、mode を切り替えて発行能力を取り戻すことはできない。
> 3. **1 日あたりの上限で計算する。** miner bond から生じるブロックと報酬には上限がある。不正をして高速に claim しても、実際に計算して claim しても、どちらも上限に当たる。つまり bond が時間内にできる claim の量は変わらない。この前提の計算を 1 日単位に固定する(Q/B/R/F は 1 日あたりの `q, b, r, f`)。
> 4. **RFC-0017 の階層係数との関係。** 追加モデル(add model)は 0.3 倍で不利になる、という前提は本 RFC の後にできた(RFC-0017 → [ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md))。
>    上限が共通なので、0.3 倍の追加モデルで攻撃を成立させるには、より多くの bond が要る。本 RFC の bond 加重は、この前提と合わせて評価する。
> 5. **肝心な条件。** claim を 1 回不正すれば slash される。その slash による損失が、不正によって得られる利益を上回ること。上限によって不正の速度の利点を消すことと、この損失条件の両方がそろって、初めて抑止が成り立つ。
>
> 下の表と節に残る「30 日平均」「Panel=0 の 7 日」「`7q, 7b, 7r, 7f`」は、改訂前の記録である。

**トークンの表示名は Misaka、ticker は BILI。** 既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Accepted design (V+), values pending measurement — 2026-10-15。物理層の設計としてV+を採用した（第3回のユーザー判断）。
  値（`σ`、`k`ほか）は測定で決める。**有効化は§14のgateで止めたまま**であり、下の3つの「ユーザー判断」表は本番値の承認でも有効化の承認でもない。
* **Implementation status:** **implemented dormant, not activated, unbuilt**（branch `integ/rfc0016-impl`、§16。ADR-0176/0177とRFC-0017のbranchをmerge済み）。
  * P4: `eee1710ce`。
  * V+・弾力的Final容量・quote・report・test source・simulation: §16と
    [rfc-0016-implementation-status-2026-10-15.md](../design/palw/rfc-0016-implementation-status-2026-10-15.md)（行 → file:line）。
  * build・test・実node試験は未実施で、PASSではない。値はすべてplaceholderで、測定記録がないため、validationはどちらのfenceのarmも拒否する。
* **Source baseline:** `origin/pre` `4fb381a87`と、統合branch `integ/rfc02-rfc06-12-g14` HEAD `7dd86f2a0`。
* **Activation:** なし。fenceは`Params::palw_model_block_rights_v1`と`Params::palw_elastic_final_capacity_v1`（休眠）。
  全presetで`None`、Some-onlyでhash、`Some(never())`は`None`へ畳む。armすると拒否される（測定値記録のgateと、未有効化のgate）。
  高さ・wire tag・RPC op・署名domainは割り当てない。
* **Not activated by:** 本RFCやcodeのmerge、test PASS、本表の設計判断。
* **Normative language:** MUST / MUST NOTは、その設計を採る場合の要件である。採用を意味しない。

#### ユーザー判断 2026-10-15（第1回: review、拘束）

| # | 判断 |
| --- | --- |
| R1 | RFC-0016はDraftのまま設計を続ける。B+を採用済みとも、元の目標を達成する設計とも書かない |
| R2 | 「物理ブロック生成機会」（共通W抽選の当選と、実際に生成されたblock）と「報酬対象ブロック権利」を分ける。経済的誘導による収束を主張するなら、条件と反例を書く |
| R3 | `α = 1.5`、30日平均、共通拘束（7日相当）は確定。280–436 DAAと400 DAAは選択肢ではなく、直すべきcodeのずれである。個別capを、配分を使い切らせるために引き上げない。使い切れない配分は報告する |
| R4 | Panel=0の前提（claimとblockの対応、B/R/Fの予約、coinbaseとescrowの区別）を先に確定する |
| R5 | 試験を追加する（entrant reserveの予算控除、モデル分割、容量0、容量の二重計上、queue中の予約、quote後の競合、無効claimによる希釈） |
| R6 | 目標`P_m`、物理block、報酬権利、Final、実際の支払、既存miner個人の収入を別々に報告する。目標シェアの増加だけで改善を報告しない。受入は物理・支払の列で測る |

#### ユーザー判断 2026-10-15（第2回: 設計決定、本番値・有効化の承認ではない）

| # | 決定 | 本文 |
| --- | --- | --- |
| D-P3 | **Kernel claimにcoinbase subsidy legを作らない。** 利用者払いescrowの報酬は残し、既存のV2 coinbase経路とは分ける | §2.3 |
| D-P1/P2 | **`B = 0`、`F = 0`を維持する。** 通常のKernel claimはtransactionで運ばれ、それだけでは独立の報酬対象ブロック権利もFinal weightも作らない。Q、R、責任担保、carrierの資源上限は残す | §2.2 |
| D-P4 | **clipを禁止する。** coinbase由来のモデル予算が利用者escrowを削る経路（§2.4）を直す。producerの共通R capは適用する。最大報酬予約が足りない場合は**受理前に拒否**し、受理後にclipしない | §2.4、code `eee1710ce` |
| D-P5 | 今回の変更では既存のV2 Panel検証経路を残す。Panel=1への移行には、検証・rotation・責任・Final条件の別途確認が要る。**Kernel経路のPanel=0が、coinbaseを受け取る採掘経路をPanel-freeにすると書いてはならない** | §2.5 |
| D-Q1 | **bondに基づく物理ブロック生成機会を優先する。** 全モデルで1計算あたりの当選確率と報酬が完全に等しいこと（ADR-0137）は絶対条件ではない。物理抽選または事前発行権への接続を設計目標とする。B+だけで物理目標を達成したと扱わない。総発行予算、個別cap、検証条件、時計・heartbeatの保護は維持する | §7・§8 |
| D-H | **拘束期間: Panel=0は7日相当DAA、Panel=1は1日相当DAA。** modeを切り替えても発行能力を再取得できないこと（予算のreset・二重取得の禁止）は必須。件数・予約額・burstは、正直な最小構成の連続REAL生産と、高速偽造側の上限を同時に測って決める | §3 |

#### ユーザー判断 2026-10-15（第3回: 設計採用、本番値・有効化の承認ではない）

| # | 決定 | 本文 |
| --- | --- | --- |
| D-V1 | **V+を、RFC-0016の物理層の設計として採用する**（旧V1を決定）。ほかの物理層の手段とB+は「検討した代替案」に下げる | §7・§8・§9 |
| D-σk | `σ`と`k`は**測定で**適切な値に決める。codeでは名前付きのfence定数として置き、測定値記録のgate（未測定・placeholderの値ではarmを拒否）を付ける | §8.6、code `palw_rfc0016_v1::values` |
| D-impl | 指示どおり実装を続ける。実装は採用・有効化を意味しない | §16 |

#### 0. 要約

1. **二つの「ブロック生成機会」**
   * 物理ブロック生成機会: 共通`W_work`抽選の当選と、実際に生成されたモデルblock。cadence・DAA・fork choiceに効く。
   * 報酬対象ブロック権利: 台帳上の権利。報酬だけに効く。
   * 現行コードでは、物理blockは計算量だけで決まり、bondは入力にない（§1）。
2. **D-Q1により、物理層をbondへ接続することが設計目標になった。**
   §7で物理層の手段（ticketの傾き、採掘前の抑制、モデル別sub-target、beacon slot、事前発行権）を比較した。
   **採用した設計は「事前発行のモデル別ブロック権利（σ上限付きの途中再発行つき）を、抽選前のquoteとadmissionで要求する」V+である**（§8、D-V1。値は測定待ちで、未有効化）。
   これにより、bondの多いモデルは物理blockの**上限**を事前に保証される。実際に物理blockを生むには、そのモデルを掘る計算が要る。
   **計算のないモデルに物理blockを生ませる規則はない**（反例E1）。
3. **B+（報酬権利だけ）** とV+以外の物理層の手段は、検討した代替案である。B+は物理目標を達成しない（§9）。
4. **Panel=0のKernel claim**
   * 抽選も物理モデルblockもなく、報酬は利用者払いescrowだけである。
   * `B = F = 0`とcoinbase legなしは決定済みである。
   * `P_m`がKernel claimに配るものはない。
   * coinbaseを受け取る採掘経路は、V2のPanel検証のままである（§2）。
5. **拘束期間**はPanel=0が7日、Panel=1が1日である。調整の対象は、bond・期間あたりQ/B/R/F・予約額・burstの組合せで、1日あたりの発行をそろえる。mode切替で発行能力を取り戻せない（§3）。

#### 1. レビュー主張の検証

`pre` = `4fb381a87`、`HEAD` = `7dd86f2a0`。

| # | 主張 | 判定 | 根拠（pre / HEAD） | 補足 |
| --- | --- | --- | --- | --- |
| (a) | 共通`W`、`p_m = min(1, CCU_m/W)`、claim数は入力にない | **CONFIRMED** | `palw_admission_v2.rs` pre L1130–1146 / HEAD L1134–1150。`palw_work_target_v1.rs` L116–125。floorはpre L1106 / HEAD L1110 | HEAD L1095–1103の`native_material_ticket_v3`も同じ関数を読む。claim数は容量admissionの入力である |
| (b) | epochごとの`W`のstep、共通、t12はDAA 0から | **CONFIRMED** | `palw_work_target_v1.rs` L94–114・L34。`params.rs` pre L10377–10401 / HEAD L10499–10523、HEAD L12303 | RC presetは7,101と7,780（HEAD L12022・L12038） |
| (c) | 容量とinflight上限が拒否する。人気モデルは検証資源で律速される | **PARTLY** | `fold_read.rs` pre L901 / HEAD L901–1063。`ClassInflightCapped` HEAD L1016・L1060。`PanelRoomExhausted` HEAD L1033・L1048。`palw_work_target_v1.rs` L205–245 | 8kはrate roomだけで判定される（L222–229）。roomはready seatに比例する（HEAD L1209–1217・L1266）。class間で差し引きがある（`palw_state_v2.rs` HEAD L13130–13146）。OPVは担保だけを見る（`ledger.rs` L2116–2129） |
| (d) | `P_m`は確定、engineは休眠、Rだけを削る | **CONFIRMED**（追加あり） | RFC-0015 §8.5.5 HEAD L627–674。`palw_bond_budget_v1.rs` L1–15、HEAD L2680、L711–734。設計書§3.5–3.6 | codeの`S_m`はminer-onlyのepoch snapshot（L10、L520–531） |
| (e) | backlog、配分と抽選の衝突、capによる制約 | **CONFIRMED**（将来リスク） | 上記とX1・X4 | 分析は§6 |

| # | レビューが扱っていない事項 | 根拠 |
| --- | --- | --- |
| X1 | Kernel claimは`B = F = 0`で、報酬はjob escrow | `palw_bond_budget_fold_v1.rs` L1501–1545、`ledger.rs` L147–151 |
| X2 | R8-BUDGET: fee-only Roundは独立の`ExecutionCap`、v1 market feeは`R_max`外 | RFC-0008 L292–301 |
| X3 | codeの拘束期間（280–436 DAA）とRFC08の400 DAAが、決定（Panel=0は7日、Panel=1は1日）とずれている | `palw_bond_budget_v1.rs` L32–37。§13のC1 |
| X4 | capは資本に線形、`P_m`は超線形 | `palw_bond_budget_v1.rs` L796–813。§9.6 |
| X5 | ADR-0177 D4・D5 | ADR-0177 L89–122・L238–250 |
| X6 | M\*-49の検査数は固定定数 | `palw_verifier_pay_v1.rs` L1–35 |
| X7 | attestation gate | PRINCIPLES §0-1、readiness §3f・§3k |
| X8 | F-Rはstake比例で、出荷では休眠 | `palw_verify_capacity_v1.rs` L21–30、`params.rs` HEAD L10769–10790 |
| X9 | 事前確認はあるが予約ではない | `palw_state_v2.rs` HEAD L12217、`palw_producer_v2.rs` L246–259 |
| X10 | ADR-0137 D5はモデル別配分を否定した。D-Q1でこの絶対性は解除される | ADR-0137 L303–325 |

---

<a id="source-v1-0016-L00157"></a>

### 旧RFC-0016 / 原文157–674行

出典: [0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)。配置理由: 主題別統合。

#### 3. 拘束期間・発行能力の調整とmode切替（D-H、ADR-0176と整合）

実装はADR-0176側（branch `integ/adr-0162-0178`の別agent）が担当する。本RFCは設計上の要件を書くだけで、code作業を重複させない。

##### 3.1 調整の対象

調整するのは拘束日数だけではない。次の**組合せ**全体である。

* bond額
* 期間あたりのQ/B/R/F
* 1 claimあたりの予約額
* 短期のburst・rate上限

**主条件（MUST）:**

1. 同じbond・同じmode・同じ期間なら、高速偽造は正直な計算より多くのQ/B/R/Fを得ない。
2. 基準となる正直なminerがcapに実際に到達できる。
   基準miner: Qwen2.5-1.5B A16の固定profile、基準機材、producer最小bond。
3. 需要・機材・検証がそろっているのに、capが低すぎてREAL生産が止まることはない。
4. heartbeatを0にしない。時計・fallbackの規則（ADR-0066/0140/0142/0165）を弱めない。

##### 3.2 Panel=0とPanel=1の比較

* 最初の比較では、**1日あたりの最大発行**をそろえる。
  * Panel=1: 1日あたり`q, b, r, f`。
  * Panel=0: 7日あたり`7q, 7b, 7r, 7f`。
* 同じ件数で7倍速く回復させる形にしてはならない（MUST NOT）。
* `W_hold(Panel=0)` = 7日相当DAA、`W_hold(Panel=1)` = 1日相当DAA。どちらも**通常時の最早の再利用時刻**である。

##### 3.3 能力の回復と担保の解放は別

* 発行能力の回復（`accepted_daa + W_hold(mode)`）と、担保の解放は別の条件である。
* 未解決のdispute・court・DA・責任期間がある限り、担保は保持する（MUST）。
* 共謀した早期のValid・早期のFinalで、Panel=1の1日を短くできない（MUST NOT）。
* 同時に必要な担保はおおよそ`λ · a · H`である。`λ`はclaimの率、`a`は1 claimあたりの責任担保、`H`は責任期間である。

##### 3.4 短期のburst上限

* 週の総量の上限だけでは足りない。短い期間のburst・rate上限を追加する（MUST）。
* 消費した能力を早く戻してはならない。二重に発行してはならない（MUST NOT）。

##### 3.5 mode切替

* reservationの最早回復時刻は受理時のmodeの`W_hold`で固定し、modeを切り替えても短縮しない（MUST）。
* mode切替では、消費履歴・hold・残存責任をすべて引き継ぐ（MUST）。window・履歴をresetしてはならない。mode別にwindowを新設して二重取得してはならない（MUST NOT）。
* 1つのbondのwindowは全modeで共有し、capは全modeの予約の合計に適用する。
* test source `rfc0016_a_mode_switch_never_reacquires_issuance_capacity`（`eee1710ce`、未実行）は、現行の単一`W`の下でこの性質を固定する。mode別`W`の実装（ADR-0176側）でもこの性質を保つ。

##### 3.6 値の決め方（3段階）

値はここで決めない。次の3段階で導出する。

1. **正直な最小構成の能力を測る。**
   * 基準miner（§3.1）と、各profile・各modeの最小構成で、連続REAL生産を測る。
   * 測る量: claim率、Final率、検証待ち、費用、`W_hold`期間の累積のp50/p99、短窓（1 span、10 span）のburstのp99。cold・warmの両条件で測る。
   * 同じrelease・同じparams・同じ網条件で、高速偽造側（計算省略、carrier・seal遅延・cap・担保・admissionだけが上限）の最大率とburstを、解析と実測の両方で同時に測る。
2. **capと予約額をsweepする。**
   * bond額 × 期間あたりのQ/B/R/F × 1 claimの予約額 × burst上限の組合せを掃引する。
   * 主条件（§3.1）、1日あたりの発行の同等性（§3.2）、`λ · a · H`の担保、を満たす範囲を求める。
3. **敵対的・境界試験を行う。** 選んだ点で、偽造・mode切替・早期Valid共謀・burst・期間境界・reorg/restartの試験を行う。

**制約が両立しない場合は、その衝突を報告する。数値を作り出して合わせてはならない（MUST NOT）。**
測定のcommit、params、機材、網条件、生データ、導出式をevidenceとして残す。UNKNOWNはPASSにしない。

#### 4. 用語と記号

| 記号 | 意味 |
| --- | --- |
| `W_work` | ADR-0137のwork target（MAC-eq） |
| `W_hold(mode)` | 拘束期間。Panel=0は7日相当DAA、Panel=1は1日相当DAA（確定） |
| `N_30`、`S̄_m`、`P_m` | RFC-0015 §8.5.5（確定）。`P_m = S̄_m^1.5 / Σ S̄_j^1.5` |
| `c_m` | モデルの物理blockシェア（実測） |
| `N_exp(e)` | epoch `e`の期待モデルblock数（`closed_expected_blocks`、DAA数） |
| `Rt_m(e)` | §8の事前発行権 |
| `σ`、`κ`、`ε`、`τ_m`、`k` | spill上限、carry上限、entrant reserve、ticketの傾きとその上限 |

#### 5. 目標と保持する制約

| ID | 内容 | 扱い |
| --- | --- | --- |
| G1p | `S̄_m`が増えると、そのモデルの**物理ブロック生成機会**が増える | **主目標**（D-Q1） |
| G1t | 目標（`P_m`）が単調 | 定義で成立 |
| G1r | 実際の支払が増える | 物理blockとFinalに依存する |
| G2 | 1計算あたりの当選確率と報酬がモデル間で等しい（ADR-0137） | **絶対条件ではない**（D-Q1）。崩す場合はその幅を有界にし、報告する |
| G4 | 人気モデルのFinalが検証者容量に比例し、不足は報告される | §10 |
| K1 | 総発行予算（per-DAA subsidy）を増やさない | 保持（MUST） |
| K2 | 個別Q/B/R/Fと`W_hold`（引き上げない） | 保持（MUST） |
| K3 | 検証条件（Final前に払わない、court、gate） | 保持（MUST） |
| K4 | 時計とheartbeatの保護 | 保持（MUST）。§8.4 |

ユーザー指示の「ADR-0172 heartbeat/fallback」は、このrepoでは次の規則を指すものと読む。
ADR-0066/0140（独立したheartbeat lane。ADR-0140 C1–C5: 難易度を締めない、他の何かが時計を進めている間は時計を進めない、fork-choice weightを取らない、稼がない・生産を奪わない、非常時に確実に始動する）、
ADR-0142（時計はcursorで、heartbeatがslotを消費する）、ADR-0165 rev5（BASE-0はidle時だけのbonded floor、REAL attemptがslot tickを運べる。FALLBACK-V1統合は撤回）。
現在のADR-0172はversioned kernelのADRである。

#### 6. 物理ブロックと報酬権利の分析（背景）

##### 6.1 経済的誘導による収束の条件（物理層を変えない場合）

次の全部が成り立つ場合に限り、報酬側の差によって物理シェア`c_m`が`P_m`へ近づく。

* C1: networkが飽和している。
* C2: 計算が移動できる（機材、RAM、cold fetch、seatingの費用が、期待利益差を下回る）。
* C3: 計算の前にquoteで見積りが分かる。
* C4: 移動先に検証容量がある。
* C5: 移動先がgateを通っている。
* C6: 30日平均と移動に要する時間がある。
* C7: spill上限`σ`が小さい。

##### 6.2 反例

| ID | 状況 | 物理層を変えない場合の結果 |
| --- | --- | --- |
| E1 | minerがモデルを移れない（機材、モデル専用設定、RAM、cold fetch、seating） | bondの多いモデルの物理blockは変わらない。予算は余り、総支払が減る |
| E2 | 低bondモデルの需要が急増する | そのモデルの物理blockが急増し、`W_work`が上がって他モデルの物理blockが減る。30日平均は遅れて動く |
| E3 | 資本が多く計算が少ない | 物理blockは計算どおり。予算は大半が使われない |
| E4 | 計算が多く資本が少ない | 低bondモデルの当選が`W_work`を上げ、bondの多いモデルの物理blockを薄める |
| E5 | 無報酬の当選で`W_work`を吊り上げる | bondの多いモデルの物理blockが減る |
| E6 | `W₀`の領域（計算供給が少ない） | 予算が拘束せず、何も変わらない |

**結論:** 物理層を変えなければG1pは保証されない。これがD-Q1の理由である。

#### 7. 物理層の設計空間（D-Q1）— V+以外は検討した代替案（D-V1）

##### 7.1 手段

| ID | 手段 | 仕組み |
| --- | --- | --- |
| P-i | モデル別ticketの有界な傾き | `T_m = MAX · min(1, CCU_m · τ_m / W_work)`。`τ_m ∈ [1/k, k]`を`P_m / c_m`から決め、計算加重平均1に正規化する |
| P-ii | 採掘前の抑制 | 物理シェアが`(1+σ)P_m`を超えたモデルに、quoteとadmissionで新しい当選を受理しない |
| P-iii | モデル別sub-target | モデルごとの`W_m`を、`P_m · N_exp`へ向けてepochごとに調整する |
| P-iv | beacon slot割当 | 各slotのモデルを`P_m`で抽選し、slot内は共通`W_work`で決める |
| P-v | 事前発行権 | epoch開始前に、モデル別のブロック権利`Rt_m = ⌊P_m · N_exp⌋`を発行する。モデルblockは、そのモデルの権利を1本消費する場合だけ受理する。quoteは残りの権利を示し、権利のないモデルには計算をさせない |

##### 7.2 保持する制約に対する評価

| 手段 | K1 総発行 | K2 個別cap | K3 検証 | K4 時計・heartbeat | G2の崩れ |
| --- | --- | --- | --- | --- | --- |
| P-i | 飽和時は保つ。`W₀`領域では正規化が要る（ないと計算あたりの発行率が上がる） | 保つ | 保つ | 保つ（slotの供給量は不変、tickの担い手だけが変わる） | 最大`k²`倍、有界 |
| P-ii | 保つ（余ったcadenceはfloor・heartbeatが埋める） | 保つ | 保つ | floorとheartbeatの始動（ADR-0140 C5）の試験が要る | 超過モデルは0（不連続） |
| P-iii | 保つ | 保つ | 保つ | class DAA状態の復活。floorのhardening（ADR-0137 §3.5）の再発リスク | 無制限 |
| P-iv | 保つ | 保つ | 保つ | slotの空き、beaconのgrinding、block validityの変更 | slot外は0 |
| P-v | 保つ（権利 ≤ `N_exp`、未使用分は発行しない） | 保つ（権利の消費はproducer bondのBの予約の中） | 保つ | 未使用の権利の分をfloor・heartbeatが埋める。ADR-0140 C1–C5の試験が要る | 権利を持つモデル間では等しい。使い切ったモデルは0 |

##### 7.3 反例に対する評価

| 手段 | E1 移れない | E2 急増 | E3 資本≫計算 | E4 計算≫資本 | E5 吊り上げ | E6 `W₀` |
| --- | --- | --- | --- | --- | --- | --- |
| P-i | 解けない（計算なし） | 罰は`k`倍まで | 1計算あたりの当選が最大`k`倍 | 低bondモデルの当選が最大`1/k`倍。bondの多いモデルの物理blockは増える | 低bondモデルでの吊り上げを弱める | 正規化があれば中立 |
| P-ii | 解けない | 最悪（急増した人気モデルを止める） | 間接的（`W_work`が下がる） | bondの多いモデルの物理blockが増える | 止まる | 飽和時だけ効かせる |
| P-iii | 解けない（cold start trapもある） | 遅れて罰する | 効く | 効く | 効く | 振動する |
| P-iv | slotが空く | 罰する | slotが空く | 効く | 効く | slotが空く |
| P-v | 解けない。権利は余り、floorが埋める（発行しない） | 権利で頭打ち。σ再発行で緩和 | 権利で物理blockの**上限**を保証。計算があれば到達する | `W_work`が下がり、bondの多いモデルが権利まで到達する | 権利を超えられない | 権利が需要を上回り、制約しない |

**どの手段もE1を解けない。** 計算のないモデルは物理blockを生めない。
物理層でできるのは、bondの多いモデルが**計算を持っているときに**、その計算が当選しやすくなるか、他のモデルの当選を抑えることだけである。

#### 8. 採用設計 V+: 事前発行権 ＋ σ上限付きの途中再発行 ＋ 抽選前のquote（D-V1で採用、値は測定待ち）

##### 8.1 規則案

1. **発行:** epoch `e`の開始ブロックのfoldで、各モデルに`Rt_m(e) = ⌊(N_exp(e) − Ent(e)) · A_m / ΣA_j⌋`を発行する（MUST）。
   `A_m = ⌊S̄_m^1.5⌋`。`Ent(e) = ε · N_exp(e)`はentrant reserveで、固定総額から先に差し引く。新規発行ではない。
   `Σ Rt ≤ N_exp`（K1）。発行はDAA数で決め、時計を読まない。
2. **消費:** モデル`m`の当選（V2 Attempt）は、admission時に`m`の残り権利を1本消費する場合だけ、モデルblockとして受理する（MUST）。
   消費はproducer bondのB/R予約（ADR-0176）の中で行い、個別capを超えない（K2）。
   権利がないモデルの当選は受理しない。quoteで事前に知らせるので、計算を使わせない。
3. **抽選:** 権利を持つモデルの間では、共通`W_work`抽選のまま（G2は権利を持つモデル間で保たれる）。
4. **途中再発行（σ）:** epochの割合`φ`を過ぎると、需要の超過したモデルは自分の権利を超えて、解放された権利を使える。
   * 上限: そのepochの合計が`Rt_m · (1 + σ)`以下（MUST）。
   * 他のモデルの保護: 各モデルの残りの見込み`min(未使用, ⌈used · remaining / elapsed · pace margin⌉)`は使えない。
   * 全体: epochの合計`Σ Rt`は超えない（MUST）。
   * 実装の形（2026-10-15）: 状態を持たない形にした。checkpointでのsnapshotを置かず、epochのカウンタとDAAの位置だけから決まる。
     超過したモデルの間の配分は`A_m`加重ではなく、抽選の順（先着）である。`A_m`加重には新しい状態が要るため、加重にするかは測定後の判断とする（V2）。
   * 残りは失効し、そのcadenceはfloor（BASE-0）とheartbeatが埋める。発行はしない。
   * `σ`はE2（急増）の罰とcadenceの損失を緩める調整値である（`σ = ∞`ならADR-0137に近く、`σ = 0`なら厳密な`P_m`）。
5. **新規モデル:** entrant reserveの権利を線形・上限付きで配る（登録から`N_30`未満、報酬gateを通過、`S_m ≥ S_entry`）。
6. **Final・支払:** 権利付きのモデルblockのclaimは、既存の検証条件（K3）でFinalになり、claim自身のcarveの全額を受け取る（`ψ`は不要）。
   有罪・voidの場合は払わず、権利は戻らない（`W_hold`と同じ扱い）。
7. **容量との結合:** quoteは、残りの権利と検証容量（§10）の両方を見て`Admit`を返す。

##### 8.2 G1pの達成範囲

* bondの多いモデルは、物理blockの上限（`Rt_m`、再発行を含む）を事前に保証される。そのモデルの計算が上限まで当選できる限り、物理シェアは`P_m`に到達する（E3・E4で効く）。
* 計算が足りない場合（E1）、権利は失効し、目標は達成されない。この不足を`unused_rights_m`として報告する（§11）。

##### 8.3 主なリスク

* 権利の失効が多いと、モデルblockの総数が`N_exp`を下回り、`W_work`は`W₀`へ下がる。floorとheartbeatが埋めるcadenceが増える。
* 権利の最後の1本をめぐる競合（並行block）。quoteの余裕幅`h`とqueue（§10）で吸収し、浪費を報告する。
* epoch内では早い当選が権利を取る。抽選の速さで決まるので、先着の殺到は抽選のrateで制限される。
* 権利はfork-choice weightではない。権利の残数を時計・DAA・bitsの入力にしない（MUST NOT）。

##### 8.4 時計とheartbeatの保護（K4）

* heartbeat lane（ADR-0066/0140）は権利を消費せず、権利の影響を受けない（MUST）。ADR-0140 C1–C5を保つ。
  * 難易度を締めない。
  * 他の何かが時計を進めている間は時計を進めない。
  * fork-choice weightを取らない。
  * 稼がない。
  * 全モデルの権利が尽きても非常時に始動する。
* 時計のcursor（ADR-0142）: slotの供給と消費の規則は変えない。権利はslot tickを増やさない（ADR-0165 B: REAL attemptが運ぶtickの規則も変えない）。
* floor（ADR-0165 A″）: 権利が尽きたモデルの空いたcadenceでfloorがidleになり、BASE-0が採掘できる。floorは無報酬の残余のままである（ADR-0137 D3）。
* DAAと`bits`: 権利はDAA・`bits`・難易度・`W_work`の調整式（`palw_work_target_step_v1`）を変えない。`W_work`の入力（実現したモデルblock数）は既存のとおり。
* 必須試験: 全モデルの権利を0にしたnetworkで、heartbeatとfloorが目標cadenceを維持し、DAAが崩れない（§12.4のH1–H4）。

##### 8.5 その他の手段の位置付け（検討した代替案）

* **`k`の正確な意味:** V+の補助としての、モデル別ticketの有界な傾き（P-i）`τ_m = clamp(P_m / c_m, 1/k, k)`の上限である。
  `c_m`はモデルの実現した物理シェアである。
  * `k = 1`（`tilt_k_permille = 1000`）で無効になる。これが既定である。
  * 権利を持つモデルの間に残る計算の偏り（同じ権利数のモデル間の当選速度）だけを調整する。
  * 計算単位の公平性（G2）を最大`k²`倍まで崩す。
  * 抽選への組込みは未実装である（純関数`palw_rights_tilt_permille_v1`のみ）。値の測定と別途の判断が要る。
* P-ii、P-iii、P-ivは、§7の評価から採らない（検討した代替案）。

##### 8.6 `σ`と`k`の導出（D-σk）

**何を制御するか**

* `σ`: `φ`を過ぎたあと、需要の超過したモデルが使える再発行権利の上限（`Rt_m · σ`）。
  * 大きいほどADR-0137（計算比例）に近づき、E2（急増）の罰と未使用権利の失効が減る。
  * 小さいほど`P_m`に厳密になる。
* `k`: §8.5の補助ticket傾きの上限。
  * 大きいほど、bondの多いモデルの1計算あたりの当選を増やせる。
  * その分G2を崩し、人気（計算）モデルを罰する。
* 同時に決める従属の値: `φ`（再発行の開始割合）と`pace margin`（他モデルの保護の強さ）。

**測定ハーネス**

* モデル: 基準のQwen2.5-1.5B A16の固定profileと、CCUの異なる追加モデル（少なくとも重いものと軽いもの）。
* 機材: 基準機材。
* 構成:
  * 正直な最小構成（producer最小bond、1 miner）。
  * 複数miner構成（同一モデルの複数miner、モデル間を移動できるminerとできないminerの混在）。
* シナリオ: E1–E6（移れないminer、需要急増、資本≫計算、計算≫資本、`W_work`の吊り上げ、`W₀`領域）と、敵対的試験A1–A12。
* 測る量: §11の全列。特に、モデル別の物理block数・実際の支払・既存minerの個人収入・未使用権利・queue待ち・heartbeat依存のDAA。
* 道具:
  * `scripts/sim/palw-rfc0016-vplus-sim.py`: 測定する領域を選ぶためのモデルで、測定ではない。
  * 実node・実機の測定: 未作成（evidence-only）。

**目的**

* bondが多く計算を持つモデルについて、物理blockと支払が`P_m`を追う度合いを最大にする。
* 失効する権利（unminted）を有界にする。
* 未使用の権利を使った妨害（griefing）ができない。
* heartbeat・時計の規則は変えない（測るだけ）。
* 人気モデル（急増）を停止させない。

**掃引範囲（初期案、測定の前提で変えてよい）**

* `σ` ∈ {0, 0.1, 0.25, 0.5, 1, 2}
* `k` ∈ {1, 1.25, 1.5, 2}
* `φ` ∈ {0.25, 0.5, 0.75}
* `pace margin` ∈ {1.0, 1.2, 1.5}

**選択規則**

* すべての合格条件を満たす領域から選ぶ。合格条件は、上の目的のそれぞれについて事前に決めた閾値（V6）である。
* 満たす領域がなければ、衝突する制約を報告し、値を作らない（MUST NOT）。
* 選んだ値は`PalwRfc0016MeasuredRecordV1`（evidenceとindependent reviewのdigest）として`PALW_RFC0016_MEASURED_RECORDS_V1`に固定する。
  それまでvalidationはarmを拒否する。

#### 9. B+（報酬権利だけ）— 検討した代替案（D-V1）

* B+は物理抽選を変えず、`P_m`で報酬権利と支払を配る（epoch closeの`ψ`、carry、σ上限付きspill、entrant reserve）。
* **B+だけでは物理目標を達成しない**（D-Q1）。V+との比較基準として残す。
* 規則案: `Base_m = ⌊(Pool − Ent) · A_m / ΣA⌋`、`ψ_m = min(1, (min(Budget, Demand) + SpillIn) / Demand)`。
  無効・有罪のclaimは`Demand`から除き、正直な`ψ`を薄めない。
* 個別capは引き上げない。使い切れない配分（`stranded_by_bond_caps_m`）は報告する（R3、MUST NOT）。
* attestation gateで拒否されたmodel-epochは、どこからも受け取らない。

#### 10. 弾力的Final容量（Panel経路）

* **検証者単位の容量:** `Cap_m = Σ_v c_v · x_{v,m}`。1人の検証者の配分の合計は`c_v`以下で、二重計上しない。Hall条件を満たす配分だけを容量として数える。
* **C7の導出上限:** `cap_m = max(1, ⌊sigma_cap · Cap_m · window_m / claim_replay_m⌋)`。`Cap_m = 0`なら0とし、受理しない。
* **quote:** `admit / queue / refuse`、残りの権利（V+）、bondの残り、容量、余裕幅`h`を返す。side blockの需要も数える。
* **queue:** 上限付き。bondの取り分は資本比例。窓はqueueを出た時点から数え、queue中はFinalにならない。予約は1 claim分まで。`T_queue`を過ぎたらslashなしでvoidとする。
* **OPV:** 容量待ちはない。検査数を請求率から導出し、`checked_share_m`を報告する。
* **検証者の誘因:** 取り分を10–30 %の範囲内で傾ける。
* **不足の報告:** `capacity_short_m`と不足量を返す。黙ってFinalを遅らせない（MUST NOT）。

#### 11. 計測と報告

| 列 | 内容 |
| --- | --- |
| (1) 目標 | `S̄_m`、`P_m`、発行した権利`Rt_m` |
| (2) 物理block | 当選し受理されたモデルblock数、物理シェア`c_m`、権利不足で受理されなかった当選、`unused_rights_m` |
| (3) 報酬権利 | 消費した権利、再発行の入出、失効、`stranded_by_bond_caps_m` |
| (4) Final | Final数、void、有罪、`checked_share_m` |
| (5) 支払 | モデル別の支払。coinbase subsidyと利用者escrowを分ける |
| (6) 個人収入 | 既存minerごとのepochあたり支払と、計算あたり支払 |
| (7) 容量 | `Cap_m`、検証者数、queue、`capacity_short_m`と不足量 |
| (8) 時計 | heartbeat・floorが埋めたslot数、DAAの偏差 |

* 目標（列1）の増加だけで、物理（列2）や収入（列5・6）の改善を報告してはならない（MUST NOT）。
* 受入は列2・列5・列6で判定する（MUST）。UNKNOWNはPASSにしない。

#### 12. 受入試験

すべての試験で§11の全列を記録する。

##### 12.1 ユーザー指定

| ID | ケース | 合格基準 |
| --- | --- | --- |
| T1 | 同じCCU、Aの計算がBの3倍 | 物理（列2）・支払（列5・6）を測る。V+では、Aの物理blockは`Rt_A`（再発行を含む）を上限に増える |
| T2 | Aのbondだけを増やす（Aに十分な計算がある場合と、ない場合の両方） | V+: 計算がある場合、Aの物理block（列2）が増える。ない場合は`unused_rights_A`として報告する。B+: 物理は不変と報告する |
| T3 | 全モデルで急増 | `W_work`が上がる。権利は`P_m`のまま。モデル別のhardeningはない |
| T4 | Aだけが急増 | V+: Aは権利と再発行の上限まで。不足は報告する。容量不足はqueueに入り、報告される |
| T5 | 検証者を追加 | `Cap`が増え、queueが減り、Final数が増える |
| T6 | 既存bondがcap | capを超えない。使い切れない分は報告し、capは引き上げない |
| T7 | 新規モデルの参入 | entrantの権利付与と、物理block・支払を測る。下限（判断V6）以上 |

##### 12.2 R5の追加試験

| ID | 試験 | 合格基準 |
| --- | --- | --- |
| N1 | entrant reserveの予算控除 | `Σ Rt + Σ Ent ≤ N_exp`。reserveの有無で総発行の上限が同じ |
| N2 | ほぼ同一のモデルを複数登録する分割 | `P`の合計は分割前以下。entrantの総額は一定。capの合計は不変。支払と物理blockの合計が分割前を超えない |
| N3 | 容量0での受理 | `Cap = 0`のモデルは受理しないか`capacity_short`を返す。quoteは`Refuse` |
| N4 | 検証者容量の二重計上 | 検証者ごとの配分の合計が`c_v`以下 |
| N5 | queue中の最大予約 | 1 claim分を超えない。queueの合計がモデル予算・cap・権利を超えない |
| N6 | quote後の競合 | `h`の範囲内ではqueueが吸収する。超えた分は報告し、浪費率を記録する |
| N7 | 無効・有罪claimによる希釈 | 正直なclaimの支払（V+）と`ψ`（B+）が下がらない |

##### 12.3 第2回の決定に対応する試験

| ID | 試験 | 合格基準 | 状態 |
| --- | --- | --- | --- |
| P4a | escrowをclipしない | モデル予算が不足しても、Kernel claimのRはescrowの全額。モデルを記録せず、モデル予算は不変 | test source `rfc0016_p4_user_escrow_is_never_clipped_by_the_model_budget`（未実行） |
| P4b | Rの不足は受理前に拒否 | 共通R capの不足は`Exhausted { Reward }`、claimごとの上限の不足は`EscrowNotReservable`。何も書かない | test source `rfc0016_p4_insufficient_r_refuses_an_escrow_claim_before_acceptance`（未実行） |
| M1 | Panel=0とPanel=1の切替で発行能力を再取得しない | 両方向で、先のreservationが`d + W`まで共有windowに残り、早期Finalでも戻らない | test source `rfc0016_a_mode_switch_never_reacquires_issuance_capacity`（未実行。mode別`W`の実装後に拡張する） |
| M2 | mode別の`W_hold` | Panel=0のreservationは7日相当、Panel=1は1日相当で、受理時のmodeで固定される | 未実装（§13のC1） |
| M3 | 1日あたりの発行の同等性 | 同じbondで、Panel=0の7日の上限（`7q, 7b, 7r, 7f`）とPanel=1の1日の上限（`q, b, r, f`）の比が、1日あたりで同じになる | ADR-0176側 |
| M4 | 能力の回復と担保の解放 | 共謀した早期のValid・Finalでも、Panel=1の1日（Panel=0の7日）より前に能力が戻らない。未解決のdisputeがあれば担保を保持する | ADR-0176側 |
| M5 | 短期のburst上限 | 週の総量の範囲内でも、短窓の上限を超えない。消費した能力が早く戻らず、二重発行がない | ADR-0176側 |
| M6 | 基準minerがcapに到達 | Qwen2.5-1.5B A16の固定profile・基準機材・producer最小bondで、連続REAL生産がcapまで到達し、需要・機材・検証があるのに停止しない。heartbeatは0にならない | 測定（§3.6） |
| M7 | 偽造は正直より多くを得ない | 同じbond・mode・期間で、高速偽造のQ/B/R/Fが正直な計算以下 | 測定・敵対試験（§3.6） |
| K1a | Kernel claimにcoinbaseが流れない | どの経路でもKernel claimへのcoinbase支払が0 | 未作成 |

##### 12.4 時計・heartbeat（V+）

| ID | 試験 | 合格基準 |
| --- | --- | --- |
| H1 | 全モデルの権利が0 | heartbeatとfloorが目標cadenceを維持し、DAAが崩れない（ADR-0140 C5） |
| H2 | 権利の失効が多い | heartbeatが難易度を締めず、weightを取らず、稼がない（ADR-0140 C1・C3・C4） |
| H3 | 権利とslot tick | 権利の消費がslot tickを増やさない。cursorの規則が不変（ADR-0142、ADR-0165 B） |
| H4 | reorg・restart | 権利の発行・消費・再発行を決定的に復元する |

##### 12.5 敵対的試験

資本の付け替え（A1）、短期借入（A2）、Sybil検証者（A3）、容量の占拠（A5）、自己取引（A6）、`W_work`の吊り上げ（A7）、reorg（A8）、attestation gate（A9）、検査しないverifier（A11）。
合格基準は第1回版と同じで、物理・支払の列で判定する。

#### 13. codeは決定に従う（選択肢ではない）

| ID | 是正 | 状態 |
| --- | --- | --- |
| C1 | 拘束期間の定数をmode別にする: Panel=0は7日相当DAA、Panel=1は1日相当DAA。1日あたりの発行を同等にする（§3.2）。`PALW_BOND_BUDGET_W_MIN/MAX_DAA_V1`（280–436）とRFC08の`W_budget = 400`を合わせる。mode切替で履歴・hold・責任を引き継ぎ、windowを共有する（§3.5）。短期burst上限（§3.4） | ADR-0176側の実装（`integ/adr-0162-0178`）が担当。本branchでは重複しない |
| C2 | `S_m`を、30日平均のMiner＋claimに拘束されたVerifier元本にする | 未実施 |
| C3 | α = 1.5を確定値にする（`alpha_halves = 3`。interimの表記を直す） | 未実施 |
| C4 | escrowをclipしない（D-P4） | `eee1710ce`（休眠、未build） |
| C5 | Rの不足を受理前に拒否する（D-P4） | `eee1710ce`（休眠、未build） |

#### 14. 有効化gate

~~~text
ACTIVATION_ALLOWED
    = V+ が採用されている（D-V1、済）
      AND §14.1 の値が §3.6 の3段階導出に基づいて決まっている
      AND §13 の C1–C5 が完了している
      AND §12 の全試験が物理・支払の列で独立review付きPASS
      AND 実node E2E（V2 Panel経路、Kernel経路、mode切替、reorg/restart/IBD）PASS
      AND ADR-0177 の二段評価を再計算
      AND 全RFCを一度に有効化する協調upgradeに入っている
~~~

##### 14.1 本RFCで決めない値と判断

| ID | 値・判断 |
| --- | --- |
| V1 | **決定済み（D-V1）: V+を採用。** `k`の値と、P-iを抽選へ組み込むかは§8.6の測定の後 |
| V2 | `σ`、`φ`、pace margin（§8.6の測定で決める）。再発行を`A_m`加重にするか（新しい状態が要る） |
| V3 | `ε`、`S_entry`、1モデル上限 |
| V4 | bond額 × 期間あたりQ/B/R/F × 1 claimの予約額 × burst・rate上限の組合せ（§3.6の3段階で導出） |
| V5 | epochの長さ（`N_30`と`W_hold`は確定済み） |
| V6 | 受入閾値（新規モデルの下限、T4の劣化許容幅） |
| V7 | queue（`L_queue`、取り分、`T_queue`）、quoteの`h` |
| V8 | 容量attestationの形式、`sigma_cap`、計算量の上限 |
| V9 | 検証者取り分の傾き、verifier payの`N`・`m`の導出、`checked_share`の目標 |
| V10 | attestation gateで留保した分の行き先（B+） |
| V11 | coinbase経路をPanel=1へ移すか（D-P5: 別途確認） |

確定事項（`α = 1.5`、30日平均、`W_hold` 7日（Panel=0）・1日（Panel=1）、`B = F = 0`、Kernel claimへのcoinbase legなし、escrowのclip禁止）は判断対象ではない。

> **整合性注記（2026-10-22、lane rfcgap-b）:** 平均期間は2026-10-16のユーザー決定（10-22再確認）で**7日相当DAA**に変わった（旧30日）。コードは既に7日（`PALW_ALLOCATION_AVERAGE_DAYS_V1 = 7`、`PALW_RFC0016_AVERAGE_WINDOW_CALIBRATION_DAA_V1 = 5,040`）。§14の`ACTIVATION_ALLOWED`は、固定拒否ではなく§14 arm-readiness record（`PALW_RFC0016_ARMING_RECORDS_V1`、空）で検査する。recordは(fence, network, 高さ)と値の組に一致し、上の各conjunctに1つずつ証拠digestを持つ。空なのでどの高さでもarmは拒否される（`consensus/core/src/palw_rfc0016_v1/values.rs`）。
個別capを引き上げて配分を使い切らせる選択肢は設けない。

##### 14.2 ユーザー判断 2026-10-15（V2〜V11）の実装状況

数値は校正・受入試験の出発点で本番値ではない。休眠・未build・未試験（PASSではない）。行番号は[実装状況](../design/palw/integration/rfc-0016-implementation-status-2026-10-15.md) §8。

| ID | 状態 |
| --- | --- |
| V1 | 決定済み（D-V1）→ 実装済み（§16） |
| V2 | 2026-10-15決定 → 実装: 校正初期値 σ=0.25・φ=0.5・pace 1.2・k=1（`PALW_RFC0016_CALIBRATION_START_V1`）、`A_m`欠損時は再発行なし（`ReissueUnavailable`、先着なし）。本番値は測定待ち |
| V3 | 2026-10-15決定 → 実装: entrant reserve ≤ 20‰、RFC-0017 Probationと一つのpool（明示的netting）、Probationモデル上限2‰、`S_entry` = 適格な最低producer bond |
| V4 | 測定待ち |
| V5 | 2026-10-15決定 → 実装: 1 epoch = 720 DAA、30日窓 = 21,600 DAA（校正）、DAAと実時間のずれをRPCとharnessで報告（「24 h」と表示しない） |
| V6 | 2026-10-15決定 → 測定・選択toolに実装（不変条件違反0、正直cap利用率≥80 %、V+なし比REAL劣化≤5 %暫定）。測定は未実施 |
| V7 | 2026-10-15決定 → 実装: bond資本比例のqueue取り分、quoteは権利でない（決定論的な再検査）、TTL超過は中立終了。`L_queue`・`T_queue`・`h`は測定待ち |
| V8 | 2026-10-15決定 → 実装: 署名対象を固定した容量attestationの型・検査、verifierのモデル間二重計上なし、`sigma_cap` ≤ 1、計算量・メモリ・DA・court負荷の一体評価。attestationのcarriage（object種別）は未割当 |
| V9 | 2026-10-15決定 → 実装: 10〜30 %・基準20 %、`checked_share`と期限内完了率を別計測。傾き・`N/m`は測定待ち |
| V10 | 2026-10-15決定 → 実装（構造上そうなっていることを試験sourceで固定）: 留保分は他へ再配分せず未発行、後日追加発行しない |
| V11 | 2026-10-15決定 → 実装: coinbaseのPanel移行なし（`PALW_RFC0016_COINBASE_PANEL_MIGRATION_V1 = false`と試験） |

#### 16. 実装（休眠、未有効化、未build）

実装は採用・有効化を意味しない。行ごとのfile:lineと試験IDの対応は、[実装状況](../design/palw/integration/rfc-0016-implementation-status-2026-10-15.md)にある。
すべての行は、実装済み、test source作成済み、evidence-onlyのどれかである。

##### 16.1 lead decision（2026-10-15、ユーザーreview待ち）

| # | 内容 |
| --- | --- |
| L1 | blockへの`P_m`の適用は1回だけで、V+の権利が行う。V+をarmするとき、ADR-0177 V3の`block_opportunity`は必ずoffにする（validationが拒否する）。V+がreleaseのブロック機会の仕組みである。coinbase側の`P_m`は既存のcoinbase配分が1回だけ適用する（ADR laneのC4もこれで解決する）。単一適用のtest sourceがある |
| L2 | 途中再発行（V2）は`A_m`加重で行う。新しいrooted状態（aux 132）を持ち、reorg・IBDで一貫する。~~`A_m`が得られないときだけ、先着をfallbackとして残す~~ → **ユーザー判断 2026-10-15（V2）で置換: `A_m`が無ければ再発行しない（先着で権利を与えない）** |
| L3 | RPCはop 255、proto 1330–1331を使う（確定済み） |

##### 16.2 実装の範囲

* **fence:** `palw_model_block_rights_v1`と`palw_elastic_final_capacity_v1`（Params、identity、validation、fork-id probe、V2 mirror）。
  * どちらもkernel route（`palw_probabilistic_constraints_v1`）が同じ高さ以下にあることを要求する。
  * 孤立calibration networkだけは、buildに入れた`MISAKA_RFC0016_CALIBRATION_VALUES`でarmできる（σ/k測定用）。
* **aux table 132–135**（131はRFC-0017が使う）: 再発行snapshot、pending queue、queue membership、telemetry。
  native scannerのarmは、generatorと生成fileの両方にある。
* **V+:**
  * 発行は`⌊(N_exp − Ent) · A_m / ΣA⌋`で、entrant reserveは固定総額から先に差し引く。
  * 消費は、既存の`epoch_counters`をそのまま使う。
  * admissionは、自分の権利か、`φ`後の`A_m`加重snapshotか（ない場合は先着σのfallback）で決まる。epoch総額は超えない。
  * quoteは、権利・容量・実際のqueue長を示す。
  * RFC-0017のeligibilityをmerge済みで、次のように読む。
    * 発行は、probation外のblock rightを持つモデルだけに行う。
    * Probationのclassは、RFC-0017のprobation grantを権利とするので、V+の検査を通す。
    * Registered/Lapsedは、RFC-0017自身のgateが拒否する。
* **弾力的容量:**
  * 検証者単位のwater-fillingで、容量0の場合は受理しない。
  * C7上限は、容量から導出する。
  * **queueはclaimのlifecycleに組み込み済み**:
    * 待機中はbind期限がない。
    * 待機中は`PanelBound`が効かない。
    * FIFOでdequeueし、そのとき`rebound_daa = now`とする。これでbind窓とanchor slotが、queueを出た時点から始まる。
    * TTLを過ぎるとBindTimeoutでvoidとする。producerへの課金はない。
    * 1 claimが予約する権利は1本だけである。
    * bondごとの取り分は資本比例で、受理前に拒否する。
  * OPV経路の検査容量gate。
  * M\*-49の検証者取り分の傾き（10–30 %の範囲内）。
* **計測:**
  * consensus-derived: 目標、物理block、権利、Final、coinbase支払、既存minerの個人収入、escrow支払、queue待ち、REAL gap、heartbeat依存DAA。
  * node-local（そう表示する）: 拒否件数。
* **RPC op 255 `GetPalwRfc0016Report`**（proto 1330/1331）: JSONで返す。目標と結果を分け、node-localの観測を区別する。
* **ツール:**
  * `scripts/sim/palw-rfc0016-vplus-sim.py`: モデルであり、測定ではない。
  * `scripts/drills/rfc0016_sigma_k_measure.py`: 実node測定のharness。
  * `scripts/sim/palw-rfc0016-report.py`: 目標の増加だけでは改善と表示しない。
* **test source（未実行）:**
  * `consensus/core/tests/rfc0016_v1.rs`
  * `consensus/src/pipeline/virtual_processor/tests/rfc0016_e2e.rs`（H1–H4、A8）
  * 各moduleのunit test
* **evidence-only:**
  * σ/kの測定値と測定値記録。
  * V+のcalibration測定。ADR laneの配分fenceがcalibrationでarm可能になることが前提である。
  * A11（検査しないverifier）。
  * M6/M7の実測。

#### 15. 参照

* [PRINCIPLES](../PRINCIPLES.md) §0
* ADR: [0066](../adr/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)、[0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)、
  [0140](../adr/0140-the-heartbeat-is-the-emergency-generator.md)、[0142](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md)、
  [0165](../adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md)、[0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)、
  [0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)
* RFC: [0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) R8-BUDGET・L134、[0015](../rfc/0015-panel-free-permissionless-verification.md) §8.3–8.5.5
* 設計: [bond-budget-and-model-allocation](../design/palw/economics/bond-budget-and-model-allocation.md) §3.4–3.6、
  [rfc-0015-implementation-status-2026-10-15](../design/palw/integration/rfc-0015-implementation-status-2026-10-15.md)、
  [activation-readiness-matrix](../design/palw/integration/activation-readiness-matrix.md) §3f・§3g・§3k
* code: [`palw_admission_v2.rs`](../../consensus/core/src/palw_admission_v2.rs)、[`palw_work_target_v1.rs`](../../consensus/core/src/palw_work_target_v1.rs)、
  [`fold_read.rs`](../../consensus/core/src/palw_state_v2/fold_read.rs)、[`palw_bond_budget_v1.rs`](../../consensus/core/src/palw_bond_budget_v1.rs)、
  [`palw_bond_budget_fold_v1.rs`](../../consensus/core/src/palw_bond_budget_fold_v1.rs)、[`palw_kernel_route_fold_v1.rs`](../../consensus/core/src/palw_kernel_route_fold_v1.rs)、
  [`ledger.rs`](../../misaka/palw/kernel/src/ledger.rs)、[`palw_verifier_pay_v1.rs`](../../consensus/core/src/palw_verifier_pay_v1.rs)、
  [`palw_producer_v2.rs`](../../consensus/core/src/palw_producer_v2.rs)、[`config/params.rs`](../../consensus/core/src/config/params.rs)

#### ユーザー方針 2026-10-15 による改定

* V+のblock権利は生成機会であり合意creditではない。権利を消費したclaimも、`SecurityCertified`になるまで合意creditを持たない。
* 同じmergesetのsibling間で、権利の消費はmerge順に依存してはならない([RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §4のcanonical順と同じ集合関数)。

**ユーザー方針(原文):**「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。補足判断(同日):「抽選した検証者の承認を認証の柱にしている。これは承認にするということは前提にするのでそのままでいい。」

* **`Final`と`SecurityCertified`は別状態である。** `Final`は手続きの完了(利用者払いescrowの経済的settlementは既存規則どおり)。合意credit(safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来の報酬leg)は`SecurityCertified`のclaimだけが生む([RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §7、Gate C)。
* **認証は前提に基づく。** claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認と、G14公開告発windowの異議なし経過で成立する。「抽選されたverifier stakeのうち正直な割合が閾値以上」を明示的な前提とし、無条件の数学的証明とは扱わない。暗号学的な計算証明は使わない(PRINCIPLES §8)。認証済みでもG14で反証されたclaimはvoidされ、合意creditは取り消される。
* **DNS-BFTは廃止済みであり、安全性にも収束にも使わない。**
* **genesisモデルにも認証の免除はない。70Bモデルは、独立した検証(抽選検証者による再実行)が成立するまで合意creditを持たない。**
* 本改定は追記であり、上の本文は履歴として残す。実装は休眠(fence `palw_independently_work_certified_v1`、全presetで`None`、有効化は別途のユーザー判断)。

#### ユーザー方針 2026-10-15 第4回 — `P_m` は階層係数 `c_m` を含み、一度だけ適用する

> **統治する前提:** [PRINCIPLES §9](../PRINCIPLES.md)(凍結前提)。規則は [ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) が定める。

* V+ が発行に使う `P_m = A_m / ΣA` は、`A_m = c_m · S̄_m^1.5` である。
  * genesis・年次フォーク指定モデルは `c_m = 1.0`、登録モデルは固定の `0.3`(ADR-0177 の改定)。
  * 係数は配分の重みで一度だけ適用し、物理ブロック権利と coinbase 配分が同じ `P_m` を使う。
  * 受理ブロックの報酬に重ねて掛けない。
* **entrant reserve(V3)と RFC-0017 の Probation 予算は、fence `palw_model_tier_v1` の下では無意味になる。**
  * モデル単位の審査は全廃した(第 5 回)。審査も昇格もなく、entrant が存在しない。
  * そのため `ε = 0` とし(`palw_rfc0016_rights_epoch_v1`)、登録 tier 全体の発行上限 `κ`(配分の重みで適用)を tier の唯一の上限とする。
  * fence 未満の V3 の規則(entrant reserve ≤ 20‰、Probation との明示的 netting)は、休眠のまま残る。

---

<a id="source-v1-0039-L00001"></a>

### 旧RFC-0039 / 原文1–23行

出典: [0039-finite-fee-consent-and-provider-payment-bounds.md](../rfc/0039-finite-fee-consent-and-provider-payment-bounds.md)。配置理由: 主題別統合。

### RFC-0039 — 通常 verifier の fee consent と provider の有限支払・保持契約

Status: **Accepted fee scopes — 通常 verifier fee と provider fee-only 条件を採択** (2026-10-28)。provider の役務を有限化する別案（A/B）は未採択。ユーザーは「この報酬条件を採択して実装」と明示した。採択対象は元の有限 X3 期限までの全額保留、期限内の客観的有罪で元 poster へ全額返金、それ以外は期限後に verifier へ支払い（ServiceHold を含む）、遅い verdict による fee の遡及没収なし、既存 bond liability 維持。実装・検証・武装・配備の完了は意味しない。

#### 問題と範囲

全機能を完成させて testnet-12 DAA 17,400 で有効化する目標に対し、SettlementOnce は通常 verifier fee と material provider fee の認証済み有限責任条件を欠く。通常 verifier の draw deadline は本人が同意した責任期限ではない。現行 provider V1 の署名は、全権利終了・最後の実支払 + W・その finality までの event-relative 保持を明示する。現在の署名の意味を有限期限へ変更しない。

この RFC は不足する責任と権利の扱いをレビューする案である。既存の paid-V2 trap、native39 issuance の例外、kernel X3、source producer が署名する既存 beacon terms を一括置換しない。モデル単位審査や計算の暗号学的証明を導入しない。claim の認証・court・Gate C は維持する。

#### 1. 通常 verifier fee consent

新しい独立の versioned consent を、既存の署名対象を変更せずに追加する。署名対象は `version, network, ruleset, claim, verifier_bond, draw_slot, poster_bond, fee, liability_until, material_until, forfeiture_policy_digest, nonce, clean=true`。signature を除く canonical bytes と専用 domain を使う。chain、claim、slot、金額、政策の変更は署名不一致となる。consent のない旧オブジェクトを新規則への同意とみなさない。

- `liability_until` は原 claim の受理 DAA に束縛された責任期限を覆う。通常 kernel では採択済み X3 の hard を用い、後の観測時刻で延長しない。paid enrollment はその署名済み lifetime を維持する。
- `material_until` は実際の材料契約の期限で裏付ける。verifier の同意だけで provider の署名済み役務を延長しない。
- 新しい consent 自体を明示的な Clean fee-attestation として扱い、その受理時に一つの draw slot の fee 全額を poster の既存 escrow から専用 held row へ移す。同じ transition で kernel の slot を paid とし、後の slash や旧 attestation がその fee を再び使わないようにする。満額を確保できない consent は状態を変えず拒否する。Gate C・claim Final・credit を作らず、他 claim の保留額と混ぜない。obligation は claim/verifier/slot ごとに一意。
- 提案する標準形は、有限の責任期限まで fee を全額保留し、期限内の客観的な有罪 verdict が Clean attestation に結び付く場合のみ fee 全額を元の poster へ返すこと。それ以外は期限後に verifier の支払権利を解除する。対象 claim が ServiceHold / 未解決でも、この fee の責任期限を延長しない。対象 claim を Final / SecurityCertified にしたり、その credit や producer 報酬を解除する処理ではない。
- Unavailable / 未回答 / service 不足 / 再編による一時的不明を fraud と扱わない。期限後の verdict で解除済み fee を遡及没収しない。既存 signer bond liability は、その既存規則と obligation のまま別に一度だけ処理する。reorg では fee の hold / verdict / release と payout を同じ履歴に巻き戻し、選択 chain 上で期限内だった verdict を再評価する。壁時計では判定しない。
- **採択済み（通常 verifier の条件）:** この「有限期限で解除 / 期限内の有罪 Clean の fee 全額を poster へ返す」組合せは2026-10-28に明示採択された。paid trap の probe 値を承認済み値として流用したものではない。

状態は `AcceptedHeld → Released | Forfeited`。書込み・資金移動・once marker は同じ transition で実行する。期限経過だけで Gate C 認証を作らない。pruning / restart / IBD / reorg でも同じ署名済み条件と terminal 判定を復元する。

---

<a id="source-v1-0039-L00051"></a>

### 旧RFC-0039 / 原文51–92行

出典: [0039-finite-fee-consent-and-provider-payment-bounds.md](../rfc/0039-finite-fee-consent-and-provider-payment-bounds.md)。配置理由: 主題別統合。

#### 4. 旧 cohort と新 wire

新 fence より前に受理した claim/fee/provider contract は元の規則を保つ。新 consent と V2 enrollment は fence 前に新効果を持たない。旧 wire tag の意味を変えず、carriage、署名 context、SDK builder、検証、state codec と restore が version を識別する。新 tag/aux prefix の番号はレジストリと照合して実装時に割り当てる。本 draft は番号を予約・再利用しない。

新 cohort でも、既存 V1 provider の同意が新しい期限を覆わなければ admission を拒否する。自動更新、暗黙の同意、TTL の無断延長は行わない。新 consent を受け取れない場合に旧 permissive path へ fallback しない。

#### 5. 実装と検証の完了条件

1. 未決政策と値を ADR に採択し、正確な wire/署名仕様をレビューする。
2. builder → signature → admission → held state → verdict → release/refund → physical queue の全経路を接続し、資金保存と一回性を検査する。
3. 誤署名、別 network/claim/slot、金額置換、旧 cohort、境界 DAA、overflow、支払/refund 競合、重複 consumer、容量不足、provider 未同意を検査する。
4. 正当な支払、期限内有罪、混雑、停止、遅れた finality と recovery を複数 node で実行し、restart/reorg/pruning/IBD 後の権利と会計一致を示す。
5. queue/保持 byte・CPU/RAM・帯域・最悪遅延の測定と独立 review を pin する。実装・試験・測定の完了後にのみ blocker と readiness を更新する。

参照: [設計凍結手続き](../governance/design-freeze.md)、[既存の未 source 調査 §4.6](../design/palw/integration/rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md)、[全機能 readiness](../design/palw/integration/g14-17000/full-activation-17400-readiness-2026-10-28.md)。


#### 6. Independent provider fee-only contract (Accepted, implementation pending)

This alternative leaves V1 service, service collateral, worker entitlements, replacement duties, physical-payment tails and finality unchanged. Only the provider's own prepaid fee becomes a separate finite liability tranche. It is not a NoLiableClaim exemption, nor a finite deadline for the underlying service.

The provider and original sponsor sign a new version binding network, ruleset, original enrollment/consent ID, exact fee context and provider slot, original sponsor/refund recipient, prepaid amount, absolute fee deadline H, policy digest and nonce. The deadline is an explicit new financial term, not a fabricated original claim X3 horizon. This binding works before root birth: enrollment identity, fee slot and escrow already exist, and no unknown claim acceptance time is substituted. No already queued or physically paid V1 fee may be migrated retrospectively. New versions and old rights remain distinguishable.

Proposed complete money rule: reserve the whole fee in its own held row through H. An objective material-provider default actually processed on the selected chain at or before H returns the whole held fee to the original sponsor. Otherwise release the whole fee to the provider after H, including when root/service/worker payments remain unresolved or the root never materializes. A later default does not claw back this fee, while existing service bond sanctions continue. The fee is compensation for the original enrollment's service undertaking; this proposal does not condition its existence on eventual claim birth. Reorg rolls back default, held amount, release and payout together.

The exact default authority is the existing monetary-material fold transition: an unanswered PalwMonetaryMaterialChallengeV1 past its reply deadline triggers PalwSettlementLegV1::MaterialProviderDefault and sets provider.charged. It is not a producer Convicted or ProducerDefault event, nor an unrelated ProviderLiableDefault event. A new durable row must bind fee context, provider, original enrollment generation and the actual default processing DAA; the existing charged boolean lacks that timestamp. Replacement must not attribute a previous provider's default or fee to its successor.

The fee cutoff is the actual default processing DAA, not challenge opening time. A challenge still awaiting its reply deadline at H does not postpone the fee release. Such a later default still charges the service bond but does not recover the fee. This late-discovery limitation is a NEW proposed financial decision, explicitly requiring adoption. A challenge whose reply deadline is H can first default at H+1, so it is outside the refundable interval. Existing provider fee queueing currently precedes default processing in the tick; implementation must evaluate and record all decisive timely defaults before evaluating fee release.

The concrete proposed deadline is H = the original enrollment terms.enrollment_until_daa. The new sponsor/provider signatures explicitly adopt that existing admission deadline as the fee-only refundable interval; the old V1 signature alone does not. Require enrollment acceptance DAA < H < u64::MAX and retain the existing max_enrollment_daa validation. No new numeric duration is proposed. A root born after H cannot reopen the fee refund interval, even though the service and service-bond obligations continue. A replacement does not move an earlier enrollment fee deadline or change its recipient. This mapping is a new financial meaning requiring explicit adoption; it is not an implication of the old enrollment window.

Under the combined settlement predicate, R is the exact prepaid fee, K and collateral are the dedicated held-fee tranche, the fee's signed hard deadline/lock/retention are H, and fee evidence is retained through H. The original event-relative service bond remains a separate liability and cannot be counted again as this fee tranche. Material evidence for a fee default must remain available through H under the original healthy covenant; failure to establish that evidence retention at admission refuses the new fee contract. This explicitly narrows the NEW fee contract's clawback obligation; it does not reinterpret the old V1 payment obligation or remove its lifetime restrictions.

Required review/tests: consent identity and signatures; negative old-cohort migration; boundary H/H+1; open-but-unresolved challenge at H; replacements; conservation and exactly-once refund/payout; no-root enrollment; queue pressure and rollback; unchanged old service/bond/entitlements. Physical queue and long-lived service resource bounds remain independent implementation requirements.

Review (2026-10-28): independent review found the fee-only rule implementable with the stated late-default limitation. Required ordering is default recording before fee release; both parties sign the new fee condition; actual payout queues and their material tails remain tracked; old fees never migrate implicitly. This review preceded the user adoption recorded below.

#### Provider fee-only 採択記録 — 2026-10-28

ユーザー回答: **「この条件を採択して実装」**。採択対象は RFC-0039 §6 の新しい provider/支払者双方の署名付き fee 契約である。H は元の `enrollment_until_daa`。報酬全額を H まで保留し、対象 provider の材料提供義務違反が選択 chain 上で H 以内に確定したときのみ元の支払者へ全額返金する。それ以外は H 後に provider へ支払い、H 以前に告発されていても違反確定が H 後なら fee を遡及回収しない。claim 未成立・告発未解決でも同じ。既存の材料保持義務、service bond、worker の報酬権利は継続し、旧契約には遡及しない。

これは有限役務・未払い worker 権利の取消し・custody 移管案 A/B の採択ではない。実装完了、試験合格、測定値承認、有効化、公開配備の宣言でもない。今回の完了範囲は全項目の実装完了で、DAA 17,400 は fence 方針として維持する。
