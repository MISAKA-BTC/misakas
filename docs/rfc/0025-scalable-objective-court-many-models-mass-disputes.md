# RFC-0025 — モデルが増え、不正に対する court が大量発生しても処理を継続できる客観的 court

- **状態:** Draft / Review Required / 未実装・未有効化。2026-10-19、ユーザー指示により RFC-0025 として登録。
- **主目的:** モデル数と不正に対する court 件数が増えても、訴追権・客観的裁定・担保回収の可能性を失わず、有限の混雑を処理して復帰できるプロトコルを定める。
- **通常時の目標:** original claim の `accepted_daa` を起点として `door ≤ 3,200 DAA`(3,120 → 3,200 の変更は明示的な承認対象。§16)。
- **混雑時の方針:** 合意状態から導出した処理予約に基づき、必要な案件の証拠提出期限・Final 保留・証拠保持・bond 返還可能時点を連動して延長する。全件の固定 3,200 は約束しない。
- **前提とする RFC:** [RFC-0024](0024-court-door-3200-to-ctx-2m.md)(A=最終裁定の 1 位置化、B=2 段先読み開示、C=先行取得と強制開示、付録 C=逐次 step 搬送の削減)。本文中の「前版」は RFC-0024 を指す。
- **関連する既存実装:** `consensus/core/src/palw_court_capacity_v1.rs`(休眠。court 専用の block 予算・予約枠・期限順の割当・新規 claim の受付制限・囮の価格・未裁定は無報酬。値は仮)。本 RFC の §5〜§7 はこれを一般化・強化する位置づけで、統合時に対応を照合する(§15 段階 0)。
- **非変更:** ZK・SNARK/STARK・zkVM・PCS による計算の正しさの証明は使わない([PRINCIPLES](../PRINCIPLES.md) §8)。モデル単位の審査は行わない(PRINCIPLES §9)。本文 §15 段階 1 の「モデル profile を審査」は、claim 受付前に class の形状が証拠上限に収まるかを決定論的に判定することであり、モデルの品質・公開状況の審査ではない。
- **付属資料:** [0025-assets/](0025-assets/README.md)(ADR 草案、source-audit-v2、capacity_planner.py と 27 試験。登録時に 27 試験の合格を再確認)。前版の資料は RFC-0024 の [0024-assets/](0024-assets/)。

---

> **決定案:** 個々の証拠と検証作業を小さく有界にする。その上で、有限の処理容量を予約・配分し、混雑時には時間と担保・保持責任を一緒に延ばす。容量限界を超える新しい経済的責任の受付を抑制し、既に受け付けた不正の訴追を優先して処理する。ブロック容量の拡張は、実測・安全性審査を経た別の容量 profile として行う。

## 0. 前版との関係・根拠

本書は前版の A＝最終裁定の1位置化、B＝最大2段の先読み開示、C＝先行取得と強制開示を引き継ぎ、モデル横断の容量管理・混雑延長・pruning/IBD を追加した**統合版**である。実装では本書を上位とし、衝突しない算術・証拠認証の詳細は [前版RFC](0024-court-door-3200-to-ctx-2m.md) を参照する。

前版の「全受理負荷で必ず3,200以下」「期限を途中で延長しない」は、v2 の **通常予約と、事前合意済みの有界な混雑延長**に置き換える。待ち行列の開始を遅らせて、元の受付からの時間を隠すことは禁止したままである。

対象は添付 `misakas-pre(2).zip` の snapshot とユーザー提示の数値であり、最新 branch・出荷 binary・現在の t12 の監査ではない。追加の読解根拠は [source-audit-v2.md](0025-assets/source-audit-v2.md)、前版の根拠は [reference-v1/source-audit.md](0024-assets/source-audit.md) にある。

コード上の重要な確認事項:

| 確認事項 | 根拠 | v2 への影響 |
|---|---|---|
| 既存 frame は `collect_until`、`material_until`、`bond_exit_after` を連動して導出する | V2-S01 | bond返還だけの延長では不十分 |
| public conviction は `[accepted_daa, collect_until_daa)` で受理。現行コメントは期限固定・非延長を要求する | V2-S02 | 新ruleset・明示的移行が必要。排他的境界に注意 |
| custodian の署名済み pledge の保持期間と frame の期限が照合される | V2-S03 | 期限だけ書き換えると署名・契約・料金が不整合になる |
| owner 上限は8で、全ownerがactiveなら追加Openを拒否する | V2-S04–05 | idle回収だけでは共謀者による枠占有を解決していない |
| pruning snapshot は未完の close group のbytesも運ぶ | V2-S06 | rootだけ残して未完courtを削除できない |
| snapshotの既存close上限の試験値は2,250,000 B | V2-S07–09 | 新しい小型証拠上限を既に達成しているとは扱わない |

ユーザー提示の **17,408 B** は新しい分割片サイズの提案値として扱う。snapshot検索では、この値を現行court stepサイズの定数として裏付ける一致は確認していない。モデルのFFN幅の17,408等と混同しない。

以降の「MUST／禁止／必要」は提案する規範要件を表す。未測定の数値・未証明の被覆を採用済み実装の事実として扱わない。

## 1. 保証と非保証

### 1.1 維持する安全性

producer・Panel・custodianが共謀しても、適格な外部検証者1人が、公開・認証可能なmaterialと所定のオンチェーン手続きから訴追できなければならない。

- **計算不正:** 認証済みoperandと既存のexact算術から矛盾を示し、ノードが同じconvictionを導出する。
- **開示義務違反:** 有効な要求と正規の応答機会があったのに資料が開示されなかった場合、ノードが同じDA/defaultを導出する。
- **混雑:** 証拠の配送待ちを理由に、正直な参加者を不当defaultにしない。未解決の責任をFinal・報酬支払い・bond解放で逃がさない。

DA/defaultは、隠れた計算が誤っていたことの証明ではない。両者の責任・罰・返金・判定コードを分ける。

### 1.2 条件付きの生存性

通常時3,200 DAAおよび混雑時の有限完了は、次の明示的な条件の下で保証する。

1. base chainが進行し、適格なcontrol/noticeと証拠に対する有界な収録機会がある。
2. ネットワーク・CPU・IO・reorg・復旧が、承認済みservice envelope内にある。
3. 新規流入はadmissionによって有界化され、既存予約の容量が後続流入に奪われない。
4. 1人の検証者が追う案件について必要な検証者資源と費用が、当該profileに含まれる。

「全員共謀」はproducer等の協力を前提にしないという意味であり、**全ブロック生成主体による永久検閲、無限の通信断、無制限の有効申立てを有限容量で全て即時処理することまで保証しない**。1人が1案件を訴追できることと、その1人が1万案件を並列処理できることも別である。

### 1.3 モデル数と負荷の区別

