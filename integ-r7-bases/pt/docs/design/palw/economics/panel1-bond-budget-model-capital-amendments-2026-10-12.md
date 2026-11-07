> **リポジトリへの取り込み（2026-10-12, Lead）:** ユーザーが提示した第2版を本文のまま収録する（第1版は第2版に包含される）。
> 各ADR/RFCには、この文書を参照する日付付きの改定節を追加した。本書の規範文は各ADR/RFCの改定節が採用する範囲でのみ規範となり、
> それ以外は提案・受入試験仕様である。実装・試験・有効化は未実施。

# MISAKA PALW — Bond/time budget・Panel=1・モデル拘束資本倍率 改定案と受入試験

対象基準: MISAKA-BTC/misakas `pre` / `bc7c469469d649b9f2af2e822093e5dd62dd9764`
作成日: 2026-10-12
更新: 第2版（2026-10-12）。Miner・Panelのclaim bond合算と、モデル別1.3〜2倍の配分倍率を追加。
状態: 設計提案。リポジトリへの適用、実装、安全性実証、ネットワーク有効化は未実施。

本書の Panel=1 は「通常の報酬付きclaimに、同時に有効な検証担当seatを1つ割り当てる」を意味する。ネットワーク全体の検証者を1名に限定しない。仮の識別子は既存のRust型、wire tag、Params fieldではない。

第2版の出典区分: モデルに拘束されたMiner・Panelのclaim bond総額へ1.3〜2倍の係数を適用し、参加資本を集める公開モデルを相対的に有利にする方針は、ユーザーの追加要請である。§3Aの配分式、固定評価期間、数値例、攻撃試験は、その方針を具体化する提案・検証条件であり、既存実装や実測結果ではない。添付された前版MDと会話中の要請を基に更新したもので、GitHubの最新状態は今回再取得していない。

## 1. 最上位の設計原則

### ADR-0176 §1の正本を維持し、RFC索引と関連RFCから参照する追記案

MISAKAは確率的検証と経済的拘束によってPALWを実現する。Miner bondと共通の合意時間が、その元本から発生できるclaim、報酬付きブロック、報酬総額、Final weightの上限を決定する。正当なモデル計算は、その枠を使用するために必要な仕事である。

同額の適格拘束元本、同一期間、同一ruleset・容量倍率に対して、正直に計算するminerと、不正な計算省略によって高速にclaimを提出するminerに与える上限を同一にする。計算省略、即時Valid、再提出、取消、失効、鍵・bond・modelの分割や移動によって、同じ元本の枠を増加・早期回復させてはならない（MUST NOT）。

枠を確保しただけでは支払わない。適用modeの正当なclaim、検証証拠、裁定、Final条件を満たした範囲でのみ報酬・確定creditを付与する。客観的に反証されたclaimは報酬を失い、適用規則に従って徴収可能な担保を失う。

この構造は「bondによる採掘機会の上限」と「確率的検証・反証・失効・没収による不正抑止」の組合せであり、無検証のstake-only採掘ではない。Panel人数や計算速度を増やしても同じproducer元本の絶対上限は増えない。

**追加の設計原則 — モデルへの実参加資本を配分へ反映する:** モデルに帰属する、検証・Finalまで返還されないMinerとPanelのclaim bondの適格元本を、重複なく合算する。その総額に応じた1.3〜2倍の係数を、モデル別のブロック生成機会と報酬の配分に反映する。モデルを公開して参加者・実拘束資本を集める方を、自己資本だけで登録者・Miner・Panelを兼ねる閉鎖的な採掘より相対的に有利にすることを目的とする。

この追加は、公開可否による資格停止でも、公開したという自己申告への報酬でもない。§3Aのモデル集合の配分係数と、§3の個々のMinerの絶対上限を分ける。配分倍率は検証・Final条件を免除せず、既存の絶対上限を後から乗算で増やさない。

同じ上限は、実際に完了したclaim件数や受取額が同じであることを意味しない。正当な高速化を不正と扱わない。未承認の仕事の再利用、偽のstatement、誤った計算関係など、適用規則違反と速度そのものを区別する。

### 正本の配置

ADR176を共通原則の正本とする。RFC15のPanel=0専用規定と、Panel有無に依存しない共通経済会計を区別する。共通台帳詳細は、例えば新設の `docs/design/palw/bond-budget-ledger-v1.md` に置き、RFC10/14/15から参照する。新設先は提案であり、本書ではリポジトリに作成していない。

## 2. 改定箇所

| 文書 | 改定 |
| --- | --- |
| ADR176 §1、D1–D6 | 共通原則、rolling枠、予約帰属、枠の非早期回復、失効と没収、経済受入条件。モデル配分係数と個別絶対capを分離 |
| ADR166 §2–§3 | 新rulesetでは失効時の即時返却で採掘枠を回復しない。交代も絶対期限内に制限 |
| ADR173 公開訴追の規定 | Panel=1でもnon-seatの証拠取得・裁定権限を維持。Panel署名を真実の最終根拠にしない |
| ADR177 D1/D2 | 配布者の提供義務と、miner/Panelの保有・遂行条件を区別 |
| ADR177 配分・倍率・経済評価の決定（D3–D7向け） | Minerのみの配分元本と区別してMiner＋Panelの実claim bondを合算。1.3〜2倍の資本依存係数、ブロック機会配分、三層cap、自己資本共謀との比較を追加 |
| RFC10 §0、§3、§6、§9 | 1-seat暫定mode、scope付き正の検証証拠、同一snapshotからの有界交代、全呼出箇所の監査 |
| RFC02 seating、RFC06 shard、RFC03 readiness | 単一seatで全検証scopeを覆える経路だけを暫定mode対象にする。旧複数seat前提を一括定数置換しない |
| RFC03 §I.4.7、Decision 22 | challenge放棄後に完全なwindowを再付与する規定を新rulesetでは置換 |
| RFC14 §7.2–§7.4 | 固定期限、開始cutoff、未解決終了、direct proofと資源割当の一貫化 |
| RFC14 §16、RFC15 §8の共通経済部分 | モデル別claim bond台帳、配分snapshot、係数、正常Finalと非不正終了・没収時の取扱い、閉鎖/公開比較 |
| RFC08・RFC12のブロック権/報酬/settlement接続 | モデル配分をブロック生成機会へ接続する新提案を明示。物理block頻度・DAA・fork choice・Final weightの変更とは分離し、別reviewとversioned activationを要求 |
| RFC07 §VI.3、§VI.8 | 独立bootstrap検証とentropy資格を分離し、依存グラフの非循環性を要求 |
| RFC11 §17 | 通常BeaconConformanceと、限定したbootstrap exact source経路を型で分離 |
| RFC15 Status・activation | Panel=0は当面有効化しない。共通会計はPanel=1にも適用する |

