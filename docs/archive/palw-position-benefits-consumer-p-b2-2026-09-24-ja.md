# P-B2: Position の特典は誰が消費するのか — 設計メモ（C1 に決定）

対象: `feat/testnet-12-regenesis`。出典: 2026-09-23 Position route matrix の P10、P11、P12、P15 と §3.4。

## 決定（2026-09-24、ユーザー）

- **Q1 は C1 に決定: 特典の消費者は chain の外の provider（gateway）です。** chain は保有、tenure、tier を証明する材料を出すだけで、特典を配りません。consensus の変更はなく、fence も不要です。
- **C2（chain 上の `HOLDER_VOICE`）は実装しません。** C3（「chain は証明、配布は chain の外」という読み替え）は C1 と両立し、この決定の説明として使ってかまいません。
- **Q2（ADR-0091 buyback の値上がり益）は未決です。** §5 の選択肢はそのまま残します。

## この段階で入ったもの（RPC の半分）

- `getPalwModelPositions`（op 172）の wire version 2。各行に `holdingSinceDaa`、`tenureDaa`、`tierIndex`、`tier` を、応答に `tipDaa` と **`tipHash`** を載せます。新しい op は作りません。
  - `tipHash` はレビューの指摘で追加しました。DAA score だけでは reorg をまたいで同じ state を指せないため、tip ブロックの hash も返します。複数の読み取りを組み合わせるときは、score ではなく hash の一致を確かめてください（§3.3 の手順 7）。
- gRPC にも同じフィールドを載せ（`tipHash` は field 4）、往復テスト `every_membership_field_survives_the_grpc_round_trip` で固定しました。
- `GetPalwModelPositionsRequest.holder` と rpc.proto の doc を直しました。holder id は payout payload ではありません。carrier lane では `palw_model_holder_of_pubkey_v1(pubkey)`（keyless の `mldsa87_key_id`）、EVM lane では `evm_holder_v1(chain_id, address)` です（P-B4 の修正と同じ整理）。
- P11（`model_position_across` の重複 id 二重計上）は同じ一連の commit で修正済みです（route matrix の (5)）。consensus の読み手がないことは source pin で固定しています。

## 残りの作業（follow-up、どれも consensus の変更なし）

1. **§3.3 の provider 検証レシピの実装。** gateway の membership check（unmerged の 773f6e7c を移植する）、challenge の ML-DSA-87 署名 context 定数、EVM lane の EIP-191 digest、CLI の証明署名（cdc5cd26 の `misaka palw benefits`）。新しい op を足すなら 177 以外の空き番号を使ってください（t12 では 177 が `GetPalwClaims`）。
2. **carrier と EVM をまたぐ合算（ADR-0095 N3）。** 1 人が複数の id を持つ場合の合算は、現状は provider 側で §3.3 の手順 7 どおりに行います。chain 側に複数 id の RPC を足すなら、重複除去済みの `model_position_across` を使ってください。
3. units だけを返す `ConsensusApi::palw_model_positions_v1` と session wrapper の削除（呼び出し元はもうありません）。
4. `misaka palw model-positions` に tenure と tier を表示する。

以下は判断のために書かれた元の分析です。§4 の C1 と §3.3 のレシピが、上の決定と follow-up の根拠です。

## 1. ADR が想定している消費者

| 出典 | 内容 | 消費者 |
|---|---|---|
| ADR-0095 §3 | chain が強制するのは lead、notice、lapse。保有量、宣言、tenure は記録するだけ。「serve the artifact, or run a queue」はできない（CANNOT） | — |
| ADR-0095 §4.3 | `palw_model_benefit_tier_v1(state, line, holder, daa)` は consensus-core に置く。「a gateway, a wallet and the explorer compute the same answer」 | gateway、wallet、explorer |
| ADR-0095 §4.8 | 保有者は challenge `(line, holder, nonce, daa)` に署名する。gateway は署名を検証し、`daa` 時点の tier を読む。何も送信せず、何も使わない | **gateway（chain の外）** |
| ADR-0095 §4.9 | `HOLDER_VOICE`: evaluation や proposal に `by_holder_tier` を記録する | **fold（chain 上）**。未実装。state root が変わるので専用の activation が要る |
| ADR-0095 §4.10 | `getPalwModelBenefitTier(line, holder)` が gateway の問いに答える | RPC |
| ADR-0101 D1、D2 | 「line が product、provider が serving、chain が membership」。`HOLDER_VOICE` の担当は chain、`PRIORITY_INFERENCE`、`INFERENCE_QUOTA`、`EXPERIMENTAL`、`PRIVATE_BETA`、`EARLY_VERSION` は root を持つ任意の provider、`DEVELOPER_ACCESS`、`SUPPORT` は line の origin | provider |
| ADR-0101 D5 | provider は、保有者が署名した証明で holding を読み、何も受け取らない | provider |
| ADR-0101 ADR-0144 alignment | 第三者による serving（D4、D6、D7 の step 2〜4）は PALW の作業から撤回。**D7 step 1（0095 の membership check の merge）と step 5（holder mark）は ADR-0095 側に残す** | — |

