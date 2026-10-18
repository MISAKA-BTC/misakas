# RFC-0023 — 未解決 claim の責任を保持し、それに依存しない claim を進める

- **Decision:** Accepted(方向)— ユーザー指示 2026-10-17。設計凍結([RFC-0021](0021-design-freeze-and-documentation-governance.md))の手続きにより [ADR-0186](../adr/0186-hold-unresolved-liability-advance-independent-claims.md) と対で採択。
- **Scope:** RFC-0018 の共有 safe frontier、claim ごとの認証・決済、未解決 claim の担保・証拠保持。
- **Implementation:** 段階 1 は休眠 fence で実装中。段階 2 は別の休眠 fence で設計・実装する(未完)。
- **Activation:** 段階 1 は次回 fence(testnet-12 DAA 16,000 目安)の候補。段階 2 は drill と測定の後に別途判断。
- **Related:** [RFC-0018](0018-dag-parallel-multi-model-consensus.md)、[PRINCIPLES](../PRINCIPLES.md)、根拠のソース抜粋 [shared-frontier-sources](../audit/palw/rfc18-dag-audit-2026-10-17/shared-frontier-sources.md)(F 番号)。

## 1. 設計目標

> **遅い claim A を「正しいこと」にして追い越さない。A の責任を未解決のまま確実に保持し、A に依存しない claim B だけを進める。**

ブロック生成・計算・claim ごとの検証は、すでに並列に進む(RFC-0018)。残る依存は、共有の確定境界(safe frontier)と、それを読む決済・fork choice・pruning である。本 RFC は、その依存を「本当に依存している責任」に限る。

## 2. 現行規則(変更前の記録)

- `palw_state_v2.rs:15043–15065`:`resolved_through` = 最も古い未解決 claim の受理位置 − 1。その範囲内で、Gate C 有効後は Final かつ SecurityCertified の Attempt claim の最深位置を frontier にする(F01)。位置は `accepted_blue_score`。
- 「解決済み」と「認証済み」は別である。Final だが未認証は足止めにならないが frontier の根拠にもならない。Voided は足止めから外れ、credit は得ない(F01b・F04)。
- frontier には退役による下限(floor)があり、退役前の認証済み claim が後で有罪になると frontier は後退し得る(F03・F05)。
- **影響範囲:** 利用者の取引の `settled` は `safe_frontier_daa >= daa_score` で判定する(`palw_settlement_v1.rs:118–125`、F07)。native の安全な確定は frontier と `lifecycle_closed` の両方を要する(F11・F13)。fork choice は frontier を比較要素に使う(F09・F10)。pruning は Gate C 下で既に未解決・認証待ち・後退し得る位置から上限を導く(F04・F08)。
- したがって、A が未解決の間、B の claim は Final・認証・報酬出力まで進み得るが、**B への支払い取引の `settled` は共有 frontier を待つ。**

現行規則は、旧規則の 2 つの失敗(未解決 0 件を要求して正常な chain で進まない/空ブロック列が frontier を進める)を避けるためのものである。足止め条件を削除して最深位置だけで進む方式には戻さない。

## 3. 段階 1 — 現行 frontier を保ち、足止め時間に上限を置く(次回 fence)

1. **claim ごとの未解決期限。** `未解決でいられる期限 ≤ 受理 DAA + L_class`。`L_class` は検証・資料取得・court・必要な収録待ちを含む。受理時に court 予約容量・期限内の処理順・再抽選と追加告発による延長の上限と整合しなければ、その claim を報酬対象として受理しない(既存の bounded admission・detection policy・retention の規則を使う)。新しい告発で責任時計を最初からやり直さない。
2. **期限到達の扱い。** 期限到達は不正の証明ではない。fraud slash は証明された不正と明確な応答義務違反(適格 default)にだけ行う。期限で credit を与えない場合も、係争中の担保・証拠は解放しない。
3. **測定。** 次を別々に記録する:B の Final、Gate C 認証、報酬保留の解除、支払い出力の作成、その取引の `settled = true`、native の safe/finalized。「出力ができた」と「確定した」をまとめない。
4. **継続攻撃。** 1 件ごとの期限が有限でも、別の古い claim が次々と足止めを引き継ぐ場合の定常遅れを測る(資料応答の遅延、正当な court、虚偽告発、再抽選、容量飽和、遅延 claim の継続投入を原因別に)。
5. **期限の異なる claim。** A と B の責任期間が異なる場合の退役 floor(退役時の floor 更新は `oldest_unresolved` を再確認しない)、late conviction による後退、その後の pruning・IBD 復元を 1 組の試験にする。

段階 1 が保証するのは「未解決 claim 由来の遅れの上限」だけで、認証済みの仕事が生まれない期間やネットワークの停止まで frontier の前進を保証しない。

## 4. 段階 2 — 未解決 claim を局所管理する(別の休眠 fence)

共通 DAG は維持し、次の 3 つを分けて管理する。

| 管理するもの | 役割 |
|---|---|
| DAG への記録・共通の順序 | どの claim・取引・証拠が取り込まれたかを全ノードで一致させる |
| claim ごとの認証・決済状態 | その claim に credit・報酬を与えてよいかを決める |
| 未解決 claim の責任・証拠状態 | 担保、期限、係争、再計算に必要な資料を保持する |

