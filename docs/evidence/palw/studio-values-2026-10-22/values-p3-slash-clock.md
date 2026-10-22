# 値の決定 lane P3 — V6・V7・V8・V9・V10(slash と時計、2026-10-22)

状態: **決定済み・独立検証待ち。** ユーザーは 2026-10-22 に値の決定を委任した([手順](../../../design/palw/integration/mac-studio-values-and-measurements-2026-10-22.md) §0)。本書は lane P3 の決定と経済監査である。台帳(`parameter-register.md` / `parameters.json`)への記録、arming record の pin、fence の武装は **していない**(独立検証の後に lead が行う)。コードの `None` には値を入れ、各定数に「decided, awaiting independent verification」と書いた。

- branch: `values/p3-slash-clock`(base `g14/integration-r7` `34d68d7e2`)
- 機材: Apple M1 Max(この Mac)。最低要件の機械での再計測は未実施。
- lane P1 の新しい実測(`values/p1-measure` の `measurements.md`、M-X3)は、本書の作成時点で push されていなかった。V10 の `B_check`・`B_cold_fetch` などは、下に書いた換算倍率と安全余裕による保守的な値である。P1 の M-X3 が出たら差し替える(依存関係の表)。
- 計算の正しさの暗号学的証明は使わない(PRINCIPLES §8)。V8 の multiproof は hash の同一性だけで、court は再実行する。モデル単位の審査は導入しない(§9)。D2 readiness は seat が自分で署名したデータの commitment に対する応答義務であり、モデルの審査ではない。

## 0. 使った入力

| 入力 | 値 | 出典 |
|---|---|---|
| t12 の DAA ペース | 中央値 184 s/DAA、観測最大 240 s、最小 144 s | [2026-10-19 passive](../../../measurements/2026-10-19-t12-passive-daa-blocks.md) |
| 換算に使う DAA ペース | **120 s/DAA**(block の目標時間。184 s より速いので、秒で決まる作業が予算より多くの DAA を使うことはない) | `BlockrateParams::new_two_minute_bps` |
| block の tx bytes | 最大 111,949 B(容量 125,000 B/DAA) | 同上 |
| 1 transaction の上限 | 120,000 B(transient mass = size × 4 ≤ 480,000)。chunk 1 part = 100,000 B、framing 1.2 倍で数える bytes は 83,333 B | `palw_mode_v2::palw_close_bytes_for_chunks_v1`、`PALW_OBJECT_CHUNK_MAX_BYTES` |
| close の最大 | 27 part(`DEFAULT_MAX_CLOSE_BYTES`) | `palw_mode_v2` |
| carrier の取り込み遅延 | mempool 初観測→chain 受理 15.1〜685.5 s、中央値 179.7 s(90 件。510 件は右打切り、1 tx/s で滞留) | [2026-10-22 devnet carrier](../../../measurements/2026-10-22-devnet-payload-carrier.md) |
| 手数料 | 署名済み DTO 24,920 B の tx に 1,244,950 sompi(= 約 49.96 sompi/B)。送信側が選んだ実額で、網の最小 fee ではない | 同上 |
| court 検証 CPU(fixture class、M1 Max) | close 中央値 0.35 ms・p99 3.3 ms(証拠 10,753 B)、root claim 1.36 ms、bottom 1.11 ms | [2026-10-19 court verify CPU](../../../measurements/2026-10-19-court-verify-cpu.md) |
| t12 の finality depth | 43,200 s ÷ 120 s = **360** | `FINALITY_DURATION`、`new_two_minute_bps` |
| t12 の窓 | `W_disclose` = `window_challenge` = 1,200、`window_court` = 3,000、bind 600・receipt 600、`window_challenge_at(16,000)` = 120 | 試験 `p3_v9_*` / `p3_v10_*` の出力(§6) |
| court clock v2 | notice `a ≤ d ≤ a + 3,120`、延長後の hard `a + 10,000`、T06 残余 6,880 | RFC-0026、`palw_court_clock_v2` |
| ML-DSA-87 署名 | 4,627 B | `MLDSA87_SIGNATURE_LEN` |
| finder の取り分上限 | 490‰ | ADR-0032 |

## 1. 決定値の一覧

