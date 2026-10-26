# kernel route Panel=1 slice の独立レビュー 第 2 部: 告発・default・期限・決済(2026-10-26)

対象: `2119c943b`(`claude/g14-panel1-17000`)+ `1653fc09f`(第 1 部)。武装予定の fence は `PALW_T12_KERNEL_SLICE_FENCES_V1`(`consensus/core/src/config/params/full_activation_v1.rs:302-313`)。
担当: Claude agent。実装 lane の報告は前提にせず、code を辿って結論を出した。試験は targeted に 1 回だけ実行した(結果は §8 と末尾)。
前提: G14 不変条件(producer と Panel 全員が共謀しても、Panel の外にいる public bonded verifier 1 人が、公開認証 material だけで客観的に conviction できる)、検出率 90〜99%、検出後は期限内に slash まで届くこと。
暗号学的な計算証明(ZK など)は、どの修正案にも挙げていない。

## 0. 前提になる事実: slice に入っていない fence

slice の 10 entry は次のとおり。audit_1004、gen_range_twin、model_court_window、receipt_spend_v4、probabilistic_constraints、kernel_route_panel_one、claim_hard_deadline、settlement_once、Gate C(`palw_independently_work_certified_v1`)、g14_route_required。

次の fence は full list にはあるが、slice には**入っていない**(`full_activation_v1.rs:165-202` と `:302-313` の比較)。
- `palw_provider_court_v1`。court scope(告発者ごと・claim ごと 4 件の上限)の fence である。kernel ledger に `CourtScopePolicyV1` として注入される(`palw_kernel_route_fold_v1.rs:396`)。
- `palw_court_clock_v2`。notice a+3,120 / hard a+10,000 の時計である。
- `palw_reward_liability_horizon_v1`、`palw_native_liability_horizon_v1`、`palw_unresolved_bound_v1`、`palw_court_capacity_v1`。
- `palw_model_refutation_seal_v1`(sealed bounty の保留)。これはどの list にも入っていない。

以下の多くの指摘は、この事実から出てくる。

## 指摘一覧

| ID | 重大度 | 観点 | 要旨 |
|---|---|---|---|
| B-1 | BLOCKER | 4 | slice の X3 は `unapproved_probe_at` を入れる(hard = a+300)。しかも X3 は kernel ledger の claim を一切 govern しない |
| B-2 | BLOCKER | 2・8 | G-K1 と e2e の主張が食い違う。canonical は「共謀 seat は課金されない」を assert し、lane closure 試験は「課金される」を assert する |
| M-1 | MAJOR | 1 | slice では court scope(4 件の上限)が無効。junk による block 予算の飽和を経済不等式で閉じていない |
| M-2 | MAJOR | 3 | kernel route の default は deadline だけで決まり、inclusion premise を読まない。post-Final の default は reservation 全額(1,000 MSK)を没収する |
| M-3 | MAJOR | 5 | escrow の残りが 0 のとき、settlement once の triple 照合を素通りする(`fb23f6a09` は未統合で、event 列を変える) |
| M-4 | MAJOR | 7 | Panel route と kernel route で、期限・default・signer 課金の規則が設計の段階で違う。shadow comparison は未実装 |
| M-5 | MAJOR | 8 | e2e の junk 飽和試験は飽和させていない(1 block に 2 件、64 runs のうち 2)。harness は slice の fence list を使っていない |
| m-1 | MINOR | 6 | Gate C の抽選に per-epoch 上限と key 順があり、junk claim で自分の claim の抽選を後ろへずらせる可能性がある(推測) |
| m-2 | MINOR | 2 | G-K1 の課金は、conviction の位置が seat の scope に入っているかを見ない |
| m-3 | MINOR | 1 | seal の上書きで時計が reset される。同一 accuser の自己 front-run は無害だが、文書化されていない |

---

## 1. `FileProof` の seal/reveal、却下 fee、4 件の上限、block 予算

### 確認したこと
- `KernelLedgerV1::file_proof`(`misaka/palw/kernel/src/ledger.rs:2601-2680`)。
  - accuser の bond と free collateral が fee 以上であることを確かめる。
  - `proof_open` で claim がまだ有責期間内かを見る。
  - court scope(有効な場合だけ)を見る。
  - 却下 fee は `dismissal_fee_v1`(`:205-210`)で、`dismissed_proof_fee × ⌈work·64 / max_work⌉` になる。
  - court を走らせる前に `charge` する。却下ならば fee を burn する。有罪ならば `bounty_holder` が報奨を受ける。