1. **未解決集合を明示的に拘束した checkpoint。** A を解決済みと偽装せず、checkpoint に未解決集合(claim、担保予約、検査 policy、乱数・登録状態の証拠、court 状態)を拘束する。Merkle root だけでは足りない。正直な検証者が pruning・IBD の後も実際に資料を取得して反証を完成できること(G14)を要件にする。
2. **依存の単位は claim とその依存関係。** B が A の出力を入力に使う、同じ execution root に依存する、同じ担保予約を使う場合は依存とみなし、B も待つ。モデル ID や同じ DAG にいることだけでは依存としない。
3. **不可逆な確定の根拠。** 取り消され得る credit(責任期間内の認証済み claim)と、不可逆な checkpoint を支える credit を区別する。未解決 A を数えず、必要な安全条件を満たした独立の仕事だけで checkpoint を支える。
4. **後から A の不正が判明したとき。** A と、A に実際に依存した対象にだけ処罰・credit 取消を適用する。無関係な B は止めない。
5. **同時に変えるもの。** checkpoint、fork choice(frontier の比較要素)、取引の settlement、native の lifecycle、pruning・IBD・証拠保持を 1 つの fence で一緒に変える。`resolved_through` の条件を消す 1 行の修正にはしない。

## 5. 採らない案

| 案 | 理由 |
|---|---|
| モデル別 frontier の最小値 | 最も遅いモデルを待つことに変わりない。休止モデルの扱いも要る |
| モデル別 frontier の最大値 | 速い 1 モデルだけで、ほかの未解決責任まで安全とみなす |
| 多数のモデルが進んだら進める | モデル数は独立した安全性ではない。モデル ID を増やせれば操作できる |
| 一定時間後に未解決 claim を無視する | 担保・告発・証拠保持を失い、不正を逃がす経路になる |

## 6. 受入条件

- A が未解決のまま、A に依存しない B・C が Final・認証・報酬解除・支払いまで進む。段階 2 では B の取引の `settled` も A を待たない。段階 1 では待ち時間が `L_class` の上限内であること。
- A が後で有罪になったとき、A と A に依存した claim だけが slash・credit 取消を受け、B は影響を受けない。
- 未解決 A の資料は pruning・IBD・restart・reorg の後も取得でき、外部の正直な検証者 1 人が反証を完成できる。
- 全ノードが同じ checkpoint・未解決集合・決済状態に収束する。fence の前は byte 単位で同じ動作。
- **統合受入キャンペーン。** 正常系と攻撃系は別シナリオとして、同一候補バイナリ(artifact hash を固定)・同一testnet設定/genesis の複数ノード環境で実行する。各シナリオで複数claimを並行させ、正常系は独立claimのFinal・認証・支払い・settled と全ノード収束、攻撃系は未解決Aと独立BおよびA依存claimを含め、有罪化後に影響がAとその依存先だけへ限定されることを確認する。攻撃系ではさらに pruning・IBD・restart・reorg 後に外部の正直な検証者1人がAを反証できることを確認する。別バイナリ間の結果比較や単一node/unit fixtureは、この受入条件の代替にならない。

## 7. 実装メモ(段階 1、2026-10-17、branch `integ/g14-court-capacity-r2`)

- **fence。** `palw_unresolved_bound_v1`(`Params` の Some-only fence。identity・schedule・fork-id の各所に名前つきで入る)。全 preset で `None`、validation は武装を拒否する。state params への mirror は `unresolved_bound_from_daa`。testnet-12 の identity は不変。
- **未解決期限。** `bound(c) = c.trace_retention_daa` とした。受理時に固定される producer の約束で、`palw_court_capacity_v1` 以後は court を要する class で 4W + W_disclose(licence 起点)以上、Gate C 以後は認証の延長分を含む。新しい告発で動かない。`L_class = bound − 受理 DAA`。
- **受理時の保証。** court 容量の bounded admission と reserve pool(本書の対である G14 完全性証明 §11.1)が、`bound − W_disclose` までに admit された行の裁定を保証する。保証できない class の claim は、`CourtClaimAdmissionFull` と `COURT_RETENTION_BELOW_GUARANTEE` で報酬対象にならない。
- **resolved prefix。** `palw_resolved_through_v1` は、期限内の最古の未解決 claim の位置 − 1 を返す。期限を過ぎた未解決 claim は prefix を止めない。
  - その claim は `unresolved` に残る。pruning の上限は止まったままで、証拠は保持される。予約・担保・係争にも触らない。
  - fence 未満では従来どおり、最古の未解決 claim の位置 − 1 である。
- **credit。** 期限より後に `Final` になった claim は frontier にならず(`palw_gate_c_advances_frontier_v1`)、certification は `Withdrawn` になる。held reward は名指されない。slash はしない。
- **floor(F05)。** 退役による floor の引き上げは prefix の内側に限る。prefix より上の退役は deferred floor の行(`'f'`、最も高いもの 1 つ)に置く。prefix がそこへ届いた block で適用する。floor が期限内の未解決 claim を越えることはない。
- **試験(core、pass)。**
  - `rfc23s1_the_resolved_prefix_skips_only_open_claims_past_their_own_bound_and_is_the_identity_below_the_fence`
  - `rfc23s1_a_claim_final_only_after_its_bound_earns_no_frontier_credit`
  - `rfc23s1_a_continuous_hand_over_lags_the_frontier_by_at_most_one_bound`
  - `rfc23s1_floor_horizons_late_conviction_and_restore_as_one_set`
- **継続攻撃の測定(模擬、§3.4)。**
  - 条件:L = 14,568 DAA(testnet-12 の court 用 retention)、攻撃 claim の投入間隔 600 DAA、正直な claim の認証まで 120 DAA。
  - fence 以後の定常遅れは 14,999 DAA(L + 間隔の範囲内)。fence 未満では、最初の攻撃 claim が全期間 frontier を止めた(170,001 DAA)。
  - 原因別の内訳(資料応答の遅延、正当な court、虚偽告発、再抽選、容量飽和)は、この模擬では区別していない。どれも期限 L の内側に収まる、という上限だけを確かめた。原因別の実測は未実施。