| # | 値 | 決定 | 縛っている制約 | 余裕 |
|---|---|---|---|---|
| V6 | `PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1` | **規則 `ClaimReserve`**: その claim の reserve(funded v11 `claim.reserved_original`、text29 `maximum.producer_reserve`)を、bond がまだ持つ分まで(G-14) | 既存の default・conviction と同額であること(沈黙を答えより安くしない/他の claim の担保に手を付けない) | 等号(同額が条件) |
| V7a | `PALW_D2_READINESS_DRAW_CAP_V1` | **64** | 見逃し率 `(1−f)^k`: f = 10% で 0.12%、f = 5% で 3.8%(G14 の許容 1〜10%) | f = 10% で目標 1% に対し約 8 倍 |
| V7b | `PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1` | **1,300,000 sompi / demand** | 正直な seat の 1 回の応答の運搬費(約 12,672 B × 49.96 ≈ 633,000 sompi)を下回らない | 2.05 倍 |
| V7c | `PALW_D2_READINESS_CONVICTION_SLASH_SOMPI_V1`(新設) | **300,000,000 sompi(3 MSK)/ root** — ただし **適用する fold はまだ無い** | 全 64 座標を要求した challenger の費用(約 122.5 M sompi)を 490‰ の取り分で回収できる(≥ 250 M) | 1.2 倍 |
| V8a | `PALW_TIR_POSITION_WITNESS_BUDGET_V2` | **83,333 B**(= chunk 1 part) | 1 位置 terminal が 1 transaction に収まる | — (上限そのもの) |
| V8b | `PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2` | **3,072 値** | 最悪の framing(24 B/値)でも tag 203/204 が 1 part に収まる(78,505 B ≤ 83,333 B) | 1.06 倍(最悪 framing)、1.5 倍(通常の 16 B/値) |
| V9 | staged court config(`PalwStagedPublicCourtConfigV1::network_v1`) | **collection 4,096 → 7,104、reorg_retention 256 → 360**(acquisition 4,096 は不変) | material が `a + 10,000 + max(W_disclose, reorg)` = `a + 11,200` を覆う | 360 DAA(material は少なくとも `a + 11,560`) |
| V10 | X3 の Δ | **B_cold_fetch 512、B_check 512、B_localize 512、B_carrier 120、B_reorg_slack 360(Δ = 2,016)— 目安(provisional, user 2026-10-22)、改定の契機は P1 の M-X3 と最低要件 rerun** | T06: `4,200 + Δ + 360 + 1 ≤ 6,880` | T06 の残り 303 DAA |
| V10 | `c_retain` | **36,058 sompi/DAA — 目安(provisional, user 2026-10-22)** | `⌈2,250,000 B × 50 sompi/B ÷ 3,120 DAA⌉`。改定の契機: M-DA と V5 | — |

**目安の扱い(ユーザー指示 2026-10-22「パラメーターは目安で決めていい　方針をあとから変更にする　pinまで進めて」)。** 実測が無かった値は、手元の最良のデータ(設計値・コード定数・既存実測)から保守的な目安を作ってコードに入れた。目安の値は V10 の 6 項すべて(Δ の 5 項と `c_retain`)。各値に根拠と改定の契機を書いた。V6〜V9 は導出に必要な入力が揃っているので目安ではない(決定・独立検証待ち)。lane P1 の M-C(負荷下、`studio/values-p1-measure`)では position close の最大が 33,629 B(V8a の予算 83,333 B 内)、D2 answer は draw 16 で 396 B・1 段 64 B 増(V7b の見積り: draw 64 で 524 B)で、本書の値と矛盾しない。

## 2. V6 — Final 後 default の slash

**定義元。** ADR-0184 §1(有罪または default のとき、未払いの手数料は requester へ返金し、担保は slash する)、RFC08 08-B、決定 D4(供給ありなら default、なしなら no-slash 保持)。`refund_native_fee_v1` の doc は「slash は family のもの(pre-Final の有罪と同じ回収可能な reserve)」と書いている。

**既存規則との整合。** funded v11 の pre-Final DA default(`palw_flat_execution_da_fold_v11.rs`)は `palw_native_producer_collectible_v1(collateral, claim.reserved_original)` を回収する。text29 の conviction(pre/post-Final とも)は `maximum.producer_reserve` まで回収する。Final 後 default も同じ額にする。固定額にはしない:

- 固定額が reserve より小さいと、不正な producer は答える(→ 有罪で reserve を失う)より黙る(→ 小さい slash)ほうが得になる。PRINCIPLES §10(slash の損失は不正の利益を上回る)に反する。
- 固定額が reserve より大きいと、bond budget が他の claim のために予約した担保に届く。持ち逃げの上限(回収可能な価値)を claim ごとに予約する担保基準とずれる。

