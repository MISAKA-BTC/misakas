> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 402–538 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): 運用者向け要約（日本語）.
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

## 運用者向け要約（日本語）

**統合時の修正（2026-09-24、IA-1〜IA-15。統合ライン `rcore/int-2` の実装と当日の決定に合わせた修正。運用者から見える変更だけ）**
- panel を引く anchor は、R-core+ の下では attempt ブロックだけです。heartbeat ブロックは anchor になりません。heartbeat なら 1 回 2^24 hash で種を引き直せたためです。
- claim は、slot 以降の最初の attempt ブロックで panel を結びます。結べなければ、そのブロックで `BindTimeout` になります（没収なし）。
- attempt ブロックが来ないまま bind 窓が切れても、同じく没収なしで `BindTimeout` です。heartbeat だけが続く間の claim は、没収なしで void になります。
- 起動後に「anchor ごとの `BindTimeout` 数」と「attempt ブロックの割合」を観測します（O-13）。bind 窓は、測った割合から決め直します。
- 「K 回の heartbeat の後は確定済みの種で anchor する」案は検討しましたが、起動では採りません（§9.2）。
- 875‰ の下限の計算に、producer 自身の重みを両側で入れます。genesis の 8 席の claim でも、1 席が埋まっただけでは止まりません。
- この修正で、Sybil stake で数えた「1 枚目の panel が提出しない場合の最悪」は 7.15M から 6.63M になります。攻撃者の資本の合計で見た閾値は変わりません。
- この 6.63M は、P3 の閾値と jury の 6.63M とは別の数です。
- lock は、買い戻しの上限（E の 5%）を必ず含めた価格で取ります。市場の状態には左右されません。
- 敗訴した challenger は、court の時間の課金で、A-6 が予約した額の最大 2 倍を引かれます（既存の価格です）。
- 不正の G は、不正の時点の値で固定します（liability の記録値を使います）。licence 時点の記録は監査の修正待ちです。
- 金庫行（vesting row）が入りました（`PALW_RCORE_VESTING_ROWS_LANDED_V1 = true`）。Final 後のすべての有罪判決が、S-4 の funnel を通って行を焼却します。
- このフラグが `false` のビルドで R-core+ の network を起動すると、kaspad は起動を拒否します。
- court で黙った（default）producer には、没収に加えて S2 の罰 min(10%·C₀, 3G) をかけます。黙る方が負けるより安くならないようにするためです。court default は通報報酬を開かず、`CourtConviction` の記録も書きません。
- S4 の罰は、seat が lock を持っている間だけかかります。strike の一覧は最大 9 件です。
- DA default で罰を受ける署名者は、full mask の署名者だけです。partial seat は起動時には縛りません。
- 2M は起動時に閉じます（U-D1。attempt も FP も）。開くのは、帰属でき、期限内に証明できると実測で示した flag day だけです。
- pruning 深さは regenesis で約 74,920 DAA に決めます（D_cap 16,000）。M12 の実測は待ちません。
- drill と実測（SEAT-0、M5・M5b・M9・M12、8k の時間計測）は、起動後に公開 t12 でまとめて行います（§8.4）。
- producer の準備判定は、ノードと RPC で 1 つです（`ready_to_produce_v3`）。`getPalwProducerFacts` は v9 で `bond_committed`・`bond_producer_floor_shortfall`・`bond_accuser_exposure` を返します。
- 出荷条件は §8.3「Ship conditions」の一覧です。R-core+ を武装するビルドには、producer の V2 DA 応答（`da_sessions` の duty に `MaterialDisclosedV2` で答える）が必ず入ります。producer への DA-7 の課金はフラグと関係なく有効なので、これがないと誠実な producer が告発のたびに課金されます。