- seal(`route.rs:612-621`, `SealProof`)。
  - `(claim, accuser)` ごとに 1 個で、`claim_seal_delay_daa` 以上古い seal のうち最も早いものが報奨を受ける(`ledger.rs:2697-2707`)。
  - 盗用した filer は、元の sealer に払う形になる。
  - **conviction そのものは seal を必要としない**。unsealed な filing でも同じ block で有罪になる。したがって、seal の遅れが「期限内の conviction」を妨げることはない。
- 同じ claim の 2 回目の filing は `Duplicate` になり、fee も slash も起きない(`:2620-2626`)。
- 取り消し: 他人の filing を取り消す経路はない。producer が claim を `TimedOut` にして告発を `proof_open` で拒否させる経路は、「一度も pass していない claim」に限られる(`:2683-2692`)。その場合は報酬も出ず、signer lock も解放されるだけなので、逃げ道にはならない。
- block 予算(`charged_v1`, `:710-722`)。
  - 64 runs のうち半分(`prosecution_reserve_permille: 500`)は `FileProof` だけが使える。
  - `FileProof` は全 64 runs を使える。admission と response は残りの 32 だけを使える。

### 結論
- **front-run と盗用**: seal の規則で報奨の横取りは防がれている。conviction は filer が誰でも成立する。G14 の観点では問題ない。
- **取り消し**: 経路はない。
- **4 件の上限**: **slice では無効**(§0)。`court_scope_in_force()` は `palw_provider_court_v1` が無いと常に false になる(`court_scope.rs:76-78`)。lane F03k の文書は「per accuser で数える」を前提に書かれている(`canonical/f03k.rs:7-8`)が、slice ではその前提が成り立たない。上限が無くても、正直な accuser の枠を他人が使い切ることはない(枠は accuser ごとに数え、署名者本人しか消費できない)。したがって、この部分は害にはならない。
- **M-1(Sybil の junk による飽和)**: junk 1 件の費用は最低で `dismissed_proof_fee` = 0.1 MSK(`palw_kernel_route_v1.rs:301`)なので、1 block の 64 runs を埋める費用は 6.4 MSK になる。
  - 共謀する producer が狙うのは、post-Final の liability(`liability_daa: 200`)が切れるまで、正直な filing を入れさせないことである。そのための費用は、おおよそ「block 数 × 6.4 MSK」になる。block 数は約 280 DAA に、DAA あたりの block 数を掛けたものである。
  - 得をするのは reservation 1,000 MSK の slash を免れる分である。1 block/DAA なら費用は約 1,800 MSK で、比は約 1.8 倍しかない。
  - この比は実際の DAA あたり block 数と fee の市場に依存する。どちらも pin されていない(第 1 部 MAJOR-2)。
  - 同じ block の中での順序は block を作る miner が決める。正直な filing は OverBudget で refuse(drop)され、次の block を待つことになる。
  - 正直な miner の block に入れば通るので、少数派の miner による検閲だけでは期限切れまで持ち込めない。ただし、**DAG の全 block を同時に埋める攻撃の費用を、mempool の fee 優先順位とあわせて示した試験も台帳もない**。
  - 必要な対処(どちらか一方)。
    - (a) 不等式 `junk 費用(horizon 全体) > reservation` を台帳に書き、値を pin する。
    - (b) `FileProof` に「その claim の最初の 1 run は予約する」などの予約を入れる(fence の変更)。

### 見ていない範囲
- mempool の受け入れ規則と fee 優先順位(`FileProof` が mempool で優先されるかどうか)。
- chunk lane(tag 113)を使う大きな filing の TTL 64 DAA と予算の関係。
- sealed bounty の contest 経路(slice では fence が無いので、動かない)。

## 2. G-K1: conviction 時の signer 課金