## 3. N1 — Bond/time台帳の具体化

### ADR176 D1/D3/D4補足案

1. 長さW DAAのrolling会計を基準案とする。期間境界で同じ元本の枠を二重使用しない。資本増減時には受理済み予約とその根拠を保持し、新規admissionに使える実効headroomだけを更新する。
2. claim受理時に、当該claimが派生させ得るclaim枠・ブロック権・報酬・Final weightの最大権利を、同一の元本・予約へ原子的に帰属させる。
3. 受理DAAをdとすると、消費済み枠を `d + W` より前に再利用しない。Valid、Final、PanelUnavailable、取消、court終了は早期回復理由ではない。
4. 予約から支払済み・確定creditへ移す処理は二重計上しない。有罪時は当該権利を失効させるが、失効額を新claimの予算へ早期に戻さない。
5. 採掘枠の回復、資金の送金可能性、court/retention責任からの解放を別々に検査する。同じ元本を新しいbond/鍵へ移して消費履歴を消してはならない。
6. rhoを増やしてもブロック総量・報酬総額・Final weight総量は増えない。端数、最小支払、複数leg、root/slice/rider/batchで増幅させない。
7. network/model/bondの三層予算を、受理・ブロック権行使・支払・Finalおよび全reader/undoで共通に検査する。
8. 同じ資本を分割しても、分割後の合計権利を分割前より増やさない。実質所有者を推定するのではなく、認証済み元本の二重計上と消費履歴の消去を防ぐ。
9. 期間内総量capと短時間のburst capを区別する。平均採掘速度だけでなく瞬間集中も制限する場合、短窓の上限または事前確定した発行可能時刻を別のversioned規則として固定する。rolling capだけで均等発行を保証したと記載しない。

共通上限:

```text
N_claims(b,I)       <= Q_max(C,W,rho)
N_reward_blocks(b,I)<= B_max(C,W)
R_attributed(b,I)   <= R_max(C,W)
FinalWeight(b,I)    <= F_max(C,W)
```

これらは同じ資本・rulesetでの機会上限である。攻撃者と正直なminerの実際の送信数、完了数、支払額を等しいと要求しない。

### ADR166との接続

新rulesetでは、PanelUnavailableは非不正の終了とし、producerを自動slashしない。ただし「無罰」と「消費した採掘枠を即時に再利用可能」は同義ではない。資金ロックを解除する実装を採る場合でも、元本の消費履歴と `reuse_not_before` を引き継ぎ、移転/再登録による迂回を禁止する。より単純な初期実装では、必要元本の拘束を既定回復時刻まで維持する。

### ADR176 D6の受入条件

同一機会数を比較する簡略評価では:

```text
p_effective * (R_at_risk + L_collectible_net) > C_saved
```

p_effectiveは検査器の検出率だけでなく、実際の検査、局所化、証拠包含、客観裁定、徴収の完了まで含む。共謀reporterへのbounty還流、担保の使い回し、同時claimの相関、外部利得は別途評価する。外部利得を含める場合は右辺へ加える等、一貫した純利益比較にする。

全未決済claimを同時に不正化した攻撃について、総没収見込額を実在する回収可能資本より大きく見積もってはならない。Panel報酬や検証費の省略益もMiner/Panel共謀の損益へ含める。正直な計算・検証が実際に設定windowへ収まることを同じhardware/resource条件で確認する。

## 3A. 追加 — モデル別Miner・Panel claim bondと1.3〜2倍の配分係数

> **2026-10-14決定:** 本節のうち、Miner＋検証者の元本合算（Panel=0に合わせてclaim拘束のpermissionless verifier元本へ一般化）とブロック生成機会への接続を採用した。配分曲線は`μ`案ではなく`α = 1.5`、平均期間は30日相当DAA、`W`は7日相当DAAとした。マイナー報酬はモデル登録者へ直接分配しない。§3A.8の`p_effective`不等式は、[PRINCIPLES §0](../../../PRINCIPLES.md)により有効化gateではなく評価指標として読む。正本は[RFC-0015 §8.5.5](../../../rfc/0015-panel-free-permissionless-verification.md)。

### 3A.1 採用したい設計方針と対象攻撃

次の攻撃を明示的な対象にする。悪意ある一人の実質所有者がモデルを非公開にし、モデル登録者、Miner、Panelを別のbond・鍵として自分で用意する。Minerは正当なモデル計算を省略し、共謀Panelがreceiptを出して、反証されないままclaimをFinalに進めようとする。同一bond/operatorの除外だけでは、別IDによる実質同一所有者を排除したことにはならない。

**ユーザーの追加方針:** モデルにおける、検証・Finalまで返還されないMinerとPanelのclaim bond元本の総額に、1.3〜2倍の係数を反映して、ブロック生成機会と報酬を配分する。第三者が参加しにくい閉鎖モデルは実拘束資本を集めにくいため、公開して実参加資本を集めるモデルより不利になりやすい構造を目指す。

これはN1/N5に対する**経済的な抑止を強める設計目標**である。「全ての閉鎖共謀を排除した」「同額自己資本にも必ず勝つ」「倍率だけで計算省略の利益が消えた」という完成宣言ではない。公開/非公開というラベルを合意入力にせず、実際に拘束された適格元本を入力にする。