- **床の不変条件の試験の内容。**
  - fence 未満では、短い horizon の B の退役が、開いている長い horizon の A を越えて floor を上げる(F05 の穴を再現した)。
  - fence 以後は deferred になり、A の期限を過ぎてから適用される。
  - その後の C の late conviction で、frontier は floor まで戻る。
  - carriage は open claim と floor の行を運ぶ。
- **未実施。** node 上の drill(fence をまたぐもの)、IBD・restart の node replay、`settled`・native safe の段階別の実測(§3.3)、段階 2(別 fence)の全部。

### 段階2の基盤(2026-10-18、`codex/rfc23-stage2`、未完)

基準は `origin/pre` の `c68d385d0`。指定された段階1の branch を `57a325676` まで merge している。**以下は基盤処理の実装であり、§6 の受入条件を満たしたという報告ではない。段階2は未完である。**

- **fence:** `palw_claim_local_settlement_v1`。全 preset は `None`、`Some(never())` は畳み込み、Some のみを名前つきで identity に拘束する。state params の mirror と一致しない設定、実際の武装は validation が拒否する。測定記録・node drill・有効化判断はまだない。
- **checkpoint の型:** `palw_claim_local_v1.rs`。claim の実際の入力、execution root、担保予約の識別子、Final・認証・責任終了を別に保持する。モデル ID と bond ID 自体は依存にしない。RED の carrying block と区別して、共通 DAG 上の accepting block を記録する。
- **依存と credit:** 入力の因果循環・自己参照・未来参照・欠落を拒否する。同じ execution root / 予約を後から使う claim は先行する claim に依存するが、後発 claim が過去の不可逆 credit を再び未解決にはしない。同時に受理された共有責任は互いに待つ。credit と不可逆な checkpoint 支持を分け、後者は依存閉包全体の責任終了を要する。heartbeat と FP だけでは attempt の frontier を作らない。
- **検査の権限:** peer が自称した flags だけから frontier・settlement を読めないよう、検査済み checkpoint にだけ読み取り口を設けた。検査には fold から得た正しいレコードと責任状態との完全一致、および資料集合と実バイト列の一致が必要である。**この正しいレコード・元の registry / 乱数・予約 / court 状態を実際の fold から導く接続は未実装。**
- **native の割り当て:** checkpoint の全 nested map / set / byte vector を Borsh の割り当て前に検査し、保持値と graph 読み取りの workspace に allocation ticket を保持する。壊れた件数宣言、末尾、重複・非正規順序の map を拒否する。正規性の比較は streaming で行い、第二の巨大なエンコード済み buffer を作らない。ローカル容量不足は不正データと異なるエラーで返す。
- **完全な資料の復元口:** `PalwLiabilityMaterialArchiveV1`。既存の署名済み manifest と実行済み state の native authority / kernel ledger に対して、元の全資料を既存の4 codecで認証する。kernel の同一性だけを確認して保管する経路は使わない。cold restore 後に実バイト列を借用でき、既存形式の court / DA 応答を作れる。呼び出し側が実行済み state から正しい manifest を得る接続、on-chain の保管・収録・pruning / IBD の転送経路は未実装。既存の有限な署名済み lease は延長していない。
- **復元前検査で見つかった欠落:** aux 136 の frontier floor (`F` とゼロ ID)、繰延床 (`f` の1バイト key)、native39 認証 (`V`) の実際の行形式を追加した。段階1の床の試験を、Borsh 往復に加えて native carriage preflight も通すよう強めたところ、保存形式が `Some` の中身を直接書くのに検査が余分な Option byte を読んでいた欠落も露見した。kernel / Panel / floor / bond budget の4つの tail adapter を実際の serializer / decoder に合わせた。保存バイト列そのものは変更していない。
- **試験:** `kaspa-consensus-core --lib` の `rfc23` は16件成功・0件失敗(0.36秒)。内訳は段階2の基盤12件と段階1の回帰4件。さらに carriage preflight の既存4件、material capacity の5件、typed kernel material の3件が成功した(各0.02 / 0.02 / 0.01秒)。後の2群には上記16件との重複が各1件あるため、成功した試験は合計26件。先行する run の14成功・1失敗が上記 carriage tail の欠落を検出した。資料試験の実物 fixture は retrieval の snapshot slice 2件と returned entry 1件の全3 unitで、cold restore を2回行い、各 unit の通常 court 応答の実バイト列を照合した。実モデルや全 codec の node 試験という意味ではない。
  - コマンドは `RUST_MIN_STACK=67108864 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_TARGET_DIR=/tmp/misaka-codex-width-target cargo test --lib -j 3 -p kaspa-consensus-core --no-default-features --config profile.dev.package.kaspa-consensus-core.opt-level=0 --config profile.dev.package.kaspa-consensus.opt-level=0 rfc23 -- --test-threads=1`。追加の3群は、この同じ生成済み unit-test binary の `palw_native_resource_v1::wire::carriage_preflight_tests` / `palw_material_capacity_v1::` / `palw_prospective_kernel_material_v4::tests::` を対象に実行した。
  - 文書の index 生成とリンク検査は、新規不備0件・既知の未解決参照43件。既知 baseline は変更していない。