ADR 上の答えは次のとおりです。**消費者は、line の holder に service を提供する provider の gateway です。chain は tier の計算と証明の検証材料までを受け持ちます。** `HOLDER_VOICE` だけは chain 自身が消費者ですが、まだ作られていません。

## 2. t12（5a559459）に実際にあるもの

| 部品 | 状態 | 場所 |
|---|---|---|
| 宣言、lead の強制、notice、lapse | あり、fold で強制 | `palw_state_v2.rs`（`model_benefit_tiers_in_effect`、enforced lead） |
| tenure clock（`model_position_since`） | あり。buy と sell で更新され、state root に入る | `touch_model_position_tenure` :9379（t12 は genesis から有効） |
| tier 関数 `model_benefit_tier[_across]` | あり。**テスト以外の呼び出し元はない** | :6289〜:6319 |
| `model_position_across` | あり。**同じ id を重複して数える潜在バグ**（P11）— 分析時点。**その後、route matrix の (5) で修正済み** | :6343 |
| challenge `palw_model_benefit_challenge_v1` | 関数はあるが、**呼び出し元はない**。署名 context の定数もない | `palw_model_benefits_v1.rs:321` |
| `HOLDER_VOICE` の書き手 | **ない**。`palw_service_descriptor_v1` が「chain の担当」と分類しているだけ | — |
| line の宣言を読む RPC | あり: `getPalwModelLine.benefits`（in-effect tiers、pending、lapse、lead）、`service_facts` | message.rs :3418 |
| 保有量を読む RPC | `getPalwModelPositions` は **`(lineId, units)` しか返さない** | message.rs :3200 |
| `getPalwModelBenefitTier` RPC | **ない**。unmerged の cdc5cd26 で op 177 として実装されたが、t12 では 177 が `GetPalwClaims` | — |
| gateway の membership check | **t12 にはない**。773f6e7c（`misaka-palw-gateway/src/membership.rs`、`--membership-line`、`GET /v1/membership/challenge`、`PRIORITY_INFERENCE` を優先キューに） | unmerged branch |
| CLI `misaka palw benefits` と証明の署名 | **t12 にはない**。cdc5cd26 にある | — |
| descriptor の検査（ADR-0101 D2、D3） | あり。純関数と pin | `palw_service_descriptor_v1.rs` |

要するに、**保有の記録と tier の計算は chain にあります。それを外に出す口（RPC）と、それを使う側（gateway、CLI）が t12 に入っていません。** P12 が source scan で「消費者 0」になるのは、このためです。

## 3. 最小の有効化（`pb2-rpc.patch`）

### 3.1 何を変えるか

| 層 | 変更 |
|---|---|
| consensus-core `PalwChainStateV2` | 読み取り関数 `model_position_since(line, holder) -> Option<u64>` を追加。規則からは呼ばない |
| consensus-core `api` | `PalwModelPositionsReadV1 { tip_daa, rows }` と `PalwModelPositionReadV1 { line_id, units, holding_since_daa, tenure_daa, tier }`。`at_tip(state, holder)` は chain 自身の `model_position_tenure` と `model_benefit_tier` を tip の DAA で呼ぶ。`ConsensusApi::palw_model_positions_read_v1` を**新しく追加**する（default は空）。既存の `palw_model_positions_v1` には触れない。pb3 がその直前に hunk を入れるので、衝突を避けるため |
| consensus（node） | `palw_model_positions_read_v1` の実装。同じ cached tip snapshot から `at_tip` を読む |
| consensusmanager session | `palw_model_positions_read_v1` の wrapper を追加する |
| rpc-core wire | `RpcPalwModelPosition` を v2 にし、`holdingSinceDaa?`、`tenureDaa`、`tierIndex?`、`tier?`（`RpcPalwModelBenefitTier`）を追加。`GetPalwModelPositionsResponse` を v2 にし、`tipDaa` を追加。どちらも末尾に追加する。v1 のデータは fail-closed で読む（clock なし、tier なし、tip 0）。JSON の新しいフィールドには `#[serde(default)]` を付ける |
| gRPC | proto の `RpcPalwModelPosition` に 3〜6、`GetPalwModelPositionsResponseMessage` に 3 を追加。変換を両方向で行う |
| service | 行ごとに詰める。tier の変換は `rpc_palw_model_benefit_tier` に切り出し、line のカードと同じ書き方にする |