**式。** `slash = min(reserve_of(family, job), bond.collateral)`。reserve は family の anchored book から読む(`palw_flat_execution_fold_v11::native_held_final_reserve_v1`、text29 は `held_final_reserve_v29`)。読めないとき(book が無い・Final でない)は default しない(返金も slash もしない。H で slash なしに解放される)。

**経済監査。**

| 立場 | 収支・資本・待ち |
|---|---|
| 不正な producer | 利益: 保持中の手数料は返金されるので 0(chain 外の利益は cap の範囲)。資本: reserve を H まで拘束。発覚時の損失: reserve 全額 + 手数料。答えても黙っても損失は同じで、沈黙は逃げ道にならない |
| 自作自演(producer = requester) | 返金は自分に戻るが reserve は burn される(post-Final default は finder に払わない)ので純損失 |
| 正直な producer | 収入: H で手数料。資本: reserve を Final + `window_court` まで。待ち: 約 3,000 DAA(184 s/DAA で約 6.4 日)。slash されるのは、供給が記録された応答窓をすべて沈黙したときだけ(no-slash 保持は供給なしのとき) |
| 告発者 | Final 後 default は finder を払わない(状態から客観に決まる)。偽の告発で正直な producer は答えるので slash されず、告発者の利益は 0 |
| requester | default で手数料が全額返る |

**保証すること。** 沈黙の損失 = 有罪の損失。slash は claim の reserve を超えない。供給なしなら slash しない。
**保証しないこと。** bond が他の slash で減っていれば回収は残高まで(G-14 の既存規則)。本番の作動は court service profile(V1、lane P2 の判断)が pin されるまで休眠のまま。供給 record が無い family(D4 で記録済み: text29 / funded11 のみ対象)。

**依存。** V1 `PALW_COURT_SERVICE_PROFILE_V1`(`court_service_f02_active_v1`)。告発の費用・告発者への支払い(V1/V4、escrow)。

## 3. V7 — D2 readiness の draw 上限・開示料金・有罪時 slash

**定義元。** ADR-0188 B(Merkle root と認証 path、上限は policy の名前付き値)、ADR-0200 C1(F02 の digest/path 開示 court)、RFC-0033 §2。

### V7a draw 上限 k = 64

beacon が選ぶ互いに異なる k 個の inventory piece で、piece の割合 f を持たない seat を見逃す確率は `(1 − f)^k`。

| k | f = 10% | f = 5% | f = 1% |
|---|---|---|---|
| 32 | 3.4% | 19.4% | 72.5% |
| **64** | **0.12%** | **3.8%** | 52.6% |
| 128 | 0.0001% | 0.14% | 27.6% |

lane P1 の実測(負荷下): demand open 41 ns、answer 1.9 / 3.1 / 4.3 µs(draw 16 / 256 / 4,096)、期限 sweep は open case 16 / 256 / 4,096 件で block あたり 0.7 / 8.7 / 207 µs(超線形)。sweep が超線形なので、root あたりの case を 64 に抑える上限は liveness の点でも小さいほうがよい(root が増えれば case 総数は増えるので、root 数の上限・料金は別に要る — escrow/予約の lane への申し送り)。64 は f ≥ 5% の欠落を G14 の許容(見逃し 1〜10%)の内側で検出する。開示 path の深さは 6(answer の path 456 B)。RFC-0033 §2 の「全許容 draw 座標の予約」は root あたり最大 64 件の要求で有界になる。1% の欠落は 1 draw では捕まえられない(span ごとの新しい draw の繰り返しで累積する)。128 にすると予約と seat の応答負荷が 2 倍になる一方、f = 10% の検出はすでに 99.88% あるので、64 を選んだ。

### V7b 開示要求の料金 1,300,000 sompi

seat の応答 1 件の運搬: answer 本体(root key 64 + ordinal 4 + path 456)+ 署名 4,627 + object framing ≈ 5,160 B。tx の固定部分(input の ML-DSA-87 署名・公開鍵・出力)は devnet 実測の DTO 24,920 B − payload 17,408 B = 7,512 B。合計約 12,672 B × 49.96 sompi/B ≈ **633,000 sompi**。料金はその 2 倍を切り上げて 1,300,000 sompi。