通常のAI利用者数、download数、ウォレット数、投票数、モデルの宣伝、入手成功報告だけでは係数を増やさない。ユーザーが増えても、適格なMiner・Panelとしての実claim bondが増えなければ、この規則上の配分は増えない。配布者への強制公開、専用Seeder、提供拒否に対するslashは復活させない。

### 3A.2 集計する元本 — MinerとPanelを合算し、同じ資金は一度だけ数える

モデルm、合意時刻dに対して、まず次を定義する。

```text
S_m_live(d) = C_miner_claim(m,d) + C_panel_claim(m,d)
```

`C_miner_claim`と`C_panel_claim`は、そのモデルのclaimに実際に紐付き、返還・再利用できない**適格な実元本**である。両集合は資金単位で重複させない。roleの名札を変えて同じ100をMiner100＋Panel100＝200とは数えない。別々の100を実際に拘束した場合は、実質所有者が同じでも実元本200として扱う。

集計単位は署名数やclaim数ではなく、認証済み元本lotとその予約部分とする。概念上の予約には、少なくとも次を保持する。

```text
capital_lot_id, reservation_id, model_id, claim_id, role
amount, bound_at, allocation_until, reuse_not_before
liability_scope, release_rule, ruleset_id
```

同じlotを複数claim/モデル/roleへ割り当てる場合、重複しない予約部分の和が実在する拘束元本を超えてはならない。claimごとに表示した「bond全額」を足し上げず、一つの担保を複数責任へ重複利用している場合は、そもそもadmissionで責任余力を検査する。

新しいモデル配分用の`S_m_live`は、**Miner自身のQ/B/R/F capを決める元本とは別の会計指標**である。Panelの元本をモデルの集計へ加えても、それをproducerが所有する新しい採掘元本としてコピーしない。モデル登録料、AMM、Position、idleなPanel登録bond、未担保の待機seat、まだ支払われていない予定報酬は、自動算入しない。

Panel=1では、一つのclaimに一つの正規担当seatの予約を結び付ける。無応答の旧seat、待機seat、交代回数から架空の追加元本を作らない。旧seatの証拠責任により残る拘束と、新担当の実予約は別に記録し、責任保持だけの旧ロックを追加の配分参加として重複加点しない。

### 3A.3 Finalまでの拘束と、Finalを遅らせて倍率を稼ぐ攻撃

正常なclaimのclaim bondは、必要な検証・Finalが終わる前には返還しない。ただし、**Finalしたら直ちに同じ枠を再利用できるという意味ではない**。§3の`d + W`、未解決court、retention、徴収可能責任による拘束を同時に満たす必要がある。

非不正の期限切れ、PanelUnavailable、客観的な有罪/defaultは、各rulesetの有限な終了・清算規則で処理する。Finalしないclaimの資金を永遠に拘束する規則にはしない。slashや返却を含む実元本変化は将来の配分へ反映し、既に固定したclaimの権利を遡及的に増額しない。

**未Final残高を単純に瞬間比較すると、receipt/Finalを故意に遅らせて残高を大きく見せる誘因が生じる。** 同じ元本の二重計上を防ぐだけでは、この問題は消えない。このため、以下を具体化の候補とする。

> 新しい適格な資本予約に、受理時点で固定した配分評価期間`[bound_at, allocation_until)`を与える。正常な早期Finalでもその期間は資金を拘束して同じ評価を維持する一方、遅いFinal、challenge、Panel交代では期間を延長しない。評価期間終了後の責任保持だけの拘束は追加倍率へ算入しない。

この候補は、**実際に返還不能なclaim bondに限定しつつ、正常Finalの早い遅いだけで優劣を作らない**ための補助設計である。少なくとも`allocation_until`と既存`reuse_not_before`の間で資金を自由化しない。配分評価期間の長さは、Model IDや自称の計算量で伸ばさず、共通rulesetで固定する。Final後も共通期間内は拘束を維持するという明示的な条件を含む。

適格性を失った予約、没収された元本、失敗後に責任上残っているだけの元本を、無条件に同じスコアへ残さない。その処理は資本・責任・配分台帳で区別し、同一block内の処理順も固定する。

配分epoch tに使用する量を次のように置く。

```text
S_m(t) = snapshot_or_time_average(
    eligible_unique_locked_claim_capital(m),
    canonical_previous_window(t)
)
```

snapshot方式か期間平均か、評価period、資本成熟、同一epoch内の加減算の扱いは採用前に決定する。期間平均だけで意図的遅延を防げるとは扱わない。instantな未Final残高を直接使う案も比較対象に残すが、遅延・滞留攻撃で有利になるなら採用しない。

### 3A.4 1.3〜2倍をどこへ掛けるか

ユーザー指定の1.3〜2倍は**係数の検討範囲**として記録する。モデル別の適格拘束元本に応じた単調非減少の係数`mu(S)`を候補にする。

```text
S_m(t) > 0 のとき:
    1.3 <= mu(S_m(t)) <= 2.0
    A_m(t) = S_m(t) * mu(S_m(t))

S_m(t) = 0 のとき:
    A_m(t) = 0
```

`A_m`は、モデル別のブロック生成機会と報酬を割り当てるための重みであり、BILIそのものでも、任意のreward mintでも、実際に生成済みのブロック数でもない。1.3から2.0までをどう上げるかの閾値・曲線は未採用である。整数/fixed-pointで計算し、overflow、丸め残余、境界値を規範化する。

**全モデルへ同じ係数を一律に掛けて正規化しただけでは、相対配分は変わらない。** 資本が集まるほど有利にする追加効果を意図するなら、資本に依存する係数/配分曲線を明示する。全モデルが2.0に飽和した場合、追加係数による差はなくなり、元本比が残る。

以下は既存上限と整合させるための**配分式の候補**である。

```text
Z(t) = sum_j A_j(t)

Z(t) > 0 のとき:
    model_reward_target(m,t) = R_network(t) * A_m(t) / Z(t)
    model_block_target(m,t)  = B_network(t) * A_m(t) / Z(t)

Z(t) = 0 のとき:
    model_reward_target = 0
    model_block_target  = 0
```