**consensus の変更はありません。** fold、state root、carriage、fingerprint、fence のいずれにも触れていません。追加したのは `&self` の読み取り関数 2 つだけです。t11 の state と挙動は変わりません。

**互換性:**
- Vec の要素は長さ付きの payload で送られるので、旧 reader は追加フィールドを行ごとに読み飛ばします。テスト `a_version_one_reader_still_reads_a_version_two_frame` で固定しています。
- 旧ノードの応答は tier `None`、`tip_daa` 0 として読みます。gateway はこれを「付与しない」と解釈します。テストは `a_version_one_writer_reads_as_no_clock_and_no_tier` です。
- op は増やしません。

**テスト:**
- `palw_state_v2::tests::model_benefits::a_holders_positions_read_carries_the_chains_tenure_and_tier`: 次の流れを fold の buy と sell の arm で通し、各時点で tier が `model_benefit_tier` と一致することを確かめます。
  1. 宣言: rung0 は tenure 不要、rung1 は 50 DAA 必要。
  2. seed と buy の直後: rung0、`since=260`、`tenure 0`。
  3. 60 DAA 後: rung1。
  4. 1 unit を sell した後: clock が 330 から再開し、rung0 に戻る（N11）。
  5. 他人の id: 行が 0。
- `rpc-core palw_model_positions_wire_tests` の 3 本: v2 の往復、v1 writer を fail-closed で読むこと、v1 reader が v2 の frame を読めること。v1 reader のテストでは、行の frame に追加フィールドが残り、応答の末尾には `tipDaa` の 8 byte だけが残ることも確かめます。

**他のパッチとの合成:** 同じ 5a559459 の上で、`pb3-lifecycle-gate.patch` の後にも `pb4-sell-payout.patch` の後にも、このパッチはそのまま当たります（どちらも確認済み）。pb3 と pb4 同士は `palw_state_v2.rs:18789` 付近で衝突しますが、これはこのパッチとは関係ありません。

### 3.2 このパッチでやらないこと

これらは次の段階です。どれも consensus の変更は不要です。
- **複数 id（carrier と EVM）を 1 人として合算する RPC。** ADR-0095 N3 にあたります。cdc5cd26 の `getPalwModelBenefitTier` を新しい op 番号で移植するなら、`model_position_across` の重複除去（route matrix の修正順 5）も必ず一緒に入れてください。今回の per-holder の読み取りは長さ 1 の slice しか渡さないので、この潜在バグを外に出しません。
- **challenge の ML-DSA-87 署名 context の定数。** unmerged では `b"misaka-palw-model-benefit-challenge-v1"`。定数がないと、holder の tool と gateway で context が一致することを保証できません。
- **EVM lane の digest。** EIP-191 `personal_sign` で、unmerged では `palw_model_benefit_challenge_evm_digest_v1`。
- **gateway の実装（773f6e7c）と CLI（cdc5cd26 の `misaka palw benefits --nonce --daa`）。**
- **units だけを返す `ConsensusApi::palw_model_positions_v1` と、その session wrapper の削除。** このパッチの後は呼び出し元がありません。pb3 と合成した後に消してください。

### 3.3 provider 側の検証レシピ（このパッチの後に使えるもの）

1. **line を決める。** `getPalwModelLine(L)` から `benefits.tiers`（その時点で有効なもの）、`lapsed`、`tip_daa` を読みます。`benefits` が `None` か `lapsed` がある場合は、誰にも何も付与しません。
2. **challenge を発行する。**
   - nonce は 32 byte の乱数で、1 回だけ使い、TTL を付けます。
   - `daa` は発行時点の tip の DAA（T0）です。
   - `network_domain` は `palw_network_domain_v2_for(<network 名>, Some(genesis hash))` で、CLI の `bond::network_domain` と同じものです。