| 立場 | 収支 |
|---|---|
| 嫌がらせの challenger | root あたり最大 64 要求 × (料金 1.3 M + 自分の要求 tx 約 0.61 M)。料金は現状 free collateral の検査だけ(移動は escrow lane)なので、実費は要求 tx の運搬費だけ。seat の応答費と 1:1 程度で、増幅はない |
| 正直な seat | 応答 1 件 ≈ 0.63 M sompi。料金が実際に移動するようになれば(Disclosed で seat が受け取る)黒字。root あたりの最悪の応答量は 64 × 12.7 KB ≈ 811 KB(125 KB block で約 7 block) |
| challenger の必要資本 | 全座標を要求するとき 64 × 1.3 M = 83.2 M sompi(0.832 MSK)の free collateral |

### V7c 有罪時の slash 300,000,000 sompi(3 MSK)— 未配線

`PathConvicted`(seat が自分の root で反証される path に署名した: 正直な seat には起こらない)と `DisclosureDefaulted`(供給された要求に期限まで答えない)に適用する額。下限: 全 64 座標を要求した challenger の費用 64 × 1.914 M ≈ 122.5 M sompi を 490‰ の finder 取り分で回収できること → ≥ 250 M。切り上げて 300 M。seat の bond(Panel floor 案 240,000 MSK)に比べて十分小さく、持ち逃げの上限の基準も満たす。現在、偽の root で seat が得るものは 0(ADR-0200: root だけの ready/slash の入口は無い)なので、slash は不正の利益を上回る。

**重要: この slash を適用する fold はまだ無い**(lane r5-d2-c1 §4: C1 court は standing を記録するだけ、料金も移動しない)。定数は値の置き場で、作動中の罰則と読んではならない。

**保証しないこと(V7 全体)。** RFC-0033 §2 の「root の受理前に全分岐を原子的に予約する」は未実装。上限を採択したので、D2 の fence を武装すると tag 230 の root は予約なしで受理される(root は ready credit を与えないので直接の資金被害はないが、RFC-0033 の受理条件は満たさない)。**予約・料金の移動・slash の配線が済むまで D2 を武装しないこと**を lead に推奨する。default は供給前提(V1 profile)が pin されるまで `NoSupplyHold` のまま。

**依存。** V1(供給前提)、escrow lane(料金の移動・slash の配線)、RFC-0033 §2 の予約。

## 4. V8 — 1 位置 terminal の witness 予算・lookahead の frontier 上限

**定義元。** RFC-0024 §4.4–4.5(`B_position` は全座標・root・opening・署名・codec framing の合計、約 12 KB は見込みで上限ではない、超過した正当な witness を slash しない)、§5.5(`r`・最大 body・carrier 分割数を profile から導出、`saved_wait > added_cost`)。

### V8a `B_position` = 83,333 B

1 位置 terminal の目的は最後の裁定を小さくすることなので、予算を「chunk 1 part に収まる数える bytes」= `palw_close_bytes_for_chunks_v1(1)` とした。これで terminal は必ず 1 transaction になり、door の計算に加わる carrier は 1 単位で済む。

| 内訳(r5-p2-terminal §3 の導出値) | bytes |
|---|---|
| 固定部分(署名 2 本 9,262 B を含む) | 9,466 |
| w = 16 の例の position round | 3,548 |
| 72 leaf・深さ 12 の multiproof | 10,068 |
| dissection-bottom の運搬に残る分 | **60,251** |

fixture の実際の position close(`palw_tir_position_terminal_v2` integration test、90 件)は 25,752〜33,555 B で、すべて予算内で受理された。lane P1 の実測(負荷下)でも最大 33,821 B(署名 2 本を含む、予算の 41%)、lookahead の展開は 0.63 µs。

- 予算を超える正当な witness は MOVE として拒否され(`PositionWitnessOverBudget`)、slash されない。whole-tile close(最大 27 part)がその経路として残る。正直な参加者の損失は door 時間だけ。
- 攻撃者が巨大な witness を送っても、評価の前に byte 数で拒否される。検証 CPU は 1 part で有界: fixture の close 検証 p99 3.3 ms / 10.7 KB を bytes に比例させると 83 KB で約 26 ms(外挿の推定で、実測ではない)。
- 保証しないこと: §4.4 の `PositionOperandResolverV2`(writer row の解決)は未実装で、witness は既存の whole-leaf の運搬を `[p,p+1)` に制限して使う。writer leaf が大きい profile では position close が予算を超え、whole-tile close に戻る。実モデル profile ごとの honest witness の最大は未計測(M-C)。