- **未実装・未実測:** claim 受理からの provenance 導出、依存待ちの認証済み credit / 報酬を一度だけ解除する fold、依存先の有罪による credit 取消と honest consumer の fraud slash の分離、node と client 共通の fork-choice 比較、取引の settled、native lifecycle、資料を保持した pruning / IBD / restart / reorg、および外部の1人が復元資料から反証を完成させる node 試験。これらを同一 fence で接続し、§6 の各段階を観測するまで、段階2を完了扱いにしない。実モデルの CPU・RAM・資料転送量・保持量も未測定。

### 原状態の保存を fold に接続(2026-10-18、未完)

- **保存口:** `palw_claim_local_fold_v1`。段階2の fence 以後、実際の `Claim` journal と claim 認証行の変更から対象 claim を得る。producer が列挙した依存一覧を権限にはしない。aux 144 の `F + accepting block` に一度だけ原状態の frame を書く。未解決 A が同じ DAG・モデルにいるだけでは、B の変更を A の変更として記録しない。
- **保存するもの:** その時点の前後の native carriage、node が与える安定した network identity と既発火 fork digest、実際に提示された lifecycle objects、own / merged の attempt・receipt spend、取引内の保持 pledge・公開 binding・custody delegation・元の TIR envelope、および既存の origin header 資料。提示された入力の保存は、その入力が受理されたという宣言とは分ける。全 native job / session の原入力と静的 policy の全値を保存し終えた、とはまだ扱わない。
- **再帰を避ける:** 前後の carriage を作るときに、この namespace をコピーの前に除外する。旧 `from_state` は従来どおりのコピーを返す。内側の carriage も実際の型で割り当て前に検査し、frame の中に aux 144 が現れたら拒否する。単に byte vector の長さだけを検査する経路にはしない。
- **保持と復元:** コピー・入力の Box / Vec・エンコード・行の journal を、構築前に typed reservation に計上する。受理済み native state からの読み込みでは、行 key・時点・carriage の正規性と namespace 非再帰を検査する。frame の自己申告だけを過去の state の権限とはしない。前後の carriage は namespace を除いた投影なので、選択 header が拘束する全 state root と単独で同一だとも扱わない。
- **生成元の修正:** `gen_rfc06_native_wire.py` が `seen_<field> = true` の末尾を本来の field 代入と取り違え、Some-only tail を余分な Option として生成していた。識別子境界を付けた実代入の検出に修正した。前節の4 adapter、`F/f/V`、aux 144 と非再帰 guard、および extras の新しい原規則 context が隔離コピーでの再生成に残ることを確認した。receipt spend の所有データも型グラフの root に追加した。
- **試験状況(容量整理後の再検証):** `RUST_MIN_STACK=67108864 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_TARGET_DIR=/tmp/misaka-codex-width-target cargo test --lib -j 3 -p kaspa-consensus-core --no-default-features --config profile.dev.package.kaspa-consensus-core.opt-level=0 --config profile.dev.package.kaspa-consensus.opt-level=0 rfc23 -- --test-threads=1` は22件成功・0件失敗(実行0.37秒、clean build 17分59秒)。今回追加した6件も含む。以前の容量不足による中断(exit 130)は成功件数に含めない。同じ生成済み binary の A-2 uniformity 17件(15.11秒)、aux registry 6件(5.47秒)、carriage preflight 4件(0.13秒)、material capacity 5件(0.07秒)、typed kernel material 3件(0.09秒)も成功した。後の資料2群に各1件の重複があるので、合計55件の異なる試験が成功した。隔離コピーでの generator 検査も成功した(aux144 / F / f / V、非再帰 guard、receipt / extras visitors、4つの Some-only payload)。node の最終 `cargo check --lib -j 3 -p kaspa-consensus --no-default-features`(同じ環境・opt-level override)も10分34秒で成功した。integration test の `palw_the_release_did_not_move` 9件(0.16秒)、`palw_activation_pool_is_t12_only` 5件(0.21秒)、`g14_court_capacity::testnet12_identity_did_not_move` 1件(0.09秒)も成功した(3 binary の build は2分21秒)。unit と合わせて70件の異なる試験が成功した。node 上の pruning / IBD / restart / reorg の完了報告ではない。
- **残り:** 静的な元 policy の全値、全 native inflight / session の保存対象、責任終了に応じた frame の回収・圧縮、資料全体の収録と取得、依存の導出と credit / 報酬の一度だけの解除・取消、node / client の frontier、settled、native lifecycle、pruning / IBD の接続と §6 の実 node 試験。休眠・武装拒否は継続している。


### fold 時点の policy 実値の保持(2026-10-18、native 固有 policy 等は未完)

- ID だけに依存しないよう、frame にその fold 時点で有効な検査 policy の全フィールド、Gate C 認証 policy、court 容量 policy と基本時間窓を収録する差分を追加した。検査・認証 policy への Borsh derive は、この休眠 namespace の保持資料に使うもので、既存の policy ID 算出・値・preset を変更しない。per-class の実期限と既存 session の期限は前後の carriage を参照する。
- 未発火の fence の policy は収録しない。将来の設定変更が過去の frame / delta を変えないことを試験する。後から作った frame の policy を、fence 前に受理された claim の元 policy と捏造する権限にはしない。
- policy list のコピーは構築前に予約し、typed native heap / wire scanner も生成元から追加する。ここでも frame の内側に別の frame は入らない。
- 追加した3試験を含む `rfc23` は25件成功・0件失敗(0.37秒、build 5分53秒)。policy codec の試験は4 route × 3 N-rule × 2 method × 2 localisation × 2 fixed-gain = 96通りを扱う(受理可能性の測定ではなく保存形式の検査)。同じ生成済み binary の A-2 17件(9.75秒)、aux registry 6件(2.77秒)、carriage 4件、material 5件、typed kernel material 3件も成功した(後の3群は各0.00秒、資料2群に各1重複)。integration の release 9件(0.16秒)、activation pool 5件(0.44秒)、testnet12 identity 1件(0.09秒)も成功した(build 2分57秒)。合計73件の異なる試験が成功。node の最終 `cargo check --lib -j 3 -p kaspa-consensus --no-default-features`(同じ環境・opt-level override)も2分35秒で成功。先行する70件だけを新しい差分の合格根拠にはしていない。
- native の job / session 固有 policy、各経路の元の保持 covenant、既存 claim の移行資料、全資料の転送・取得、依存からの credit と報酬、frontier / settlement / lifecycle / pruning / IBD は引き続き未完了。段階2全体の完了条件は変更しない。