`R_network`と`B_network`は、別途確定した当該期間のPALW報酬予算・報酬付きブロック機会の予算である。ブロック機会は整数の権利/採択確率/固定小数点creditなど、選ぶ表現を別途規範化する。確率で配分する場合でも実際の個別・network capを別に強制し、期待値だけで上限を満たしたことにはしない。

倍率はこの配分重みの段階で一度反映する。配分後の実支払やブロック数へさらに1.3〜2倍を重ねない。未使用枠と丸め残余は既定の予算内で処理し、二重配分・追加発行を禁止する。元本だけでは支払わず、有効claim、Panel証拠、Final、unique-workと予約の条件はそのまま適用する。

### 3A.5 モデル配分と、Minerごとの絶対上限の整合

本書の基準案は、**新係数を既定の絶対上限内の配分に用いる方式**とする。§3のproducerごとのQ/B/R/Fを、`mu`で後付け増額しない。Panel元本を増やしただけで同じproducerの最大Q/B/R/Fを増やす抜け道も作らない。

```text
実際の支払:
    networkの残予算以内
    AND modelの配分/予約以内
    AND 帰属producerのR_max以内

実際の報酬付きブロック機会の行使:
    networkの残機会以内
    AND modelの配分/予約以内
    AND 帰属producerのB_max以内
```

同じproducer元本で既に絶対capへ達している場合、モデル係数が上がっても、そのproducerの実収入・生成機会は増えないことがある。追加の適格Miner元本が入れば、モデル集合のproducer上限の合計は増え得るが、同じ元本を再利用した増加ではない。モデル内の既存Miner、追加Miner、Panel、登録者の取り分は別に定義する。

**総枠内の相対配分ではなく「各bondの絶対上限自体を1.3〜2倍へ変更する」案は別案**である。その場合は基準上限と最大上限をADR176に明記し、新rulesetとして予約会計・総発行・攻撃評価を再設計する必要がある。本書ではその変更を暗黙に採用しない。両案を同じ式で混ぜず、配分だけの方式でユーザーの期待する利得差が実現するかを測定する。

また、今回の要請はブロック生成への影響も含むため、coinbase配分だけの変更として済ませない。RFC08/10/12、ADR177のブロック/配分境界、台帳・writer/readerに、新しいブロック機会の帰属規則を明示する。物理blockの総頻度、DAA/難易度、fork choice、Final weightを同時に変更するとは決定していない。特にFinal weightへ係数を自動乗算しない。必要な変更は別のversioned review・試験・activationを要する。

### 3A.6 自分のclaimで自分の倍率を即時に上げない

claimに使用する`S_m`、`mu`、model配分、producer帰属、最大支払/ブロック権は、事前のcanonical snapshotとrulesetで固定する。そのclaim自身の新規拘束が、同じclaimの配分係数を遡及的に増やしてはならない。

新しい実元本の予約は、定義した成熟・snapshotの規則に従って**後続の配分**へ反映する。遅い支払時点で高くなったモデル倍率を選び直さない。reorg/restart/IBDで同じsnapshotと権利を再構成する。

新規モデルについて「配分が0なのでclaimを予約できず、claimがないので配分元本が0」の循環も防ぐ。候補案は、実資本・Miner/Panelの能力・通常のadmission条件を満たす有限の`PendingAllocation`予約を、現epochの報酬やweightを付与せずに受け付ける方法である。後続snapshotで枠が得られた場合のみ通常経路へ進み、取得できなければ既定の有限期限で無報酬終了する。fee・資源cap・資金拘束・返却を規範化し、架空のclaim/無担保予約を資本に数えない。これは提案状態名であり、既存enumではない。

この配分起動手順は§6のWork Beacon bootstrapとは別問題であり、前者を解決しても後者の乱数安全性を満たしたことにはならない。

### 3A.7 数値例 — 資本と倍率の効果を分離する

以下は配分式を説明する仮定例であり、実際のMISAKAのbond額、採用閾値、利回り、報酬量ではない。資本単位は任意とする。

| モデル | 適格Miner claim元本 | 適格Panel claim元本 | 合計S | 仮の係数mu | 配分重みA |
| --- | ---: | ---: | ---: | ---: | ---: |
| 閉鎖・自己資本中心 | 800 | 200 | 1,000 | 1.3 | 1,300 |
| 公開・追加参加資本あり | 4,000 | 1,000 | 5,000 | 2.0 | 10,000 |

この2モデルだけなら、絶対capを適用する前の目標比率は、閉鎖モデルが約11.50%、公開モデルが約88.50%となる。係数なしの元本比では約16.67%と83.33%なので、資本差に加えて係数差も働く。

1.3と2.0の間の係数差だけなら`2 / 1.3 ≈ 1.5385`倍である。この差を「どのモデルも実支払が必ず2倍」「公開しただけで2倍」と説明しない。総報酬が固定なら相対配分であり、個別capや未使用枠でも実支払は変わる。

閉鎖攻撃者が同じ5,000を同じ条件で本当に拘束できるなら、同じ係数・同じ配分重みを得る。公開/非公開だけで異なる係数を与えない。実質所有者を知っているという仮定も置かない。

### 3A.8 N1/N5として残す経済的な検証条件

この追加は「閉鎖的な自己資本だけの運用は、追加参加資本を集めたモデルより配分上不利になりやすい」ことを狙う。**自己資本を十分に用意できる攻撃者、不正による計算費用節約、全Panel共謀を、倍率だけで必ず排除できるとはしない。**

比較はモデル全体の報酬だけでなく、同じ実質所有者が受け取るMiner報酬・Panel報酬・登録者の取り分を合算した純利益で行う。公開後のモデル報酬が増えても、その全部を既存の登録者/Minerが受け取るわけではない。

最低限、次の条件を分けて評価する。

