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