### V8b frontier 上限 r = 3,072 値

tag 203(disclosure)と tag 204(disclosure + path)を 1 part に収める。

| frontier の形 | disclosure | tag 203 | tag 204 |
|---|---|---|---|
| 最悪 framing: 16 群 × 192 claim × 1 要素(24 B/値) | 73,798 B | 78,493 B | 78,505 B |
| 2 段の形: 16 × 16 claim × 12 要素(各 1 値) | 62,534 B | 67,229 B | 67,241 B |

(上の行は試験 `p3_a_lookahead_at_the_frontier_bound_fits_one_carrier_part` が borsh で実際に数えて 1 part に収まることを確かめる。)

- §5.5 の条件: 1 part の運搬(1 block 前後)と i128 の fold(µs)は、別々に応答した場合に 1 回分減る応答 turn より短い。上限の内側なら depth 2 は時間最適化になる。
- 上限を超える profile(256 claim × 要素 12 個超)は lookahead を拒否され、V1 の 1 段 round が経路として残る(slash ではない)。その最悪経路が door 3,200 に収まるかは profile ごとの door 予算の問題で、本 lane では扱わない。

**依存。** `palw_tir_bounded_door_v1` の武装と frame cost record(R6)。M-C の実モデル profile の witness bytes。

## 5. V9 — staged court config の保持期間

**定義元。** r5-clock-deps-relay §3: clock v2 の延長(dispute record の作成)は、すべての依存の horizon が `a + 10,000 + max(W_disclose, reorg_retention)` 以上でなければ原子的に拒否される。staged route の material(`a + acquisition + inclusion + collection + reorg`)は `network_v1` の 4,096 / 4,096 / 256 では `a + 8,448 + inclusion` で、`a + 11,200` に届かない。このままでは governed な staged claim の延長がすべて拒否される(紛争が延長できない = 不正が 3,120 で逃げ切る)。

**決定。**

- `reorg_retention_daa` = **360**: t12 の finality depth。reorg はこれより深く court object を取り消せない。旧 256 は finality depth より短かった。
- `collection_daa` = **7,104**: `acquisition + collection = 11,200 = 10,000 + W_disclose(1,200)` で、inclusion 項が 0 でも collect_until は `a + 11,200` 以上、material は reorg 項を足して `a + 11,560` 以上。必要量に対し 360 DAA の余裕。
- `acquisition_daa` 4,096 は変えない(開始の door。notice の 3,120 を覆う)。

T06(clock v2 の検査)は reorg 項 360 で、X3 なしの場合 `1,200 + 3,000 + 360 + 1 = 4,561 ≤ 6,880`。

**経済監査。** 正直な producer の staged material の保持が約 3,112 DAA 延びる(184 s/DAA で約 6.6 日、120 s/DAA で約 4.3 日)。bond の退出も同じだけ遅れる。この費用は staged court の rent(価格は本 lane の対象外)で払われる。攻撃者側の利得は無い: 延長が拒否されなくなるので、短い custody で紛争を打ち切る経路が消える。liveness: claim あたりの保持行数は `maximum_owned_rows_per_reserver = 1` で有界のまま。

**試験で分かったこと(正直な記録)。** rendering plan の inclusion 項は大きい: 試した 3 つの plan(prefix/vocab = 4/32、1/2、8/256)で 82,612 / 33,076 / 150,868 DAA。これらの plan では旧 config でも material が `a + 11,200` を覆っていた(r5 の「おそらく短い」は、この 3 plan では当たらなかった)。新しい値の意味は「inclusion 項が 0 でも覆う」(plan に依存しない保証)であり、inclusion の小さい他の staged route(history recovery・selected source など、`palw_staged_bundle_v1` の窓の組み立て)で短くならないようにするためである。逆に、1 token の prefix でも material が約 33,000 DAA(184 s/DAA で約 70 日)保持されるという inclusion の大きさは、保持費の観点で別に見直すべき点として lead に申し送る(本 lane の値ではない)。

**保証しないこと。** `W_disclose` が 1,200 を超える網では 7,104 では足りない(試験は t12 の値で確かめる)。config の値は fingerprint に入っていない(fence の高さだけ)ので、同じ高さで値を変えた binary は区別されない — 武装前に lead が確認すること。EXEC worker・provider lease の horizon は依存として接続されていない(r5 の OPEN のまま)。

