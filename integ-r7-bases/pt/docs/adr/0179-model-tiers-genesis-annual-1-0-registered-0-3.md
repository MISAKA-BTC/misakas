# ADR-0179 — モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **統治する前提 — [PRINCIPLES §9](../PRINCIPLES.md)(凍結前提、2026-10-15 ユーザー決定):** モデルの二階層と、モデル単位の検証の不採用。本 ADR は §9 を実装上の規則として書き下したものであり、§9 と衝突する場合は §9 が優先する。本 ADR は §9 を再審しない。

* **Status:** Accepted(ユーザー決定 2026-10-15 第 4 回・第 5 回、[判断記録](../history/decisions/2026-10/user-decisions-2026-10-15-adr-rfc16-rfc17.md) §7b・§7c)。実装は休眠(fence `palw_model_tier_v1`、全 preset で `None`、有効化は承認していない)。build・試験は未実施で PASS ではない。
* **Supersedes:** [RFC-0017](../rfc/0017-model-eligibility-examination-and-renewal.md) のモデル資格審査(Registered → Probation → Eligible → Lapsed、抽選審査、更新、失効、試行手数料・試行上限)。RFC-0017 は本 ADR の階層制度の文書に改定した。
* **Related:** [ADR-0175](0175-registered-models-are-permanently-immutable.md)(登録の不変性)、[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)(配分 `α = 1.5`、30 日平均)、[RFC-0016](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)(V+)、[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md)(Gate C)、[PRINCIPLES §8](../PRINCIPLES.md)(暗号学的な計算証明を使わない)。

## 決定

1. **genesis と年次フォークで指定されたモデルは 1.0 である。** `ApprovedModels(h)` は、genesis のモデルと、高さ `h` までに有効化された年次フォーク(ソフト / ハード)が指定した「モデル ID + 固定 artifact root」の集合である。
   * 年次フォークは `ApprovedModels` に**追加するだけ**で、既存モデルを上書きしない(ADR-0175)。
   * リストへの指定は政策的決定であり、「公開されている」「独立検証可能である」ことの技術的証明ではない。
2. **後から登録したモデルは、登録した時点から審査なしで 0.3 に固定する。**
   * 昇格・更新・失効・試行手数料・試行上限はない。
   * 何年経っても、どれだけ使われても 0.3 のままである。
   * 1.0 になる経路は年次フォークでの指定だけである。
   * registry の `Active` などの状態は運用状態であり、階層の資格とは関係しない。
3. **モデル単位の検証・審査は行わない(不採用)。**
   * 理由(ユーザー): モデルに対して検証を行うと、その検証において何を信頼するか(誰が・何を根拠に「正しく共有・運用されている」と判断するか)という問題が新たに発生し、それは解決しない。
   * チェーンは、モデルの公開状況・保持者数・取得可能性・共有状況を審査しない。
   * ユーザー判断(原文):「genesis/fork モデルは 1 倍で有効、後から追加されるモデルは 0.3 倍で、そこから検証などはなく 0.3 倍のまま固定。つまりモデルに対しての検証がなくなった。」
4. **係数は配分の重みに一度だけ掛ける。**
   * `A_m = c_m · S̄_m^1.5`(`α = 1.5`、Miner + Verifier の 30 日平均の適格 bond、ADR-0177)。
   * この `P_m = A_m / ΣA` を、物理ブロック機会(RFC-0016 V+)と coinbase 配分の両方が使う。
   * 受理ブロックの報酬に係数を重ねて掛けない(0.09 倍にしない)。
5. **登録モデル全体に発行上限 `κ` を設ける。** 多数のモデルを登録しても 0.3 の枠は増えない。
   * 登録 tier の重みの合計が `ΣA` の `κ` を超えると、登録 tier の各重みを同じ割合で縮める。
   * 縮めた分は発行しない。1.0 の tier へ再配分もしない。
   * bond 総量と共通の Q/B/R/F 上限(ADR-0176)は、そのまま維持する。