3. **保有者が署名する。**
   - 署名対象は `palw_model_benefit_challenge_v1(network_domain, L, holder_id, nonce, T0)` の 64 byte です。
   - carrier lane では、position を持っている ML-DSA-87 鍵で署名し、`public_key` と `signature` を送ります。
   - EVM lane では `personal_sign(0x<64 byte>)` で署名し、`address` と `signature` を送ります。
4. **gateway が id を導出する。** request に書かれた holder id は信じません。
   - carrier lane は `palw_model_holder_of_pubkey_v1(public_key)`。これは keyless の `mldsa87_key_id` で、fold が position の key にしているのと同じ関数です。
   - EVM lane は `evm_holder_v1(chain_id, recovered_address)`。
   - B4 の修正で holder id の定義が変わった場合は、fold の key に合わせてください。RPC は常に fold と同じ key で答えます。
5. **署名を検証する。** challenge を gateway 側で計算し直してから検証します。nonce は検証の前に消費します。失敗した証明を使い回されないようにするためです。
6. **保有を読む。** `getPalwModelPositions(holder_id)` を呼び、次を確かめます。
   - `tip_daa ≥ T0` であること。`tip_daa == 0` は旧ノードなので拒否し、別のノードに問い合わせます。
   - `lineId == L` の行があること。その `tierIndex`、`tier.grants`、`tenureDaa`、`holdingSinceDaa` を使います。
   - `holdingSinceDaa ≤ T0` なら、challenge の時点で保有が続いていたことが分かります。
7. **1 人が複数の id を持つ場合（N3）。** 証明された id を重複なしで集め、`units` の合計と、保有中の id の中で最も短い `tenureDaa` を求めます。それを使って、**同じ tip で読んだ** `getPalwModelLine(L).benefits.tiers` に `tier_for_units` を適用します。各応答の `tipHash` が一致しなければ（`tip_daa` の一致だけでは reorg をまたげません）読み直してください。
8. **grants に従って serve する。** たとえば `PRIORITY_INFERENCE` なら優先キューに入れます。`INFERENCE_QUOTA` の単位は tier の note に書かれています（consensus は関与しません）。
9. **信頼の前提。** RPC の答えは証明ではありません。provider は自分のノードに問い合わせるか、複数ノードで照合してください。position は譲渡できないので、証明を貸すことは自分の枠を代理で使うことにしかなりません（ADR-0095 A6）。

## 4. Q1: B2 の消費者の選択肢

| 案 | 内容 | consensus | fence | 工数 | 備考 |
|---|---|---|---|---|---|
| **C1 chain 外の provider**（ADR の本線） | このパッチ、`model_position_across` の重複除去、署名 context と EVM digest、773f6e7c の gateway check（証明は op 172 の v2 で読む。新しい op を足すなら 177 以外の空き番号）、CLI の証明署名 | なし | 不要 | 中 | ADR-0101 の ADR-0144 alignment は membership check（step 1）を残している。serving の marketplace（D4、D6）は撤回されたまま |
| **C2 chain 上の `HOLDER_VOICE`**（§4.9、ADR-0101 D7 step 5） | evaluation と proposal に、書いた bond の holder id の tier を `by_holder_tier` として fold が記録する | **あり**（state root の行が変わる） | `palw_audit_2026_09_23` で t12 だけ有効にし、t11 は変えない | 中〜大 | **決めることがある:** bond の署名者を holder id にどう対応させるか（`palw_model_holder_of_pubkey_v1(bond の登録鍵)` か）、B4 修正後の id 定義、EVM lane の holder を除外するか |
| **C3 仕様として受け入れる** | 受け入れ基準の「プロトコルが定めた特典を受ける」を、「プロトコルは membership を証明し、配布は chain の外で行う」と読み替える。ADR-0095 §7 の step 5 と 6 は未完了と明記する | なし | 不要 | 小（文書のみ）＋このパッチ | route matrix の P11 と P12 は「消費者は chain の外、RPC で公開済み」に変わる |

私の見立てでは、**C1 と C3 は両立し、ADR にも忠実です。** C2 は chain 上の特典が本当に 1 つ要る場合の追加で、state root が変わるので t12 限定の fence を使う必要があります。