## 6. V10 — X3 の Δ と c_retain

**定義元。** RFC-0014 §7.2 補足、ADR-0188(2026-10-18 決定: `resolve = B_disclose + B_court + Δ`、各項と `c_retain` は 0 にしない)、`palw_court_clock_v2_t06_v2`(T06)、`palw_claim_pursuit_may_start_v2`(clock v2 × X3: `start + B_remaining ≤ a + 10,000`)。

**縛っている制約は T06。** t12 では `B_remaining = 1,200 + 3,000 + Δ`、T06 は `B_remaining + reorg(360) + 1 ≤ 6,880`、よって **Δ ≤ 2,319**。clock v2 が治める claim では、Δ を大きくしても開始は notice 窓(`a + 3,120`)で先に縛られる(`B_remaining = 6,216` なら `start ≤ a + 3,784`)ので、正直な challenger の開始を狭めない。Δ の費用は X3 だけが治める claim の保持延長(`c_retain`)だけである。そこで、各項を保守的な見積り以上にしたうえで、T06 に 300 DAA 以上の残りを置いた。

| 項 | 値(DAA) | 導出 | 入力の状態 |
|---|---|---|---|
| `B_cold_fetch` | 512 | 最大の genesis モデル + claim material ≤ 161 GB を、仮定した最低帯域 50 Mbit/s で取得 = 25,760 s = 215 DAA(120 s/DAA)→ × 2 → 切り上げ | 帯域は仮定。実測なし(M-X3) |
| `B_check` | 512 | claim 1 件の検査を、4,096 token を 16 token あたり 60 s(35B-A3B の既存観測の上端)= 128 DAA とし、最低要件ハードウェアの未測定倍率 × 4 | 代用値。最低要件での実測なし |
| `B_localize` | 512 | lane P1 の実測(負荷下)の最大 462 DAA(owner の 11 turn × turn 期限 42 DAA)を切り上げ(1.11 倍)。最初の見積り 288(12 手 × 遅延 6 DAA × 2 × 2)は P1 の最大を下回ったので捨てた | P1 実測(fixture class、負荷下) |
| `B_carrier` | 120 | close 27 part を 2 DAA に 1 part(block の半分 = court の予約枠 R = ⌊cap/2⌋ を想定)= 54 DAA + 観測最大遅延 6 DAA、× 2 | 予約枠は V4/V11(lane P2)に依存 |
| `B_reorg_slack` | 360 | t12 の finality depth。取り込んだ object は final になるか、この範囲で再取り込みされる | 規則値(実測ではなく上限) |
| **Δ** | **2,016** | | |

lane P1 の M-X3(`studio/values-p1-measure` 13072773c、`measurements.md`、すべて負荷下・fixture class)との照合: `B_cold_fetch` 実測 1 DAA(計算 ≤ 94 ms、転送を含まない)、`B_check` 1 DAA(全 leaf ≤ 277 ms)、`B_localize` 11〜462 DAA、`B_carrier` 1〜3 DAA、`B_reorg_slack` 未実施(restart/reorg drill が要る)、`c_retain` の価格は未実施。Δ(reorg 除く)の実測は 16〜467 DAA。本書の値は各項とも実測の上端以上(`B_localize` だけ実測で引き上げた)。`B_reorg_slack` 360 は、T4 が確かめた「厳密に経済的に得をする reorg の深さ 8 DAA」の 45 倍。実モデルと最低要件ハードウェアの値ではないので目安のまま。

結果(t12、X3 を DAA 16,000 で composition): `B_remaining = 6,216`、T06 の残り 303 DAA、X3 だけが治める claim の hard は `a + 7,536`(§7 の試験出力で確認)。

**経済監査。** Δ が大きいほど challenger が終われる確率は上がり、clock v2 の claim では開始窓を狭めない。X3 のみの claim では producer の保持が Δ だけ延び、その費用は `extra_daa × c_retain` として C_upper と beacon source の material 項に入る(`palw_x3_material_term_with_retention_v1`)。攻撃者は Δ を使って得をできない(期限は受理時に固定、REQ-X3-3)。

**`c_retain` = 36,058 sompi/DAA(目安)。** 定義は claim 1 件の保持費(sompi/DAA)で、本来は `⌈保持 bytes/claim × V5 の sompi/byte ÷ V5 の TTL⌉`。保持 bytes/claim(M-DA)も V5(lane P2)もまだ無いので、目安として:

