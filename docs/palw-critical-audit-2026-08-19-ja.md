# PALW 致命的欠陥監査

**対象**: `misakas-palw-adr0038-9cfcbf99.zip` / `palw-only-v4` / README 記載 commit `9cfcbf99`（2026-08-19）  
**範囲**: PALW のみ。EVM、DNS finality、一般的な Kaspa/GHOSTDAG 実装は、PALW と直接交差する箇所以外は対象外。  
**手法**: ADR、コンセンサス型、block admission、PoW、panel、receipt、court、fork choice、class DAA の静的・経路監査。

## 受領時の独立検証（被監査側、2026-08-19）

監査環境に `cargo` が無く静的監査であると但し書きがあるので、**こちら側で主要な指摘を実行して確認した**。結果は下表のとおりで、**確認した範囲はすべて監査が正しい**。

| 指摘 | 検証方法 | 結果 |
|---|---|---|
| P0-2 commitment 署名が admission で未検証 | `check_palw_block_admission_v1` の本体を grep（`verify`/`signature`/`sign` が 0 件） | **確認。検証していない** |
| P0-3 fresh tip が必ず未解決 | 制御フロー：fold は `std::iter::once(chain_tip)` で tip 自身を含む → panel は `accepted_daa + delta_bind` 以降の anchor を要求 → tip 自身に未来 block は無い → `None` → `chain_weights_v1` は `UnresolvedBlock` で chain 全体を拒否 | **確認。循環は構造的** |
| P0-6 conviction/equivocation の署名 context が誤り | `processor.rs:2692-2700` の closure が `PALW_RECEIPT_MLDSA87_CONTEXT` 固定。正しい `PALW_S_MLDSA87_ATTESTATION_CONTEXT` は `palw_slash.rs:95` に存在し別 path で使用 | **確認** |
| P0-7 executor 除外 ID が別 namespace | `processor.rs:2664` が `executor_bond_outpoint.transaction_id` を渡す。candidate 側は `validator_pubkey_hash` | **確認** |

### 帰属についての注記

**P0-3、P0-6、P0-7 は 2026-08-19 当日に本セッションで書かれた行である**（それぞれ commit `73ed5aeb` / `2b01da91` / `a8df6bb0`）。P0-2 はそれ以前からの欠落だが、同日の commit `a953ee4f` が sidecar signer の doc と commit message で

> a signature over a foreign bond simply fails verification at admission, because the registry resolves the key from the bond rather than from the commitment

と書いており、**この安全性主張は成立していない**。admission は署名を一度も検証しない。当該 doc は本 commit で訂正済み。

つまり、同日に「実装完了」と報告した層に対して、**その報告と同じ日のコードから P0 が 4 件出ている**。この発見率自体が、再監査なしに activation を語れないことの根拠である。

## 対応状況（被監査側）