**v3.1 の追記（2026-09-24、post-edits。運用者から見える変更だけ）**
- 8k は licence で replay を解放します。Final まで保持し、静的な上限で止めるのは C7（2M）だけです。
- C7 は「検証窓が 1,000 span 以上のクラス」と、R-core+ の一覧（2M）を合わせたものです。
- escrow を Final まで保持するのも C7（2M）だけです。8k は SR-1 のとおり licence で解放します（U1）。
- 生産するには、差し入れた担保が 13,000 MSK 以上必要です（producer の下限、U2）。
- S0′ を 1 回受けた 13k の producer は、積み増すまで生産できません。
- 今のコードには、bond に担保を足す方法がありません。
- そのため今の「積み増し」は、新しい鍵と新しい operator id で bond を登録し直すことです（B-4）。
- 同じ bond にその場で足す方法を作るかどうかは、§9 の Q11 で決めます。
- 自分の attempt が下限で拒否されても、そのブロックは無効になりません。
- kaspad も同じ判定を使い、「top up <不足額> sompi」とログに出します（P6。S を含むビルドの drill の前に必須です）。
- Final 後に有罪になった FP claim は、executor の bond から上限付きの段階分を slash します（U3）。
- SEAT-R：seat が `Valid` に署名するのは、replay（または S1 の再開）をしたときだけです。
- SEAT-R は F2 の fence と同じバイナリで必ず出荷します。
- SEAT-0 は drill の後、稼働中の t12 に当てます。fingerprint は `c746f07c` のままです。
- SEAT-0 は 1 台ずつ入れ替えます。
- 作業時間帯は先にユーザーに確認します。
- SEAT-0 は launch の系列にも入ります。
- Kimi カーネルのクラスは、admission で拒否します。
- FP の `output_root` の規則を 1 つにします。`OutputMismatch`（10）が FP の model claim も裁けるようになります。
- そのため FP の root と `misaka-palw-derive` の出力は、regenesis で一度だけ変わります。
- model class の canonical job は `(n_ctx/8 − 1, 2)` に固定します。登録者は選べません。
- 4,096 id を超える prompt（2M）の偽りは、新しい矛盾 `PromptNotAnchored`（13）で裁きます。
- prompt 全体を計算し直す `Whole` は、1 ブロックに 1 本までです。
- held-context の class（8k/2M。artifact の held map のことで、C7 の hold とは別です）では、`AttnFused` の step leaf の計算の嘘を、まだ有罪にできません。
- この穴は起動の gate です。2026-09-24 に次のとおり決まりました（§3.9「Decided」）。
- 8k は A-held で塞ぎ、起動の gate に入れます。新しい object `CourtAttnRootClaimedHeld`（tag 57）と C1〜C5 を足します。
- 8k の producer には新しい義務ができます。held の係争が来たら、期限内に object 57 を出さなければ有罪です。engine に windowed builder が要り、実重みの 8k の時間計測 drill を起動前に回します。
- 2M は起動時点では帰属できないので、経済上の上限で受け入れます。未カバーは 1 本 63,599 MSK、同時 1 本、月最大 9.35M MSK です。Final までの保持と RT#2 の没収は残します。**（統合時の修正 IA-12 で置き換え：2M は起動時に閉じます、U-D1。）**
- 期限は「通常の応答」「計算を伴う検証」「経済責任」の 3 種類に分けます。具体的な DAA 値は実測してから決めます。
- SR-10（V3 の補完 door）は M4（私）で入ります。それまで S2 の licence は格上げできません。
- 抽選の重みは、operator の bond 1 つの差し入れ担保（1,000,000 MSK で頭打ち）です。空き担保は「抽選に出られるか」だけを決めます（post-edit 6 の案は採りません）。

