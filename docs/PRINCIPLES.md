# MISAKAの設計前提 — 不可侵原則

**確率的検証・経済的抑止・公開証拠による独立客観裁定**
*Probabilistic Verification, Economic Deterrence, and Independent Objective Adjudication*

**この文書の位置(2026-10-09、ユーザー決定)**

- この文書は MISAKA の**設計前提**であり、すべての ADR・RFC・Spec・設計文書・実装レーン・有効化判断の**上位**に置く。
  - ADR・RFC・Spec はこの前提の下で書かれ、判断される。
  - 下位の文書がこの前提と衝突する場合は、前提が優先する。衝突する記述は改定対象になる。
- この前提そのものは ADR や RFC では変更できない。変更はユーザーの明示的な決定だけで行う。
- この前提は wire id、合意 fingerprint、有効化の高さ、パラメータ値のいずれも割り当てない。
- 現在のコードがすべての条件を満たしたと宣言するものでもない。§6 は、報酬と consensus work weight を与える前に満たすべき条件である。

**下位の主要文書**

- [ADR-0173](adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md): 公開検証者による不正追及の完結性という目的。
- [ADR-0171](adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md): 確率的検査と異議時の court。
- [ADR-0172](adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md): versioned kernel。
- [RFC-0007 Part VI](rfc/0007-palw-verification-certificates-and-algebraic-checks.md): commit 後 challenge。
- [RFC-0014](rfc/0014-panel-independent-fraud-prosecution.md) と [RFC-0015](rfc/0015-panel-free-permissionless-verification.md): Panel 非依存の訴追と Panel=0。
- 有効化判断の照合先: [全面有効化の readiness matrix](design/palw/activation-readiness-matrix.md)。

> **MISAKA不可侵原則 —「確率的に検出し、公開証拠で局所化し、決定論的に裁き、被害をbondで有界化する。モデルの非公開は経済的に不利にする。」**
> *Probabilistic Detection. Public Localization. Deterministic Adjudication. Bond-Bounded Damage. Closure Is Uneconomic.*
>
> 2026-10-14改定前の標語は「…決定論的に裁き、経済的に不正を抑止する。」（*Economic Deterrence*）であった。§0の改定で置き換える。

---

## 0. 最終Goal — 2026-10-14改定（ユーザー決定）

> **MISAKAの最終Goalは、モデルを非公開にしておくことが経済的に不利になる仕組みを成立させることである。
> 非公開での不正を不可能にすること（不正の完全排除）は目標にしない。**

この節は本書の他の節と下位の全文書に優先する。§2・§4・§6と下位文書のうち、検出確率の下限の証明や「不正の期待利益が負」を
最終条件・有効化gateとする記述は、この節に従って読む。

1. **経済Goal。** モデルを共有して外部minerのbond参加を増やしたモデルが、非公開のまま自己資本だけで採掘するモデルより、
   モデル全体として多くの配分を得る構造を実現する（モデル別のブロック生成機会とcoinbase配分、30日平均bondの`α = 1.5`、[ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md)）。
   chainはモデルが共有されたかを判定せず、実際に拘束されたbond資本と、検証が成立したかだけを読む。非公開は次の二つで不利になる。
   - 公開モデルへbondが集まる結果として、閉鎖モデルが相対的な配分を失う。
   - 検証attestation gate（ECON round 2、readiness §3f）により、bondから抽選された検証者が自分のモデルのコピーでclaimを検査し、
     所定数が承認しないmodel-epochにはモデル配分を払わない。誰にも共有されないモデルは検証が成立せず、配分を受けられない。
     理由は「検証が承認されなかった」であり「モデルが配布されなかった」ではない。claim・escrow・Final・weight・slashには触れない。
2. **共有されたモデルの不正は客観的に裁ける。** 登録モデルが1人にでも共有され、正しいPanel（正直な検証者）を建てられれば、
   そのモデルのclaimの不正は、claim固有の公開・認証済み証拠から局所化し、客観的なcourtで立証してslashできる。これを制度要件とする（§3、
   [ADR-0173](adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)）。