| | 状態 |
|---|---|
| **P0-1** commitment が PoW に未束縛 | **CLOSED** — `bind_l1_tag_v1` で commitment root を L1 tag に混合。`one_pow_solution_cannot_carry_two_commitments` が trace/output/bond の 3 フィールドを個別に固定 |
| **P0-2** commitment 署名が未検証 | **CLOSED** — admission が ticket より前に検証。`a_commitment_nobody_signed_is_refused_before_the_inference` |
| **P0-3** fresh tip が必ず未解決 | **CLOSED** — panel 未生成は空 panel（= `Provisional`）であって未解決ではない |
| **P0-4** candidate weight が sink 依存 | **CLOSED** — carriage は candidate chain の fold、bond view は chain path 再生で candidate ごとに導出 |
| **P0-6** 署名 context が誤り | **CLOSED** — 検証器が context を受け取り、各 family が自分のドメインを指定 |
| **P0-7** executor 除外 ID / operator dedup / no-show 罰則 | **一部** — 除外 ID は bond record から解決（CLOSED）。operator dedup と no-show 罰則は未着手 |
| **P0-5** header tip と virtual tip が別ルール | **境界で決着** — 配線ではない。PALW weight は accepted tx の関数で、header processor は body より上流なので header-only 近似は「2 つ目の fork choice」になり defect そのもの。**headers-selected tip は sync hint であって chain authority ではない**ことを言明し、消費者を監査（IBD の要求先選択と RPC 推定のみ。pruning/finality/acceptance は virtual sink を読む）|
| **P0-10** bond exposure 上限なし | **CLOSED** — `palw_exposure` (`Σ immature_pwu ≤ collateral / penalty_per_pwu`、prefix-mandatory、bond ごと、overflow は拒否) を chain weight fold に配線。超過分は live weight を失うが block は有効（そうしないと他人の bond に安い commitment を先着させる griefing になる）。`penalty_sompi_per_pwu` は既存の fork-choice fence に追加（6 本目を作らない — β と対で意味を持つため）|
| **P0-7** executor 除外 ID / operator dedup | **CLOSED** — 除外 ID は bond record から解決、`operator_root` は owner hash（`None` だと 1 operator が k bond で k 席、quorum の購入価格が bond 下限 × k）|
| **P0-7** no-show 罰則 | **意図的に未実装** — 現状の consequence は「share の没収」（credit は receipt から payee を作るので既に発生している）。それを超える罰則は `BondMutation` に `Slash`（**全額 burn**）しか無く、no-show に配線すると liveness 障害を equivocation と同罰にし、**seated な validator を 1 duty window だけ eclipse すれば担保全額を焼ける**というより安い攻撃を作る。必要なのは partial-forfeit mutation と罰則の大きさ（後者は ADR-0038 が「決めない」と明記する数値パラメータ）|
| **P0-8** 法廷が通常ノードで判決不能 | **機構 CLOSED / 母集団は前提待ち** — `palw_artifact` で証明付き operand（leaf は位置も束縛、奇数ノードは promote、tail 付き path は拒否、1 件でも偽なら全体を拒否）。法廷の算術は不変で、oracle だけが「持っているファイル」から「証明された証拠」に変わる。**残るのは inventory の登録**（どの tensor をどの順で行に切るか）で、これは実モデルの shape profile が無いという既知の前提に依存する |
| **P0-9** bisection court 未完成 | **一部** — 項目 5（`Open` が slash 対象 bond を一意に定められない）を CLOSED：`responder_bond_outpoint` を追加し、解決不能な bond を名指す Open は dispute を開かない（開けると「担保 0 で block を永久 Provisional に留める無料の veto」になる）。項目 1（`mid_state` が何も束縛しない）を **部分 CLOSED**：endpoint 状態 (`lo_state`/`hi_state`) を維持し、そのどちらかを繰り返す開示を拒否。開示を「真」にはしない（full node には決定不能）が、responder を**自分の主張の連鎖に拘束**し、端点が等しい＝分岐が無い区間へ誘導できなくする。この pair は terminal check の anchor pair そのものなので**その不在**はもう terminal を塞がないが、**その弱さは塞ぐ** — 毎 rung 異なる junk を出す responder は依然として区間を誘導でき、これだけを根拠に terminal move を足すと fail-open になる。残るのは terminal opening 自体（項目 2）、withheld execution の authorship（項目 3）、ladder 深さ（項目 4）|

### P0-1 で監査の推奨 remedy を採らなかった理由

監査は「PoW finalizer が `Expand(commitment_root)` を L1 tag として消費する」ことを求めている。**これは `l1_tag_bytes` が既に実装している内容だが、採用しなかった。**

それは推論を**置き換える**変更、すなわち W1 そのものだからである。tag の生成が無料になり、監査自身が P0-10 で「W1 だけを直すと fake-root grinding が急に安くなる」と指摘している。**bond ごとの未成熟 exposure 上限（P0-10）が入る前に work を無料にしてはならない。**

そこで採ったのは**束縛のみ**：推論は work のまま残し、commitment root を tag に混合する。これで「1 つの PoW 解 → 無制限の block identity」は閉じ、work の価格は変わらない。単独で安全に着地できる。

`bind_l1_tag_v1` は leaf discriminator を `2` にして `l1_tag_bytes` の `1` と分離してある。同じ root に対して両者が同じ bytes を出すと、bound-work 体制と bound-inference 体制の間をネットワークが digest に気付かれず移動できてしまうため。

## 結論

**現状は NO-GO。PALW を有効化して価値を載せてはいけない。**

少なくとも以下の **10 系統の独立した activation blocker** がある。単なる未最適化ではなく、ブロック偽造・bond なりすまし・fork-choice 無効化・同一 DAG でのノード間不一致・裁判不能・担保を超える未成熟 work 発行を引き起こす。