### 確認したこと
- 課金の流れ。
  - `lock_kernel_signer_v1`(`palw_g14_lane_closure_fold_v1.rs:27-58`)。Pass receipt を数える前に、seat の free collateral から lock を取る。lock を取れなければ、その receipt は数えない(`palw_kernel_route_fold_v1.rs:2348-2360`)。
  - lock は V2 の committed ledger に入る(`palw_kernel_route_v1.rs:602-640`、`palw_state_v2.rs:2816`)。したがって、seat は lock を持ったまま exit できない。
  - conviction の batch で `charge_kernel_signers_on_conviction_v1` が呼ばれる(`palw_kernel_route_fold_v1.rs:1458`)。closing tick の sweep が安全網になる(`:2420`)。
- 解放の規則(`palw_g14_lane_closure_v1.rs:136-162`)。
  - `TimedOut` と行の消失は解放する。
  - Final は `liability_until` まで保持する。
  - `Unavailable` も horizon まで保持する。
- 課金額は `palw_seat_lock_required_v2(claim_reward, quorum)` である。Panel=1 では quorum 1 なので、lock は報酬以上になる。
- slash は burn され、報告者への配分はない。

### 結論
- **共謀 seat が逃げられるか**: lock は committed に入るので、exit で逃げることはできない。`slash_bond` は clamp する(`transition_builder.rs:9249-9260`)が、committed にある間は他の lane が先に使い切ることはできない。free の範囲内でしか lock を取らないからである。結論として、逃げ道は見つからなかった。
- **正直な seat が誤って罰されるか**(m-2)。
  - seat の Pass は Freivalds の標本検査であり、`derived_soundness_bits` の確率で見逃す(`receipt.rs:221-224`)。
  - 課金は「その claim の conviction」だけを条件にしている。conviction の位置が seat の scope に入っていたかは見ない。
  - Panel=1 / WholeClaim なら scope は claim 全体なので、残る誤課金は「見逃し確率 2^-bits × lock」だけである。bits が 40 以上なら実害はない。
  - Segments scope を使う構成にするときは、位置による限定が必要になる。
- **B-2(e2e との矛盾)**。
  - `canonical.rs:667-677` は、conviction の後に「the colluding seats are not charged by the producer's conviction」を assert している。
  - 一方、`g14_lane_closure.rs:100-101` は同じ conviction で `slashed + lock` を assert している。
  - fold の G-K1 には fence の分岐がない(`:2348`、`:1458`)ので、両方が同時に通ることはない。実行結果は §8 に書く。
  - どちらの仕様が正しいかを決め、片方の試験を直さない限り、G-K1 が実運用で動くかどうかを試験が示していない。

### 見ていない範囲
- `charge_exec_root_signers_on_proven_false_v1`(G-X1、EXEC 経路で、slice の外)。
- 同じ seat が V2 の Panel route と kernel route の両方で同じ不正に課金される二重課金(offence key の共有)。

## 3. F02: duty、default、inclusion premise

### 確認したこと
- `CourtServiceInclusionPremiseV1`(`palw_court_service_binding_v1.rs:52-80`)。`PALW_COURT_SERVICE_INCLUSION_PREMISE_V1 = None` である。
- premise を読む箇所は TIR public fold(`palw_tir_public_fold_v1.rs:1228`)と native post-Final(`palw_native_post_final_duty_v1.rs:404`, `palw_native_post_final_default_fold_v1.rs:131`)だけである。**`palw_kernel_route_fold_v1.rs` には参照が 1 つもない**。
- kernel の default(`ledger.rs:3180-3250`)。demand の deadline(court 20 DAA + chunk 数)が過ぎると、無条件に次のとおりになる。
  - pre-Final: `default_penalty`(100 MSK)を課し、49% を demander に配る。
  - post-Final: **reservation 全額を burn する**。
- `Respond` は block 予算を消費しない。exit 中でも受け付ける(`ledger.rs:1583`)。
- spine 表の S1/S2/S3c' は UNIMPLEMENTED のままである(`canonical/spine.rs:24-77`)。

### 結論
- **取得不能が計算誤りの有罪にされるか**: されない。default は `MaterialUnavailable` であり、`convicted` は立たない(`canonical.rs` の default 経路も、`!convicted` を assert している)。
- **M-2**: kernel route の default は、inclusion premise も供給の有無も読まない。
  - producer の `Respond` が 20 DAA 以内に入らなかった場合を考える。原因は検閲でも混雑でも DA でもよい。この場合、正直な producer であっても 100 MSK を失う。post-Final なら 1,000 MSK を失う。
  - lane SPINE の「読まない」という報告は正しい。lane F02 が修正中だが、slice の commit には入っていない。
  - TIR route では「premise が None なら no-slash」になる。kernel route はこれと逆の動きをしている(M-4 にもつながる)。
