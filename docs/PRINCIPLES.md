# MISAKAの設計前提 — 不可侵原則

> **2026-11-08追加採択:** EXEC credit・future schedule・1 BPSとcourt容量の規範は[§14](#exec-credit-one-bps-and-court-capacity)を参照する。§13のNode非保有・検証先行Finalを維持する。

> **2026-11-08 採択:** 新しいclaim版では [§13 モデル非保有Node・敵対的なモデル秘匿・非対話positive](#model-free-adversarial-verification) を優先する。Nodeはモデル本体を持たず、モデル内Miner／Panelの寡占を想定し、独立検証が成立した後に成功Finalを置く。旧claim・旧profile・model-epoch配分は従来の意味を保持する。

> **現在の変更管理:** [§11 設計凍結](#design-freeze)。**初期 mainnet の対象範囲:** [§12 画像・動画・改善の延期](#post-mainnet-scope)。既存技術の再採択・有効化を意味しない。

**確率的検証・経済的抑止・公開証拠による独立客観裁定**
*Probabilistic Verification, Economic Deterrence, and Independent Objective Adjudication*

**この文書の位置(2026-10-09、ユーザー決定)**

- この文書は MISAKA の**設計前提**であり、すべての ADR・RFC・Spec・設計文書・実装レーン・有効化判断の**上位**に置く。
  - ADR・RFC・Spec はこの前提の下で書かれ、判断される。
  - 下位の文書がこの前提と衝突する場合は、前提が優先する。衝突する記述は改定対象になる。
- この前提を含む採用済み設計は原則凍結する。重大問題時の変更は [§11](#design-freeze) に従い、新規 RFC と新規 ADR の両方の採択、および設計責任者（現行のユーザー承認主体）の明示的承認を必要とする。
- この前提は wire id、合意 fingerprint、有効化の高さ、パラメータ値のいずれも割り当てない。
- 現在のコードがすべての条件を満たしたと宣言するものでもない。§6 は、報酬と consensus work weight を与える前に満たすべき条件である。

**下位の主要文書**

- [ADR-0173](adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md): 公開検証者による不正追及の完結性という目的。
- [ADR-0171](adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md): 確率的検査と異議時の court。
- [ADR-0172](adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md): versioned kernel。
- [RFC-0007 Part VI](rfc/0007-palw-verification-certificates-and-algebraic-checks.md): commit 後 challenge。
- [RFC-0014](rfc/0014-panel-independent-fraud-prosecution.md) と [RFC-0015](rfc/0015-panel-free-permissionless-verification.md): Panel 非依存の訴追と Panel=0。
- claim の裁定: [公開裁定の方針 v0.1](design/palw/consensus/public-adjudication-policy-v0.1.md)。合意ノードはモデル計算を端から端まで再実行しない。
- 有効化判断の照合先: [全面有効化の readiness matrix](design/palw/integration/activation-readiness-matrix.md)。

> **MISAKA不可侵原則 —「確率的に検出し、公開証拠で局所化し、決定論的に裁き、被害をbondで有界化する。モデルの非公開は経済的に不利にする。」**
> *Probabilistic Detection. Public Localization. Deterministic Adjudication. Bond-Bounded Damage. Closure Is Uneconomic.*
>
> 2026-10-14改定前の標語は「…決定論的に裁き、経済的に不正を抑止する。」（*Economic Deterrence*）であった。§0の改定で置き換える。

---


> **恒久決定(2026-10-15):** LLM・モデル計算の正しさを暗号学的に証明する仕組み(ZK、zk-SNARK/STARK、zkVM・zkML、PCS ベースの succinct proof など)は一切使用せず、今後も使用しない。詳細は [§8](#8-モデル計算の暗号学的証明を使用しない2026-10-15ユーザー恒久決定)。

> **凍結前提(2026-10-15):** genesis と年次フォークで指定されたモデルは 1.0。後から登録したモデルは登録時点から審査なしで 0.3 に固定(昇格・更新・失効・試行手数料なし、1.0 への経路は年次フォーク指定のみ)。モデル単位の検証は「何を信頼するか」の問題が解決しないため不採用。claim 単位の検証は全モデルで維持。詳細は [§9](#9-モデルの二階層とモデル単位の検証の不採用--2026-10-15-ユーザー決定凍結前提)。

## 0. 最終Goal — 2026-10-14改定（ユーザー決定）

> **新版の適用:** 以下の原則・履歴のうち今回の新claim版と衝突する範囲は[§13](#model-free-adversarial-verification)を優先する。model-epoch配分とclaim固有認証は別制度であり、旧版の実行・Final・署名の意味は保持する。

> **MISAKAの最終Goalは、モデルを非公開にしておくことが経済的に不利になる仕組みを成立させることである。
> 非公開での不正を不可能にすること（不正の完全排除）は目標にしない。**

この節は本書の他の節と下位の全文書に優先する。§2・§4・§6と下位文書のうち、検出確率の下限の証明や「不正の期待利益が負」を
最終条件・有効化gateとする記述は、この節に従って読む。

1. **経済Goal。** モデルを共有して外部minerのbond参加を増やしたモデルが、非公開のまま自己資本だけで採掘するモデルより、
   モデル全体として多くの配分を得る構造を実現する（モデル別のブロック生成機会とcoinbase配分、7日平均bondの`α = 1.5`（2026-10-16 ユーザー決定、2026-10-22 再確認。旧 30 日）、[ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md)）。
   chainはモデルが共有されたかを判定せず、実際に拘束されたbond資本と、検証が成立したかだけを読む。非公開は次の二つで不利になる。
   - 公開モデルへbondが集まる結果として、閉鎖モデルが相対的な配分を失う。
   - 検証attestation gate（ECON round 2、readiness §3f）により、bondから抽選された検証者が自分のモデルのコピーでclaimを検査し、
     所定数が承認しないmodel-epochにはモデル配分を払わない。誰にも共有されないモデルは検証が成立せず、配分を受けられない。
     理由は「検証が承認されなかった」であり「モデルが配布されなかった」ではない。claim・escrow・Final・weight・slashには触れない。
     この配分の attestation は claim の裁定ではない。claim の裁定は [公開裁定の方針 v0.1](design/palw/consensus/public-adjudication-policy-v0.1.md) に従い、合意ノードはモデル計算を端から端まで再実行しない。
2. **共有されたモデルの不正は客観的に裁ける。** 登録モデルが1人にでも共有され、正しいPanel（正直な検証者）を建てられれば、
   そのモデルのclaimの不正は、claim固有の公開・認証済み証拠から局所化し、客観的なcourtで立証してslashできる。これを制度要件とする（§3、
   [ADR-0173](adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)）。
   Panel が止まっていても、bond 付きの第三者経路は残る。Panel の定足数は評決ではない。裁定の手順は [公開裁定の方針 v0.1](design/palw/consensus/public-adjudication-policy-v0.1.md) に従う。
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

> **新版の適用:** 以下の原則・履歴のうち今回の新claim版と衝突する範囲は[§13](#model-free-adversarial-verification)を優先する。model-epoch配分とclaim固有認証は別制度であり、旧版の実行・Final・署名の意味は保持する。

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

> **新版の適用:** 以下の原則・履歴のうち今回の新claim版と衝突する範囲は[§13](#model-free-adversarial-verification)を優先する。model-epoch配分とclaim固有認証は別制度であり、旧版の実行・Final・署名の意味は保持する。

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
4. モデルを入手できた1人の適格な公開検証者による有界な不正局所化と客観的裁定（§0-2、[公開裁定の方針 v0.1](design/palw/consensus/public-adjudication-policy-v0.1.md)）。Panel は取得、再実行、局所化、証拠作成をする作業者であり、署名は署名者を識別する。定足数は評決ではない。止まった Panel や結託した Panel のあとにも、bond 付きの第三者経路が残る。
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


<a id="8-llm計算の暗号学的証明を使用しない--2026-10-15ユーザー決定"></a>
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

7. **理由。** ZK などで LLM の計算を実用的に証明することは不可能である(ユーザー判断)。
8. **既存文書。** ZK・succinct proof・compact proof を「将来」「研究」として挙げている箇所(例: `docs/design/palw/rfc9-l2-fork-choice.md` 案 C、RFC-0015 §10、`docs/design/palw/rfc-0009-remote-record.md`)は本決定により除外する。過去の記述は履歴として残し、除外注記を付ける。
9. **エージェント(Claude Code、Codex など)へ。** 設計・実装・レビュー・提案で上記の暗号学的計算証明を持ち出さない。既存の記述や実装を見つけたら、除外である旨の注記を付けるか撤回する。リポジトリ直下の `CLAUDE.md`・`AGENTS.md` にも同じ決定を記載している。

(本節は 2026-10-15 に Codex と Claude Code がそれぞれ追記した 2 つの §8 を 1 つに統合したもの。)

## 9. モデルの二階層とモデル単位の検証の不採用 — 2026-10-15 ユーザー決定(凍結前提)

**本節は原則凍結した前提である。** 通常は再審・変更しない。変数・パラメーター調整と重大問題時の例外変更は [§11](#design-freeze) に従う。例外では新規 RFC と新規 ADR の両方の採択、設計責任者の明示的承認、対象節の改定を要する。今回、階層規則や数値は変更しない。

1. **genesis と年次フォークで指定されたモデルは 1.0 である。** 年 1 回程度のソフトフォークまたはハードフォークで、モデル ID と固定 artifact root の組を承認リスト(ApprovedModels)に追加する。追加のみで、既存モデルは上書きしない(ADR-0175)。リストへの指定は政策的決定であり、「公開されている」「独立検証可能である」ことの技術的証明ではない。
2. **後から登録したモデルは、登録した時点から審査なしで 0.3 に固定する。** 昇格・更新・失効・試行手数料はない。何年経っても、どれだけ使われても 0.3 のままである。**1.0 になる経路は、年次フォークでの指定だけである。**
3. **モデル単位の検証・審査は行わない(不採用)。** 理由: モデルに対して検証を行うと、その検証において何を信頼するか(誰が・何を根拠に「正しく共有・運用されている」と判断するか)という問題が新たに発生し、それは解決しない。モデルの公開状況・保持者数・取得可能性・共有状況をチェーンが審査することはしない。
4. **係数は配分の重みに一度だけ掛ける。** `A_m = c_m · S̄_m^1.5`(α=1.5、7 日平均の適格 bond。2026-10-16 ユーザー決定・10-22 再確認、旧 30 日)。この P_m を物理ブロック機会(RFC-0016 V+)と coinbase 配分の両方に使い、受理ブロックの報酬に重ねて掛けない。登録モデル全体に発行上限を設け、多数登録で 0.3 の枠を増やせない。
5. **claim 単位の検証は、すべてのモデルで維持する。** モデル単位の検証を行わないことは、claim の検証を省くことではない。各 claim は階層に関係なく、確率的検出・公開証拠による局所化・決定論的裁定(court)・claim ごとの抽選検証者による認証(`SecurityCertified`、正直な割合の明示的前提)を受ける。認証されていない claim は、どの階層でも fork choice・決済深度・Rule E 参加・settled anchor・weight を進めない(§0 の 2026-10-15 改定、RFC-0018)。

## 9. モデルの二階層とモデル審査の不採用（2026-10-15、ユーザー凍結前提）

**モデル単位の検証・審査は採用しない。** モデルの検証で何を信頼するかという問題は解決しないため、審査・再審や代替のモデル評価制度を追加しない。この節は、旧文書のモデル資格審査・Probation・昇格・更新・失効・試行手数料や、モデル審査の合格を登録・配分の前提とする記述に優先する。

- genesis と年次フォーク（年1回程度のソフト/ハードフォーク）で指定されたモデルは **1.0**。
- 後から登録したモデルは、登録した時点から審査なしで **0.3** に固定する。昇格・更新・失効・試行手数料はない。
- 後から登録したモデルが 1.0 になる経路は、年次フォークでの指定だけである。
- **claim 単位の検証は全モデルで維持する。** court、Gate C / SecurityCertified、claim ごとの抽選検証者認証を、モデルの階層を理由に省略しない。1.0 という指定自体は claim の正しさの承認ではない。
- material のデータ同一性、claim の実行条件・取得・資源上限の確認を、モデル単位の資格審査や階層の変更に転用しない。§8 の禁止と認証の前提も引き続き適用する。

この節はユーザーの決定の記録である。RFC-0016・0017・ADR の実装作業は Claude の担当であり、RFC-0006〜0008・0012 の作業で引き取らない。

> **claim の裁定（2026-11-07）。** §8 と本節は凍結したままである。§8 は、モデル計算の正しさを暗号学的に証明しないことと、抽選検証者の承認を正直な割合の前提の下で合意 credit の認証に用いることを定める。本節は、モデル階層 1.0 / 0.3、モデル単位の検証を行わないこと、claim 単位の検証を残すことを定める。claim をどう裁くかは [公開裁定の方針 v0.1](design/palw/consensus/public-adjudication-policy-v0.1.md) に従う。§8 の検証方法の列挙は、暗号学的証明の禁止を定めるものであり、合意ノードがモデル計算の全体を再実行する規則ではない。Panel の定足数は評決ではない。階層の数値と「claim 単位の検証を省かない」という規則は、このポインタでは変えない。

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

## §0 — ユーザー方針 2026-10-15 による改定

> **新版の適用:** 以下の原則・履歴のうち今回の新claim版と衝突する範囲は[§13](#model-free-adversarial-verification)を優先する。model-epoch配分とclaim固有認証は別制度であり、旧版の実行・Final・署名の意味は保持する。

* **閉鎖モデル・`p = 0`の残余リスクは経済的なものに限る。** §0(2026-10-14改定)は、非公開モデルの`p = 0`をbond単位の上限で被害を有界化する残余リスクとした。これに次を加える: その残余リスクは**合意finalityにもsettlement finalityにも決してならない**。残余リスクが許すのは有界な経済的損失だけであり、未認証のclaimは合意creditを持たない。

**ユーザー方針(原文):**「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。補足判断(同日):「抽選した検証者の承認を認証の柱にしている。これは承認にするということは前提にするのでそのままでいい。」

* **`Final`と`SecurityCertified`は別状態である。** `Final`は手続きの完了(利用者払いescrowの経済的settlementは既存規則どおり)。合意credit(safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来の報酬leg)は`SecurityCertified`のclaimだけが生む([RFC-0018](rfc/0018-dag-parallel-multi-model-consensus.md) §7、Gate C)。
* **認証は前提に基づく。** 合意 credit の認証は、claim 固定後の乱数で global pool から抽選した検証者の m-of-k 肯定承認を、正直な割合の明示的な前提の下で用いる（§8）。「抽選された verifier stake のうち正直な割合が閾値以上」を前提とし、無条件の数学的証明とは扱わない。暗号学的な計算証明は使わない（§8）。認証済みでも G14 で反証された claim は void され、合意 credit は取り消される。異議がないことは、計算全体の正しさの証明ではない。公開資料、容量、または独立した検証者が欠けるときは、安全な hold または abort であり、通常の Final ではない。hold は 10,000 DAA 以内の成功ではない。
* **履歴（2026-10-15、この設計では公開裁定の方針が優先する）:** 「m-of-k 肯定承認と、G14 公開告発 window の異議なし経過で成立する。」異議の不在を計算全体の証明とする読みは、[公開裁定の方針 v0.1](design/palw/consensus/public-adjudication-policy-v0.1.md) が置き換える。
* **DNS-BFTは廃止済みであり、安全性にも収束にも使わない。**
* **genesis モデルにも認証の免除はない。** 70B モデルの未認証 claim は合意 credit を持たない。合意ノードはモデル計算を端から端まで再実行しない。claim の検証は、署名、ハッシュ、Merkle opening、および元の claim に束縛した局所化した一つの primitive の決定論的再計算である。ブロック、署名、通常トランザクション、決済の検査は残る。
* **履歴（2026-10-15、この設計では公開裁定の方針が優先する）:** 「独立した検証（抽選検証者による再実行）が成立するまで合意 credit を持たない。」モデル全体の再実行を全ノードの合意要件とする読みは、公開裁定の方針が置き換える。
* 本改定は追記であり、上の本文は履歴として残す。実装は休眠(fence `palw_independently_work_certified_v1`、全presetで`None`、有効化は別途のユーザー判断)。

## 10. PALW を成り立たせる前提 — bond が時間あたりの claim を制限する(2026-10-16 ユーザー決定)

> **新版の適用:** 以下の原則・履歴のうち今回の新claim版と衝突する範囲は[§13](#model-free-adversarial-verification)を優先する。model-epoch配分とclaim固有認証は別制度であり、旧版の実行・Final・署名の意味は保持する。

MISAKA は、確率的検証と経済によって PALW を実現する。この前提に加えて、次を設計の前提とする。

### 10.1 Bond が採掘速度と報酬総額の上限を決める
- **Bond が、採掘速度と報酬総額の上限を決める。** モデル計算は、その枠を使うために必要な正当な仕事である。
- **不正をしても枠は増えない。反証されれば担保を失う。**
- つまり、miner bond から生じるブロックと報酬は、不正をして高速に claim しても、実際に計算して claim しても、同じ上限に当たる。bond で時間内に出せる claim の量は変わらない。
- 時間内に出せる claim を bond で制限することで、正しい計算を促し、PALW を成り立たせる([ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)、[RFC-0016](rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)。上限は 1 日単位で計算する)。

### 10.2 モデルを公開しない自作自演への対処
想定する攻撃:悪意あるユーザーがモデルを公開せず、miner・Panel・モデル登録をすべて自分で行い、計算をしないで claim を通す。

これを、次の 2 つで抑える。
- **モデル別配分に 1.5 乗をかける。** モデルの Panel と miner の claim bond の総額(検証されて Final になるまで返ってこない担保)に 1.5 乗をかけ、ブロック生成と報酬の配分に使う(`A_m = c_m · S̄_m^1.5`、[ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md))。
  - 利用者が集まりにくいモデルには bond も集まりにくい。そのため、そうしたモデルは配分で不利になりやすい構造にする。
- **genesis とフォークのモデルを正規モデル、追加モデルを非正規モデルとする。**
  - 非正規モデルの報酬とブロック生成は、正規モデルの 0.3 倍程度にとどまる([ADR-0179](adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md)、§9)。
  - これにより、モデルが共有されない攻撃のリスクを大幅に下げる。

### 10.3 G14 によって正規モデルでの「計算しない提出」を防ぐ
- **G14 の定義。** 計算をせずに claim を出す miner がいても、bond 付きの公開提出者が、認証された資料と、元の claim に束縛した局所化した一つの primitive の決定論的再計算で不一致を示せる。native court が有界な局所検証、責任、無効化、bond、報酬、Final を行う。Panel の署名は署名者の識別である。多数も定足数も評決ではない。止まった Panel や結託した Panel のあとにも、bond 付きの第三者経路が残る。
- この定義が満たされていれば、genesis・フォークのモデルに正しい検証者が 1 人でもいれば、計算しないで提出する不正を公開裁定で扱える。手順は [公開裁定の方針 v0.1](design/palw/consensus/public-adjudication-policy-v0.1.md) に従う。

### 10.4 設計が満たすべき条件(肝心な点)
本設計は、次の 2 つを満たすことを前提とする。
1. **不正や DoS のように claim を出しても、bond の slash による損失が、想定される利益を上回ること。**
2. **客観的な court を悪用して、正常に計算している miner・Panel を slash させる不正ができないこと。**


<a id="design-freeze"></a>
## 11. 採用済み設計の原則凍結と例外変更

**今回のユーザー指示を文書化（整理日 2026-10-17）。** 設計の変数・パラメーター以外は原則凍結し、通常変更禁止とする。重大な問題が確認された場合は、新規 RFC と新規 ADR の両方、レビュー、明示的採択によって変更できる。

変数・パラメーターの調整も、構造と不変条件を維持し、旧新値・根拠・影響・試験・承認を [台帳](governance/parameter-register.md) に残す。数値で認証や責任を外す変更を通常調整としない。

本書の「恒久」「再審しない」「変更不可」は通常の変更禁止として維持し、重大問題の例外手続きは本節に統一する。本書自体の改定にも設計責任者の明示的承認を要する。意味不変の実装・修正・最適化・試験・文書整備は継続できる。未採択 Draft の採用・技術方式の撤回・稼働有効化は今回行わない。

正本: [設計凍結](governance/design-freeze.md)。[ADR-0181](adr/0181-design-freeze-and-controlled-change.md) / [RFC-0021](rfc/0021-design-freeze-and-documentation-governance.md)。

<a id="post-mainnet-scope"></a>
## 12. 初期 mainnet の対象範囲 — 画像・動画・モデル改善の延期

**追加のユーザー指示を文書化（整理日 2026-10-17）。** RFC-0003 の画像・動画の経路と RFC-0004 のモデル改善は、mainnet 稼働開始後に取り組む機能として延期し、当面無効とする。初期 mainnet の必須実装・提供・有効化に含めず、mainnet 開始後も自動有効化しない。

必要なテキスト・TIR・固定登録・claim 認証・court・bond 予算は維持する。共有機能からの迂回的な有効化を許可せず、延期経路が閉じていることと共通経路の回帰を確認する。旧履歴・既存責任を遡及的に消さず、過去の実装・試験記録を今後の有効化承認にしない。

mainnet 後に、機能別の実装・試験・影響・互換性・移行とレビューを確認し、明示的な提供・有効化承認で再開する。構造変更は §11 に従う。文書への反映と runtime の無効化実施は別である。

正本: [延期・無効化方針](governance/post-mainnet-scope.md)。[ADR-0182](adr/0182-defer-media-and-model-improvement-until-post-mainnet.md) / [RFC-0022](rfc/0022-post-mainnet-media-and-model-improvement-scope.md)。

<a id="model-free-adversarial-verification"></a>
## 13. モデル非保有Node・公開資料・一人の正直な検証者 — 2026-11-09限定改定

本節はユーザーの明示指示に基づく新claim/profileへの限定改定である。[ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md)のモデル非保有・検証先行Final、[RFC-0049](rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](adr/0217-palw-execution-integrity-and-unilateral-refutation.md)の反証完全性・一人の異議・経済効果を維持し、[RFC-0050](rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](adr/0218-public-materials-and-one-honest-panel.md)により公開資料、cold取得、最低一件の完全承認、条件付き正常完了を採択する。詳細の所有者は[RFC-v2-0016〜0020](rfc-v2/README.md)。新profileに限り§0、§1–2、2026-10-15の§0追記、§10、旧Positive Court R2の衝突箇所に優先する。旧claim、旧profile、model-epoch配分を遡及変更しない。

### 13.1 Nodeのモデル非保有

Nodeはモデルの全重み・checkpointを取得または保持せず、全推論・GPU・モデルrunner・外部モデルURL・Panel workerを必須にしない。通常処理、court、cold起動、IBD、再起動、reorgでも同じ条件を守る。

Nodeが扱うのは固定claim・root・有界な実行定義、選出・署名・native状態、および局所courtの認証付き断片である。必要な重み・入力・中間値の小片は読むが、欠落証拠を全モデルfetchで補完しない。

### 13.2 Minerの主張と反証の完全性

Minerはモデル・program・入力・規則・seed・依存・出力と、canonical executionのWellFormed・初期状態・全遷移・終端を初回に署名固定する。全ての計算関係が有界なnative検査へ接続され、誤った実行なら少なくとも一つの元claimに結び付いた関係を反証できるprofileだけを受理する。関係の省略、後付けの細分trace、巨大演算の名称だけによる有界性を認めない。[16](rfc-v2/0016-miner-execution-commit-and-material-supply.md)と[19](rfc-v2/0019-model-free-node-and-bounded-evidence.md)が定義する。

この「計算証明」は固定されたLLM実行の整合性と、その反証・認証の契約を意味する。Miner本人または各Panel本人が特定GPUで新規に全演算を実行した物理的行為、非外注、非共有、実作業時間を証明しない。

### 13.3 公開資料とモデル未保有の独立検証

各claim、または固定後抽選する有限batch全体を能動的に完全検証する正直な選出者が少なくとも一人いることを前提とする。その人がモデルを事前に持つこと、モデル保有者が多数であること、他Panelが正直であることを前提にしない。Nodeが正直な人を識別できるとはしない。

Minerは重み・構造・program、prompt・token列・tokenizerと前処理、数値規則、seed・乱数・初期KV等の依存状態、初回traceと認証情報を元claimに束縛して公開供給する有限義務を負う。独立provider・cache・再配布者から認証付きchunkを取得でき、所有者の許可・独占席・Minerの推薦を取得や検証の条件にしない。公開URL・root・保存署名だけを供給完了とみなさない。適格な第三者にも有限の取得・異議経路を残す。

global poolからclaim固定後の乱数で選出し、モデル事前保有や取得成功者だけへ母集団を狭めない。選出者はcold取得を含め必要資料を取得し、全体を独立再実行する。この新profileに限り§1–2の通常全量再実行不要という一般方針を置き換える。全Nodeの再実行義務にはせず、旧sampled profileは保持する。少人数への任意委任・layer分担で一人の全体coverageを失わない。

正常時の資料・承認は[17](rfc-v2/0017-noninteractive-positive-verification.md)に従い一括提出できる。必要資料を取得し所定の検証を完了できなければ肯定しない。資料公開をモデル登録時審査・登録取消・tier昇格へ変換しない。

### 13.4 一人の異議と有効な反証の優先

一人の適格notice・資料要求はnative未解決状態を作り、他Panelの肯定署名数にかかわらず解決まで成功を止める。資料要求に算術反証を前提としない。資料供給履行と検査完了を分け、InspectionPendingを同じCaseId・scope・原時計で継続する。一人の完成反証がnative検査で成立すれば、元claimを無効化する。Minerの再署名・通常Panelの許可・悪意側の共同提出・追加quorumを必要としない。[18](rfc-v2/0018-interactive-negative-court-and-defaults.md)が入口・対話・直接証拠・帰責を所有する。

root不一致、私的未受信申告、未認証資料は算術有罪としない。反証成立、客観的な供給義務違反、偽異議、Panel停止、未検証終了を区別する。一人の合法経路を他者の先着予約・重複claimで排除しない。無効packetの棄却で既存caseを閉じず、共有資料のIDで申立人ごとの権利を吸収しない。

### 13.5 最低一件の完全承認と検証先行Final

今回採択した新profileのlogicalな最低肯定数を **k_min = 1** とする。選出された有資格者の完全検証承認一件以上、必須局所検査、cold取得から全体検査・異議提出までの機会、Qの解消、依存・資金・原期限がそろった場合だけ、[20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで成功Finalへ進む。一件の署名直後に成功へ進めず、全員の受領・署名を別名で必須条件へ戻さない。最低承認数1はPanel人数1の採択ではない。旧75／41、旧抽選上限、旧署名と稼働設定を変更しない。

資料不足や検証未完了は、反証を完成できなくても成功不可である。規定期限で未検証Abortと契約別精算へ進め、計算不正と推定しない。原受理a、notice a+3,120、係争hard a+10,000を維持し、取得・receipt・交代で時計をresetしない。収集と保持は別の義務である。

同batchの適格異議・反証・Q hold・依存失効を、Final・仕事報酬・credit発行だけでなく、既発行creditの予約・schedule有効化・permit消費・関連経済効果より先に反映する。既受理consumedの保存は当該batchより前のcanonical消費に限る。消費済みEXECの全取引を一括rollbackする新規則は加えない。

### 13.6 正常完了・容量・完成判定

誤claimの反証優先には、正直な一人の能動的な完全検証、必要資料または適格要求、完全な関係集合、資格・費用、保護された容量、期限内のcanonical包含、native規則の実施が必要である。正常claimの条件付き完了には、正しいMinerの供給・応答と、最大合法な虚偽caseを元期限内に終結する予約・実サービスを加える。他者の不署名だけでは停止しないが、単なる反対を永久vetoにもせず、認証terminal・客観default・case別累積予算で処理する。実サービス不足時は成功不可・有限終了を維持し、正常完了の前提破れを記録する。

cache、固定後抽選の有限batch、初回commitされた上限付き計算区間を使えるが、全leafの検証・責任・一回会計を省略しない。抽選、全再実行、cold配信、trace保持、Node court、包含を資源別・期限区間別に計上する。旧54claim／日や1 BPSを、新profileの実測容量と扱わない。全件拒否だけで利用可能なPALW完成としない。

署名は主張と責任、openingは同一性、局所positiveは当該関係だけを確認する。全員共謀、永久秘匿、永久検閲、規則を実施しないNodeまでの無条件正当性・無条件完了を主張しない。§8の計算暗号proof禁止、§9の二階層・モデル審査不採用、一つの鎖、延期機能、§14のEXEC契約を維持する。既存報酬式の不整合・bootstrap乱数接続・未実装kernelは有効化前の別gateとして残す。

モデルを一度も保有したことのない唯一正直なPanelで、正常Final、不正反証、不供給、遅延、悪意多数の不署名・虚偽case、cold最大負荷を同一実装で確認する。条件付き論証、独立レビュー、文書検査、抽象モデル、native実装、実LLM実測、公開有効化を区別する。今回の採択は **ADOPTED_DESIGN_NOT_ACTIVATED** である。


<a id="exec-credit-one-bps-and-court-capacity"></a>
## 14. EXEC生成権の分離・1 BPS・全体容量 — 2026-11-08追加採択

[新RFC-0048](rfc/0048-exec-credit-schedule-and-one-bps.md)と
[ADR-0216](adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)により、
ユーザーの今回の指示を新claim／EXEC profileの設計として採択する。
§13の検証先行Final、Node非保有、秘匿・寡占の脅威とglobal承認の保証限界を維持する。
本節は実装・測定・activationの完了を示さない。

### 14.1 高速取引を担当するEXECの通常時目標

EXEC_TXの生成目標を1 BPS、slot幅を1,000 ms、global slotの正規ブロックを最大1つとする。
有効credit、事前公開schedule、適格producer、検証・伝播・包含容量を前提とする。
modelごと・producerごとの1 BPSを合算せず、重複候補・再送を達成数へ数えない。
目標は[10](rfc-v2/0010-dag-consensus-and-native-settlement.md)の運用条件と測定分母を付けて評価する。

1 BPS、TPS、暫定実行の遅延、本体受理の遅延、不可逆決済の遅延は別の指標である。
既存の本体／Heartbeat、DAA、報酬周期、court期限を1秒へ置き換えない。

### 14.2 PALW仕事から有限creditへ

[07](rfc-v2/0007-claims-exec-and-answer-accounting.md)の単一gateで、成功Final、有効認証、
成熟、依存、資本・bond予算、失効を確認して期限付きExecCreditを一度だけ発行する。
Finalの一覧・snapshot・slice・scheduleを別の発行根拠にしない。
空slotや供給不足を解消するために、未認証仕事や期限切れの権利を復活させない。
EXEC_SLICEはrootの計算記録であり、それだけでTxPermitを増やさない。

### 14.3 未来scheduleと現在slot

対象集合をseedより前に固定し、future scheduleを合意stateへcommitする。
ScheduleIdが生成資格を指し、EXECの接続先はその時点で許可された取引履歴・anchorを指す。
通常tipの伸長は割当を変更せず、失効permitだけを無効にする。
authorityのreorgは版付きの状態復元として処理する。

producerは現在slotのpermitから生成する。過去round回収、モデル推論、証拠取得、
次期schedule作成、Claim詳細の再検査をその通常ループへ入れない。
validatorの包含規則はlocalな受信秒から独立させ、1秒の署名時刻を客観的に証明したとは扱わない。
起動・credit補充・work anchor・認証・公開courtにEXECの先在を要求しない。

### 14.4 courtとEXECを合算した資源計画

[05](rfc-v2/0005-bounded-resources-and-long-context.md)に従い、依存DAGの最長経路、
全合法分岐・複数claimの総需要、保証供給、期限内に実行可能な予約scheduleを同時に確認する。
owner専有枠の順番待ちを公開者の必須経路にせず、認証共通資料と独立処理の並列化を使う。
真の依存、cold取得、包含、reorg・再起動、保持義務は残す。

1 BPS目標だけでcourtの供給容量を増やした扱いにしない。共通のCPU・メモリ・IO・帯域・包含資源は合算する。
元のa+3,120／a+10,000と保持期間の区別を維持し、供給不足を成功Finalや自動slashへ変換しない。
Nodeに全モデル取得・全推論を要求するfallbackを追加しない。

### 14.5 完成判定

超過計画を拒否できることは受付の安全性であり、対象実モデルを使えることとは別である。
目的のモデル・文脈長・jobについて、明示した供給・配送・包含・復旧前提の下で、owner枠を全て共謀者が占有してもcold outsiderが取得から裁定まで進めること、
EXEC負荷とcourt負荷が重なっても約束した供給を維持することを、同じ実装・設定・対象機材で示す。
成功例だけの中央値や短いfixtureを、全サンプル・全合法経路の完了保証へ転用しない。
全保有者の永久秘匿で供給前提が成立しない場合は、算術裁定の無条件成功を約束せず、成功停止・未検証終了・適格なdefault・残存責任の処理を別に確認する。