登録モデル数 `N_models`、未確定original claim数 `N_claims`、独立な係争数 `N_courts`、異議参加者数 `N_filers` を別々に計数する。1万モデルが登録されても、1万courtが常時発生するとは限らない。一方、1モデルへの1万の独立な実行claimが1万courtを生む場合はある。

容量・料金の単位はモデル名ではなく**検証作業と未解決責任**とする。モデルを多数登録しても帯域割当を無料で増やせない。クラス別制限は補助であり、アドレス・class・モデルを分割するSybilへの防御をそれだけに依存しない。

## 2. 固定上限の証拠と局所演算

### 2.1 3種類のサイズを区別する

| 項目 | 定義 | v2 draftの扱い |
|---|---|---|
| `S_step_fragment_max` | 1個のcanonical proof fragment。定義したheader・認証metadataを含む | **17,408 Bを暫定候補**とする。carrier追加分は別計上 |
| `S_close_max` | 最終exact courtに必要な全operand・proof・規則・metadata | **32 KiBを開発目標候補**とする。未達なら有効化不可 |
| `S_transcript_max` | 1つの独立なcourtの、開始からterminalまでの最大オンチェーン総量 | **64 KiBを`SC64`の開発目標候補**とする |

32/64 KiBは今回提案する設計目標であり、導出済み・実測済みの上限ではない。正式な`S_*`は完全なcodec、全fault routeと最悪分岐から導出・審査・pinするまで`UNPINNED`とし、armingを拒否する。`SC32`等は将来候補であり、本書の容量表では感度分析にだけ使う。

`S_transcript`には、notice、request、先読みfrontier、部分値、source opening、原本強制開示、permit/WorkDebit、choice、close、terminal、署名、carrier framingを含む。裁定成立に必須の準備データがチェーン上に載るなら、名前を「preparation」と変えて除外しない。全非協力・キャッシュなしの強制経路を算入する。

共有物は**各courtを単独で成立させるときの上限**には全量を算入する。並列court間のdedupは実績として別会計する。ネットワーク再送・orphan/reorg再提出は`S_wire_total`と別の回復予算に計上し、費用や時間から消さない。

分割片が17,408 B以下でも総量が448 KiBなら、容量計算には448 KiBを使う。**fragmentへの分割は運搬を可能にするが、総証拠量の削減ではない。**

### 2.2 固定上限が成立する範囲

受理するprofileの集合 `P`について以下を要求する。

```text
すべての p∈P、許容M,C、対象fault routeについて:
  S_close(M,C,p)       ≤ S_close_max
  S_transcript(M,C,p)  ≤ S_transcript_max
  W_verify(M,C,p)      ≤ W_verify_max
  IO_verify(M,C,p)     ≤ IO_verify_max
  RAM_peak(M,C,p)      ≤ RAM_peak_max
  rounds(M,C,p)        ≤ R_max
```

初期context対象の上限候補は `2,097,152 tokens`。モデルの層数、tensor形状、head幅、program長、trace leaf数、address幅、Merkle深さ、operand数も有限の上限を持つ。これらから分岐数・proof長・parser作業の上限を導出する。

これは**許容範囲内で共通の上限を持つ**という意味であり、無制限のモデル・contextに対してMerkle proofまで真に定数長になるという主張ではない。profile追加はグローバル上限を自動で引き上げない。

### 2.3 Aの一般化: 1位置でも大きければさらに分割する

attentionのhistoryを1位置に絞っても、head幅・reduction出力幅・複数headのK/V・query・原本leafが大きければ上限を超える。この場合、演算を固定幅の積和、reduction部分値、state更新等の**最小検証単位**にloweringする。

canonicalなoperandアドレス・型・値・program fragment・前後state・original rootへのpathを認証する。sourceの部分領域だけを開くには、元のwriter leafか明示的なsubcommitmentが必要である。巨大leafの一部のbytesだけを全leafのhashに対するproofと称してはならない。

分割で元の算術、丸め順、overflow、mask、正規化、first divergenceが変わらないことを証明する。整数Sum/Maxのexact foldを使える場合と、一般の浮動小数点reductionを区別する。FPを勝手に再結合しない。softmaxを1位置だけで正規化し直さない。

多段化すると総round数・総proof量が増えるため、`S_close`だけでなく`S_transcript`も再導出する。上限超過profileは、**claim受付前**に未対応として拒否する。受理後に「モデルが大きく証拠が入らない」ことを検証者の過失にしない。

### 2.4 全fault routeの被覆

attentionだけでなく、GDN/recurrent state、FFN・MoE、logits、token selection、tokenizer/入力解釈、出力同一性、HistTileコピー、memory/address、artifact→operand認証を対象にする。重い旧fallbackが残る場合、その最悪値をprofileに含める。

全ノードが読む必要があるmodel/program materialを小さい証拠に置換できるかは別の実装義務である。冷状態の準備・モデル取得・外部検証者の再実行費用がモデルサイズに依存することはあり得る。これをcourt最終証拠の定数上限と混同しない。

## 3. A・B・Cを統合した経路

```text
モデルprofileを審査・rootに固定
        ↓
original claim受付: 容量・保持・最大延長費用・訴追機会を予約
        ↓
C: 原本と取得indexを生成時から保存し、任意に先行取得
        ↓
有効noticeを小さい保護control経路で収録
        ↓
合意状態にcourtを登録し、Final/bond/保持を保護
        ↓
B: 最大2段の全候補を先読み開示 → 検証者がまとめて選択
        ↓
A: 1位置、必要なら固定幅primitiveへ局所化
        ↓
認証済み最小witnessをノードがexact検証
        ↓
conviction / DA-default / 正規の棄却
        ↓
決済・回復安全性を確認して責任を解放 → 安全な証拠pruning
```

Bは、各段で次に選ばれ得る**全候補**を含む。16分岐2段なら最大256のfrontier候補になる。部分値・認証path・CPUを含めて予算内の場合だけ使用する。大きいprofileではdepth=1等を開始前に固定し、相手の協力を前提にした省略をしない。

tile→positionの段は最後のB bundleに含め、独立した追加往復を避けてよい。ただしlogicalな被覆を省略しない。

Cのオフチェーン経路は通常時の高速化であり、最悪上限はオンチェーン強制開示または客観的defaultに到達する経路で算定する。hashだけの事前公開をデータ可用性保証と呼ばない。

## 4. 時間の定義: 通常3,200、混雑時は予約に応じて延長

### 4.1 変数

| 変数 | 意味 |
|---|---|
| `a_i` | original claimのaccepted DAA。後から変えない |
| `n_i` | 有効なnoticeが受理されたDAA |
| `s_i` | 重いcourt処理の予約開始。受付の起点とは別 |
| `F_i` | 全合法進行・最遅応答・default・必要回復を含む、terminalの最終収録可能位置 |
| `d0_i` | 通常の排他的期限 `a_i + 3,200` |
| `E_i` | 合意状態から導出する混雑延長。後記の上限内 |
| `d_i` | 有効な排他的`collect_until` |