- 必要な修正: kernel default の fold で premise を読み、None なら no-slash で保持する。S3c'(供給なしなら slash しない)を PASS にする。

### 見ていない範囲
- funded original coordinator の admission 経路。kernel route には配線されておらず(S1)、読んでも判定の対象がなかった。

## 4. X3 の hard deadline

### 確認したこと
- slice の `PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY`(`full_activation_v1.rs:100-113`)。t12 preset の値は `None`(`params.rs:7469` ほか)なので、`unapproved_probe_at` を入れる。その値は prepare 40 / observe 60 / resolve 200、つまり **hard = a+300** である(`palw_ruling_fences_v1.rs:127-143`)。
- 決定済みの合成方針は `palw_x3_decided_policy_v1`(`:233-240`)だが、slice の set からは呼ばれていない。
- validation は arm-readiness record の無い X3 を拒否する(`palw_g14_gates_v1.rs:309`)。
- notice a+3,120 / hard a+10,000 は `palw_court_clock_v2` の時計であり(`palw_ruling_fences_v1.rs:470-473`)、clock v2 は slice に**入っていない**。
- X3 が govern するのは `PalwClaimStateV2`(V2 native、staged、legacy)だけである(`palw_claim_deadlines_v1`, `:396-411`)。kernel ledger の claim は、kernel 自身の窓(challenge 50 / court 20 / grace 10 / liability 200, `palw_kernel_route_v1.rs:290-300`)で動く。kernel fold には `hard` の参照がない。

### 結論(B-1)
- slice をこのまま武装すると、次のどちらかになる。
  - (i) validation が record の無い X3 を拒否し、preset が成立しない。
  - (ii) record を付けると、probe 値(a+300)で V2 claim の期限が切られる。
- どちらの場合も、kernel slice の claim には X3 の保証(期限の固定、延長禁止、REQ-X3-4 の開始判定)が**届かない**。
- 延長と reset について。kernel の demand は Final 後も liability horizon の中でしか開けない(`ledger.rs:2955-2977`)。pre-Final の default は `liability_until` を `max` でしか動かさない(`:3226-3229`)。したがって kernel 側で期限を伸ばせるのは、paid claim の lifetime と EXEC hold だけである。これは X3 とは別の規則である。
- 必要な修正: slice の X3 entry に決定済みの合成 policy を入れる。そのうえで次のどちらかを選び、ADR に書く。
  - kernel claim の窓を X3(clock v2)の a+3,120 / a+10,000 に揃える。
  - X3 は kernel slice には適用しないと明記する。
- 境界試験(hard の前後 1 DAA)を、slice の params で行う。

### 見ていない範囲
- clock v2 の実装そのもの(slice の外)。
- `palw_settlement_liability_fold_v1` の X3 依存。ここは X3 が None のとき `Err` を返す(`:102`)。kernel claim の leg は `KernelClaim` を使うので、ここを通るかどうかは追い切れていない。

## 5. settlement once

### 確認したこと
- `kernel_producer_reward_at_v1`(`palw_settlement_once_fold_v1.rs:106-122`)は、`PayJobEscrow`、`ReleaseJobEscrow`、`FinalReward` が**連続する 3 件**であることを要求する。
- ledger の `settle` は amount が 0 の instruction を出さない(`fb23f6a09` より前)。escrow が報酬とちょうど同額だと、`ReleaseJobEscrow` が欠ける。
- その場合、`settle_once_liable_v1` は呼ばれない(`palw_kernel_route_fold_v1.rs:1170-1225`)。一方、Gate C の保留(`kernel_producer_final_reward_v1`, `:993-1010`)は欠けた場合も許すので、報酬は保留される。そのため、FinalReward は once marker が無いまま一般の経路で処理される。
- `fb23f6a09` は HEAD に含まれていない(`git merge-base` で確認)。commit message は「untested」である。この修正は event 列に 0 の `ReleaseJobEscrow` を加えるので、kernel の receipt と root が変わる。つまり consensus が変わる。