- 閉鎖・自己資本・正当な計算と、閉鎖・同じ自己資本・計算省略＋Panel共謀。
- 公開・追加参加資本あり・正当な計算と、公開しても追加資本が集まらない場合。
- 閉鎖者が同じ総元本を自己資本または借入で用意する場合。異なる鍵/roleも同じ所有者へ損益帰属させる。
- 元本集中、モデル複製/分割、Panel元本偏重、全モデルの係数飽和、個別cap飽和、検証遅延による拘束残高操作。

実支払、正当な計算・検査費、資本拘束/借入費、配布費、fee、未決済責任、共謀bounty還流、実効検出・裁定・回収確率を同じ期間・同じ費用単位で比較する。経済試験は、公開前後の所有者利益、独立参加者の利益、network全体の予算を別に報告する。

§3の`p_effective * (R_at_risk + L_collectible_net) > C_saved`は維持する。検査費の省略や外部利得も含む同額機会での純利益比較へ拡張してよいが、`mu`を導入しただけでこの不等式が成立したと扱わない。`p_effective=0`で閉鎖者が自己資本を用意できる場合、費用節約の優位が残ることがある。

公開優位を合格とする資本範囲・市場仮定・必要利得差を評価前に固定する。条件が満たせなければ、係数範囲、model内配分、拘束設計等の再検討結果と未解決点を報告し、倍率を導入したという理由だけでN1/N5を解決済みにしない。

### 3A.9 ADR/RFCへ転記する規範文案

> モデル別のブロック生成機会と報酬配分には、そのモデルに実際に拘束されたMinerおよびPanelの適格claim bond元本の和を使用する（MUST）。同じ実元本を複数claim・role・modelで重複計上してはならない（MUST NOT）。
>
> 適格元本に応じた1.3〜2倍の配分係数を設計候補とし、公開によって実参加資本を集めるモデルを、自己資本だけの閉鎖的な運用より相対的に有利にすることを目的とする。倍率曲線・閾値・snapshot・評価期間・整数丸め・model内配分はrelease前に確定する。
>
> claim bondは、正常claimの検証・Finalおよび共通の枠回復・責任保持条件を満たす前に再利用してはならない。Finalの故意の遅延、challenge、Panel交代で評価期間・採掘枠・絶対期限を延長してはならない。
>
> 配分係数は検証とFinalの代替ではない。network/model/producer-bondの上限を同時に維持し、既定の個別絶対capや総発行を後付けの乗算で増やしてはならない。モデル資本にはPanel元本を含めても、producer元本として二重に扱わない。
>
> モデルの非公開や第三者への提供拒否だけで登録者を罰しない。同じ適格元本・条件なら、自己資本と外部資本を同じ規則で扱う。公開の利益は追加の実参加・実資本から生じるという仮定を、敵対的な経済試験で評価する。
>
> 通常claimのPanel=1、必要な正の検証証拠、non-seatの客観反証、N1の実効検出・経済評価は維持する。モデル拘束資本倍率を追加しただけで、Miner/Panel共謀に対する安全性を証明済みと宣言してはならない。

### 3A.10 改定の位置づけ

本節はADR176の個別絶対上限を維持しつつ、ADR177のモデル配分へ新しい集計対象・倍率・ブロック機会の接続を提案するものである。既存の「Miner元本」と、今回の「Miner＋Panelの適格claim元本」を同じ変数の意味として黙って置換しない。新しいpolicy/versionと移行規則を明記する。

RFC14 §16およびRFC15 §8の共通経済部分に集計・清算・経済試験を置き、RFC10には正規Panel予約と交代、RFC08/12にはブロック権・支払・Finalとの接続を反映する。既存claimには元のsnapshot・rulesetを適用し、後から大きな係数を与えない。本文書の更新はこれらの原本へのcommitやネットワーク有効化ではない。

## 4. N5 — Panel=1暫定mode

### RFC10 §0追記案

通常の新reward-bearing modeは、1 claimにつき同時に有効なassigned verification seatを1つ持つ。Panel=0へのtimeout fallbackを禁止する。候補集合には複数の参加者を認め、non-seatの検査・反証権限を制限しない。

Minerとassigned Panelは、適用scopeの正しい登録モデルを保有し、実行/検査と認証に必要な能力を備える。モデル登録者に対する一般第三者への提供義務は導入しない。合意判定は認証済み状態と証拠に基づき、特定nodeのHTTP結果・model取得速度を直接入力にしない。

Panelは承認済みの確率的checker、固定scope、commit済みstatementに対する適正なchallenge、必要な正の検証証拠に基づくreceiptを提出する。署名数1だけを計算正当性の証明としない。

producer/registrantとの同一bond・operatorを除外する。これは同一実質所有者でないことの証明ではなく、Sybil/共謀を攻撃モデルに含める。

### 有界の交代

snapshot、claim seal、選出policy、代替候補順序、最大交代数はclaimの規則に固定する。交代のために新しい乱数・新しい候補集合・新しい採掘枠・新しい絶対期限を得ない。

1つ目のseatが無応答なら、許可された残り時間の範囲で、固定順序の未使用seatへ交代する。健全な検査に必要な残り時間がない場合は交代を開始しない。使える候補または時間が尽きればPanelUnavailable/UnverifiedExpired相当で無報酬終了し、未払いPALW報酬・Final creditを付与しない。名前は概念上の終了分類であり、新しいwire enumの割当ではない。

検査不一致が出た場合、単にValidを黙って出さずに交代を待つだけでなく、適格なbounded pursuitまたは完成したobjective proofへ進む。中身のない疑義で無期限停止させない。検証者のローカル失敗や沈黙だけをproducer fraudへ変換しない。

### 正の証拠を必須にしたFinal

```text
FinalEligible =
    OriginalRulesetAndIdentityBound
    AND NetworkModelBondBudgetsSatisfied
    AND AuthorizedSingleSeatPositiveEvidenceAccepted
    AND RequiredClaimEvidenceAndCourtObligationsSatisfied
    AND ObservationCutoffPassed
    AND NoUnresolvedAcceptedPursuit
    AND NotRefuted
    AND UniqueWorkAndSettlementSatisfied
```