3. **非公開モデルの不正は有界化する残余リスクである。** 誰にもモデルが共有されず実効的な検出確率が0（`p = 0`）になる場合、
   不正の排除は保証しない。これは有効化を止める安全性gateではなく、bond単位のBlock / Reward / Final Weight上限、共通拘束期間、
   Final前の払い出し禁止で被害を有界化する残余リスクとして扱う（[ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)）。
   上限は正直なminerにも不正なminerにも同じに適用する。
4. **検出確率と期待利益は評価指標である。** 検出確率の下限の証明と「不正の期待利益が負」は、最終条件・有効化gateにしない。
   測定・導出して報告する評価指標とし、未知の値はUNKNOWNと表示する。根拠のない検出確率の仮定、検査対象外の演算を検査済みと表示すること、
   検査されていないのに検査されたと扱うことの禁止（§2・§4）は維持する。
5. **達成条件は二段で評価する。**
   - **(a) モデル全体:** 非公開のときと共有して外部bondを集めたときで、モデルの総bond・配分割合・経済規模がどれだけ変わるか。
   - **(b) 所有者個人:** 元の所有者自身の取り分が共有で増えるか。マイナー報酬はモデル登録者へ直接分配しない。登録者の収入はModelPositionの売買手数料と、自らMiner/Verifierとして参加した場合のその役割の報酬である（[RFC-0015 §8.5.5](rfc/0015-panel-free-permissionless-verification.md)）。モデル内の配分、自己資本を追加してbondを増やす場合との資本コスト・期待収益の差を含める。
     自己資本の追加でモデルのbondを増やすことは禁止しないが、評価には含める。

   モデル全体の伸びと所有者個人の伸びを混同しない。

   例: 他モデルのbondが合計900M BILI（1モデル）、モデルAの自己bondが100M BILIで、共有により外部bond 200M BILIが参加した場合。

   | | 非公開 | 共有後 | 倍率 |
   |---|---|---|---|
   | モデルAの総bond（自己100M + 外部） | 100M | 300M | 3倍 |
   | 配分割合（比例、α = 1） | 10.0% | 25.0% | 2.5倍 |
   | 配分割合（採用した α = 1.5、2026-10-14確定） | 3.6% | 16.1% | 4.5倍 |

   非公開のままでは、この差分の配分を得られない。これが目標とする「非公開の経済的な不利」である。
   ただし表はモデル全体の配分であり、所有者個人の収益が同じ倍率で増えることは意味しない（(b)で別に測る）。

## 1. 不可侵原則

モデル計算の不正裁定は、Panelの多数派、単独Panel、producer、または特権的operatorの主観的判断を最終根拠としてはならない。合意 credit の認証には、抽選された検証者の承認を、正直な割合の明示的な前提の下で用いる（§8）。承認の認証と計算不正の客観的裁定を区別する。

MISAKAは従来のPoWと異なり、有用計算の実行結果を検証する費用が、その計算自体の実行費用に匹敵し、場合によっては上回るという問題を前提とする。

そのため、すべての計算を複数の検証者が最初から最後まで再実行することを、通常時の合意要件とはしない。

代わりに、次の4つを組み合わせる。

- 確率的検証による不正の検出
- 公開証拠による不正箇所の有界な局所化
- 決定論的なexact court
- 担保と経済的処罰

これにより、全量再実行の費用を常時負担せずに、有用計算の正当性を経済的かつ検証可能な形で支えることを目指す。

## 2. 確率的検証 — 全量再実行を通常要件としない

通常時の検証では、producerが計算結果と必要な証拠を先にcommitし、その後で決定される検証対象について検査する。

検証対象は、承認済みのverification planに従い、ランダムに選ばれた次のいずれかとする。

- 計算区間
- tensorの一部
- 状態遷移
- Freivalds 等による直接の代数的スポットチェック（§8。計算 validity proof は用いない）

検証者は、原則として巨大なモデルの全計算を再実行する必要はない。少数の検査で不一致を発見し、発見後に対象を絞り込んで客観的な裁定へ進める。

初期検査を1区間から開始することも許容する。ただし、1区間の検査だけで全体の不正を十分な確率で検出できるとは限らない。特に、巨大な計算のうち1区間・1要素だけを改ざんする攻撃では、単純なランダム抽出の検出確率が極めて低くなり得る。

したがって、すべての報酬対象planは、次を満たさなければならない。