### 結論(M-3)
- slice の中では、kernel ledger 自身が escrow を一度しか払わない。したがって、kernel 単体で二重払いが起きる筋は見つからなかった。
- それでも、settlement once fence が約束する「consumer をまたいで一回だけ」(reorg、restart、IBD、pruned import をまたぐ marker)は、残りが 0 の場合に成り立たない。
- 必要な対処: `fb23f6a09` を統合し、slice の params で試験する。試験は 2 つ要る。
  - 残りが 0 の escrow で、marker が書かれること。
  - reorg と IBD の後も、2 回目が `AlreadySettled` になること。
- spine の S4(Final・default・settlement の DAA をまたぐ reorg)は UNIMPLEMENTED のままである。

### 見ていない範囲
- pruned import で marker が引き継がれるかどうか(コードは読んでいない)。
- restart での marker の永続化。

## 6. beacon source

### 確認したこと
- Gate C の抽選 seed は `H(id, anchor.span, anchor.execution_key)` である(`palw_security_certification_v1.rs:360-362`)。anchor は「attempt を運んだ span の最後の chain block の execution commitment」で、block hash は seed に入らない(`palw_execution_lane_v1.rs:392-410`)。
- 抽選の条件は次の 2 つである。
  - `a.span ≥ committed_span + anchor_delay_spans`
  - `network.draws < draws_per_epoch`
  - 行は key の順に処理し、1 claim につき 1 回だけで、再試行はない(`palw_security_certification_fold_v1.rs:790-810`)。
- `palw_beacon_source_*`(承認済み paid claim を source にする beacon)は slice に入っていない。2026-10-17 のユーザー決定(固定 seed、承認済み paid claim)は、slice の Gate C 抽選にはまだ配線されていない。

### 結論
- producer が anchor を変えるには、別の勝つ execution が要る。ただし、span の最後の attempt block を出すか出さないか(withholding)で、数通りの bias をかけられる。これは推測である。
- m-1(推測): `draws_per_epoch` の上限と key の順序があるので、producer は低い key の junk claim で epoch の枠を埋められる。そうすれば、自分の claim の抽選を次の anchor へずらし、seed を引き直せる可能性がある。
  - claim 1 件の費用は job fee(1 MSK)+ escrow + reservation の拘束である。
  - `draws_per_epoch` の値を確かめていないので、推測にとどまる。
  - 必要な試験: 枠を埋めて引き直すときの費用と、m/k=75/41、f=0.25 での成功確率を見積もる。
- 第 1 部 MAJOR-1(seat seed の grinding)は、この部の範囲外とした。

### 見ていない範囲
- RFC-0010 の round beacon。
- `beacon_source_inputs_v4`(slice では fence が無いので、動かない)。

## 7. shadow comparison(Panel route と kernel route)

### 確認したこと
- shadow comparison は arm-readiness の「measured」要件として挙がっているだけで、実装も記録もない(`palw_kernel_route_arming_v1.rs:60`)。

### 設計上、判定が食い違う箇所(M-4)
1. **期限**。kernel は liability 200 DAA である。V2 の Panel route は window_challenge 1,200 + court 3,000(t12、`palw_ruling_fences_v1.rs:155-160` の計算)に、X3 / clock v2 が加わる。kernel の horizon を過ぎてから届いた正しい proof は、kernel では「past the liability horizon」として拒否される。V2 では有罪になり得る。
2. **default**。kernel は deadline だけで penalty を課す(premise なし)。TIR / native は premise が None なら no-slash にする(§3)。同じ withholding に対して、片方は slash し、片方は保留する。
3. **signer の課金**。kernel の G-K1 は lock = 報酬基準で、burn する。V2 は S4 の covering-signer funnel を通り、offence key で一回だけ課金する。課金額と、二重課金を避ける key が違う。
4. **seat の数**。slice は Panel=1 である。V2 Panel と harness は 3-of-3 である(`palw_kernel_route_v1.rs:116-117`)。

- 必要な対処: 同じ claim の集合を両方の route で処理し、verdict、期限、金額を比較する差分試験を用意する。差が設計どおりのものは ADR に列挙する。

### 見ていない範囲
- V2 Panel route の verdict 規則の詳細(読んでいない)。

## 8. e2e 試験の質(`g14_kernel_route_e2e/canonical/`)