### 受理時点の資料を識別する reader(2026-10-18、metadata reader 検証済み・段階2全体は未完)

- 受理済み state にある `F + accepting block` を読み、前後の claim 行と実際の提示入力の対応を確認する reader を追加した。後の timeout / policy 変更 frame を元の受理資料にしない。退役と同じ transition 内で同一 FP ID を再受理する場合も、ID だけでなく受理 DAA / blue score で区別する。carrying header と共通の accepting block も区別する。
- 返すのは元 claim と frame の metadata provenance である。計算の正しさ、Gate C 認証、資料全体の取得可能性を認証する capability ではない。node の通常の state/root 認証が前提で、peer が提示した frame だけからこの authority を得ない。fence 前には新しい frame を検査しない。native の job / mining / EXEC credit lot の reader はまだ接続していない。
- frame 内の carriage は、割り当て前の検査と予約を保持した callback 内で読む。返す claim の独立コピーと lookup key と既存 `attempt_id_v2` が使う Borsh workspace も予約する。ローカル容量拒否を欠落資料や不正 claim として扱わない。
- 新しい FP の試験で、wire generator が enum variant 全体の属性を先に消し、内側の `borsh(skip)` を見失っていることが判明した(28成功・1失敗、0.46秒)。variant 先頭の属性だけを先に除くよう生成元を修正し、FP kind 12 の scanner から wire に無い3項目を除いた。旧 object の Borsh bytes は変更しない。省略される `job_pin` / `public_job` / `eval` の実際の提示値は、休眠 frame の `FreePromptNative` 入力欄へ別途保持する。改善 DTO の外部 wire / 有効化条件には触れず、eval の保存欄は既存の job と tail の tuple で表す。改善経路を mainnet 前に開く変更ではない。
- 隔離再生成で FP kind 12 が15 wire fieldであり、3つの省略項目を読まないこと、`F/f/V/144` と非再帰 guard が残ることを確認した。元の受理 / 後の frame / merged carrier / FP ID 再受理 / native 専用値 / cold restore / 欠落入力 / fence 前の未知 bytes を含む `rfc23` は29件成功・0件失敗(0.38秒、build 5分57秒)。A-2 17件(13.30秒)、aux registry 6件(4.20秒)、carriage 4件(0.03秒)、material 5件(0.01秒)、typed kernel material 3件(0.03秒)も成功した。資料2群に各1重複があるので unit は62件の異なる試験が成功。integration の release 9件(0.21秒)、activation pool 5件(0.44秒)、testnet12 identity 1件(0.05秒)も成功した(build 3分46秒)。合計77件。node の最終 check も2分00秒で成功した(各コマンドの環境・opt-level override・`--no-default-features`・`-j 3` は前節と同じ)。初回の compile 中断と中間の compile error は合格件数へ加算しない。文書 index / link 検査は既知43・新規0。
- 入力からの依存導出、credit / 報酬の一度だけの反映・取消、全 native policy / signed covenant / raw material の保持と転送、GC、共有 fork choice / settled / lifecycle / pruning / IBD、外部1人の反証を含む node drill は未完了である。


### native 行だけが変わる fold の原資料(2026-10-18、保存検証済み・段階2全体は未完)

- chain claim / 認証行だけでなく、実行 journal の `KernelRouteRow` が変わる transition でも frame を作る。台帳の job / claim / demand / court / EXEC 行などを同じ writer で扱い、モデルや registry の行を claim やモデル審査として解釈しない。自分の aux 144 は対象外にする。
- frame は行の table / key / old / new を journal 順で保持する。作成・更新・削除が1 transition に同居して両端では存在しない行も失わない。前後の carriage と各行の連続性を検査し、自己 namespace・no-op・途中値の不一致を拒否する。この検査は実行結果の正しさや Gate C 認証を証明するものではない。
- 履歴の Vec と行のコピー、検査時の BTreeMap と key の workspace を割り当て前に予約する。全て休眠 fence の内側であり、既存 native object の wire bytes は変更しない。
- この履歴は原状態を残す接続である。native credit lot の依存 reader、資料の完全な署名済み covenant と実バイト列の収録・転送、frame の回収、credit / 報酬・frontier / settled / lifecycle / pruning / IBD の一体接続、および外部1人による反証の node 試験は引き続き未完である。
- **試験:** 同じ scoped 環境・opt-level override・`--no-default-features`・`-j 3` で `rfc23` は32件成功・0件失敗(0.26秒、build 6分32秒)。今回の3件は、chain claim の無い実 fold の行履歴と cold restore / delta replay / revert、実 row writer での作成→更新→削除と途中値・順序改変の拒否、fence 前の state / delta のバイト一致を扱う。途中行の試験は writer seam の検査であり、全 native job / court 経路の node drill として数えない。A-2 17件(13.08秒)、aux registry 6件(3.90秒)、carriage 4件・material 5件・typed material 3件(各0.00秒)も成功した。資料2群の各1重複を除いて unit は65件。integration の release 9件(0.15秒)、activation pool 5件(0.19秒)、testnet12 identity 1件(0.07秒)も成功した(build 4分07秒)。合計80件の異なる試験が成功した。
- node の最終 `cargo check --lib -j 3 -p kaspa-consensus --no-default-features` も同じ環境・opt-level overrideで成功した(3分01秒)。文書 index / link 検査は既知43・新規0。全 native 経路の G14 / pruning / IBD / restart の node 受入試験の完了を意味しない。