- 計算・job・input・output・stateの全ての報酬対象制約を、承認済みの検証関係で被覆する。
- 単純な区間抽出では十分な検出確率が得られない場合、追加抽出、反復、代数的な集約検査などを用いる。
- producerが検査位置を事前予測・選別できないよう、commit後challengeの独立性とgrinding耐性を保証する。
- 見逃し確率の上限、必要な検証費用、公開データ量、検証時間を定量化する。
- 必要なsoundness boundを証明できないplanを、報酬対象として有効化しない。

確率的検証とは、不正を曖昧に許容することではなく、明示された見逃し確率の範囲で検査費用を抑えることである。

## 3. 不正発覚後の客観的裁定

承認済み検査で不一致が発見された場合、その後の有罪認定をPanelの投票や主観的判断に委ねてはならない。

対象は、すべての報酬対象class・kernel・verification planが対象とする計算、job、input、output、state、およびData Availabilityの違反である。producerとPanel全員が共謀する場合でも、次が成り立たなければならない。

- Panel外の1人の適格なpublic bonded verifierが、producerの秘密状態やPanelの協力なしに、公開・認証済みmaterialから独立に検査できる。
- その検証者が、不正箇所を有界に局所化できる。
- その検証者が、客観的な裁定まで進められる。

裁定の結果は、次のとおり扱う。

- 計算不正が立証された場合、決定論的なexact courtにより、客観的な有罪判定、claimの無効化、および責任追及へ到達する。
- 義務付けられた証拠が開示されない場合、計算不正を推定して有罪とはしない。客観的なDA/defaultと、それに対応する責任を判定する。
- 有効な異議申立てが継続中の場合、定められた有界の手続きが完了するまで、不正claimを経済的Finalへ逃してはならない。
- 不正が立証されなかった場合、正当なclaimや検証者が恣意的に処罰されないよう、棄却・担保返還・費用配分を決定論的に処理する。

不正の発見は確率的であっても、発見された具体的な不正の有罪判定は確率的であってはならない。

## 4. 経済的抑止 — 検証費用を担保と確率によって支える

PALWでは、計算全体の再実行を通常の検証要件とすると、検証費用が有用計算の報酬やネットワークの処理能力を圧迫する。

MISAKAはこの問題を、少数の検査に必要な費用と、不正が発覚した場合の経済的責任を組み合わせる方式で解決することを目指す。

基本的な抑止条件は、次のとおりとする。

**不正による最大利益よりも、検出確率を考慮した期待処罰額が十分に大きくなること。**

**2026-10-14改定（§0）:** この抑止条件は最終条件・有効化gateではなく、測定・報告する評価指標とする。成立しない範囲（とくに非公開モデルの`p = 0`）は、bond単位の上限で被害を有界化する残余リスクとして扱う。以下の量の算出と、下の禁止事項は維持する。

ここでいう検出確率は、数学的な検査の検出率だけで評価してはならない。次の確率もすべて含めて評価する。

- 適格な正直な検証者が実際に検査する確率
- 公開データを取得できる確率
- 期限内に証拠を提出できる確率
- challengeの操作可能性

次の値は、これらの前提に基づいて決定する。

- 担保とslash
- claim当たりの最大価値と同時exposure
- 検証者への報酬
- DA費用とcourt費用

次の行為を禁止する。

- 根拠のない検出確率を仮定して担保を低く設定すること。
- 検査対象外の演算を、確率的検査に含まれていると表示すること。
- 検証者が実際には存在しないのに、公開参加可能という理由だけで検査が行われたと扱うこと。
- 低い担保や有限のchallenge期間で、無制限の経済的利益を扱うこと。
- 不正検出後の裁定費用を無制限にして、公開検証者が追及を完遂できない設計にすること。

経済的抑止は客観的裁定の代替ではない。確率的検出と客観的裁定が成立して初めて、担保が安全性に寄与する。

## 5. Panel=1 / Panel=0の位置づけ

Panel=1およびPanel=0は、検証の計算負荷と運用依存を減らすための構成であり、公開検証と客観的裁定を弱めるものではない。

Panelが存在する場合、その検証結果は通常処理を効率化する。しかし、Panelの署名や多数決によって不正が正当化されてはならない。

Panel=0の場合でも、次はすべて維持される。

- 公開検証者
- 認証済みDA
- challenge window
- exact court
- 担保と客観的処罰