ただし、README が明記する通り、現在の shipped preset では `palw_credit`、`palw_block_commitment`、`palw_schedule`、`palw_ramp`、`palw_fork_choice` がすべて `None` であり、PALW は dormant である。したがって、以下は「現在稼働中のネットワークが直ちに奪われる」という意味ではなく、**現在のコードのまま fence を有効化すると成立する欠陥**である。

---

## P0-1: PALW commitment が PoW ticket に束縛されていない

### 事実

- block identity hash は非空の `palw_commitment` を含む。
- すべての PoW-path digest は `palw_commitment` を明示的に除外する。
- `bound = true` では、shape gate が非空 PBC1 commitment を受け入れる。
- しかし live admission は `StateLayer0::calculate_pow_layer0` を呼び、PALW L1 tag は `(pre_pow_hash, timestamp, nonce, network_id)` からの LLM 実行結果であり、commitment root を消費しない。
- CPU-only 用に用意された `PalwBlockCommitmentV1::l1_tag_bytes` は live admission / PoW path から参照されていない。

### 影響

一つの有効な PALW PoW 解について `palw_commitment` だけを差し替えると、**同じ PoW のまま異なる block identity を無制限に生成できる**。現在の admission は PBC1 の shape・active bond・class PWU を見るだけなので、後述の署名欠落と組み合わせると、任意 root・任意 active bond を載せた sibling block 群を生成できる。

これは source comment 自身が禁止している「one PoW solution, unlimited distinct valid block identities」の再発である。

### 根拠

- `consensus/core/src/hashing/header.rs:26-29, 90-130`
- `consensus/core/src/hashing/header.rs:372-385`
- `consensus/core/src/pow_layer0.rs:412-417, 419-452`
- `consensus/pow/src/palw_admission.rs:113-141`
- `consensus/pow/src/lib.rs:223-225, 260-275, 309-325`
- `consensus/core/src/palw_block_commitment.rs:341-360`

### 必須修正

fence を開く変更は、最低でも次を一つの atomic activation として入れる必要がある。

1. `challenge = H(network, pre_pow_hash, timestamp, nonce, class, bond)` を再計算する。
2. `commitment_root = H(challenge, class, bond, trace_root, output_root, pwu)` を再計算する。
3. PoW finalizer が `Expand(commitment_root)` を L1 tag として消費する。
4. commitment の一ビットでも変更したら PoW が失効することを consensus test で固定する。

---

## P0-2: Block commitment の ML-DSA-87 署名が admission で検証されない

### 事実

`PalwBlockCommitmentV1` の契約は、bond registry から鍵を解決して署名を statefully verify すると明記する。しかし実装は以下のみである。

- `validate_shape`: 署名長のみ。
- `validate_executor_bond_v1`: 指定 outpoint が Active かのみ。
- admission: shape → active bond → class/PWU → LLM ticket。
- commitment signature verifier は呼ばれない。

テスト fixture は `0x5A` を並べただけの署名を「complete block」として admission させている。

### 影響

bond を持たない攻撃者でも、他人の Active bond outpoint を commitment に書くだけで W8 の「no bond, no block」を通過できる。つまり W8 は collateral requirement ではなく、**Active bond 名簿から誰か一人の名前を書く requirement** になっている。

責任主体・payee・将来の slash attribution も commitment signer と結びつかない。sidecar の説明にある「foreign bond は admission で署名失敗する」という安全性主張は、現在の consensus path では成立しない。

### 根拠

- `consensus/core/src/palw_block_commitment.rs:241-265, 281-306`
- `consensus/pow/src/palw_admission.rs:113-141`
- `consensus/src/pipeline/virtual_processor/utxo_validation.rs:699-720`
- `consensus/pow/tests/palw_admission_fixture.rs:50-60, 88-106`
- `kaspa-pq-validator-core/src/lib.rs:264-268`

### 必須修正

active bond record の `validator_pubkey` で、`commitment.message(network, pre_pow_hash, timestamp, nonce)` を `PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT` により検証し、ticket 計算より前に失敗させる。

---

## P0-3: 新しい PALW tip は必ず「未解決」になり、PALW fork choice が作動しない

### 証明

PALW-only chain の候補 tip `T` 自身に commitment があるとする。