## 5. Q2: ADR-0091 の buyback が生む値上がり益と ADR-0095 の矛盾

**事実:**
- ADR-0091 では、PALW Final ごとに claim の escrow の 5% が line の pair の reserve に入ります。K は落ちず、position の価格が上がります。
- 保有者には何も支払われません（B4 の意味で「paid」はない）。しかし**売り戻したときの受取額は、他人の mining から来た slice の分だけ増えます。**
- ADR-0091 §4 自身の例: 1,000 MSK で 4,656 position を買います。1 年分の slice（subsidy が 100 MSK/block と仮定）の後に全部売ると、net で **1,147.74 MSK** を受け取ります。slice がなければ、すぐに売り戻すのと同じで、2 回の leg を引いて **約 884 MSK** です。差の約 264 MSK（元本の約 26%）は miner の 5% から来ています。
- この上がり幅は保有 units に比例します。「保有量に比例して利益を受け取るのではなく」（ADR-0095 §1、運用者の言葉）とも、§0 の「a position buys no income」ともぶつかります。
- ADR-0091 の動機は「分配より証券から遠い」ことでした。しかし他人の努力から来る値上がりへの期待は、分配と同じく証券性の論点になります。route matrix §3.4 はこれを設計上の緊張として挙げています。

**選択肢:**

| 案 | 内容 | 値上がり益 | 「使われたモデルは membership が高くなる」 | consensus / fence | 備考 |
|---|---|---|---|---|---|
| **T1 現状を維持し、開示する** | ADR-0091 と 0095 の文言を正直に書き直し、「保有中は何も受け取らないが、使われた line では売値が上がる」と site と CLI の trade 画面に明記する | 残る | 残る | なし | 最小。ただし ADR-0095 §0 の一文は偽のまま |
| **T2 slice を売り手に渡さない** | slice は reserve に入るが、seed と同じく永久にロックする。buy の quote は全 reserve で計算し、使われるほど入会価格が上がる。sell の quote は `reserve − buyback_sompi`（drill がすでに計算している `traded`）で計算する | **消える**（自分の払った MSK から leg を引いた額までしか戻らない） | **残る**（新規参加者の価格にだけ効く） | あり。sell の quote、M1/M2、EVM window の説明を変える。`audit_2026_09_23` で t12 のみ | ADR-0091 と ADR-0095 の両方の意図を保てる唯一の案。buy と sell の非対称（spread）が時間とともに広がる。仕様を詰めて drill で確認する必要がある |
| **T3 slice を燃やす** | miner は 95%、5% は mint しない。market の行には書かない | 消える | 消える（MSK 全体のデフレになる） | あり、小（`finalize_claim` だけ） | ADR-0091 Decision 3 の「nothing is burned for the model」を覆す |
| **T4 buyback を 0 にする** | `PALW_MODEL_BUYBACK_PERMILLE` を t12 で 0 にし、miner が 100% を受け取る | 消える | 消える | あり、最小 | ADR-0091 を t12 で事実上撤回する |
| T5 slice を line の運営か provider に払う | — | 消える | 消える | あり | ADR-0088 §10 で no と決まっており、ADR-0144 P1 の non-goal にもあたる。**推奨しない** |
| T6 tenure の条件を付けて売却時に減額する | — | 残る（遅れて実現する） | 残る | あり | income であることは変わらない。**推奨しない** |

私の見立て:
- 「使われたモデルは membership が高くなる」ことに価値を置くなら **T2**、単純さを取るなら **T3** か **T4** です。
- どの案でも t11 は ADR-0091 のまま動かしてください。変更は `Params::palw_audit_2026_09_23`（t12 のみ）の内側に閉じ込めます。

## 6. 関連する注意点

- **B4 との関係:** carrier lane の holder id は keyless の BLAKE2b(pubkey) で、使える payout payload（keyed）とは別物です。`GetPalwModelPositionsRequest.holder` と rpc.proto の doc comment は、この段階で直しました。
- **P11 の潜在バグ:** `model_position_across` は重複した id を二重に数えていました。修正済みです（production の呼び出し元がないので consensus には影響しません。source pin で固定）。
- **CLI:** `misaka palw model-positions` はまだ units しか表示しません。tenure と tier の表示は C1 の CLI 移植と一緒に行う想定です。このパッチでは CLI に触れていません。