### EXEC journal と入力参照の導出(2026-10-18、保存・入力参照は検証済み・段階2全体は未完)

- EXEC の root / slice / job / anchor / reward / hold / job-use / origin は `KernelRouteRow` ではなく `ExecV2Row` に journal される。chain claim と kernel 行だけを見る保存 hook では、EXEC だけが変わる transition を取りこぼしていた。両 journal を実行順の1本の履歴に収録し、owner を明示して同じ table / key を混同しないようにした。自分の aux 144 の除外は kernel owner に限る。
- EXEC の行境界は8 table の実型で比較する。key は固定幅の Copy 型として読み、行の値は現在の typed carriage から正規エンコードを streaming 比較する。巨大な行をもう一度エンコードしたり、履歴の未信頼な行 bytes を先に Borsh decode したりしない。途中の作成→削除も両 owner を含む順序で保持する。追加 owner tag は休眠 frame 内だけの形式であり、既存 EXEC / kernel object の wire bytes は変えない。
- `palw_claim_local_origins_v1` は frame の実際の受理済み EXEC 行から、session の元 chain claim / bound prefix、session の admitted slices、slice の kernel evidence と直前 slice / prefix への入力参照を導く。モデル ID、bond、たまたま等しい output hash からの推定はしない。session からその基礎 chain claim へ向かう参照はあっても、後発 slice から基礎 credit を逆に依存させない。chain claim / kernel claim / session / slice の namespace を分け、同じ raw ID を同じ対象として畳み込まない。
- legacy `Unbound` session の prefix / evidence 宣言を、後から認証済み kernel 参照に変換しない。未束縛の root を明示して返す。ここで返すのは実行時点に束縛された入力参照の投影であり、raw ID 再使用時の birth 解決、資料取得、Gate C 認証、credit 付与の capability ではない。
- この接続により、EXEC だけの状態変更も元資料の保持対象になる。全 claim / native 経路の依存閉包と責任終了、credit / 報酬の一度だけの解除・取消、checkpoint / fork choice / settled / lifecycle / pruning / IBD、資料の収録・転送・回収、および外部1人の反証を含む node drill は未完了である。
- **回帰の切り分け中:** `rfc23` は追加4件を含む36件が成功した(0.50秒、build 6分26秒)。先行 run の35成功・1失敗は、拒否された slice が state を変えない場合にも新しい frame があるとした試験仮定を検出した。修正後は EXEC state / row journal / frame の不変と非ゼロの拒否 verdict を確認する。初回の import 不備による compile error も成功件数に含めない。
- EXEC fold 全体の回帰は94成功・1失敗・1 ignored(18.54秒)。失敗した既存試験は、後発 slice の不正・default と、Panel が署名した元 prefix 自身の不正の全条件で担保不変を要求していた。既存 G-X1 は後者の署名者だけを処罰する実装であり、consumer 伝播の追加罰とは分ける必要がある。変更前 commit `6bace5d13` の独立 checkout で同じ試験を再実行した(結果は次項)。現在の試験差分では、G-X1 の場合だけ元の署名者 lock と既定 S4 の debit を期待し、他の bond / 後発 slice / default では担保不変、重複 verdict で二重徴収しないことを要求する。元の失敗を隠すための skip / ignore は追加しない。この時点では修正後の全回帰は再実行前だった。最終結果は後述する。
- **変更前との差の確認:** `6bace5d13` の clean な独立 checkout でも、元の既存試験が同じ担保比較で失敗した(0成功・1失敗、0.75秒、build 6分23秒)。今回の保存差分による回帰ではない。署名者2〜6への各1,909,341 sompiの既存 S4 debit が一律の「担保不変」期待と矛盾していた。変更前ログは保持し、比較用の managed worktree は確認後に archive した。現行の node check は2分29秒で成功している。処罰ロジック自体は変更せず、試験の条件分けを修正した差分で再検証した(最終結果は次項)。
- **最終検証:** 同じ scoped 環境・opt-level override・`--no-default-features`・`-j 3` で `rfc23` は36成功・0失敗(0.64秒、build 7分21秒)。同じ生成済み unit binary の EXEC fold 全体は95成功・0失敗・既存の resource probe 1 ignored(10.05秒)で、修正した G-X1 / 非関連 consumer / 二重徴収の全条件も通った。A-2 17件(6.94秒)、aux registry 6件(1.99秒)、carriage 4件・material 5件・typed material 3件(各0.01秒)も成功した。実行名の集合で照合すると、EXEC 内の新規4件と資料群の2件の重複を除き、unit は160件の異なる試験が成功した。integration の release 9件(0.12秒)、activation pool 5件(0.19秒)、testnet12 identity 1件(0.04秒)も成功した(build 4分15秒)。合計175件。node の最終 check は同じ production 差分で前述の2分29秒成功であり、その後のコード編集は試験の期待値だけである。
- ここでの EXEC 試験は既存 `amendment_1` fixture の元で実際の fold / row writer / delta replay・revert / cold carriage を通すもので、kernel claim の受理・署名・Final の fixture seam は引き継いでいる。外部1人の filer を含む node G14 / pruning / IBD / restart の受入試験としては数えない。全 native 入力の birth 解決と依存閉包、credit / 報酬・checkpoint / frontier / settled / lifecycle / pruning / IBD、完全な資料の保持・転送・回収は引き続き未完了であり、§6 の完了条件は変えない。