```text
d_i = max(a_i + 3,200, F_i + 1)
E_i = d_i - (a_i + 3,200)
D_frame_i = d_i - a_i = 3,200 + E_i
```

`F_i`はcapacity schedulerとphase別の有界テンプレートから導出する。`N×bytes/rate`だけで求めない。単発のbound自体が3,200を超えるprofileを「混雑した」として隠さない。そのprofileは通常時未達である。

`D_active`、`D_queue`、`D_frame`、`D_notice_to_terminal`、`D_bond_lock`を別々に報告する。実処理だけ3,200以下でも、`a_i`から1万DAA待った案件を「door3,200達成」と表示しない。

### 4.2 受付窓とterminal余裕

`notice_until`と`collect_until`を分離する。有効noticeの受付窓の後にも、反証作成・応答・最終収録に必要な時間を予約する。`notice_until`は最小検査時間と所定収録機会を奪わない形でprofileから導出する。

有効noticeの到着がその許容窓の最終位置だった場合も、`F_i`を守れることを要求する。準備や異議検出に必要な仕事を開始前へ移して計上から消さない。modelを事前取得済みとするprofileはその前提を明記し、完全cold profileとは区別する。

`collect_until`は排他的境界なので、terminalは`now < d_i`で収録する。境界ぴったりに収録してよいとするoff-by-oneを禁止する。[V2-S02](0025-assets/source-audit-v2.md#v2-s02)

### 4.3 通常・混雑・過負荷

- **Normal:** 予約済みの全phase・冷状態・保証対象負荷が`a_i+3,200`に収まる。`E_i=0`。
- **Congested:** 審査済みの有限burstにより将来の処理枠を予約する。`0<E_i≤E_max`。その案件の期限と責任を連動延長する。
- **Overload admission:** 新しい経済的claimを受けると`E_max`、保持予算、active state上限、control容量を超えるなら、その新規受付を抑制する。既存の有効courtを捨てない。
- **Service failure:** 合意されたservice envelope自体が崩れた場合は別扱い。通常の混雑達成として報告せず、安全なFinal保留と復旧手続きへ移る。無制限障害時の有限返還を約束しない。

`E_max`は一律10,000という暫定値で埋めず、処理容量、burst契約、保持・担保費用、復旧条件から決める。10,000件のburstを対象にするなら、その最終案件まで収容できる値かを確認する。

### 4.4 1 originalに複数courtがある場合

各courtは個別の`d_i`と予約を持つ。original frameの実効`collect_until`と経済的保留は、そのframeを参照する有効な未解決courtと元の責任窓から必要な最大値を導出する。

```text
frame_collect_until = max(base_frame_collect_until, applicable_court_deadlines)
```

frameの保留が伸びても、既に走っている別courtの個別deadlineや`notice_until`を一緒に延ばしてはならない。全ての判定入口はv2の個別期限・frame保護を正しく参照し、v1の単一termsだけを読むconsumerを残さない。

## 5. モデル横断の容量予約とスケジューリング

### 5.1 service profileと資源ベクトル

`CourtCapacityProfileV2`は、次をrulesetに固定する新しい提案型である。

```text
profile_id, ruleset_id, activation_rule
court_wire_rate_lower_bound      [B/DAA]
control_wire_rate_lower_bound    [B/DAA]
verify_work_rate_lower_bound     [work/DAA]
disk_service_rate_lower_bound    [B/DAA]
max_service_gap_daa
max_court_mass_per_block / max_control_mass_per_block
max_active_state_bytes / max_inflight_witness_bytes
max_notice_bytes / max_notices_per_window
max_nonpreemptible_work / max_parser_work
max_reorg_depth / replay_budget / inclusion_margin
E_max / escrow_price_id / resource_template_root
```

各courtは`(wire bytes, consensus work, verifier work, IO, live state, retained byte-DAA, control数)`の資源ベクトルを持つ。bytesだけで軽重を決めない。CPUを大量消費する小さいwitnessも容量を予約し、無効codecの拒否処理にも上限を設ける。

実測値の平均を合意上の下限にしない。実測は、指定機材・ネットワーク・負荷に対するservice envelopeの根拠として使用する。各ノードが自分の回線速度を測り、独自に`E_i`を計算する実装は禁止する。

### 5.2 original claim受付で責任容量も確保する

新しいoriginal claimを報酬対象として受理する前に、そのprofileの検査窓・準備・保護notice・許容する敵対的分岐・最悪court・保持と回復を予約する。全claimを先に無制限受理し、後からcourtだけを容量不足で拒否する構造は禁止する。

異議が生じないときの未使用容量は他のbest-effort処理に貸してよい。ただし、予約を行使された時点で取り戻せることと、既に開始した非preemptible処理の最大遅延を含めて保証する。

複数claim間で同時不正が起きないという仮定は使わない。指定burstの全claimが同時にcourtを必要とする場合をadmissionで評価する。確率的overbookingを採用するなら、それを全員共謀時のworst-case保証として表示しない。

実装の強化前に、既存owner上限等によってこの予約モデルを満たせないprofileを無理に有効化しない。

### 5.3 予約カレンダー

各courtのphaseテンプレートは、依存順序、最大bytes/work、相手の合法応答窓、所定収録余裕、terminal/default、回復を含む。frame受付時の最悪notice時刻と、実際のnotice受理後の残余予約を整合させる。

accepted chainの決定的な順序から`reservation_seq`を割り当て、空き処理枠へ予約する。local mempool順、local clock、見えたpeer数、operatorの判断は用いない。予約は合意stateに記録し、再起動・reorg・IBDで再生可能にする。

**先行案件の予約を、後から到着した通常の申立てで後ろへ動かしてはならない。** 早期完了分で後続処理を前倒ししてよいが、参加者への応答義務・deadlineを不意に短縮しない。

資源`r`と任意のカレンダー区間`[u,v)`について、予約需要が、その区間で供給可能な下限と既存予約を超えないようにする。

```text
Σ demand_of_reserved_phases(r,[u,v)) ≤ guaranteed_supply(r,[u,v))
```

これはphaseのrelease時刻、相手の応答依存、最大service gap、fragmentの非preemptible時間、control枠を考慮したチェックでなければならない。期間全体での総bytes一致だけでは十分でない。

初期実装は、固定比率の複数laneに完全なphaseテンプレートを予約する保守的な方式でよい。将来、EDF/DRRやphase単位詰込みで効率を上げても、各資源の供給保証と既存予約を維持する。アルゴリズム名だけでdeadline保証を宣言しない。

### 5.4 controlと証拠本体を分離する

notice、応答要求、extension record、terminal/default、bond/finality更新には保護control容量を与える。巨大witnessがブロックを埋めて、訴追開始や既存処理の完了まで止めることを避ける。

上限のあるnoticeには、original claim、scope、争点へのcommitment、profile、料金/担保、再送識別子を含める。notice受理は有罪認定でも全証拠検証でもないが、所定の継続義務を開始し、Finalと責任を保護する。

「control枠がある」だけで1人の外部検証者の収録を保証したとは扱わない。control帯域への敵対的流入、料金、枠の占有、正規noticeの最大収録遅延も明示的に評価する。

### 5.5 容量枠と実際の収録保証は異なる

ブロックの上限やcourt用空き容量を定義することは、特定の未収録transactionが必ず入ることの証明ではない。ローカルmempoolにしか存在しないtransactionを「含めなかった」としてブロックをinvalidにしてはならない。

本書の保証には、acceptedな義務・予約に対応するpublic carrierへの有界な収録規則または明示的なbase-chain inclusion前提が必要である。consensusで強制できる容量隔離と、proposerの収録行動・ネットワーク配送の前提を分けて記録する。

応答時計は**予定された正規の処理機会**に基づいて進める。有効bytesが届いたときだけ時計を進める方式は禁止する。相手が永久に沈黙すると時計も永久停止するためである。

逆に、合意上必要な収録機会が与えられなかったことを、相手の資料不提供と同一視しない。service違反の扱いと参加者defaultを区別する。

## 6. 混雑時の延長・bond・経済的Final

### 6.1 有界なextension contract

v2のoriginal acceptanceに、`base_collect_until`、`extension_policy_id`、`E_max`、最悪保持期限、追加費用の予約を含める。custodianと責任主体は、この最大範囲を含むtermsに署名する。

```text
CourtExtensionV2 {
  frame_id, prior_effective_terms_hash,
  scheduler_profile_id, reservation_seq, reservation_root,
  old_collect_until, new_collect_until,
  new_material_until, new_bond_exit_after,
  new_final_floor,
  extra_resource_charge, escrow_debit,
  reason_code
}
```

新しい名称は提案であり、既存wire tagは割り当てない。extensionはノードが予約状態と承認済み規則から検証する。operator・custodianの申告だけで期限を延ばさない。

元の署名対象frameを破壊的に書き換えず、versionedなeffective termsとextension chainを導出する。旧hashに依存するpermit、lease、allocation、source binding、料金照合をすべて更新対象にする。

### 6.2 必ず同時更新するもの

1つの原子的状態遷移で、影響するcourt/session/原本義務の期限、`collect_until`、Final floor、credit/receipt/escrow保留、`material_until`、custody pledgeの実効責任、bond exit hold、保持料・予約量を更新する。

**bond返還だけを延期し、古い`collect_until`で正しいconvictionを拒否する実装は禁止する。** 逆に、courtだけ延ばし担保が先に抜けることも禁止する。

bondの関係は、例えば次のような上界関係を満たすように導出する。

```text
material_until_i ≥ max(collect_until_i,
                       terminal_settlement_safe_i,
                       latest_reference_or_recovery_need_i)

bond_exit_after_i ≥ max(material_until_i,
                        settlement_and_liability_end_i)
```

単発のdoorが3,200でも、reorg・settlement・責任保持の余裕によりbondの返還可能時点は3,200より後になり得る。実際の返還transactionの収録はさらに別である。

### 6.3 後続攻撃による無期限延長を防ぐ

通常の混雑延長は、original受付時と所定のnotice予約時に合意stateから決める。いったん成立した処理予約を、後続の別claim、大量の重複notice、無効応答、partial fragment、再要求で繰り返し延期しない。

同一争点の重複は既存の予約を参照し、時計をリセットしない。新しい独立な争点は別予算を消費するが、他の既存予約を奪わない。`notice_until`自体を新しいnoticeのたびに後ろへ延ばさない。

reorg等による変更は事前合意済み回復予算に限る。予算を超えるservice障害は通常延長の成功として扱わず、事故記録・新規受付停止・既存責任の安全な保護に移る。`E_max`に達したからという理由で、未解決不正を無罪・Finalにすることは禁止する。

### 6.4 延長費用と資本拘束

延長は無料ではない。保存byte-DAA、追加I/O、再取得、witness carriage、追加検証、bondの機会費用を別々に見積もる。簡単な保存料項の形は次のとおり。

```text
追加保持料 = Σ retained_bytes(object) × 延長DAA(object) × price_per_byte_DAA
```

通常延長はoriginal frameとchallengerの事前予約資金から賄う。最終的な費用負担は、確定した不正・DA/default・無効な濫用・正当な棄却に応じた承認済み規則で配分する。正当な検証者に、全共謀者の無制限な作業料を負担させない。

他人のbondを安価なspamで長期間拘束するgriefingを評価する。返金は二重払いを許さず、同じbondの複数責任には露出額合計の上限を適用する。混雑と無関係な解決済みclaimの担保を、ネットワーク全体の混雑だけで一律に拘束しない。

## 7. 外部1人の訴追権と大量濫用

### 7.1 owner枠占有を安全性の穴にしない

既存のK_owner=8とidle回収は、全員がactiveに動く共謀を排除できない。[V2-S04](0025-assets/source-audit-v2.md#v2-s04) v2では、枠を持つownerだけが反証できるという構造から分離した、permissionlessな反証提出・共用開示への接続が必要である。

共通の認証済みmaterialは誰でも再利用できる。同じclaimの不正を直接示す有効witnessを、既存ownerの署名・承諾・辞退に依存させない。局所化が必要な検証者にも、control受付と処理予約を含む経路を残す。

**「外部検証者用1枠」と名付けるだけでは不十分。** 共謀者も別アドレスで取得できる。identity別の優先枠、料金、単なるFIFOだけでこの問題が解決したとは扱わない。

### 7.2 boundedな対話とdedup

同じoriginal claim、同じroot、同じ争点・範囲・rulesetの再提出は、canonical keyでまとめる。異なる証拠や独立した不正を「同じモデルだから」とまとめて捨てない。counterclaim、branch、最後の反証を含む敵対的増幅をprofileに計上する。

有効/無効の判定に仕事が必要な以上、「正当なものだけ無制限に無料受付」は採用しない。一方、capacity capを共謀者が先に埋めれば正しい外部1人を排除できる、という設計も採用しない。

admissionの保証範囲は、**正規noticeの収録機会が与えられ、既存責任について予約されたboundedな対話経路**である。その経路に対して全ての敵対的分岐・枠占有を展開しても反証できることを、形式化または反例探索と実ノード試験で示す。未証明なら`OneOutsiderCapacityIncomplete`としてarmingを拒否する。

### 7.3 停止すべき入口

容量が不足するときは、まず新しい報酬対象original claim、optional work、新しい保持義務を抑制する。既存の有効noticeのdeadline・権利を消して帳尻を合わせない。

新規モデル登録が共通資源を増やすなら、登録CPU・active metadata・保持負担にも上限と料金を設ける。モデル数を増やしてcontrol枠や検証枠を無料で取り直せない。

### 7.4 持続負荷の限界

resource `r`について長期平均の受理仕事率を`λ_r`、保証サービス率を`μ_r`とする。定常的な有限queueには、余裕を含めて`λ_r < μ_r`が必要である。到着が処理を超え続けるなら、期限延長だけではqueueと担保拘束が増え続ける。

その場合は、原本claimの受付制限、work/rent価格の見直し、承認済み容量増強を行う。無限の攻撃を有限期限で処理できるとは表示しない。有限burstの処理完了と、持続attack中の有界な受理・既存案件の完了を別の受入条件にする。

## 8. 容量試算と3,200 DAAの達成判定

### 8.1 総量から得られる必要条件

同時に全量がreadyな同一サイズのcourtが`N`件、1件のcharged transcriptが`S` B、court用実効帯域が`B_c` B/DAAなら、少なくとも次の転送容量が必要になる。

```text
D_wire_lower ≥ ceil(N × S / B_c)
N × S ≤ B_c × D_wire_budget
```

これは**理想化した容量の下限**であり、実際のcourt完了期限ではない。応答の直列依存、CPU/IO、control、reorg、収録gapはさらに確認する。wire以外の時間を別に予約するなら、3,200全体ではなく残りの`D_wire_budget`を用いる。

125,000 B/DAAはユーザー提示の感度分析用の値、court割当20%は試算仮定であり、出荷ノードの測定値・現行ブロック容量・合意上保証値とはしない。

### 8.2 20%配分の感度分析

`B_c=125,000×0.20=25,000 B/DAA`。括弧内は元の総帯域が1/10、`B_c=2,500 B/DAA`の場合。Bはdecimal、KiBは1,024 B。

| court総量/件 | 100件の転送下限 DAA | 1,000件 | 10,000件 |
|---:|---:|---:|---:|
| 32 KiB | 132（1,311） | 1,311（13,108） | 13,108（131,072） |
| 64 KiB | 263（2,622） | 2,622（26,215） | 26,215（262,144） |
| 448 KiB | 1,836（18,351） | 18,351（183,501） | 183,501（1,835,008） |

448 KiB×1,000件は458,752,000 Bで、25,000 B/DAAなら18,351 DAA以上となる。1件の総量を64 KiBにできても、1万件では26,215 DAA以上であり、全てを3,200に入れることはこの仮定下では容量だけで不可能である。だからこそ混雑延長とadmissionが必要になる。

32/64/448 KiBは比較用サイズであり、実モデルのcourt総量を測定した値ではない。前版の約12 KBは1位置の見込みであり、総transcriptとして表に流用しない。

### 8.3 1万件を3,200に入れる帯域の最低条件

他の全コストを仮にゼロとした場合の必要court帯域は次のとおり。

| 総証拠/件 | 必要court帯域 B/DAA | court20%なら必要な全体帯域 B/DAA |
|---:|---:|---:|
| 32 KiB | 102,400 | 512,000 |
| 64 KiB | 204,800 | 1,024,000 |
| 448 KiB | 1,433,600 | 7,168,000 |

実際には待機・計算・復旧余裕が必要なので、これより大きい容量が必要になり得る。wireだけ改善してもCPUやstate root更新がボトルネックなら、doorは比例して縮まらない。

### 8.4 正式な判定

通常時3,200のgateは、各profileの全fault route、最後だけ資料不提供、合法的な最遅応答、指定同時負荷、control競合、回復まで含めた`F_i+1−a_i≤3,200`とする。

混雑時のgateは`F_i<d_i`、`E_i≤E_max`、正直な参加者の不当defaultゼロ、担保と保持の整合、有限burst後のbacklog排出である。混雑時に通常3,200を超えた割合と、**予約期限に違反した割合**を別々に報告する。

初期の旧感度分析3,012/3,196 DAA等との比較は、最新ADRの正式なdoor式と対応付けるまで参考にとどめる。旧doorに`N×S/B`を無条件に足すと、既に含む通信・待機を二重計上し得る。

## 9. ブロック容量拡張の検討

### 9.1 同じブロックサイズでもできる改善

最初にA/B/C、canonical encoding、pathのdedup、共有material、court/controlの予約枠、boundedな並列検証、state更新のバッチ化を評価する。これらは通常transaction容量との配分変更やCPU使用増を伴うことがあるので、無料の改善として扱わない。

court比率を20%から40%にすれば、他条件一定のwire容量は2倍だが、通常transactionの利用余地を減らす。容量配分を変えただけでネットワーク全体の物理帯域が増えたとは扱わない。

### 9.2 上限を増やす場合

同じ合意・ブロック生成条件の下で、総byte上限、court carriage上限、control上限、検証work/mass、最大body伝搬時間、state書込量の組を`CapacityProfile`として審査する。単一の`block_size`だけを変更しない。

比較候補を`1× / 2× / 4×`としてよいが、これは試験候補であり自動採用ではない。単一巨大tx/closeの上限と、ブロック全体の上限も分ける。

ブロックbytesからB/DAAへ換算するときは、accepted DAG、DAA score、実際の収録機会、重複/孤立blockを含む関係を明示する。1 DAA＝1秒、1 DAA＝1物理blockと勝手に置かない。

### 9.3 採用条件

候補を、同じreference node/帯域制限で次の観点から比較する。

- propagation、検証wall time、p95/p99と観測最大、導出最大service gap、CPU、RSS、disk/DB write amplification。
- 通常transactionの進行、10,000件burst、巨大/無効witness、同一モデル/異種モデルによる競合。
- orphan/reorg挙動、復旧時間、checkpoint作成時の遅延、snapshotサイズ、pruned IBD、catch-up速度。
- 低い性能の承認対象ノードが脱落しないこと。並列化しても全ノードが同じ結果を導出すること。

最終的に増やすのは、**出荷可能と審査された実効service envelope**である。ブロック上限を2倍にしただけで保証帯域を2倍にしてはいけない。

### 9.4 動的増大は初期v2に入れない

混雑件数やローカル帯域を見てブロックサイズを無制限に自動拡大する機能は導入しない。攻撃者がノード負荷を引き上げる経路になり得るためである。初期版は明示的なruleset/fenceで容量profileを変更する。

将来の弾力的容量は、合意から導出できる指標、hard ceiling、変更率制限、hysteresis、料金、ノード要件、伝搬安全性を別RFCで定める。

## 10. 10,000 DAA checkpointと安全なpruning

### 10.1 4つの時計を分ける

```text
checkpoint_interval       = 10,000 DAA（作成間隔の提案）
pruning_point             = base consensusが安全と認める位置
court/material retention = 各objectの未解決責任と必要期間
bond exit                 = 判定・保持・決済の必要期間後
```

checkpoint間隔10,000は、直近10,000だけ残して全削除する規則でも、全courtの証拠保持期限10,000でもない。10,000件混雑によってcourtが26,215 DAA以上かかる場合、必要証拠は複数checkpointをまたいで保持する。

### 10.2 削除可能性の条件

objectごとの`retain_until`を、全参照元の要求から導出する。

```text
retain_until(object) = max(
  unresolved_court_need_end,
  material_lease_end,
  settlement_or_liability_need_end,
  reorg_and_restart_need_end,
  validated_snapshot_recovery_need_end
)
```

各項は同じ絶対DAA時刻で表す。相対期間を異なる起点のままmaxで混ぜない。notice→court→settlement→recoveryと直列に必要な期間は、まず足して絶対期限に変換する。

削除は、合意のsafe pruning pointより古く、未解決参照がなく、保持義務と再構築上の必要性が終わり、対応stateが検証可能なcheckpointに反映された場合だけ許可する。pruningされたデータを新しい有効courtが暗黙に読み直す構造を禁止する。

### 10.3 データ階層

**Active state:** reservation、期限、bond exposure、未解決session、必要な部分witness、extension chainの再構築に必要な状態。全検証ノードが同じ意味で復元できる必要がある。

**Retained evidence:** 未解決または回復上必要な認証済みmaterial。ノード・custodianの責任区分を明示し、rootだけをもってbytesの代替にしない。

**Resolved history:** finality・reorg・責任期間を過ぎた詳細証拠。検証可能な結果commitmentと残存状態を保ち、履歴はpruneしてよい。archival nodeは監査・再取得の任意提供者であり、現に有効なcourt成立を善意archiveにだけ依存させない。

全ノードが全モデル・全原本を永久保持する義務は設けない。ただし現在のconsensusが必要とするbytesを省けることは、具体的なstate/commitment/availability設計で示す。

### 10.4 保存量の上限

queueが伸びればlive stateとretained bytesも増える。延期はstorage問題を消さない。`max_active_state_bytes`、`max_pending_notice_bytes`、`max_inflight_witness_bytes`、`max_retained_byte_DAA`をadmissionのhard resourceに含める。

最終witnessが小さくても、原本モデル・履歴の保持は大きい場合がある。モデル共用による削減は参照countとleaseを正しく持ち、最後の未解決参照を消すまでpruneしない。

## 11. 検証可能なcheckpointとpruned IBD

### 11.1 checkpointに必要な状態

既存base-chain pruning proof/UTXO/state検証に加えて、court関連のprimary stateを含める。

```text
ruleset / active capacity profile / resource template root
class/profile registryと必要な認証済み参照
未確定claim、court/session、notice、next sequence
処理予約カレンダー、残余予算、queue、phase cursor
base terms / effective terms / extension policy / 延長記録
bond exposure、exit hold、Final/credit/escrow hold
custody obligation、lease、retention reference
未完fragmentの必要bytesまたは合意上十分な認証可能表現
既に処理済みのkey/重複防止状態、settlement結果
```

derivableなindexは再構築し、primary stateと食い違えばsnapshotを拒否する。partial close、予約、extensionを落とすとarchival nodeと異なる判定になるため禁止する。既存snapshotが未完close groupのbytesを保持する点を維持する。[V2-S06](0025-assets/source-audit-v2.md#v2-s06)

### 11.2 rootの一致だけでは状態遷移の正しさの証明にならない

snapshot bytesが指定rootに一致することと、そのrootが正しいchain状態を表すことは別である。新ノードは、base consensusの安全性前提の下でcheckpointを認証する既存のpruning/chain-selection/状態検証経路を通す。

管理者署名だけ、rootを載せただけ、checksumが一致しただけのsnapshotを「trustless IBD」と称さない。検証済みcheckpointからcatch-upして、同一tipのarchival nodeとstate root・court verdict・bond残高・deadlineが一致することを要求する。

このRFCは新しいSNARK/STARK等のvalidity proofを導入しない。全履歴再実行なしで何をどの前提で検証できるかは、既存pruning proofの信頼・攻撃モデルと合わせて明記する。

### 11.3 測定項目

IBDを、proof/header、snapshot、active court payload、必要material取得、catch-up replayに分解する。100、1,000、10,000件の未解決courtを持つcheckpointでbytes・時間・CPU・RSS・diskを測る。

pruningにより古い履歴を減らせても、現在のmodel registry・UTXO・live courtが増えればIBD状態量は増える。全履歴依存を減らすことと、モデル数・未解決件数に対してIBDが定数になることを混同しない。

10,000 DAA境界の作成中にブロック処理を長時間止めない。atomic snapshot、整合性確認、失敗後再開、増分生成の方式を実装し、そのコストを通常service envelopeに計上する。

## 12. ノード実装のスケーリング条件

1万courtをon-chainで有界化しても、実装が毎ブロック全courtの全witnessをコピー・再hashするならCPU/RAM/IOが詰まり得る。byte容量の改善とノード処理の改善をセットで評価する。

### 12.1 必須の実装制約

- message長・proof深さ・operand数・work上限をallocationや重いhashの前に検査する。
- witness組立はbounded bufferと認証済みfragment単位で行い、未完案件数×巨大連続bufferをRAMに常駐させない。
- schedulerは期限/phaseのindexを持ち、全model×全court×全DAAの無制限scanを作らない。1ブロックの更新件数・CPUをmassに計上する。
- court間の検証を並列化してよいが、state適用順と競合解決は合意順序に固定する。
- timeout、default、bond release、checkpoint境界に1万件が集中する場合も、処理上限と決定的な分割・保留規則を持つ。単に一括処理を次へ送って既存deadlineの意味を変えない。
- RAM上限だけでなく、database write amplification、journal、reorgでのundo/redo、snapshot exportのピークを計測する。

### 12.2 state commitmentの効率化

同じ意味のstate rootを保てるindex/cachingの最適化と、root形式そのものを変えるincremental Merkle化を区別する。既存snapshot digestはcanonical carriage全体をserializeしてhashする。[V2-S11](0025-assets/source-audit-v2.md#v2-s11) これがどの頻度で呼ばれるかをprofileし、毎ブロックのボトルネックと未測定のまま断定しない。

root形式を変更する場合は、新version・domain separation・reorg・pruned IBDの同値性と移行規則を別途定める。hashを省いても「同じ保証」と称してはならない。

## 13. 状態・codec・再生の契約

提案するprimary stateは次のとおり。具体的なwire番号と既存型とのmappingは統合時に割り当てる。

```text
ScalableCourtStateV2 {
  capacity_profile_id,
  reservation_calendar,
  next_reservation_sequence,
  original_frame_reservations,
  pending_notices,
  active_courts,
  effective_terms,
  extension_records,
  exposure_and_holds,
  material_references,
  bounded_resource_counters
}
```

全recordをnetwork/ruleset、original claim、class/artifact、execution、court/scope、sequenceでdomain-separateする。整数はchecked arithmetic、切上げ、canonical orderを使う。UNKNOWNな帯域・仕事量・保持料はゼロ扱いせずadmissionを拒否する。

extension処理は原子的に行い、失敗時は期限・料金・担保を全てrollbackする。checkpoint取込時にはreservation、effective terms、retention、bond exposureの整合性を再構築して検査する。

reorgで取り消されたnoticeやextensionをローカルcacheだけで生かさない。accepted chainに基づいて再適用し、古い枝のbond解放・slash・二重返金を禁止する。snapshotのchecksum一致だけでこれらの状態遷移が正しいとみなさない。

## 14. 受入条件と試験計画

### 14.1 規範的な受入条件

| ID | 必須条件 |
|---|---|
| C1 | 全対応モデル/形状/context/fault routeに共通の`S_close/S_transcript/W_verify/IO/RAM/rounds`上限を導出し、悪意ある最大codecを含めて検査する |
| C2 | 単発および宣言した通常予約負荷で、ctx2Mを含め`D_frame≤3,200`。queue/準備を起点から隠さない |
| C3 | 100/1,000/10,000件の有限burstを予約できる範囲で受理し、全件が自分の予約期限内に完了。通常超過は`E_i`として公開 |
| C4 | 新規流入とモデル分割によって、先行予約がstarvation/無期限延長にならない。過負荷時に新規責任の入口を抑制する |
| C5 | 全custodian/producer/Panel非協力、全owner枠占有、適格外部1人で、exact convictionまたは適格DA/defaultに進める。初期の無応答だけでなく最後だけ隠す経路も含む |
| C6 | 期限・Final・bond・保持料・署名済み最大責任が一致。3,200および排他的境界を厳密に検査する |
| C7 | 10,000 DAA間隔のcheckpointをまたぐ長期混雑でも、未解決証拠をpruneしない。責任終了後は安全に削除できる |
| C8 | genesisから全bodyを取得せず検証可能なcheckpointからIBDし、archival nodeと同一のstate/verdict/bond/deadlineへ到達する。信頼前提を記録する |
| C9 | court容量比率変更・block容量1×/2×/4×候補で伝搬・CPU・RAM・IO・reorg・IBDを測り、採用するprofileを審査する |
| C10 | source/binary/codec/profile/service/料金/全実測/evidenceをpinし、未被覆・UNKNOWN・未承認recordでfenceを武装できない |

### 14.2 負荷matrix

件数だけではなく、次の軸を組み合わせる。

```text
モデル数:     1 / 10 / 100 / 審査対象の最大registry
context:      短文 / 32K / 2M
court件数:    1 / 100 / 1,000 / 10,000
流入形態:     一斉burst / 時刻分散 / capacity以下の継続 / capacity超過の継続
争点:         同じ実行の重複 / 独立実行 / 独立モデル / 全fault-route混合
帯域:         profile下限 / 感度分析125,000 / 12,500 B/DAA
状態:         warm / cold-state / 完全cold（要件を区別）
相手:         協力 / 最遅合法応答 / 最後だけ不提供 / 全不提供 / 無効応答spam
ノード:       reference最小機材 / 高性能 / pruned IBD直後 / 再起動直後
```

### 14.3 個別の攻撃・境界試験

| 試験 | 期待結果 |
|---|---|
| サイズ超過、偽proof深さ、巨大operand、無効codec | 無制限allocation/hashなしで拒否。誤slashしない |
| 小さいpayloadだが高CPUのwitness | CPU予約/上限が働く。wire枠だけで通さない |
| 256候補bundleの1候補欠落、fork間で矛盾した部分値 | 全候補整合性で拒否。未開示分の後続待機を隠さない |
| 1万件到着後にさらに1万件、モデル/アドレスを分割 | 先の予約は不変。後続は別予約またはoriginal admission抑制 |
| 同じ争点を大量複製 | bounded dedup。originalの期限をリセットしない |
| owner全枠を共謀者がactive維持 | outsiderの反証経路を排除しない。失敗ならarming不可 |
| 応答側が完全沈黙 | 処理機会の時計は進む。適格な期限経過でDA/default |
| controlが巨大witnessや大量noticeで競合 | 宣言したcontrol inclusion boundを維持する |
| extensionなしで古いbondが解放される、または期限だけ延長 | validation拒否。原子的更新がなければ成立しない |
| `collect_until−1`と`collect_until`でterminal収録 | 前者は他条件を満たせば受理、後者は範囲外 |
| extension record重複、順序違い、overflow、資金不足 | 状態変更なしで拒否 |
| 延長直前/直後のreorg、再起動、snapshot import | canonical stateに復元。同じverdict/hold/返金 |
| queue中にcheckpoint、2回以上checkpointを跨ぐcourt | reservation/material/holdが消えない |
| 1万件が同時terminal/default/返金可能に到達 | 決済処理もbounded。CPU spikeで全block処理を止めない |
| capacity下限未満のservice障害 | 正直な相手を資料不提供と決めつけない。事故経路を記録 |
| 小/大block profileで同じ有効証拠を処理 | consensus結果同値。性能差・ノード要件だけを評価 |

### 14.4 合格の報告形式

全件について`a/n/s/F/d/E`、reserved/actual bytes/work/IO、bond lock期間、保持byte-DAA、cold条件を出力する。集計は、3,200以内率、予約期限超過数、backlogピーク/排出期間、不当default数、最大RAM/DB増分、IBD量を含める。

平均やp99が良くても、宣言した保証範囲で予約期限に違反した案件があれば、そのprofileのhard-deadline gateは不合格。実測が全て速くても、形式的な上限・敵対的経路・inclusion前提の穴があれば保証完成とはしない。

## 15. 実装順序と統合箇所

| 段階 | 作業 | 主な候補箇所・成果物 |
|---|---|---|
| 0 | 最新door branch/ADR-0191、tag230/245、D2/X3、既存期限とsnapshot版を照合 | snapshotとの差分台帳、正式door定義 |
| 1 | 最小primitiveと行レベル認証、全fault被覆、完全transcript/work bound | `palw_tir_dissect_v1.rs`、`palw_tir_court_v1.rs`、class admission、codec |
| 2 | A/B/Cの小型profileを作り、cold/強制経路の単発3,200を検証 | witness builder、frontier、source/custodian経路 |
| 3 | モデル横断capacity ledger、control保護、予約calendar、notice、one-outsider経路 | `palw_tir_public_allocation_v1.rs`、public/owned/fold、node scheduler |
| 4 | v2 effective terms、bounded extension、Final/bond/lease/料金の原子的更新 | `palw_tir_public_custody_v1.rs`、`palw_state_v2.rs`、staged folds |
| 5 | node全体の100/1,000/10,000件burstと持続attack試験 | 出荷binary、reference機材、負荷generator、回復drill |
| 6 | checkpoint10,000、active state保存、pruning/IBD、必要なroot性能改善 | carriage、pruning proof、DB/journal、snapshot codec |
| 7 | court比率とblock容量候補の比較、価格/保持/露出/同時件数を確定 | capacity profile、service evidence、承認 |
| 8 | record pin、fence drill、段階配備 | release manifest、network identity、運用承認 |

live node・main branch・fence・t12を本書の作成作業で変更しない。未確認のwire番号や動作中networkの高さを文書に仮置きしない。

## 16. 移行と互換性

v1のopen frameは受付時のrulesetで処理する。新しい容量予約が旧frameの保証を奪わないよう移行用容量も予約する。v1にv2の延長機能を勝手に適用しない。

既存署名済みpledgeやasset lockを越える期限延長は、新しい合意規則と責任主体の事前契約なしに導入しない。v2は、max extensionを含む署名・料金・capを備えた新規frameから導入するのを基本とする。

既存の3,120 gateを3,200へ変更することは、別の明示的パラメータ承認を必要とする。80 DAAの差を誤差として扱わない。新しい小型profileと旧大型witnessの受理条件をversionで分離し、旧fallbackを新上限の例外にしない。

fence前後、旧/新frame混在、旧/新checkpoint、reorgがfenceを跨ぐ場合を試験する。capacity profileの増強で既存参加者の応答時間を突然短縮しない。

## 17. arming recordと未確定事項

### 17.1 pinする証跡

`PALW_G14_ARMING_RECORDS_V1`等の既存gateを迂回せず、統合時にv2の審査recordとの対応を定める。投稿中の`PALW_TIR_FRAME_COST_RECORDS_V1`は最新branchと照合する。

```text
source / dependency commit / binary SHA-256 / build flags
ruleset / class-artifact / witness codec / resource-template fingerprints
S_step / S_close / S_transcript / W / IO / RAM / rounds の導出
全fault route、cold条件、強制開示とdefaultの最大値
service lower envelope / inclusion前提 / control容量 / block profile
予約アルゴリズム / bounded adversarial branching / one-outsider evidence
E_max / effective-terms / retention / escrow / pricing / exposure
100/1,000/10,000件burst、持続attack、restart/reorg、pruning/IBDログ
参照機材・通信条件・最大値・観測span・全結果のhash
独立レビュー / 採択 / 有効化条件
```

### 17.2 採択前に確定すること

`SC64`の64 KiB総量と32 KiB close目標が全対象routeで成立するか、17,408 Bの定義/codec、min検査窓、service rate/gap、control収録条件、`E_max`、保持単価・延長funding、one-outsiderの枠占有耐性、checkpoint検証経路、block容量の採用値は、現時点では未確定である。

これらは「実装者が適当に値を入れてよい」項目ではない。schemaでUNPINNEDを明示し、1つでも残るprofileは武装不可とする。目標サイズが成立しない場合は、lowering/codec/対象範囲を見直すか、別の上限変更を明示的に再審査する。

## 18. このRFCが達成したと主張しないこと

本書は、1万件courtの実ノード処理、SC64上限、単発3,200、network帯域保証、node E2E、暗号学的soundness、pruning/IBDを実証したものではない。

同梱`capacity_planner.py`は、容量下限の整数計算と、先行予約を後続が動かせない**保守的なtoy scheduler**を提供する。実行した27試験はその算術・原子的予約・snapshot再生・1万件有限入力の整合性に対するものであり、1万件のネットワーク試験ではない。

本RFCの設計上の到達点は、**「各courtを小さく有界にする」「通常3,200と混雑時の実期限を分ける」「有限burstを安全に排出する」「持続過負荷を新規受付で止める」「責任と証拠を保ったままpruning/IBDを成立させる」**ための実装・検証契約を一体化したことである。

## 19. 参考一次資料

プロジェクト固有の事実は[source-audit-v2.md](0025-assets/source-audit-v2.md)および前版のsource-auditに限定して根拠を示した。次は設計上の参考であり、このnetworkへの適用や性能を証明する資料ではない。

- [RFC 2212 — Specification of Guaranteed Quality of Service](https://www.rfc-editor.org/rfc/rfc2212.html), 1997-09。帯域予約・流量制御・遅延上限を組み合わせる設計と、その経路条件を参考にした。
- [OP Stack Specification — Fault Dispute Game](https://specs.optimism.io/fault-proof/stage-one/fault-dispute-game.html), 参照2026-10-19。単一stepの認証済み入力、ゲーム時計の有界延長、裁定とbond分配までの確定待ちの分離を比較対象とする。既存実装の移植ではない。
- [Kaspa Research — Pruning safety in the vProgs architecture](https://research.kas.pa/t/pruning-safety-in-the-vprogs-architecture/411), 初回投稿2025-08-25。pruned dataを後続検証が暗黙に要求しないことの論点を参考にした**研究提案**。採用済みの一般規則や、このRFCのIBD検証の根拠としては扱わない。

## 整合性修正 — 2026-10-19(RFC 全件整合性監査)

- **混雑延長は hard deadline の内側に限る。** ユーザーが採用した RFC-0014 2026-10-19 改定の `a + 10,000` を主契約とし、`a + 3,200 + E_i ≤ a + 10,000`(`E_max ≤ 6,800`)と読み替える。これを超える延長は採用していない別の将来方針。10,000 の内側で処理できない負荷は、original claim の受付で抑制する([RFC-0026](0026-court-policy-master-contract.md) §2.1・§3)。本書 §4.3 の「`E_max` を一律 10,000 で埋めない」は、この上限の内側で容量から導く、という意味に限る。
- **SC64 は現行 profile の達成値ではない。** 既存の各手番の署名(ML-DSA-87、4,627 B)を残すと、2M・4 分岐では署名だけで 19 × 4,627 = 87,913 B となり 64 KiB を超える(算術上の下限)。SC64 は認証方式・投稿数を含めて別 profile として検証する目標(F05、RFC-0026 §7.1)。
- §0 の「既存の休眠実装」`palw_court_capacity_v1.rs` は別 branch(`release/t12-16000`)にあり、`pre` には未統合(F10、RFC-0026 §8.1)。
- 「本書を上位とする」は Draft どうしの参照関係であり、採択済みの規範に対する置換ではない。正式な置換は RFC-0021・凍結規程どおり ADR と置換節で行う(F11、RFC-0026 §8.2)。