1. `palw_chain_weights_v1` は `T` 自身を最初に fold 対象へ入れる。
2. `palw_block_weight_v1(T, T, ...)` は panel を要求する。
3. panel anchor target は `accepted_daa(T) + delta_bind`。
4. `delta_bind > 0` は params validation で必須。
5. `T` の chain は `daa(T)` より未来の block を含まないため、anchor は必ず `None`。
6. block weight が `None` になり、`chain_weights_v1` は一つでも `None` があれば chain 全体を拒否する。
7. すべての live tip が `None` となり、`order_tips_v1(PalwWeighted)` は `(None, None)` で既存 blue-work order に戻る。

### 影響

**PALW-only fork choice は live tip に対して一度も PALW weight を使わない。** これはデータ到着待ちではなく、未来 anchor を現在 tip 自身の評価に必須化した構造的循環である。

### 根拠

- `consensus/src/pipeline/virtual_processor/processor.rs:2597-2619`
- `consensus/src/pipeline/virtual_processor/processor.rs:2642-2666`
- `consensus/src/pipeline/virtual_processor/processor.rs:2729-2743, 2772-2808`
- `consensus/core/src/palw_schedule.rs:318-323`
- `consensus/core/src/palw_chain_weight.rs:99-131, 189-201`
- `consensus/src/pipeline/virtual_processor/processor.rs:6751-6761`

### 必須修正

fresh commitment は future panel が未生成でも `Provisional` として deterministic に weigh できなければならない。panel は anchor 到達後に追加導出し、`ReceiptLicensed` / `Final` への遷移だけに必要とする。

---

## P0-4: 同じ DAG でも、現在の node sink により candidate weight が変わる

### 事実

- `palw_carriage_on_chain_v1` はコメントどおり APPLIED chain の store しか持たず、未適用 candidate branch の carriage を復元できない。
- それにもかかわらず fork choice は複数の未適用 candidate tip をこの helper で weigh する。
- 初期 candidate 全体の weight は、previous sink 時点の一つの mutable `bond_view` で計算される。
- search 中に parent candidate を追加すると、その時点で別 candidate の UTXO walk により変更された `bond_view` が渡される。
- panel capability も candidate-chain snapshot ではなく node-global store から読む。

### 影響

同じブロック集合・同じ DAG を持つ二つの node が、以前どの branch を virtual sink として適用していたかにより、同じ candidate について異なる bond status、receipt、conviction、capability、panel、weight を得られる。

これは ADR の W3「equal DAGs ⇒ equal weights」を直接破り、**永続的な fork / partition** を生む。

### 根拠

- `consensus/src/pipeline/virtual_processor/processor.rs:2540-2569`
- `consensus/src/pipeline/virtual_processor/processor.rs:2671-2677`
- `consensus/src/pipeline/virtual_processor/processor.rs:2742-2758`
- `consensus/src/pipeline/virtual_processor/processor.rs:6840-6891, 6915-6983`
- `consensus/src/pipeline/virtual_processor/processor.rs:2163-2181`
- `docs/adr/0038-palw-is-the-consensus-work.md:511-517, 640-645`

### 必須修正

weight の全入力を candidate chain point から導出する必要がある。node の current sink store を読む API は weight path から禁止し、candidate ごとに以下を再構成する。

- active bond snapshot
- class state / target / freeze / coverage
- capability declarations
- carriage records
- panel anchor と eligible set

加えて、異なる prior sink、restart、IBD start、pruning point から同じ DAG を与え、全 candidate weight が一致する differential test が必要。

---

## P0-5: Header-selected tip・IBD・pruning と virtual fork choice が別の規則を使う

### 事実

header processor は PALW fence の状態に関係なく、candidate と previous tip の両方へ `palw = None` を渡している。したがって header-selected-tip store は恒久的に blue work 順である。

ADR-0039 自身がこの site を「変更対象」と明記している。また `safe` の型コメントは IBD、deep reorg、finality に使うとするが、実際の PALW weight wiring は virtual tip ranking に限られている。

### 影響

P0-3 を直して virtual sink が PALW weight を使い始めても、header reachability hint、IBD、pruning/finality の chain authority が blue work のまま残る。内部で二つの canonical-chain 観が生じ、pruning boundary や同期結果が PALW fork choice と食い違う。

### 根拠

- `consensus/src/pipeline/header_processor/processor.rs:436-449`
- `docs/adr/0039-palw-only-block-production.md:205-222`
- `consensus/core/src/palw_chain_weight.rs:90-96`