RPC上のFinalだけでなく、実際の支払、vesting、ブロック権、Final weight、EVM/bridge等のreaderも同じ結論を使用する。既存の許可済み暫定creditは別の上限とreorg規則に従い、未検証の不可逆利益にしない。

Panel=1は「必ず正直な検証者がいる」を意味しない。唯一のseatとproducerが共謀し、外部の実検査もない場合、未検出不正の残余リスクを評価に残す。枠の保存はこの場合も破れないが、計算正当性まで無条件に保証したとは報告しない。§3Aのモデル拘束資本倍率は、この閉鎖共謀の相対利益を下げる追加策として評価するものであり、反証経路や正の検証証拠を省略する理由にはしない。

Panelのmissと客観的な虚偽証言を区別する。承認samplingの検査scope外で不正が後から見つかっただけで、Panelを自動的にFalseValidとしない。署名済みの主張・証拠が具体的に何を保証し、どの反証で責任が成立するかを固定する。

### 1-seat移行監査

`seat_count=1` の一括置換を禁止する。segment mask、`seats-1`、quorum、最低独立seat数、shard outsider、署名者担保、Panel fee配分、再抽選、FalseValid attribution、DA/court、全Final経路を点検する。単一seatで全検証scopeを担えないprofileはこのmodeでは有効化しない。監視コストの低下で余ったPanel配分を新規mintやproducer枠増加へ転換しない。

RFC15はPanel=0の将来設計として保持するが、当面の実装・release対象から外す。共通bond予算はRFC15の有効化を待たずに適用する。

## 5. X3 — 固定期限と有限終了

### RFC03 §I.4.7 / Decision 22の将来規則を置換

challengeの放棄、失敗、close、Panel交代により、claim全体のchallenge windowを最初から付与し直してはならない。licence anchorを記録上更新する場合でも、challenge開始cutoff、absolute hard deadline、producer枠の回復時計は変更しない。

### RFC14 §7.2補足

経済的なclaim受理時点d0で、rulesetに基づく最大の準備期間、観測期間、裁定期間と絶対deadlineを固定する。最初のcourt session受理時まで絶対deadlineの設定を先延ばししない。

```text
receipt_deadline = d0 + T_prepare
open_cutoff      = receipt_deadline + T_observe
hard_deadline    = open_cutoff + T_resolve
```

すべての期間は合意上のDAA単位とし、実時間・包含・reorg仮定を別に定義する。receiptが早く届いてもopen_cutoffを早めず、遅いreceiptで各時計を延ばさない。

新しいinteractive pursuitは、`start <= open_cutoff` に加え、準備済み状態から必要な最悪費用 `B_remaining` について `start + B_remaining <= hard_deadline` の場合だけ受理する。モデル保有が前提の参加者とcold取得の参加者を同じ時間計測とみなさない。

通常Finalはopen_cutoff通過後、未解決の適格pursuitがない場合にだけ可能とする。受理済みpursuitはhard_deadlineまでにValid/Refuted/規定default/非不正の未検証終了のいずれかへ進む。期限到達だけで未解決claimを自動Finalにしない。

challengerの放棄は当該sessionの費用・deposit規則で終了し、producerを自動有罪にしない。producerが適格なclaim固有証拠の義務を破った場合は、客観的に成立したDA/default規則を適用する。対象modelの一般的な配布拒否はこのdefaultに含めない。

direct objective proofは、別sessionが開いているだけでは拒否しない。適用されるpre-Final/post-Finalの証拠受理期限を明示し、Final後の無期限rollbackは追加しない。

期限境界の包括/排他、同一blockでの証拠処理とtimeout処理順、canonical orderingを一意にする。例として受理を `d <= deadline`、期限切れsweepを `d > deadline` とする。メモリ上の受信順やnodeの時計は用いない。

資源不足で適格pursuitを完了できないケースは、非不正の未検証終了として範囲を限定して扱い、安全な完了と誤認しない。攻撃者によるその誘発をliveness損失として測定する。枠/責任/資源の保持も有界にする。通常のchain進行・包含仮定が崩れた場合、実時間での期限保証はしない。

## 6. N3 — Bootstrapの具体的設計候補

### RFC07 §VI.3 / RFC11 §17の提案

現在の「既存Active work → Beacon → 新Active」という関係に、自己依存しない根を追加する候補を設計する。

`BootstrapExact`（仮称）は、小さく資源有界な、実際のモデル/kernel計算を検証するsource専用経路とする。全validatorが明示された入力・実行規則から厳密に検証でき、その成立に通常のWork Beacon、当該BeaconによるPanel選出、候補model自身のconformance、将来のPALW Finalを要求しない。検証規則とsettlementの依存関係も閉じて点検する。

これは通常の巨大小モデルclaimを全nodeで再実行する方針ではなく、起動の根だけを独立にする設計候補である。source-only経路はoptimisticなPanel=0 modeではなく、別の明示的exact-validation種別とする。新しいcodec/資源/DoS/移行reviewを要する。

初期案ではsource-onlyの記録から追加PALW報酬・ブロック権・Final weightを発生させない。source試行自体のfee/resource/bond機会上限は別途固定し、source支援が止まるliveness条件も記録する。後から報酬を与える場合はADR176の既存予算内で別途規範化し、bootstrap例外による追加mintを禁止する。

この経路からsource候補を得たことは、乱数としての資格を意味しない。次を別々の判定とする。

```text
IndependentValidityVerified
EntropyEligibilityVerified
```

後者には、targetのcommit後の予測困難性、候補の自由な選別、withholding、private fork、複数bond、隠れた未公表試行、reorg、source ID再包装への耐性が必要である。正しい確定的な計算でも、結果が事前計算可能なら不適格である。小さい計算は安くgrindできる可能性があるため、exact検証が安いことだけでbootstrap採用を決めない。

同一元本のreward capは、未公表の乱数候補を試す回数そのものの上限ではない。sourceのバイアス評価へreward capだけを代入してはならない。