Panelがいなくても裁けることと、誰も検査しなくても必ず不正を発見できることは異なる。MISAKAは前者を不可侵の制度要件とする。後者については、承認済みsoundness boundと、検証参加・データ可用性・経済的誘因の明示された前提によって安全性を評価する。

## 6. 報酬・consensus work weightの有効化条件

以下が揃わないclass・kernel・verification plan・task・contextは、登録候補として保持できても、有用計算の報酬やconsensus work weightを与えてはならない。

1. 全ての報酬対象計算関係に対する検証被覆。
2. 承認済みの検査器の健全性とgrinding耐性（検出確率の下限の証明は要件にしない、§0）。
3. 公開・認証済みmaterialの可用性。
4. モデルを入手できた1人の適格な検証者（正しいPanel）による有界な不正局所化と客観的裁定（§0-2）。
5. 不正が立証された場合の徴収可能な担保・処罰と、bond単位のBlock / Reward / Final Weight上限による被害の有界化（2026-10-14改定。期待利益が負であることは要件にしない、§0）。
6. 正直な検証者が期限内に参加できるだけの資源・費用・検証インセンティブ。
7. 異議申立て、DA/default、Final、reorgの整合性。

## 7. MISAKAが守るべき本質

MISAKAは、巨大な有用計算を全員で再実行して正しさを保証するチェーンではない。

MISAKAは、次の4つによって、計算費用よりも検証費用が重くなり得る有用計算を持続可能な合意資源にするチェーンである。

- 確率的な部分検証によって不正を発見する。
- 発見した不正を公開証拠から決定論的に裁く。
- 立証された不正をslashし、残る被害をbond単位の上限で有界化する。
- モデルを共有してbondを集めた方が、非公開より多くの配分を得る経済構造にする（§0）。

そして、**MISAKAの安全性は、検査範囲、不正追及の公開性、裁定の客観性、bond単位の被害上限によって評価する。合意 credit の承認には、抽選された検証者の正直な割合と参加・取得可能性の明示的な前提がある（§8）。MISAKAの最終Goalは不正の完全排除ではなく、モデルの非公開を経済的に不利にすることである。**

> **MISAKA不可侵原則 —「確率的に検出し、公開証拠で局所化し、決定論的に裁き、被害をbondで有界化する。モデルの非公開は経済的に不利にする。」**
> *Probabilistic Detection. Public Localization. Deterministic Adjudication. Bond-Bounded Damage. Closure Is Uneconomic.*


## 8. モデル計算の暗号学的証明を使用しない（2026-10-15、ユーザー恒久決定）

**MISAKA は LLM・モデル計算の正しさを暗号学的に証明する仕組みを一切使用せず、今後も使用しない。**
この節は本書の他の節、既存・将来の ADR・RFC・設計・実装・有効化計画・研究項目に優先する。
過去の文書に採用・拡張候補と書かれていても、その記述は撤回対象であり、採用を継続する根拠にはならない。

1. **採用禁止。** zero-knowledge 証明、zk-SNARK / zk-STARK、zkVM・zkML、多項式コミットメント（PCS）等に基づく succinct argument / validity proof、再帰証明・proof aggregation を使用しない。GKR や producer が送信する多項式証明 transcript による計算 validity argument へ迂回しない。導入を将来課題や研究候補にも残さない。
2. **検証方法。** 確率的検出（サンプリング再実行・決定論的再計算・代数的スポットチェック）を行い、不一致を公開証拠で局所化し、決定論的に裁定し、経済的に抑止する。直接の Freivalds 検査などは、その検査器が読んだ入力と検査した関係だけを対象とし、全モデル計算の暗号学的証明とは呼ばない。
3. **合意 credit の認証。** 抽選された検証者の承認を、正直な割合の明示的な前提の下で用いる。その割合、抽選・重み・相関、取得可能性、期限、検出範囲を報告する。署名や quorum は承認者と対象の認証であり、算術真実の暗号学的証明ではない。異議時の有罪判定は公開証拠と決定論的 court による。 **楽観的な Final または一致した local replay は、抽選された検証者の承認を代替しない。** material の再計算、契約の受入、データの同一性確認だけで consensus credit の承認を得たと扱ってはならない。
4. **利用できる暗号機構。** ハッシュ・署名・Merkle commitment・VRF/beacon は、データの同一性・認証・乱数に使用できる。Merkle opening は保持データの所属を認証するもので、モデル計算の正当性を証明するものではない。
5. **実装と互換性。** 禁止した方式を新規生成・受理し、報酬や weight・credit の計算正当性の根拠にしてはならない。旧履歴の読み取りや wire ID の保存が必要な場合も、その目的に限る。旧 ID の意味を別方式へ再解釈せず、必要な移行は明示的な version/fence で行う。
6. **評価。** 128-bit 等の数字を全計算の暗号学的健全性として保証しない。許容する検査の誤検出・見逃し、反復・grinding、正直な検証者の参加、データ取得、費用と経済条件を、それぞれの前提を明示して評価する。未測定値は UNKNOWN とし、禁止方式の実装を不足解消の条件にはしない。