### 必須修正

PALW fork-choice order を一つの純粋関数として、header-selected tip、virtual sink、IBD、pruning、finality/deep-reorg gate の全 chain-selection site に適用する。少なくとも「同一入力で subsystem 間の selected tip が一致する」統合テストが必要。

---

## P0-6: conviction / equivocation の署名コンテキストが weight resolver で誤っている

### 事実

`resolve_block_facts_v1` は一つの generic `verify_signature` closure を以下すべてに使う。

- verification receipt
- step conviction の execution attestation
- equivocation の execution attestations

live caller は closure を `PALW_RECEIPT_MLDSA87_CONTEXT` 固定で構築する。しかし step/equivocation attestation は `PALW_S_MLDSA87_ATTESTATION_CONTEXT` で検証しなければならない。別の slash path は後者を正しく使用している。

### 影響

正しい execution attestation でも weight resolver では signature failure になり、`convicted_before_close` が立たない。特に equivocation は別 path で bond を slash できる一方、対象 PALW block は fork-choice weight を保持する。

つまり **slashing state と work state が矛盾する**。

### 根拠

- `consensus/core/src/palw_facts.rs:433-510`
- `consensus/src/pipeline/virtual_processor/processor.rs:2692-2699`
- `consensus/core/src/palw_carriage.rs:846-880, 913-976`
- `consensus/core/src/palw_slash.rs:93-95`
- `consensus/src/pipeline/virtual_processor/processor.rs:3019-3028`

### 必須修正

contextless closure を廃止し、object family ごとに型を分ける。

- receipt verifier
- block commitment verifier
- execution attestation verifier
- bisection/court object verifier

一つの closure へ任意 family の digest を渡せる API は再発しやすいため不可。

---

## P0-7: panel が executor 自身を除外できず、no-show に罰則もない

### 事実

executor exclusion に渡される ID は `executor_bond_outpoint.transaction_id`。一方、candidate の `validator_id` は `validator_pubkey_hash`。比較対象が異なる namespace であるため、通常は一致せず producer 自身が eligible に残る。

さらに `operator_root` は常に `None` であり、operator-level dedup は実質無効。`panel_duty_v1` は no-show を計算するが、コメントが「live slash path does not exist」と明記し、production call も存在しない。

### 影響

- producer が自分の work を「独立検証」する panel seat を得られる。
- 複数 bond による同一 operator の seat 集約を防げない。
- verifier は receipt を出さず safe maturation を止めても consensus penalty がない。

### 根拠

- `consensus/src/pipeline/virtual_processor/processor.rs:2658-2666`
- `consensus/core/src/palw_job_panel.rs:99-115, 151-180`
- `consensus/core/src/palw_facts.rs:585-675`

### 必須修正

resolved executor bond record の `validator_pubkey_hash` を exclusion ID として使う。operator dedup の consensus source を定義する。assigned duty は deadline 後に slash または明示的 collateral loss へ接続し、reorg/replay 時にも同一結果になるよう chain-scoped に導出する。

---

## P0-8: arithmetic court は通常 full node では判決不能

### 事実

MatMul、Requantize、RoPE、Rescale の adjudication は raw model artifact row を `PalwWeightOracleV1` から取得する。ところが full-node production type `PalwNoWeightsV1` は全 row を `None` にし、すべて `Unadjudicable` にする。

live weight resolver と slash path はどちらも no-weights oracle を使用する。class freeze は panel 生成時に `|_| false` と hardcode されており、`Unadjudicable → class freeze` の設計も閉じていない。

### 影響

現在の安全側 wiring では arithmetic fraud を誰も conviction できない。逆に、model artifact を持つ node だけ real oracle に差し替えると、ローカル artifact availability が consensus verdict に入り、node 間不一致になる。

W1 が要求する「model artifact なしの full node が Merkle-proven operands だけで一 primitive を裁定する」状態ではない。

### 根拠

- `consensus/core/src/palw_step_refute.rs:269-329, 418-444`
- `consensus/src/pipeline/virtual_processor/processor.rs:2652-2677`
- `consensus/src/pipeline/virtual_processor/processor.rs:3036-3049, 8565-8575`
- `consensus/src/pipeline/virtual_processor/processor.rs:2758`
- `docs/adr/0038-palw-is-the-consensus-work.md:385-391, 639-643`