- 保持 bytes/claim `R` = 2,250,000 B: court の close 1 件が運べる証拠の最大(`DEFAULT_MAX_CLOSE_BYTES`)。court が要求し得る量の上限として使う。
- 価格 `p` = 50 sompi/B: devnet 実測で tx 1 byte に払われた 49.96 sompi を切り上げ(chain 自身の 1 byte の値段)。
- 期間 `T` = 3,120 DAA: その byte が通常負う court 窓。

`⌈2,250,000 × 50 ÷ 3,120⌉ = 36,058`。延長 1,912 DAA の X3 claim なら約 68.9 M sompi(0.69 MSK)を material 項に加える。0 ではないので「未知の費用を無料と読む」ことにはならない(ADR-0188)。**改定の契機:** M-DA の保持 bytes/claim か V5 の料金・TTL が採択されたら、本来の式に置き換える。実際の保持量が `R` より小さい class では過大(payer 側の安全側)、大きい class があれば過小になる — M-DA で確認する。`palw_x3_decided_policy_v1` は目安の値で `Ok` を返す(X3 の武装検査の「0 の項なし」を満たす)。武装は lead の record pin 後。

**保証しないこと。** `B_cold_fetch`・`B_check` は最低要件の機械で測っていない(rerun 待ち)。混雑時の carrier 遅延は右打切りで上限が無い。`B_carrier` は court の予約枠が実際に供給されることに依存する。clock v2 の claim の保持は v2 の `a + 11,200` になり、X3 の価格計算(`palw_x3_retention_extension_at_v1`)はその差を数えない — pricing lane への申し送り。

## 7. 試験(実行結果)

Apple M1 Max、debug profile、`cargo test -p kaspa-consensus-core`(shared target、`nice -n 10 -j4`)。ログは lane の scratchpad に保存(commit していない)。

**lane P3 の新しい試験(決定値での成功経路と拒否経路)— すべて pass:**

| 試験 | 確かめること |
|---|---|
| `palw_tir_position_terminal_v2::tests::p3_the_position_witness_budget_is_one_carrier_part` | 予算 = 83,333 B = 1 part、whole-tile の上限より小さい |
| `palw_tir_position_terminal_v2::tests::p3_a_lookahead_at_the_frontier_bound_fits_one_carrier_part` | 3,072 値の tag 203/204 が最悪 framing でも 1 part |
| `readiness_root::tests::p3_tag230_adopted_draw_cap_admits_64_and_refuses_65` | 64 leaf の root は受理・fold、65 は拒否、見逃し率の算術 |
| `readiness_disclosure::tests::p3_d2c1_adopted_cap_and_fee_record_and_demand` | 採択した cap・料金で record と demand が通り、別の料金は `FeeMismatch`、slash 下限の算術 |
| `r5_f02_wire_v1::p3_v6_the_production_slash_needs_the_claims_readable_reserve` | anchored book が無ければ本番の slash 額は拒否、job v39 は対象外 |
| `r5_f02_wire_v1::p3_v6_the_slash_is_the_collectible_reserve` | 回収可能 reserve の 3 境界と、その額での返金 + slash |
| `palw_court_clock_v2::tests::p3_v9_the_staged_config_covers_the_clock_v2_extension_on_testnet_12` | t12 で material が延長を覆う、reorg 項 = finality depth 360、T06(X3 なし)残り 2,319 |
| `palw_court_clock_v2::tests::p3_v10_the_decided_x3_terms_fit_t06_and_leave_only_c_retain_open` | Δ = 2,016、`B_remaining` 6,216、T06 残り 303、notice 最終日の開始可・翌日不可、Δ を残り + 1 増やすと T06 拒否、目安 `c_retain` で価格付け・0 は拒否 |
| integration `palw_tir_position_terminal_v2`(追記した assertion) | fixture の実際の position close 90 件: 25,752〜33,555 B、すべて予算内で受理。budget なしは `PositionWitnessBudgetIncomplete`、bytes − 1 は `OverBudget`。fixture の lookahead は採択上限内で受理、値の数 − 1 の上限で拒否 |