6. **claim 単位の検証は、すべての階層で維持する。** ユーザー判断(原文):「これは残す。そうじゃないと claim の検証ができない」。
   * モデル単位の検証を行わないことは、claim の検証を省くことではない。
   * 各 claim は階層に関係なく、確率的検出、公開証拠による局所化、決定論的裁定(court)、claim ごとの抽選検証者による認証を受ける。
   * 認証は RFC-0018 の `SecurityCertified`(Gate C)であり、正直な割合の明示的前提に基づく。
   * 認証されていない claim は、どの階層でも次のものを進めない: fork choice、決済深度、Rule E 参加、settled anchor、weight。genesis・年次指定モデルも Gate C を免除されない。
   * 階層は経済上の権利だけを決める。

## 数値

| 名前 | 値 | 種別 |
| --- | --- | --- |
| `PALW_MODEL_TIER_APPROVED_PERMILLE_V1` | 1000(1.0) | 決定 |
| `PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1` | 300(0.3) | ユーザーの設計値(fence の値として名前付き) |
| `PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1`(`κ`) | 300‰ | 設計値・未承認(測定値記録なし。validation は arm を拒否する) |

## 他の規則との関係

| 規則 | 扱い |
| --- | --- |
| RFC-0017 の資格審査 | fence `palw_model_tier_v1` の下では到達しない。全ての資格 read は「対象外」になり、inner 74/75 の審査 object は拒否され、sweep は何もしない。`palw_model_eligibility_v1` は、tier fence が同じ高さ以下にあるときだけ arm できる(validation) |
| RFC-0017 の Probation に基づく SecurityCertified 不許可(U-2 の bar) | 対象が存在しないため削除した。Gate C の claim 単位認証だけが安全性の gate である。Gate C 自体は弱めていない |
| RFC-0016 の entrant reserve(`ε`)と RFC-0017 の Probation 予算 | tier fence の下では審査も昇格もなく entrant が存在しないため、無意味になる。`ε = 0` とする。`κ` が tier の唯一の上限である |
| ADR-0177 の配分 | `c_m` を `A_m` に一度だけ掛け、`κ` を適用する(`roll_epoch_v3_tiered`) |
| ADR-0175 | `ApprovedModels` は追加のみ。membership は class 自身の artifact root と一致しなければならない |

## 実装(休眠、未 build、未試験)

* **module:** `consensus/core/src/palw_model_tier_v1.rs`(単体試験 source `palw_model_tier_v1/tests.rs`)。
* **fence:** `Params::palw_model_tier_v1: Option<PalwModelTierFenceV1>`。
  * fence の値は、genesis list、年次 list(それぞれの有効化高さ付き)、`c`、`κ` である。
  * 全 preset で `None`。identity では collapse、named list、Some-only の fingerprint(list と値を高さの横に書く)、visitor(各年次 list の有効化高さを含む)。
  * validation:
    * 追加のみを検査する。同じモデル ID の再掲を拒否する。年次 list は fence 以後で、有効化高さが厳密に増加しなければならない。
    * 配分 fence と RFC-0018 Gate C が同じ高さ以下にあることを要求する。
    * その上で arm を拒否する。
  * fork-id の probe 名と、state params の mirror(`#[borsh(skip)]`)がある。
* **狭い interface:** `palw_model_tier_v1(state, params, class) -> PalwModelTierV1`(`_at_v1` は高さ指定版、`_of_model_v1` は配分 model key 版)。RFC02 の登録 lane と、RFC-0019 の年次カタログ(`ApprovedModels(h)` をカタログに置き換える)が使う。
* **配分:** `PalwBondBudgetStateV1::roll_epoch_v3_tiered`(fold は tier fence 以後で使う)。
* **RPC:** registry 行(wire version 4、`RpcPalwModelLifecycle` の proto field 46/47、この message 内の番号)に `modelTier` と `modelTierPermille` を載せる。
* **新しい番号:** object tag、kernel inner kind、aux table、RPC op、`messages.proto` 番号はいずれも割り当てていない。
* **RFC-0019 による置き換え(2026-10-15、branch `integ/rfc0019-annual-catalog`):** fence の値の genesis list と年次 list は、[RFC-0019](../rfc/0019-annual-model-upgrade.md) の年次モデルカタログの連鎖 `PalwModelTierFenceV1::catalogs`(`AnnualModelCatalogV1` を `previous_catalog_root` でつなぐ。genesis = edition 0)に置き換えた。並行する list は残さない。階層の API(`palw_model_tier_v1`、`_at_v1`、`_of_model_v1`、`palw_model_tier_of_v1`、係数・重み・cap)は呼出元から見て不変。`PalwTierSourceV1::AnnualFork` は `index` ではなく `edition` を持つ。identity では、まだ有効でない edition を中身ごと外し(旧新の build が切替まで同じ identity)、fork id は各 edition の高さで gate する(RFC-0019 §7.2–7.3)。上の「追加のみ」の検査は、連鎖全体での class id の一意性・root の連鎖・高さの厳密な増加として連鎖の検証に入った。