### 確認したこと
- harness の params(`canonical.rs:75-101`)。t12 に `palw_probabilistic_constraints_v1 = 0` と `palw_reorg_strict_economic_win = 0` を足しただけで、validation を迂回している。`PALW_T12_KERNEL_SLICE_FENCES_V1` は使っていない。
  - panel_one を武装していないので、3 seat の Panel になる(`canonical.rs:607` が 3 を assert する)。
  - Gate C、settlement once、X3、g14_route_required は Run 段では動いていない。
- cert.rs は `TEST_ONLY_KERNEL_CERT_POLICY_V1`(`cert.rs:19`)を使う。本番の m/k と prosecution window の値ではない。
- f03k.rs(M-5)。`crowd` は Sybil 2 枚で、1 block に 2 件の junk を出すだけである(`f03k.rs:25-44`)。64 runs の予算は飽和しない。outsider の filing は crowd とは別の block に入る。したがって「crowded court を通って有罪にする」は、飽和した block での順序を示していない。文書の前提である court scope も、slice では無効である(§1)。
- t0308.rs。fixture のカード鍵は「別の署名者」を示すだけで、運用者が独立であることは示さない。これは試験自身が明記している。
- spine.rs。S1、S2、S3b'、S3c'、S4 が UNIMPLEMENTED なので、表の試験は設計上 FAIL する(`spine.rs:96`)。
- canonical.rs と g14_lane_closure.rs は、G-K1 で矛盾している(B-2)。

### 試験の実行
`buildslot.sh` を使い、`g14_spine_panel1_kernel_route_stage_table`、`kernel_route_valid_signers_are_charged_on_conviction`、`g14_canonical_panel_a_post_genesis_bond_on_an_ibd_node_convicts_a_covered_lie_before_final` を 1 回だけ実行した。結果は末尾の「試験結果」に書く。

### 必要な対処
- slice の fence list(`palw_t12_arm_fence_list_at_v1(params, PALW_T12_KERNEL_SLICE_FENCES_V1, Some(0))`)と、pin した値で動く harness を用意する。
- Panel=1 の seat 抽選を通す。
- 1 block の 64 runs を junk で埋め、正直な filing が次の honest block で入って期限内に slash まで届くことを確かめる試験を作る。
- B-2 の assert をどちらかに統一する。

---

## 総合判定
**武装不可。** 理由は BLOCKER が 2 件(B-1 と B-2)あるためである。

- B-1: slice の X3 は probe 値になり、そもそも kernel claim を govern しない。
- B-2: G-K1 について e2e の主張が互いに矛盾しており、少なくとも片方の試験が誤っている。

武装の前に、次の 5 つを閉じる必要がある。
- M-1: 予算の飽和に対する不等式か、予約。
- M-2: kernel default が premise を読むこと。
- M-3: `fb23f6a09` の統合と試験。
- M-4: shadow の差分の列挙。
- M-5: slice の params で動く e2e。

第 1 部の MAJOR-1 と MAJOR-2 も、未解決のまま残っている。
読んだ範囲には、ZK や計算の暗号学的証明への依存も、モデル単位の審査もなかった。

## 試験結果
`cargo test -p kaspa-consensus --lib` で 3 件を指定して実行した(`2119c943b`+`1653fc09f`、buildslot、1 回)。

| 試験 | 結果 |
|---|---|
| `g14_lane_closure::kernel_route_valid_signers_are_charged_on_conviction` | ok |
| `canonical::g14_canonical_panel_a_post_genesis_bond_on_an_ibd_node_convicts_a_covered_lie_before_final` | **FAILED**(`canonical.rs:676`。共謀 seat が課金されている) |
| `canonical::spine::g14_spine_panel1_kernel_route_stage_table` | **FAILED** |

spine の内訳は次のとおり。
- PASS: S3a、S3a' ×2、S3c。
- FAIL: S3b(pre-Final)、S3b(post-Final)。どちらも `canonical.rs:676` で落ちた。
- UNIMPLEMENTED: S1、S2、S3b'、S3c'、S4。

B-2 は実際の失敗として確認できた。G-K1 は fold では効いているが、canonical の conviction 経路の assert は G-K1 の前の仕様のまま残っている。lane SPINE の表は、「outsider が conviction する」という中心の段を PASS にできていない。