**v3.1 で変えたこと（2026-09-24）**
- F1/F2 の名前・型・判別値を監査 session の仕様（`f2f1_spec.md`）に合わせました。fence は `palw_offence_attribution`、虚偽 Valid は `PanelFalseValidV2 = 3`、producer への反証は `ExecutorRefuted = 4`、矛盾は `IdentityMismatch = 9`・`OutputMismatch = 10`・`ForgedOutputTiled = 11`・`LogitsNotStepOutput = 12` です。R-core の DA default と court 有罪の記録は `DaDefault = 5`・`CourtConviction = 6` にずらしました。v22 の layout は 1 つです（§6）。
- **panel の抽選を起動前に stake 加重にします**（Q4 の決定）。ADR-0124 D5 / ADR-0130 の「operator 1 つに 1 票」を t12 で取り消します（§3.14）。
- **F1-M（8k/2M の relabel、tiled/A16 decode の偽出力）と F1c（正しい step 木の上のゴミ logits）を起動 gate に入れます。** model class は genesis から生産します。
- §9 の Q1〜Q8 は決定済みです（Q4 は stake 加重、ほかは v3 の既定値）。
- **v3.1 草案のレビュー（SW-A1〜A6、R1〜R12）を反映しました。** 抽選について変えたのは 5 点です。(1) panel は anchor block の中で、受理と同じ状態から引き、そこで結べなければ結びません（後の block で同じ種を引き直す道を閉じます）。(2) 抽選に出られる stake が登録済み stake の 7/8 未満なら結びません（誠実な seat が埋まったときに、働かない Sybil だけで panel が埋まる崖を、停止に変えます）。(3) 重みは operator ごとに 1,000,000 MSK で頭打ちです。(4) t12 では operator id が一意なので、operator は bond 1 つです。(5) ADR-0147 の admission jury は stake 加重にしません（Q4 は panel の抽選の決定なので、jury は今のままです）。
- 提出された `ObjectiveOffence` の kind 5・6 は名指しで拒否します（fold だけが書きます）。F1-M・F1c の関数と試験の fixture は監査の追補（`f1c_f1m_spec.md` §4-bis）を正とします。

**v3 で決めたこと（2026-09-24 の運用者決定 1〜7、および 3 検証者の指摘の反映）**
- t12 は R-core+ で起動します。bond は、長く slash されうる 1 つの口座です。
- 起動の条件は「F1（F1-M・F1c を含む）〜F4 と stake 加重の抽選が GREEN、fold/reorg/restart のテストが GREEN、Phase 2 の要のテストが GREEN、短い drill」です。**9 日の soak は起動前には行いません。** 公開 t12 の上で観測プログラムとして走らせます（§8.4）。
- 2 回目の ReceiptTimeout の全額没収（#10）は、**X10 を後から fence で武装するまで残します**。上限は付けません（Q1）。無料のタイムアウトにはしません。1 枚目の panel の失敗（RT#1、SR-9、S2 の格上げ失敗）は、どの class でも redraw で、没収しません。
- 無料 prompt の abandon hold（600 DAA、監査 C5）は残します（予約の式に `now_daa` を入れます）。
- 土台は `0533e1de`（`f8c91f19` の 2 つ先。free prompt の commitment を数える前に束ねる修正を含みます）です。Final まで replay を負い、静的な上限で止まるのは C7（2M）だけです（post-edit 5 で訂正。8k は licence で replay を解放します）。生産を止めるという意味ではありません。

**帰責の経路（監査 session が実装。F2 → F1）**
- F2：虚偽 Valid は 1 つの判定関数 `palw_check_panel_false_valid_v2` で裁きます。processor は署名込み、fold は署名なしで同じ関数を呼びます。root で束縛し、`claim_id` とは比べません。V2（full）と V3（segment）の receipt を両方受け、segment の mask で責任を決めます。台帳の鍵は (seat, claim) ごとに 1 つです。fence の後は kind 1 を拒否します。
- F1：claim に `job_identity`（attempt なら運んだ header の execution anchor、FP なら `palw_fp_job_pin_v1`）を記録します。0 は「記録なし」で、決して有罪にしません。受理も拒否しません。
- 他人の job の root を借りた claim は、`ExecutorRefuted{IdentityMismatch}` で有罪になります。DA の開示も同じ規則で拒否されます。
- 金庫行（vesting row）は `job_identity`・`free_prompt`・`trace_root`・`segment_count` を写して持ちます。
- `ProducerWithholding` を Valid 署名者への矛盾として使えるのは、DA で withholding が確定したときだけです。Sampled・Incapable・Unavailable を出した seat には使いません。罰は S4（上限 3G）です。
- evidence を自動で作る kaspad の filer は Phase 2（私）です。