### chain の認証済み保留報酬への最初の接続(支払 consumer を検証、段階2全体は未完)

- `palw_claim_local_rewards_v1` を Gate C の chain claim の保留 issuance reward の解除と退役に接続した。対象は段階2の fence 以後に受理された claim であり、それ以前の claim の支払条件は変更しない。raw Gate C 認証は引き続き抽選された検証者の承認を読む。同じ純粋な認証 predicate を、履歴に保持した実際の認証行にも適用する。fence 前と fail-closed の判定では、その行を読まない。
- 元の受理 frame に束縛された claim と、受理時点以前の同じ非ゼロ execution root の claim を読む。モデルや同じ bond 自体から依存を作らず、後発 claim で過去の支払条件を再び開かない。live source の未解決・未認証・係争・既存の責任期限は保留する。元の入力が取得できない場合も保留し、不正とは認定しない。Gate C の chain 報酬では、EXEC root の prefix と全 slice の native Claim row を追加で検査する。native row が欠落・pending・未選択承認なら保留し、Defaulted / Convicted なら consumer の未払い報酬を拒否する。解除には native の selected approval、Final、および各 row の liability deadline 終了を要する。これで native input の birth を claim-local checkpoint に統合したわけではなく、historical native source の復元・native credit lot / mining・他の reward owner は別途未接続である。
- 実際の chain `Claim` 行も kernel / EXEC と同じ journal 順に収録する owner を追加した。前後の carriage だけから正常な退役を推測せず、birth と removal の実際の旧行、および removal より前の最新の認証行を読む。同じ transition 内の更新・削除でも、`Final` だったという境界だけを支払の根拠にしない。
- 依存が残っている退役では claim と未払い額を保持し、aux 144 の `R + claim ID` に birth と再確認時点を記録する。この行は scheduling hint であり、認証や credit の権限ではない。期限 index と cold restore の再導出でも同じ hint を読む。解除・拒否・退役で消し、払った額は認証行でゼロにする。既存の独立した settlement-once fence の有効化には依存しない。hint は署名済みの trace lease や既存の material covenant を延長しない。
- 依存先が失効・有罪なら、consumer の未払い額を消す。consumer の raw Gate C 認証を偽の承認に書き換えず、consumer の新たな fraud slash も加えない。producer の既存の conviction debit は維持する。
- 読み取りは元 frame / carriage / Claim / 認証行を割り当て前に shape 検査し、保持 lease の下で行う。既存 EXEC horizon reader が読む可変長 Claim 行と key buffer も先に予約する。ローカル容量不足と不正データは区別する。履歴を走査する初期実装であり、保持量・CPU・RAM の測定、index / GC はまだない。
- EXEC native dependency reader は prefix と全 slice の Claim row を既存 preflight / reservation の後に読み、`palw_exec_v2_claim_outcome_v1` と selected receipt approval を照合する。有限な native horizon だけで解除しないこと、および EXEC root が cached `Verified` を持っていても実 native Claim row が欠落すれば保留することを実 fold fixture で確認した。追加した positive reader fixture は root の retention terms から得た deadline を越えた Final prefix / 全 slice row と selected approval を確認し Ready を返す。同じ admitted slice の Claim row を欠落させると Hold へ戻る。この試験は readiness helper の fixture 検証であり、Gate C 報酬の実 fold payout、再起動後に native row が収集済みの場合の履歴復元・生存性、job / mining を含む全 native owner はまだ検証していない。
- **EXEC reward consumer handoff (2026-10-18):** `inputs_known` の一律 native-reward 拒否を外し、元の claim birth frame の provenance 検査後に `live_source` が同じ native dependency reader を評価するようにした。reader の `Hold` / `Denied` を真偽値へ潰さず、欠落・未選択・期限内は保留し、確定した Defaulted / Convicted はconsumerの未払い報酬を拒否する経路に到達する。native rowの欠落を保留する既存回帰と、EXECを含まない既存chain支払・依存拒否の回帰を含む `rfc23s2_reward_` 8件は `kaspa-consensus-core --lib --no-default-features`、crate opt-level 0、`-j 3` で成功した(3分10秒build、0.47秒実行)。これは chain issuance reward consumer 1箇所の handoff で、native owner 全体や複数node受入ではない。
- **範囲の限界:** chain の保留 issuance reward の1 consumer だけである。credit weight と不可逆な checkpoint / frontier、取引の settled、FP receipt と native の全支払経路・lifecycle、完全な資料の収録・転送・保持・回収、pruning / IBD と外部1人の反証を含む node drill は未完である。この差分だけで段階2の完了、G14 の全経路成立、武装可能性は主張しない。