---

## 9. モデルの二階層とモデル審査の不採用（2026-10-15、ユーザー凍結前提）

**モデル単位の検証・審査は採用しない。** モデルの検証で何を信頼するかという問題は解決しないため、審査・再審や代替のモデル評価制度を追加しない。この節は、旧文書のモデル資格審査・Probation・昇格・更新・失効・試行手数料や、モデル審査の合格を登録・配分の前提とする記述に優先する。

- genesis と年次フォーク（年1回程度のソフト/ハードフォーク）で指定されたモデルは **1.0**。
- 後から登録したモデルは、登録した時点から審査なしで **0.3** に固定する。昇格・更新・失効・試行手数料はない。
- 後から登録したモデルが 1.0 になる経路は、年次フォークでの指定だけである。
- **claim 単位の検証は全モデルで維持する。** court、Gate C / SecurityCertified、claim ごとの抽選検証者認証を、モデルの階層を理由に省略しない。1.0 という指定自体は claim の正しさの承認ではない。
- material のデータ同一性、claim の実行条件・取得・資源上限の確認を、モデル単位の資格審査や階層の変更に転用しない。§8 の禁止と認証の前提も引き続き適用する。

この節はユーザーの決定の記録である。RFC-0016・0017・ADR の実装作業は Claude の担当であり、RFC-0006〜0008・0012 の作業で引き取らない。

---

## 付録 — この前提が現在の作業をどう縛るか (Lead, 2026-10-09)

This appendix records where the principle already decides open work. It is a pointer, not part of the principle.

**2026-10-15 追補:** 以下の旧モデル資格・Probation に関する説明は §9 により置き換えられた。モデル審査や審査合格による配分開始の要件として用いない。claim 単位の検証と裁定・資源の条件は全モデルに維持する。

- **§6 is the gate for earning.** A class, kernel or plan that does not meet all seven conditions may stay registered, but it earns no reward and no consensus work weight. The devnet showed that the V2 lifecycle today lets a class reach Probation on seat readiness alone. Closing that is a release blocker (readiness matrix, "G14-for-rewards").
- **§2 and §4 decide the real-scale question** for large models, where reading a whole claim is not feasible (the soundness review dossier's SG-06 and reviewer question Q-01). A plan earns only if:
  - its bound is proven, including the effective bits after retries and grinding;
  - its detection probability is stated, including the probability that a capable honest verifier actually checks;
  - its collateral covers the maximum gain divided by that probability.

  Direct randomized algebraic checks may reduce repeated arithmetic work; their detection scope, actual input-read cost and honest-verifier assumptions must be explicit. A cryptographic computation-validity argument is prohibited by §8, including as a proposed remedy for expensive verification.

  **Amended 2026-10-14 (§0):** the detection-probability bound and the collateral-covers-gain/p condition are no longer gates for earning. They are measured and reported (UNKNOWN when unknown). The gate is the per-bond Block / Reward / Final Weight caps, objective court and slash for any model shared with at least one honest verifier, and the two-stage closure-disadvantage evaluation. `p = 0` for a closed model is a bounded residual risk.
- **§2's grinding resistance** is why the sealed-source beacon (v3) and the complete-check bootstrap are prerequisites for any sampled approval.
- **§3's "no escape to economic Final"** is enforced by the bounded demand/proof/grace deadlines of the kernel route and OPV.
- **Condition 7 of §6 (reorg consistency)** is why the fork-choice decision (ADR-0178) is a prerequisite of the full-activation release.