**DA court の作り直し（F3）**
- session は (claim, 告発者) ごとです。panel の seat は生涯上限の対象外で、1 claim あたり 4 本まで。seat 以外は同時 3 本、生涯 16 本までです。
- 告発者が名指しした 1 単位に加えて、chain が 3 単位をくじで引きます。くじは実行済みの範囲の中から引きます。
- licence 後も、Final 後（行が未成熟で、claim 記録がある間）も告発できます。session が開くと行の満期がその期限 + 120 まで延び、lock も同じだけ生きます。1 本の session で全体の支払いが止まることはありません。
- 止まっていた時間を返すのは、panel の seat が開いた session だけです。
- 棄却された告発の費用は「その段階の報酬の基準額 × 10%」（floor 320.10 MSK、上限 13,000 MSK）です。この額は預かりにし、後でその claim が有罪になれば返します。
- DA の報酬は、最初に default した session の告発者に払います。基準は producer の回収額だけです。

**quorum の数え方（F4、私が実装）**
- `Sampled` は quorum にも coverage にも数えず、**served にも数えません**（Incapable と同じく Final まで escrow を保持します）。lock も取らず、Final 後に slash されません。seat への支払いは Valid と同じです（Q3）。
- k は、受け取った receipt から区間ごとに数え直します。V2 の full-replay Valid はすべての区間を覆います。
- S2 は速い licence としては残します。ただし k ≥ 2 に格上げされない限り Final になりません。S2 の licence は第二の時計を進めません。

**panel の抽選（stake 加重、私が実装、§3.14）**
- 重みは operator の bond の差し入れ担保（MSK 単位、1,000,000 MSK で頭打ち）です。t12 では operator id が一意なので operator は bond 1 つです。各 operator は anchor から決まる鍵 `−log2(u) ÷ 重み` を持ち、小さい順に 5 席が座ります。重複なし・operator ごと・整数演算だけです。
- panel は anchor block の中で、受理の検証と同じ状態（その block の sweep の後）から引きます。anchor block で結べなかった claim は後から結ばず、`BindTimeout`（没収なし）になります。anchor の hash ができた後に入力を動かす道はありません。残るのは anchor block を掘る人の掘り直し（1 回ごとに推論 1 回）だけです。
- 抽選に出られる operator の重みが、出られるはずの operator（Active・下限以上・anchor 前に登録・能力あり）の重みの 875‰ 未満なら、抽選は結びません。誠実な seat が lock で埋まって抽選から外れ、働かない Sybil だけが残る状態を、乗っ取りではなく停止にします。
- 空き担保（担保 − 拘束）は重みにしません。働かない Sybil が働く誠実な seat の最大 2 倍の重みを持つからです（閾値が 17.29M から 11.70M に下がります）。空き担保は「抽選に出られるか」の条件として残ります。
- 1,000,000 MSK の頭打ちは genesis の 8 席（各 939,063 MSK）と 130k の Sybil には効きません。大口の誠実な保有者は、1,000,000 MSK 以下の operator に分けて登録すれば重みを失いません。分けると共謀の閾値が大きく上がります（100M を 1 operator にすると上限なしで 73.19M、頭打ちで 19.76M、100 operator に分ければ 258.70M）。
- ADR-0147 の outsider 席も同じ方式にします。admission jury は今のまま（operator 1 つに 1 票）です。重み付けすると、genesis の 8 席が持っていない誠実な class が通りにくくなるためです（130k × 20 の ready で 1 回の監査あたり 0.13、今は 0.88）。S3 の標本の割り当てと ADR-0133 の full seat の割り当ては変えません。
- 1,000,000 MSK 以下なら、分割しても最初の 1 席の確率は変わりません。2 席目以降の得は、下の表がすでに最良の分割（130k ずつ）で見込んでいます。

**罰（m = 3）**