- **試験(最終 source):** 同じ scoped 環境・opt-level override・`--no-default-features`・`-j 3`・`--test-threads=1` で `rfc23` は44成功・0失敗(0.97秒、最終 test binary build 10分35秒)。追加した8件は、実 fold の独立 B の支払い・退役と A 依存 C の保留 / 失効後の拒否、正常に退役した source の実履歴からの一度だけの解除、旧 cohort の支払条件維持、producer の既存 conviction debit と consumer の追加没収なし、removal の旧 phase 改変の拒否、履歴用認証 predicate の遅延評価、EXEC の native Claim row 欠落時の保留、認証行欠落時の retry clock 保持を扱う。EXEC reader は既存の native outcome / selected-approval を使い、単に native horizon が閉じただけでは解除しない。cold carriage preflight / restore と各 fold の delta apply / revert も確認する。native reader の正当な selected-approval 後の positive payout / full lifecycle は未検証である。旧 cohort の試験は認証時の支払いを分離するため retirement を無効にした fixture を用い、旧ルールの retirement schedule を修正するものではない。
- 抽選・承認行は既存の test seam で提示し、その後の認証 verdict / 支払 / 退役は production fold を通す。producer conviction は既存の writer seam、認証行欠落と native receipt は fault / reader fixture である。外部 filer の実署名・反証 admission、実モデル、native 全経路、node の restart / pruning / IBD を実証した試験としては数えない。途中の compile error と最終 source を反映するための意図的な build 中断は成功に加算しない。
- 同じ最終 unit binary の EXEC 回帰は95成功・0失敗・既存 resource probe 1 ignored(13.90秒)。A-2 は17成功(22.70秒)、aux registry は6成功(8.98秒)、carriage は4成功(0.02秒)、material は5成功(0.01秒)、typed material は3成功(0.02秒)。実行名を照合し、EXEC 内4件と資料群2件の重複を除くと unit は168件の異なる試験が成功した。0件に一致した中間 filter は加算しない。generator の検査は全66回の出力を一時ディレクトリに向け、`F/R`・既存の `F/f/V` dispatch が残ることを確認した。
- integration は release 9成功(0.16秒)、activation pool 5成功(0.31秒)、testnet12 identity 1成功(0.06秒)、既存 G-15 の liability horizon / 報酬保留 1成功(0.63秒)。4 binary の build は6分53秒。unit と合わせて184件の異なる試験が成功した。testnet-12 の pin・既存 baseline は変更していない。node の最終 `cargo check --lib -j 3 -p kaspa-consensus --no-default-features` も同じ scoped 環境・opt-level override で成功した(5分28秒)。段階2の node 受入試験の完了を意味しない。文書 index / link 検査は既知43・新規0。
- **追加回帰 (2026-10-18):** native dependency reader の正例と、同じ admitted slice の Claim row 欠落時の fail-closed を追加。期限は root の native retention terms から取得し、元の予約担保を保ったまま slice を受理した後、prefix と全3 slice の Final Claim row にそれぞれ canonical enrollment index / authority / Final approval row を与え、全期限経過後に `Ready` になることを確認した。approval enrollment 欠落は期限後も `Hold`、native Claim row 欠落は `Hold(u64::MAX)` となる。署名・receipt-round admission の node 経路や実 Gate C payout / 全 native lifecycle はこの reader fixture の対象外。`cargo test --config 'profile.test.package.kaspa-consensus-core.opt-level=0' -j3 -p kaspa-consensus-core --lib rfc23 -- --test-threads=1`: 45成功・0失敗。EXEC amendment-1 全体は54成功・0失敗。文書検査は `check_docs.py` PASS (819 markdown、既知43・新規0)。段階2完了条件は未達のまま。
- **追加 provenance reader (2026-10-18、`3737b951a`):** claim の authenticated birth context から、元 frame が保持する origin header の PAV3 binding / manifest を取得するAPIを追加した。受理 attempt の完全一致、claim ID、foldで記録した execution key、origin header hash と記録 block の一致を要求し、欠落した header から manifest を推測しない。header と PAV3 body は Borsh decode 前に shape scan と native reservation を行い、返却 binding にも lease を保持する。provider archive・各 material unit の認証、checkpoint record への収録、pruning / IBD 転送を行う接続ではない。
  - `rfc23s2_admission_context_recovers_the_exact_pav3_manifest_from_its_origin_header`: 1成功・0失敗。実 fold で作った birth frame にPAV3 origin headerを保持させて、signed manifestの出所を照合し、別block hashを持つ置換headerを拒否する。PAV2 frameではbindingを導かない条件も同じ回帰群で確認済み(5件成功・0失敗)。
  - 段階2は未完了。checkpointのfold-derived record / liability state builder、全native claimとそのnamespace付き依存閉包、実material archiveの収録・保持・pruning / IBD / restart / reorg転送、frontier・transaction settlement・native lifecycle接続、node受入drillは引き続き未実装または未実測である。§6の統合キャンペーン(同一候補artifactで複数node・複数claimの正常系と攻撃系を別シナリオとして実行)も未実施で、従来のunit・node fixture別々の合格記録をこの条件の代替にしない。
- **namespace-aware checkpoint identities (2026-10-18):** checkpoint record key と入力 edge を raw `Hash64` から `ChainClaim` / `KernelClaim` / `ExecSession` / `ExecSlice` の `PalwClaimLocalWorkRefV1` に変更し、canonical codec と native preflight scanner もこの型を読むようにした。同じ raw bytes の chain claim と kernel claim は別 vertex になり、依存・有罪影響の探索で誤って alias しない。実行 root / reservation が同一なら、従来どおりその実際の共有 identity による依存は残る。`rfc23s2_` の core unit 群は43成功・0失敗(0.74秒、crate opt-level 0、`-j 3`)。namespace 衝突を直接検査する回帰を含む。これは checkpoint graph の型基盤であり、native record builder・material archive・runtime credit / frontier / settlement への接続や §6 の統合キャンペーンは未実装 / 未実測である。