依存グラフは型付きの有向非循環グラフとする。candidate XのsourceにX自身やXの新しいsemanticsに依存するworkを含めない。登録/conformanceだけでなく、sourceのFinal、settlement、Panel assignmentまで含めて循環を検査する。

```text
独立exact source検証
  → entropy資格の別審査
  → qualifying future workの収集
  → Work Beacon
  → 通常のmodel conformance・Panel選出
  → 通常のPanel=1 claim
```

source不足では既定のpending/finite expiryへ進み、固定seed、block hash、heartbeat、BASE-0、署名、委員会seed、local RNGへの隠れたfallbackを追加しない。既存chainの進行と対象PALW機能の停止を分離する。

この案は循環を切る構造案であり、利用可能な安全なentropy sourceの完成証拠ではない。候補kernel、資源値、entropy仮定が未確定なら新しいpermissionless modeの有効化条件は未達である。単体/統合試験用のsynthetic randomnessはproduction不可として型・設定を分離する。

## 7. 受入試験

| ID | 攻撃・操作 | 合格条件 |
| --- | --- | --- |
| B01 | 同額bond・同じrulesetで正直claimと高速偽claimを飽和投入 | 両者の許可されたQ/B/R/F上限が一致し、どの経路でも超過しない |
| B02 | Valid/Final/取消/PanelUnavailable/convictionを最速で反復 | d+W前に消費枠を回復できない |
| B03 | epoch境界/rolling境界/DAA同値で集中提出 | 想定rolling区間で上限を迂回しない。burstは別の定義値以内 |
| B04 | bond分割・鍵交換・元本移転・model間移動 | 同じ元本の履歴が消えず、配分合計が増えない |
| B05 | rho、slice、rider、batch、payout leg、端数を増やす | claim容量以外の総経済creditを増幅しない |
| B06 | 全outstanding claimを同時に反証し、退出やbounty共謀も実行 | 実回収額、二重徴収防止、未払失効、資本不足処理が一致 |
| B07 | receipt支払/EVM/RPC/bridge/Final writer別々に参照 | 同一予約とFinal条件を使い、未検証利益を先取りしない |
| P01 | 唯一のseatが停止、または全候補が無応答 | 有界時間で非不正終了、無報酬、無自動slash、枠は早期回復しない |
| P02 | scope/seed/model/claim違いのreceipt、署名だけのreceipt | 決定的に拒否する |
| P03 | 悪い計算を正直seatが検査 | 承認checkerの検出率を測定し、検出時は期限内にexact/default裁定へ到達 |
| P04 | producerと唯一のseatが共謀、外部検証者が参入 | 必要modelを保有したnon-seatが証拠から反証でき、Panel署名で妨害されない |
| P05 | producerとseatが共謀、外部検査が一切ない | 未検出リスクを明示し、p=1と報告しない。Q/B/R/Fは依然上限内。§3Aの自己資本/公開資本比較でも無条件安全としない |
| P06 | 同じ資本の多数seatと選択的な沈黙・交代 | 最初の選出だけでなく代替を含むcapture確率と利得を評価 |
| P07 | seat数1、seats−1、空mask、quorum/分母/fee残余 | divide-by-zero/未検査scope/誤った支払を生じさせない |
| D01 | decoy challengeを繰り返し放棄 | cutoff、hard deadline、枠の回復時計が変わらない |
| D02 | deadline−1/deadline/deadline+1、同一blockでcloseとsweep | 全nodeで同じ境界判定、十分な残り時間がない新sessionは拒否 |
| D03 | 全interactive枠を占有してdirect proofを送る | 許可資源/包含仮定内で本物のproofを排除しない |
| D04 | hard deadlineで未解決またはproducerが正当な証拠を未開示 | 未解決を自動Finalにせず、非不正と客観defaultを区別 |
| S01 | Active sourceを0件にした起動 | 独立検証経路はBeaconに依存しない。安全条件未達時にfalse passしない |
| S02 | candidate self-source、相互source、settlement経由の循環 | 型/グラフ検査で拒否 |
| S03 | 正しいが予測可能なbootstrap出力 | exact PASSでもentropy資格を付与しない |
| S04 | private candidates、source選別、withholding、再包装 | 明示した攻撃予算内でbias評価。単なる統計乱数testで合格にしない |
| M01 | fenceをまたぐ旧claim、reorg、restart、IBD、pruning | 元のrulesetと予約/時計/Panel/証拠/清算を独立nodeが同じに再構成 |

checkerの試験は、単一不正tile、低rank差分、丸め境界、routing、別model operand、trace借用、時系列不正を含める。検出できるように仕込んだcourt到達試験と、無作為/敵対入力の見逃し確率評価を混同しない。有限回のmutation試験で極小epsilonを実証したことにせず、coverage/soundnessの根拠と測定結果を分離する。

### モデル拘束資本倍率の追加試験

以下の試験仕様を追加する。これらのprotocol/economic試験は本書作成時点では未実施である。