既存試験で値の採択に合わせて書き換えたもの(production 定数の `None` を前提にしていた拒否の検査は、`None` を明示して渡す形にした — 拒否の検査は残る): `the_position_witness_budget_is_typed_missing_until_adopted` → `p3_*`、`tag230_unadopted_draw_cap_refuses_admission_and_fold`、`d2c1_unadopted_cap_and_fee_refuse_record_and_demand`、`d2c1_registry_is_dormant_below_the_fence_and_refuses_unadopted_values`、`r5_f02_wire_post_final_default_is_dormant_and_production_is_typed_missing`、`r6_d4_silent_supplied_actor_defaults_with_the_test_slash_value`、`no_shipped_build_adopts_a_typed_missing_value`(採択した 4 値の明示 list と一致すること)、kaspad の `palw_fraud_filer` D2 試験(**kaspad は build・実行していない**)。

**回帰の範囲(lib、V9 の config 変更の影響を見るため staged/prefix/history/frame cost/court service/g14 gates/ruling fences などで絞った 340 件):** 332 pass、8 fail、遅い e2e 3 件は 60 分を超えたので止めた(未完)。**8 件の fail は base `34d68d7e2` の source でも同じ 8 件が fail した**(同じ test 名を `--exact` で base に戻して実行: 0 passed / 8 failed)。本 lane の変更による回帰ではない:
`d2c1_answer_discloses_or_convicts_and_is_inclusive_at_the_deadline`、`req_x3_3_no_extension_or_regrant_for_a_new_claim_and_the_old_rules_stay_for_old_claims`、`audit_verify1_b04_*`、`audit_verify1_b05_*`、`g14_native_funded_candidate_fixture_generator_v1`、`material_intake_rooted_store_builds_identical_evidence_to_borrowed_path`、`rooted_service_hold_retains_final_collateral_and_material_past_hard`、`r5_f02_wire_missing_supply_is_a_no_slash_hold_and_the_sweep_resumes_once_supplied`。

**integration:** `court_clock_v2_fold` 5/5、`g14_close_gates` 12/12、`palw_tir_position_round_onchain_v2` 4/4 pass。`palw_tir_position_terminal_v2` 3/4 — fail は `a_two_level_lookahead_and_one_choice_path_equal_the_old_two_rounds` の 324 行(B-03 の「改変した frontier」が明示の大きな CAP で受理される)。本 lane が追記した assertion(248〜260 行)はその前で通っている。この検査は採択値を読まない(`CAP = 1 << 20` を渡す)ので、本 lane の変更と無関係と**コードを読んで**判断した(base での再実行はしていない)。

**最終 source での再実行**(`c_retain` 目安 36,058 と P1 実測による `B_localize` 512 を入れた後、filter `p3_ palw_ruling_fences palw_court_clock_v2 r5_f02_wire_post_final no_shipped_build_adopts d2c1 tag230`): 54 pass / 2 fail。fail の 2 件(`d2c1_answer_discloses_or_convicts_and_is_inclusive_at_the_deadline`、`req_x3_3_*`)は上の base でも fail する 8 件に含まれる。p3 の 8 件はすべて pass。出力: `B_remaining` 6,216 DAA、T06 残り 303 DAA、X3 だけが治める claim の hard `a + 7,536`、V9 の T06(X3 なし)残り 2,319 DAA、t12 の turn 期限 42 DAA。試験名 `p3_v10_..._leave_only_c_retain_open` は `c_retain` を目安で入れる前の名前のまま(中身は目安の `c_retain` で policy が `Ok` になり、0 なら拒否されることを確かめる)。integration 4 本は `B_localize` 変更の前の source で実行した(X3 の Δ を読む integration 試験は `g14_close_gates` の X3 candidate だけで、candidate は Δ を 0 のまま使うため影響しない)。

## 8. 依存関係と未決

| 値 | 依存 | 状態 |
|---|---|---|
| V6 | V1 court service profile(lane P2) | V1 が pin されるまで本番は休眠 |
| V7 | V1、escrow lane(料金の移動・slash の配線)、RFC-0033 §2 の全分岐予約 | slash 定数は未配線。D2 は予約が入るまで武装しないことを推奨 |
| V8 | 実モデル profile の witness bytes(M-C)、frame cost record(R6) | fixture では予算内 |
| V9 | `W_disclose` ≤ 1,200 の網であること、config の fingerprint | t12 では被覆を試験で確認 |
| V10 | P1 の M-X3(`B_cold_fetch`・`B_check`・`B_localize` の実測)、最低要件 rerun、V4/V11(予約枠)、V5 + M-DA(`c_retain`) | 6 項とも目安でコードに入れた。改定の契機は左の各入力 |