### 必須修正

refutation evidence 自体に、必要な weight row、quantization parameter、RoPE table slice と、その class artifact root への Merkle proof を含める。判決関数は chain-registered artifact root と証明だけを入力にし、ローカル model file を読まない。

---

## P0-9: bisection court は soundness と liveness の両方が未完成

source の test comment 自身が、以下を remaining gap と明記している。

1. terminal index に到達しても adjudication されない。
2. terminal-opening move が存在しないため、honest responder 後に challenger が消えると永久に Provisional。
3. `mid_state` が一度も検証されず、responder が interval を任意に誘導できる。
4. withheld execution には execution attestation がないため、現行 step conviction object を構成できない。
5. `Open` は `responder_id` しか持たず、slash 対象の bond outpoint を一意に定められない。
6. shipped schedule は 10 rounds、最大 1,024 steps しか到達できず、README は pinned model の実 trace が数十 token で超える可能性を認める。

### 影響

- fail-closed のままでは dispute を開くだけで block maturity を永久停止できる。
- terminal を安易に Final 扱いすると、不正 responder が bisection を誘導して fraud を通せる。
- 深い fraud は challenge window 内に terminal/conviction が入らず、制度上 prosecution 不可能。

### 根拠

- `consensus/core/src/palw_facts.rs:1866-1922`
- `consensus/core/src/palw_schedule.rs:160-206`
- `README-ADR0038.md:35-39`
- `docs/adr/0038-palw-is-the-consensus-work.md:570-573, 609-623`

### 必須修正

- midpoint state commitment の意味と verification rule を定義する。
- terminal opening、deadline、challenger/responder default の責任帰属を完成させる。
- commitment carriage を authorship proof として使うか、direct one-step adjudication へ接続する。
- session を bond outpoint に束縛する。
- 実際の `step_leaf_count` を測定し、全 class について challenge window 内に terminal verdict まで到達できることを activation condition にする。

---

## P0-10: 一つの bond が無制限の未成熟 PALW work を背負える

### 事実

設計は fake root を admission で暗号学的に真と判定できず、sampling・court・bond economics で抑えると明記している。一方で `Provisional` / `ReceiptLicensed` は正の live weight を得る。

しかし block commitment 側には、bond ごとの以下の consensus state がない。

- in-flight commitment 数
- immature PWU 総量
- reserved slash exposure
- maturity / void 前の bond 再利用制限

ADR-0039 にあるのは relay の in-flight candidate 制限案だけであり、private fork や直接 peer 送信に対する consensus rule ではない。class epoch budget も「no enforcement point」と明記される。

### 影響

W1 を正しく実装して full node の再推論を外すと、攻撃者は真の inference をせず random root を grind できる。一つの bond で多数の未成熟 claim を同時に発行し、最初の slash が確定する前に collateral の何倍もの live weight を積める。

したがって現在は、「全 node が LLM を再実行している」という別の設計違反が、偶然この経済攻撃を高価にしているだけである。W1 だけを直すと攻撃が表面化する。

### 根拠

- `consensus/core/src/palw_block_commitment.rs:48-62`
- `consensus/core/src/palw_chain_weight.rs:90-131`
- `docs/adr/0039-palw-only-block-production.md:350-376`
- `consensus/core/src/palw_class_daa.rs:300-306`

### 必須修正

bond ごとに consensus-reserved exposure を持たせる。最低条件は次のような形である。

```text
sum(immature_pwu_backed_by_bond) <= slashable_collateral / penalty_per_pwu
```

commitment admission または live-weight eligibility の時点で exposure を reserve し、Final / Voided / timeout で解放する。relay limit は補助的 DoS 対策に留める。

---

## 追加の activation blocker

### W1 がまだ実装されていない

live admission は全 node で `StateLayer0::calculate_pow_layer0` を呼び、algo 4/5 は external LLM runtime を実行する。したがって model/runtime のない VPS は block validation 不能であり、runtime determinism と header-validation DoS が残る。

- `consensus/pow/src/palw_admission.rs:134-136`
- `consensus/pow/src/lib.rs:256-275`
- `consensus/pow/tests/palw_admission_fixture.rs:82-85`

### Per-class DAA / class lifecycle が未配線