| ID | 攻撃・操作 | 合格条件/必須の報告 |
| --- | --- | --- |
| C01 | 一つの元本を複数claim・Miner/Panel両role・複数modelに表示 | 適格元本の予約合計が実在額以下。同じlotを二度数えない |
| C02 | 別の実資本をMinerとPanelに拘束 | roleが違っても合計は正しく増えるが、Panel元本でproducerの絶対capは増えない |
| C03 | idle登録bond、予定報酬、待機seat、AMM/Positionを追加 | claim用の適格な実予約でない残高は自動算入しない |
| C04 | receipt/Finalを最速・通常・最遅で比較し、challengeも故意に開始 | 正常予約の固定評価期間を延長できず、遅延だけで累積配分利益を増やせない。満たせない集計方式は不採用 |
| C05 | 無応答とPanel交代を反復してロックを積み上げる | 一正規seatの配分帰属を維持し、旧責任保持を重複加点せず、cap/期限も回復しない |
| C06 | epoch境界のみ資本追加、短期借入、移転、Final直後の再登録 | 固定snapshot・成熟・評価期間・元本履歴で同じ期間の二重配分を防ぐ。借入費も経済評価 |
| C07 | 自分のclaimで資本を増やし、そのclaimを高い倍率で支払う | 自己参照や遡及増額を拒否。事前の係数/配分/producer予約を全readerが使用 |
| C08 | S=0、最小元本、倍率閾値前後、最大整数、全model飽和 | 0除算/overflow/不定な丸めなし。係数範囲と元本単調性を維持 |
| C09 | 全modelを一律1.3倍/2倍にして正規化 | 相対配分が不変であることを確認し、一律倍率を公開優位の証拠にしない |
| C10 | 閉鎖自己資本1,000対公開追加資本5,000の仮定例 | 係数/元本/配分/個別capの効果を別々に表示。数値例を実測と表示しない |
| C11 | 閉鎖者が同額総資本を自己調達し、全roleを制御 | 同額には同じ係数。計算省略/Panel省略/借入費/実効pを含め純利益を比較 |
| C12 | 多数鍵、モデル複製/分割、role分割、許可される資本移動 | 同じ総資本を分割/重複して総機会を増やせない。model内配分と端数も検査 |
| C13 | Panel資本だけを極端に増やしproducerが個別capへ到達 | model配分増をproducer絶対cap超過へ変換できない。未使用モデル枠の損益を報告 |
| C14 | 無意味なclaim滞留、無報酬予約、PendingAllocation spam | 実拘束・fee・資源cap・有限終了を検証。仕事をしないスコア獲得が有利ならgate不合格 |
| C15 | 全claim同時反証、没収、元本減額、自己reporterへのbounty | 実元本/将来配分を正しく減らし、徴収責任を二重に数えず、既存権利を恣意的に増額しない |
| C16 | block配分とreward配分を独立行使し、端数・複数legを追加 | network/model/producerの全上限を維持。倍率の二重掛け/二重creditを拒否 |
| C17 | snapshot/Panel交代/Final/没収をまたぐreorg・IBD・restart | 同じ資本台帳、mu、ブロック権、支払、旧新rulesetを独立nodeで再構成 |
| C18 | 公開後に追加資本が来ない、元本集中、全員倍率飽和 | 公開優位が成立しない条件も報告。モデル全体と既存所有者の純利益を区別 |

受入基準は不変条件と経済目標を分ける。二重計上・cap超過・期限延長は決定的に拒否する。一方、公開が閉鎖共謀より利益上有利となる範囲は市場/資本/監視仮定付きで評価し、目標未達を文言で解決したことにはしない。

## 8. 実装順とrelease判定

1. ADR176共通原則、ADR166の枠回復、ADR177の役割境界・モデル拘束資本配分、RFC10のPanel=1 scope、RFC15の保留を文書で統一する。
2. 共通予算台帳と全credit writer/readerを先に実装し、高速claim・終了・資本移動・reorgの不変条件を試験する。
3. 固定期限・direct proof・finite expiryを共通化した上で、1-seatのreceipt/交代/Finalを接続する。
4. Miner＋Panelの実予約台帳、固定評価期間、事前snapshot、1.3〜2倍係数、モデル内配分を実装・試験する。ブロック機会への接続は単なるcoinbase配分と別に検証し、個別絶対capを維持する。
5. 独立bootstrap source検証とentropy適格性を別タスクとして設計・評価する。通常の検証とPanel選出の依存を両方点検する。
6. profile別にcheckerのsoundness、現実の計算/検査/court時間、元本拘束、共謀時の純損益を測定する。公開追加資本と同額自己資本、Final遅延、係数飽和、所有者ごとの利益も比較する。
7. 旧claimのdrainと新claimのfence、codec、fingerprint、署名domain、pruning/undoを固定してから、明示された対象networkだけを協調有効化する。

未決の値: W、Q/B/R/Fの資本関数、短窓burst条件、T_prepare/T_observe/T_resolve、retry回数、Panel報酬の帰属、最大同時責任、checkerのcoverage/error目標、許容capture/bias、bootstrap kernel/source仕様。追加で、適格claim bondのscope、配分評価期間、snapshot/期間平均、資本成熟、1.3〜2倍の曲線/閾値、モデル内leg、B_networkの意味と整数割当、未使用枠、PendingAllocation、公開優位の数値基準を決める。1.3〜2倍はユーザー指定の検討範囲として保持し、具体値・式は承認した脅威モデルと測定で決定する。

## 9. 参照した正本（上記commit）

- `docs/adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md` §1、D1–D6
- `docs/adr/0166-verifier-unavailability-is-not-producer-fraud.md` §§2–3、mission alignment
- `docs/adr/0177-model-bond-allocation-without-availability-consensus.md` D1–D2
- `docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md` §§0、3–9
- `docs/rfc/0014-panel-independent-fraud-prosecution.md` §§7.2–7.4
- `docs/rfc/0003-palw-generative-model-classes.md` §I.4.7
- `docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md` §VI.3
- `docs/rfc/0011-permissionless-model-and-long-context-onboarding.md` §17
- `docs/rfc/0015-panel-free-permissionless-verification.md` §§7–8


## 10. 第2版の追加方針と確認範囲

- 追加要請: 閉鎖したモデルの登録者・Miner・Panelを同じ実質所有者が運用する攻撃に対し、そのモデルのMiner＋Panelの返還不能なclaim bond総額に1.3〜2倍の係数を反映し、ブロック生成機会・報酬の配分で公開参加を相対的に有利にする。
- 前版の前提: 個別Bond/timeのQ/B/R/F絶対上限、枠の非早期回復、Panel=1、有限期限、客観的反証、source bootstrapの未完了状態を維持。
- 今回の追加設計: §3A、C01–C18、改定対応表とrelease順を追加。元のMDにない具体式・評価期間・数値例は提案として明示した。
- 今回実施した確認: MDの構造・参照節・テストIDの重複、上限/配分の区別、数値例の計算を確認。protocolの実装試験、乱数安全性の証明、経済シミュレーション、GitHub原本の再取得・編集は実施していない。