## 試験 source

| 要件 | 試験 |
| --- | --- |
| 登録モデルは登録時から審査なしで 0.3。何年後でも 0.3 | `palw_model_tier_v1/tests.rs` `a_registered_model_is_0_3_from_registration_with_no_review_and_forever`。fold 側 `palw_state_v2/tests/model_eligibility_fold_v1.rs` `round5_no_model_review_under_the_tier_fence_and_a_registered_model_stays_0_3` |
| 1.0 になる経路は年次フォークの指定だけ | `only_an_annual_fork_listing_makes_1_0` |
| 年次 list は追加のみで上書きしない | `an_annual_list_only_adds_and_never_re_roots_a_model` |
| 係数は一度だけ(0.09 にならない) | `c_m_is_applied_once_never_0_09` |
| N 個登録しても tier の合計は増えない | `n_registrations_never_raise_the_registered_tier_total` |
| どの階層でも合意 credit に SecurityCertified が必要 | `palw_rfc0018_v1/tests.rs` `rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt` |
| reorg・IBD・restart で一貫する | `the_tiered_roll_is_the_same_after_reorg_ibd_and_restart`、fold 試験の carriage round-trip |
| fence は休眠で、arm すると拒否される | `the_fence_is_dormant_some_only_and_refused_when_armed` |

## 追記(2026-10-16 ユーザー決定):RFC-0017 の方針を採用した理由

**ユーザーが追加したモデルが共有されているという前提は、保証できない。**
- 追加モデル(add model)の重みが、miner や検証者の間で実際に共有されていることを、チェーンで保証する方法は無い。
- Torrent などの共有の仕組みを作っても、「誰がそれを検証するのか」という問題が残る。
- 取得が遅いとき、それが攻撃による遅延なのか、正直な事情によるものなのかを判別できない。
- このため、共有を審査・検証する仕組みは採らない(PRINCIPLES §9 のモデル単位の検証不採用と同じ理由)。

**解決:追加モデルの報酬とブロック生成の権利を 0.3 倍にする。**
- miner bond から生じるブロックと報酬には上限がある。不正をして高速に claim しても、実際に計算して claim しても、どちらも上限に当たる。つまり bond が時間内にできる claim の量は、どちらでも変わらない(ADR-0176、RFC-0016)。
- この制約のもとでは、0.3 倍の追加モデルは 1.0 倍の genesis・年次フォークのモデルより不利である。追加モデルで攻撃を成立させるには、より多くの bond が要る。
- この性質を理由に本設計を採用した。モデル単位の検証や共有の証明に頼らず、bond による上限と階層係数で、共有を保証できない追加モデルの影響を抑える。

**前提として肝心なこと:不正による損失が利益を上回る。**
- claim を 1 回不正すれば slash される。
- その slash による損失は、不正によって得られる利益を上回らなければならない(ADR-0176 の bond 予算、ADR-0032 の告発者取り分の上限と合わせて評価する)。
- この条件が崩れる構成では、0.3 倍の不利だけでは抑止にならない。

## 追記(2026-10-16 ユーザー決定):Position は追加モデルだけ

genesis・年次フォークのモデル(1.0)は、Model Position 化できない。Position を持てるのは、追加モデル(0.3)だけである。
正規モデルの指定から先行者利益を生まないためである。規則と実装は [ADR-0162 の 2026-10-16 改定](0162-the-pair-opens-on-a-virtual-reserve.md)。
