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