| 行為 | 罰 |
|---|---|
| 2 回目の panel の失敗（X10 まで） | 予約を没収（strike なし） |
| DA で確定した withholding | 予約を没収 + strike。licence 後・Final 後は §3.6 のとおり |
| Final 前の不正（court / ExecutorRefuted） | 予約 + min(10%×担保, 3G) |
| court での default（producer が黙った。統合時の決定） | 予約 + min(10%×担保, 3G)。`CourtConviction` の記録なし、通報報酬なし |
| Final 後の不正（producer） | 行を全焼却 + min(25%×担保, 3G) |
| Final 後の不正（FP の executor。行はない） | min(25%×担保, 3G)（U3。G は liability の値。率は S-4 の実装） |
| 虚偽 Valid（seat。自分の mask が覆う場所の不正に限る） | lock + min(25%×担保, 3G) |
| 二重署名（t12 のみ） | min(担保, 3G) |

- 通報報酬は commit–reveal です（実行を証明する有罪だけが対象）。commit は有罪になった evidence の digest に結びつけます（監査の台帳の鍵は claim ごとに公開なので、鍵だけだと先回りの commit が全部勝つため）。合理的な犯人は自分で先に commit できるので、報酬は提出の動機ではありません。提出は Phase 2 のノードが自動で行います。

**数字**
- floor の予約は 3,200.95 MSK、seat の lock は V1 で 106.74、S1 coverage で 160.11 MSK です。
- duty は floor 256.07、8k 459.12、2M 12,588.76 MSK です。どの class でも「拘束 ÷ 没収」は 1 以下です。
- （この 2 行の lock と duty は `s = 0` の値です。武装後の lock は買い戻しの上限 `s_cap` を含むので、およそ `s_cap / k′` 高くなります（8k の `lock_3` で約 53 MSK、`rcore_whole_gain_and_buyback`）。8k の duty は `lock_2` で決まるので一緒に動きます。再計算はしていません。IA-3）
- 共謀の成功（検出されない）には、嘘をついた区間の担当 2 者（full seat と partial seat）の両方が必要です。どの door でも同じです。
- **stake 加重の抽選では、genesis の 8 席がすべて抽選に出られるとき、130k の Sybil 133 個（17.29M MSK）で初めて EV が正になります。誠実な seat の一部が埋まった最悪の状態（875‰ の下限が許す範囲）でも 12.74M です。** v3 の一様な抽選では 20 個（2.60M MSK）でした。下限がなければ、genesis の席が埋まるにつれて閾値は 0.52M まで下がり、全部埋まれば Sybil 5 個で全 panel が取れます。1 枚目の panel の誠実な seat が何も提出しなければ 8.32M（最悪 6.63M。IA-1b の前は 7.15M）、V1 door で誰も提出しなければ 6.63M（最悪 5.72M）です（一様では 1.30M / 1.04M）（model）。
- 草案にあった「anchor の後で Sybil を退役させて並びを選び直す」約 8.97M は、失敗した抽選を後の状態で引き直す道にだけある穴でした。t12 では anchor block でしか結ばないので、この道は閉じました。

**閉じないもの**
- 2M は不変量を満たしません（ADR-0153 まで）。
- held-context の class（8k/2M。C7 の hold とは別）の `AttnFused` の嘘は、まだ有罪にできません。これは閉じるまで起動を止める gate です（§8.3）。
- 共謀の閾値は、誠実な stake の合計ではなく、誠実な operator の数と大きさで決まります（1 つの 7.51M の operator なら 0.52M しかありません）。genesis の 8 席に対して、community の 858M の中には単独で閾値を超える保有者（100M の行が 2 つ）がいます。運用者の本体財布から、1,000,000 MSK 以下の operator を複数登録すれば閾値は上がります（genesis と同じ大きさで 8 席足せば 35.49M）。§9 の Q10 です。
- 875‰ の下限を割るほど自分の bond を lock で埋めた攻撃者は、binding を止められます（没収のない停止で、V-8 の停止の一種です）。
- admission jury は今のままなので、13k の Sybil 40 個で登録者が自分の class を通せます（既存の穴）。ただしその class の claim には stake 加重の outsider 席が座り、outsider の Valid がなければ licence されません。

**判断が必要な点**は §9 の Q10（誠実な operator を足すかどうか、運用上の判断で、起動は止めません）と Q11（下限を割った producer の積み増しの方法。決まるまでは登録し直し）です。Q1〜Q8 は決定済み、後回しの一覧は §9.2 に残します。

---