`palw_class_facts_for_block` は一つの登録 class しか認めず、retarget step を空 vector に固定するため target は boot target のまま。class epoch budget は derivation のみで reject point がない。multi-class resilience、freeze redistribution、cadence recovery は現在の live path では成立しない。

- `consensus/src/pipeline/virtual_processor/utxo_validation.rs:1576-1613`
- `consensus/core/src/palw_class_daa.rs:300-306, 1261-1284`

---

## 具体的な攻撃シナリオ

### 攻撃 A: 一つの推論から sibling block を大量生成

1. 攻撃者が PALW nonce を一度解く。
2. 任意の Active victim bond を commitment に記載する。
3. 長さだけ正しい偽署名を付ける。
4. `trace_root` / `output_root` を差し替えて PBC1 を複数作る。
5. PoW path は commitment を見ないため、全 header が同じ PoW を通る。
6. identity hash は commitment を見るため、全て別 block として DAG に入る。

### 攻撃 B: W1 修正後の fake-root private chain

1. attacker は一つの bond を用意する。
2. inference を行わず random commitment root を大量に試し、ticket を満たす root を選ぶ。
3. 同一 bond の exposure 上限がないため、court が一件閉じる前に多数の Provisional block を積む。
4. 各 block は正の live weight を持つ。
5. verifier no-show に罰則がなく、court も terminal / weight operand 不足で閉じない。

### 攻撃 C: slash された work が fork choice に残る

1. 正しい equivocation certificate が chain に入る。
2. slash path は attestation context で検証し、bond を slash する。
3. weight resolver は receipt context で同じ signature を検証し、失敗する。
4. `convicted_before_close = false` のままになり、対象 block の PALW weight が残る。

### 分岐事故 D: 同一 DAG、異なる prior sink

1. Node A は branch A を applied sink、Node B は branch B を applied sink にしている。
2. 両 node が同じ DAG 全体を受信する。
3. candidate weight が applied-chain carriage store と current bond/class/capability stateを読む。
4. 同じ candidate に異なる facts / panel / weight が出る。
5. A と B が別 tip を選び続ける。

---

## Activation の最低条件

以下がすべて閉じるまでは、五つの PALW fence を `Some` にしてはならない。

1. **Ticket binding**: commitment root を PoW finalizer に束縛し、commitment mutation が必ず PoW failure になる。
2. **Signer authentication**: block commitment signature を active bond key で admission 検証する。
3. **Fresh-tip semantics**: panel 未生成の fresh block を `Provisional` として deterministic に weigh する。
4. **Candidate-chain purity**: 全 facts を candidate chain snapshot から導出し、current sink store を weight path から排除する。
5. **Single fork-choice authority**: virtual、header tip、IBD、pruning、finality が同じ PALW order を使う。
6. **Typed signature APIs**: receipt、commitment、attestation、court で context を型分離する。
7. **Proof-carrying court**: primitive adjudication に必要な operand と artifact proof を evidence に含める。
8. **Complete bisection**: midpoint verification、terminal opening、default rule、bond binding、十分な depth を完成させる。
9. **Collateral accounting**: bond ごとの未成熟 PWU exposure を consensus 上限内に reserve する。
10. **Panel enforcement**: executor exclusion を正しい validator ID で行い、operator dedup と no-show penalty を配線する。
11. **Class lifecycle**: Active/frozen/coverage、per-class retarget、share redistribution、epoch budget enforcement を chain-scoped に実装する。
12. **Determinism test suite**: prior sink、restart、IBD start、pruning point、store insertion order が異なっても同一 DAG の weight/tip/verdict が一致することを検証する。

---

## 監査判定

| 項目 | 判定 |
|---|---|
| 現行 shipped preset の直ちの危険 | PALW fence が dormant のため限定的 |
| PALW testnet activation | **不可** |
| PALW value network / mainnet activation | **不可** |
| 小修正で閉じるか | **閉じない。admission、fork choice、court、economics を同時に直す必要がある** |
| 最優先修正 | P0-1、P0-2、P0-3、P0-4 |

## 検証上の制約

この監査環境には `cargo` 実行環境がなく、README 記載の test suite を再実行できなかった。そのため本判定は source / call-path の静的監査である。ただし、P0-1、P0-3、P0-6、P0-9 は制御フローまたは source 内の明示コメントから直接導け、テスト再実行の成否に依存しない。
