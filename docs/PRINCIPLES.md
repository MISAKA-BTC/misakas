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

> **MISAKA不可侵原則 —「確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。」**
> *Probabilistic Detection. Public Localization. Deterministic Adjudication. Economic Deterrence.*

---

## 1. 不可侵原則

MISAKAの有用計算は、Panelの多数派、単独Panel、producer、または特権的operatorの正直さを、安全性の最終根拠としてはならない。

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
- Freivalds/GKR等によって集約された計算制約

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
2. 承認済みの確率的健全性とgrinding耐性。
3. 公開・認証済みmaterialの可用性。
4. 1人の適格なPanel外検証者による有界な不正局所化と客観的裁定。
5. 不正による最大利益と整合する、徴収可能な担保・処罰。
6. 正直な検証者が期限内に参加できるだけの資源・費用・検証インセンティブ。
7. 異議申立て、DA/default、Final、reorgの整合性。

## 7. MISAKAが守るべき本質

MISAKAは、巨大な有用計算を全員で再実行して正しさを保証するチェーンではない。

MISAKAは、次の3つによって、計算費用よりも検証費用が重くなり得る有用計算を持続可能な合意資源にするチェーンである。

- 確率的な部分検証によって不正を発見する。
- 発見した不正を公開証拠から決定論的に裁く。
- その検出確率と客観的処罰を経済設計に組み込む。

そして、**MISAKAの安全性は、Panelの人数や誰かの正直さではなく、検証の健全性、不正追及の公開性、裁定の客観性、経済的責任によって定義される。**

> **MISAKA不可侵原則 —「確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。」**
> *Probabilistic Detection. Public Localization. Deterministic Adjudication. Economic Deterrence.*

---

## 付録 — この前提が現在の作業をどう縛るか (Lead, 2026-10-09)

This appendix records where the principle already decides open work. It is a pointer, not part of the principle.

- **§6 is the gate for earning.** A class, kernel or plan that does not meet all seven conditions may stay registered, but it earns no reward and no consensus work weight. The devnet showed that the V2 lifecycle today lets a class reach Probation on seat readiness alone. Closing that is a release blocker (readiness matrix, "G14-for-rewards").
- **§2 and §4 decide the real-scale question** for large models, where reading a whole claim is not feasible (the soundness review dossier's SG-06 and reviewer question Q-01). A plan earns only if:
  - its bound is proven, including the effective bits after retries and grinding;
  - its detection probability is stated, including the probability that a capable honest verifier actually checks;
  - its collateral covers the maximum gain divided by that probability.

  Where that is unaffordable, aggregated checks (Freivalds/GKR-style) are required, not a lower assumed detection rate.
- **§2's grinding resistance** is why the sealed-source beacon (v3) and the complete-check bootstrap are prerequisites for any sampled approval.
- **§3's "no escape to economic Final"** is enforced by the bounded demand/proof/grace deadlines of the kernel route and OPV.
- **Condition 7 of §6 (reorg consistency)** is why the fork-choice decision (ADR-0175) is a prerequisite of the full-activation release.
