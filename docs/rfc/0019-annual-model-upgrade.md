# RFC-0019 — 年次モデル追加フォーク: 段階的ソフトウェア展開・年次モデルカタログ・合意は不変

> **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。本RFCはこの前提の下にあり、衝突する場合は前提が優先する。
> **凍結前提 — [PRINCIPLES §9](../PRINCIPLES.md)(2026-10-15 ユーザー決定):** genesis と年次フォークで指定されたモデルは 1.0、後から登録したモデルは登録時点から審査なしで 0.3 に固定する。モデル単位の検証・審査は不採用(何を信頼するかの問題が解決しないため)。claim 単位の検証(court、Gate C / `SecurityCertified`、claim ごとの抽選検証者認証)は全モデルで維持する。本RFCは §9 を再審しない。§9 の「年次フォーク」を、ソフトウェア展開と合意規則の切替を分けた手順として具体化するだけである。
> **暗号学的な計算証明を使わない — [PRINCIPLES §8](../PRINCIPLES.md)(恒久):** 本RFCが使う暗号はハッシュ(カタログ root、成果物の sha256)と署名(リリース manifest の署名)だけであり、計算の正しさの証明には使わない。カタログへの掲載は「公開されている」「独立検証可能である」ことの技術的証明ではない(§9.1)。
> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) の bond 当たり Q/B/R/F 上限と共通 DAA 拘束、[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) のモデル入手不介入と配分を適用する。[ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)(登録モデルは永久不変)、[ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md)(モデル階層)の下にある。

**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存の protocol/CLI/API 識別子は維持する([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md))。

* **Status:** Draft — 2026-10-15。設計(§1–§10)、休眠実装(**implemented, dormant, unverified**、§15)、リリース道具と drill driver(§8・§9・§12)。cargo build/test と実ノード試験は未実施で、どの Rust 項目も PASS ではない。リリース道具 2 本の Python self-test と、使い捨て鍵による `ssh-keygen` 署名・検証の往復は本 lane が実行した(§15.4)。
* **Decision:**
  1. 三つを分ける。(1) Solana 型の段階的ソフトウェア展開(コードは fence の後ろで出荷し、ノードは旧規則のまま順次更新、決めた DAA で全員が切り替わる)、(2) 年次モデルカタログ(`ApprovedModels(h)` の唯一の保存形式)、(3) PALW + DAG の合意(**不変**)。**validator 投票・PoS の readiness 投票を合意に持ち込まない。** readiness の証拠は運用だけに使う(§9)。
  2. `AnnualModelCatalogV1` を `previous_catalog_root` で連鎖させ、**追加のみ**とする。genesis はカタログ edition 0 である(§5)。
  3. 階層規則は `model_tier_permille(class_id, artifact_root, h) = 1000`(h で有効なカタログが `(class_id, artifact_root)` を含む)、それ以外は 300。登録 registry の `Active` 状態は 1.0 ではない。係数は配分重み `A_m = t_m · S̄_m^1.5` に一度だけ掛ける(§6)。
  4. 合意に入るのは、固定されたカタログ root、各 edition の有効化 DAA、有効化後の規則(配分 roll が読む階層)の三つだけである(§3)。
  5. 段階的展開では、有効化前に旧バイナリと新バイナリが同じブロック・state root・fork choice を出すこと、有効化時点で旧バイナリに残ったノードは分岐し得ること、その分岐を fork id が名前付きで検出することを要件とする。**rolling update は「決して fork しない」ことを意味しない**(§7、§10)。
  6. **ローカル LLM の年次追加は、genesis モデル ID の上書きではない。** 既存 genesis の `class_id` / `artifact_root` と edition 0 を不変に保ち、別の新規モデル ID と固定 root を年次 edition に**追加のみ**で載せる。drill の主目的はバイナリのバージョン変更自体ではなく、Solana 型の事前展開・DAA 有効化によって追加が実際に認識され、旧モデル・新モデルのローカル推論がともに動くかの確認である(§11 R-7・R-8、§12.1)。
* **Activation:** **新しい fence はない。** カタログ連鎖は ADR-0179 の fence `palw_model_tier_v1` の値そのものであり(`PalwModelTierFenceV1::catalogs`)、各 edition の有効化 DAA はその値の中の副 fence である。全 preset で `None`、flag-day リストに含めない、fingerprint は Some-only、`Some(never())` は `None` へ畳む、arm すると `validate_palw_model_tier_v1` が拒否する。**最初の実使用は testnet-12 の DAA 14,000 更新**(同じカタログ = edition 0、新モデルなし。§12)であり、その有効化はユーザーの承認による。
* **Baseline:** 検証の基準は統合ブランチ `integ/rfc02-rfc06-12-g14` の `9fad35323`、ADR-0179 の階層実装は `8b58ae301`(`integ/decisions-1015` `6db8ee95a` から merge)。実装は `integ/rfc0019-annual-catalog`。本文の code 行番号はこの branch のもの。
* **Not activated by:** 本RFCの merge、設計値表(§13)、試験 source の存在、driver の出力、readiness の数値、リリース manifest の署名。
* **Normative language:** MUST / MUST NOT は fence 有効化後の規則、またはリリース手順上の義務である。設計値は名前付きの未承認値であり本番承認ではない。

## ユーザー方針 2026-10-15(本RFCを拘束する)

1. **採用した設計:** モデル追加フォークは、Solana 型の段階的ソフトウェア展開・年次モデルカタログ・PALW+DAG 合意(不変)の三つに分ける。合意へ validator 投票や readiness 投票を持ち込まない。readiness の証拠は運用専用である。
2. **feature gate モデル:** コードは gate の後ろで出荷し、合意は旧規則のままノードを順次更新し、固定した有効化 DAA で新カタログが全員に有効になる。有効化時点で旧バイナリに残ったノードは分岐し得るので fork_id がそれを検出しなければならない。
3. **階層の適用は一度だけ:** `A_m = t_m · S̄_m^1.5`(α = 1.5、30 日の miner + verifier 資本)。採掘権(RFC-0016 V+)と報酬予算は同じ `P_m` から出す。受理ブロックの報酬に 0.3 を重ねて掛けない。登録 tier 全体の固定予算と bond 単位の Q/B/R/F 上限は維持する(登録モデルしかない場合、正規化だけでは絶対上限にならないため)。
4. **過去を再評価しない:** 有効化前に受理された claim は旧規則で完結する。有効化をまたいで merge される RED ブロックについて「受理 DAA」を一様に定義し、権利を二重に発行しない。
5. **階層は経済だけ:** 年次掲載モデルにも safe_frontier・settlement 上の特権はない。claim 単位の Gate C / `SecurityCertified` は全モデルに適用する。
6. **最初の開発段階:** testnet で**同じカタログのまま**「rolling でノード更新 → 指定 DAA で切替 → 全ノード収束」を再現する。その後で初めて 0.3→1.0 の年次掲載と kernel 更新を加える。
7. **時期(2026-10-15 追加):** 稼働中の testnet-12 を統合実装へ更新し、新 fence を DAA 14,000(10-22 22:00 JST 頃)で有効化する([リリース計画](../design/palw/t12-daa14000-release-plan-2026-10-15.md))。これが本RFCの段階的展開の最初の実使用である(同じカタログ = edition 0、新モデルなし、rolling 更新、DAA 14,000 で切替)。順序は「全部完了 → 一括検証 → 実ノード検証・測定 → 出荷バイナリで fence をまたぐ drill → 承認後に配備」で固定。

## 0. 要約

| 分けるもの | 何であるか | 合意に入るか |
| --- | --- | --- |
| (1) 段階的ソフトウェア展開 | 署名済みリリース manifest、再現可能 build、checksum、運用者が時期を選ぶ install、readiness の測定 | **入らない**(運用) |
| (2) 年次モデルカタログ | `AnnualModelCatalogV1` の連鎖。edition 0 = genesis、各 edition が年次フォークの追加分 | root と有効化 DAA と有効化後の階層規則だけが入る |
| (3) PALW + DAG 合意 | GHOSTDAG、PALW の admission・court・Gate C・rule E・settlement | **変えない** |

カタログの効果は一つしかない。**有効化後、ある allocation epoch の roll がモデルの配分重み `A_m` に掛ける係数が 300‰ から 1000‰ になる**。それ以外(block validity、fork choice、Gate C、frontier、settlement、heartbeat、BASE-0)は何も変わらない。だから切替の前後で何が一致すべきか、どこで分岐し得るかを狭く定義できる(§7、§10)。

## 1. 用語

| 用語 | 定義 |
| --- | --- |
| edition | カタログの版。edition 0 は genesis カタログ、edition `e ≥ 1` は年次フォーク 1 回分の追加 |
| catalog root | `root(edition)`(§5.2)。`previous_catalog_root` により、edition `e` の root はそれ以前の全 edition に commit する |
| `ApprovedModels(h)` | 高さ `h` で有効な edition 0..=active(h) の和集合(ADR-0179 の集合そのもの) |
| 切替(switch) | ある edition の `activation_daa`、または flag day の fence 高さ(testnet-12 の DAA 14,000) |
| 新バイナリ / 旧バイナリ | その切替を schedule に持つ build / 持たない build |
| readiness | 新バイナリへの移行度合いの**測定値**。合意の入力ではない |

## 2. Solana(Agave)からの参照 — 採ったパターンと採らなかったもの

コードの複写・clone・実行はしていない。web で読んだのは次の公開ソースと文書だけである。

| Agave のパターン | 出典 | MISAKA での対応 | 採らなかったもの |
| --- | --- | --- | --- |
| **gate registry:** feature id ごとの module と名前表、`active`(id → 有効化 slot)と `inactive` の集合、名前表全体の hash を feature set の識別子にする | [`feature-set/src/lib.rs`](https://raw.githubusercontent.com/anza-xyz/agave/master/feature-set/src/lib.rs) | `Params::palw_fences_v1` の網羅的な名前表と、`consensus_params_id` / `consensus_schedule_id` / fork id(既存)。カタログ edition は `palw_model_tier_v1` の値の中の副 fence で、schedule id にも fork id にも載る(§7) | feature account(鍵の保有者が作ると有効化される on-chain 状態)。MISAKA では有効化高さをバイナリに固定する |
| **境界での有効化:** pending の feature は次の epoch の最初の slot で有効になり、有効化 slot を記録する。genesis と snapshot 復元では pending を含めずに再計算する | [`runtime/src/bank.rs`](https://raw.githubusercontent.com/anza-xyz/agave/master/runtime/src/bank.rs)(`compute_active_feature_set`、`compute_and_apply_new_feature_activations`、`compute_and_apply_genesis_features`) | edition は自分の `activation_daa` で有効になる。階層は allocation epoch の roll で一度だけ読まれる(§6.4)。再起動・IBD・reorg では Params と DAA だけから同じ答えを再計算する(§11 D-1) | slot / epoch 境界そのもの。MISAKA の境界は DAA と allocation epoch の roll |
| **状態の報告:** `solana feature status` が feature ごとに inactive / pending / active since を表示し、cluster の software version と feature set の分布を表示する | [`cli/src/feature.rs`](https://raw.githubusercontent.com/anza-xyz/agave/master/cli/src/feature.rs) | RPC `getPalwModelRegistry` v4 の `annualCatalog`(edition ごとに SCHEDULED / ACTIVE、root、次の有効化)と `misaka palw registry`(§7.7)。分布は `misaka-release-readiness.py` が測る(§9) | **stake 加重の 95% 検査による有効化の可否判断。** Agave の CLI は有効化前に stake と RPC ノードの 95% 以上を要求するが、これは stake 投票であり採らない。MISAKA の ~95% は運用目標の測定値で、合意にも有効化にも効かない |
| **ソフトウェア install と有効化の分離:** 機能は無効のまま出荷され、validator が先に更新し、有効化は別に起こる | [Backward compatibility policy](https://docs.anza.xyz/backwards-compatibility) | 署名済み release manifest と運用者が選ぶ install 時期(§8)。有効化は schedule に固定した高さで起こる | on-chain 命令による有効化 |
| **cluster の順序と soak:** testnet → devnet → mainnet-beta、同時に一つずつ、soak 期間 | [Feature Gate Activation Guidelines](https://github.com/anza-xyz/agave/wiki/Feature-Gate-Activation-Guidelines) | 時期案 T-90 … H(§8.5)。testnet での混在バイナリ試験を mainnet より先に置く | — |
| release channel(edge / beta / stable)と backport | [`RELEASE.md`](https://raw.githubusercontent.com/anza-xyz/agave/master/RELEASE.md) | PREPARED(stable release)と LOCKED_IN(中止しない約束)の区別(§4) | version floor の数値(この文書は定めていない) |

**採らなかったものの理由。** Agave の有効化は「feature 鍵の保有者が on-chain で有効化する」「CLI が stake の 95% を確かめる」という二つの権限・投票に支えられている。MISAKA は合意に投票を入れない(ユーザー方針 1)ので、有効化の高さをバイナリに固定し、合意の外で readiness を測るだけにする。その代わり、有効化の取り消しは on-chain 命令ではなく **中止リリース** という別の展開になる(§4.2)。

## 3. 合意に入るもの・入らないもの

| 項目 | 合意 | 扱い |
| --- | --- | --- |
| バイナリが持つカタログ連鎖(各 edition の番号・高さ・root) | **入る** | `consensus_params_id` の raw 値、`consensus_schedule_id`、fork id。identity id には有効化済み(genesis 有効)の edition だけ(§7.2) |
| 各 edition の `activation_daa` | **入る** | fence 高さとして schedule と fork id の gate に入る(§7.3) |
| 有効化後の規則 | **入る** | allocation roll が読む階層 `t_m`(§6) |
| PROPOSED / TESTED / PREPARED / LOCKED_IN / ABORTED | 入らない | release manifest と運用道具だけ |
| readiness の 5 指標、~95% | 入らない | `misaka-release-readiness.py` の測定と報告(§9) |
| version の自己申告、schedule id の広告 | 入らない | 偽装できるので readiness では SELF-REPORTED と表示し、相互運用試験を必須にする |
| モデルごとの自動昇格審査 | **存在しない** | PRINCIPLES §9.3 |

## 4. 運用 lifecycle

### 4.1 状態

| 状態 | 意味 | どこにあるか |
| --- | --- | --- |
| PROPOSED | カタログ案(モデル・root・kernel version・plan id)と有効化 DAA 案の公開 | 草案 |
| TESTED | その案を testnet / devnet で混在バイナリ試験した(§11 R 系) | 試験網のバイナリ、drill 記録 |
| PREPARED | stable release がその edition と有効化 DAA を持って出荷された | 出荷バイナリ、署名済み manifest |
| LOCKED_IN | リリース責任者が「この後は中止リリースを出さない」と約束した時点。合意上は PREPARED と同じ | 署名済み manifest |
| ACTIVE | `activation_daa ≤ tip` | チェーンから導出 |
| ABORTED | LOCKED_IN の前に中止した。中止リリースを出す | 署名済み manifest |

* ノードが導出できるのは **SCHEDULED(バイナリが持ち、高さ未満)と ACTIVE** だけである(`PalwCatalogBinaryStateV1`、`palw_annual_catalog_v1.rs:375`)。RPC はこの二つしか報告しない。PREPARED と LOCKED_IN の区別は manifest にしかない。
* バイナリが持てる edition は PREPARED・LOCKED_IN・ACTIVE だけである(`PalwCatalogLifecycleV1::may_ship_in_a_binary`、`:367`。manifest 検査も同じ)。
* **モデルごとの自動昇格審査はない。** lifecycle は edition(カタログの版)単位の手続きであり、モデルを審査する手続きではない。

### 4.2 中止(ABORTED)は別の展開である

有効化の高さはバイナリに固定されるので、PREPARED のバイナリを入れたノードは、何もしなければ H で切り替わる。LOCKED_IN の前に中止するには、その edition を持たない **中止リリース** を出し、PREPARED を入れた全ノードが入れ替える必要がある。中止リリースと PREPARED リリースは H で互いに分岐する(fork id が名前付きで拒否する)。中止そのものが逆向きの協調展開である。これが T-14 の測定と T-7 の LOCKED_IN を置く理由である。LOCKED_IN の後は中止しない。追加のみの規則(§5.4)により、掲載したモデルを後から外す経路もない。

## 5. カタログの型と規則

### 5.1 型(`consensus/core/src/palw_annual_catalog_v1.rs`)

```text
ApprovedModelV1      { class_id: Hash64, artifact_root: Hash64, kernel_version: u16, verification_plan_id: Hash64 }   (:64)
AnnualModelCatalogV1 { edition: u32, activation_daa: u64, previous_catalog_root: Hash64, models: Vec<ApprovedModelV1> }  (:80)
PalwAnnualCatalogChainV1 { editions: Vec<AnnualModelCatalogV1> }                                                    (:164)
```

* 所属は `(class_id, artifact_root)` で決まる。`kernel_version` と `verification_plan_id` は運用者が読む commitment であり root に束縛されるが、階層の入力ではない。`kernel_version = 0` と `verification_plan_id = 0^64` は「class 自身の登録時の binding」を意味する。
* `models` は**その edition が追加するモデル**である。`ApprovedModels(h)` は edition 0..=active(h) の和集合である。

### 5.2 正規の符号化と root

```text
catalog_bytes_v1 = le16(1) ‖ le32(edition) ‖ le64(activation_daa) ‖ previous_catalog_root[64] ‖ le32(n)
                   ‖ n × ( class_id[64] ‖ artifact_root[64] ‖ le16(kernel_version) ‖ verification_plan_id[64] )   class_id 厳密昇順
root             = blake2b-512(key = "misaka-palw/annual-model-catalog/v1", catalog_bytes_v1)
```

borsh ではなく手書きの byte 列にしたのは、リリース道具(Python 標準ライブラリだけ)が同じ root を再計算するためである。Rust 側(`catalog_bytes_v1`、`:98`、`root`、`:123`)と Python 側(`misaka-release-manifest.py` `catalog_root`)の一致は、両方に置いた同じ試験ベクトル(R0・R1・EMPTY)で確かめる(§11 A-1)。

### 5.3 連鎖の規則(MUST、`PalwAnnualCatalogChainV1::validate`、`:195`)

1. 連鎖は edition 0 から始まる。edition 0 は `edition 0`、`activation_daa 0`、`previous_catalog_root = 0^64` である。
2. edition `i ≥ 1` は番号 `i`、`previous_catalog_root = root(edition i−1)`、モデルを 1 つ以上追加し、`activation_daa` は実在の高さ(`u64::MAX` でない)で前の edition より厳密に大きい。
3. 各 edition のモデルは `class_id` の厳密昇順(一つの集合に一つの符号化)、ゼロでない id と root、上限 `PALW_ANNUAL_CATALOG_MAX_MODELS_PER_EDITION_V1` 以下。edition 数は `PALW_ANNUAL_CATALOG_MAX_EDITIONS_V1` 以下。
4. `kernel_version ≤ PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1`(このバイナリが実装する kernel)。超える edition はそれを実装するバイナリを要する協調 upgrade であり、「知らないものは飛ばす」ことはしない([ADR-0172 §3](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md))。
5. 年次 edition は階層 fence 以上の高さで有効になる(`PalwModelTierFenceV1::validate`、`palw_model_tier_v1.rs:83`)。

### 5.4 追加のみ(ADR-0175)

* **MUST NOT** 既存モデルを上書き・削除しない。既存モデルの root を変えない。同じ `class_id` は連鎖全体で一度だけ現れる(同じ root での再掲も、別 root での再掲も拒否)。
* 過去の edition を書き換えると次の edition の `previous_catalog_root` が合わなくなる。
* リリースをまたいでは、新しいリリースの連鎖が前のリリースの連鎖を**接頭辞として**含まなければならない(`PalwAnnualCatalogChainV1::extends`、`:234`。manifest の `--previous` 検査も同じ)。
* **「genesis モデル ID の更新」は ID の置換・再割当てを意味しない。** genesis assembly の既存登録、edition 0、旧 `class_id`、旧 `artifact_root`、登録 ID / line / 経済履歴は変更しない。新しい重み・tokenizer・計算仕様等は、別の `class_id` と固定 `artifact_root` を持つ**独立したモデル**として登録し、edition 1 以降に追加する。`genesis_id`(genesis 8 テンプレートの番号)を振り直すことと、合意上のモデル追加を混同しない(ADR-0175)。旧 ID を別 root に向ける alias / 自動切替は認めない。

### 5.5 genesis = edition 0

* edition 0 は genesis モデルを明示して並べる(SHOULD)。並べると edition 0 の root が genesis モデル集合に commit し、manifest と RPC で読める。
* genesis assembly が登録した class(`registrant_bond = None`)は、並べなくてもチェーン上の事実として 1.0 である(ADR-0179 の `PalwTierSourceV1::GenesisAssembly`、既存の挙動を保持)。したがって edition 0 が空でも階層は同じであり、testnet-12 の DAA 14,000 更新(§12)では edition 0 の中身は機能に影響しない。並べるかどうかはユーザー判断(§14 U-5)。

## 6. 階層規則

### 6.1 `model_tier_permille`

```text
model_tier_permille(class_id, artifact_root, h) = 1000  if (class_id, artifact_root) ∈ ApprovedModels(h)   — h で有効なカタログ
                                                = 300   otherwise                                     — 登録 tier(設計値 c)
                                                = 1000  for every model below palw_model_tier_v1      — A-2
```

実装は `model_tier_permille`(`palw_annual_catalog_v1.rs:302`)で、ADR-0179 の**単一の階層 API**(`palw_model_tier_of_v1`、`palw_model_tier_coefficient_permille_v1`、`palw_model_tier_v1.rs:183`)を合成しただけである。別の階層定義は作らない。所属判定は `PalwModelTierMirrorV1::listed_at`(`palw_model_tier_v1.rs:136`)がカタログ連鎖の `listed_at`(`palw_annual_catalog_v1.rs:253`)を呼ぶ。

### 6.2 registry の `Active` は 1.0 ではない

`PalwModelLifecycleV1::Active` は運用状態である(`palw_model_registry_v1.rs:363` に明記)。登録モデルが何年 `Active` でも 0.3 のままであり、カタログに載ったモデルは `Held` でも配分重みは 1.0 である。階層 API は lifecycle を引数に取らない — これが分離の構造である。RPC は `state`(lifecycle)と `modelTier`(階層)を別の field で報告する。

### 6.3 一度だけ、`A_m` に

* `A_m = t_m · S̄_m^α`(α = 1.5、30 日の miner + 認証済み verifier 資本、ADR-0177 V3)を allocation epoch の roll で一度だけ計算する(`roll_epoch_v3_tiered`、`palw_bond_budget_v1/allocation_v3.rs:209`、fold の呼出は `palw_bond_budget_fold_v1.rs:2121`)。
* その `P_m = A_m / ΣA` が RFC-0016 V+ の物理ブロック権利と coinbase 配分の**両方**の入力である。受理ブロックの報酬や claim の reservation に 0.3 を重ねて掛けない(0.09 にしない)。
* 登録 tier 全体の上限 κ(設計値 300‰ of ΣA)を維持する。登録モデルしかないとき、正規化だけでは 0.3 が相殺され絶対上限にならない。κ で削られた分は未発行のまま残り、1.0 のモデルへ再配分しない。bond 単位の Q/B/R/F 上限も維持する。
* RFC-0016 の entrant reserve は階層 fence 以後 0 になる(審査がないので entrant がいない。`palw_rfc0016_fold_v1.rs:142`)。

### 6.4 受理 DAA と境界の規則(権利を二重に発行しない)

1. **claim の受理 DAA** は、その claim を mergeset で適用した chain block の DAA である(`PalwBlockContextV2::daa_score` → `PalwClaimStateV2::accepted_daa`、`transition_builder.rs:4386`)。BLUE と RED の merged block で同じであり、merged block 自身の header DAA ではない(`palw_v2_merged_works` は accepting block の `point` で admission する、`processor/palw.rs:9723`)。
2. **階層は allocation epoch ごとに一度だけ読まれる。** roll はその epoch の最初の chain block(DAA `now`)で起こり、`palw_model_tier_of_model_v1(.., now)` で各モデルの階層を読む。同じ epoch で roll は二度起こらない(`rolled` の判定、`palw_bond_budget_fold_v1.rs:2084`)。
3. したがって、ある claim の権利と報酬が反映する階層は「受理 DAA を含む allocation epoch の roll DAA における階層」である(`palw_catalog_tier_reference_daa_v1`、`palw_annual_catalog_v1.rs:317`)。edition が epoch の途中の H で有効になっても、新しい階層が効くのは roll DAA ≥ H の最初の epoch からである。
4. **RED の跨ぎ:** header DAA < H で掘られ、DAA ≥ H の chain block に merge された RED block は、その accepting block の epoch で一度だけ受理され、その epoch の一つの snapshot から権利を消費する。V+ の権利は epoch ごとに一度だけ発行され、受理時に一度だけ消費される。カタログの有効化は再発行を起こさない。
5. **過去を再評価しない:** H より前に受理された claim は、受理時に予約された権利・escrow・報酬のまま完結する。Final・court・payout は階層を読まない。
6. 推奨(SHOULD): edition の `activation_daa` を allocation epoch の境界に合わせる。合わせなくても規則は決定論的だが、RPC の「ACTIVE」と配分に効く時点が一致して読みやすい(§13)。

### 6.5 階層は経済だけ — Gate C は全 edition に

年次掲載モデルにも genesis モデルにも、safe frontier・safe weight・settlement 深度・rule E 参加・settled anchor・weight の特権はない。認証されていない claim(`SecurityCertified` でない claim)は、どの edition・どの階層でもそれらを進めない(PRINCIPLES §9.5、§0 の 2026-10-15 改定、RFC-0018 §7)。試験は RFC-0018 の `rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt` に edition 0 掲載と edition 1 掲載の両方を加えた(§11 C-3)。

### 6.6 掲載は取得可能性ではない

カタログは、誰も動かしていないモデルや、チェーンに未登録の class を名指してもよい。検証はそれを問わない(ADR-0177 の入手不介入)。そのモデルは資本を持たないので配分行も share も持たず、ブロックの有効性・heartbeat・BASE-0 には何の影響もない(本 module のどの関数もブロックの判定を返さない)。

## 7. 既存基盤への対応

### 7.1 `ForkActivation` と `Params`

* 新しい `Params` field は追加しない。カタログ連鎖は `Params::palw_model_tier_v1: Option<PalwModelTierFenceV1>`(`config/params.rs:3193`)の値 `catalogs` である。ADR-0179 の `genesis` list・`annual` list は**このカタログ連鎖で置き換えた**(並行する list は残さない。`palw_model_tier_v1.rs:56`)。
* 全 preset で `None`(mainnet・testnet・simnet・devnet の 4 literal と t12)。live testnet-12 の挙動と preset は変えない。
* `Params::palw_annual_catalog_chain_v1` / `palw_annual_catalog_activations_v1`(`palw_annual_catalog_v1.rs:428`)が、fence が実在の高さのときだけ連鎖と高さを返す。

### 7.2 identity(`config/params/identity.rs`)— 切替まで旧新は同じ identity

`consensus_identity_id`(`params.rs:4359`)は、全 fence を「genesis で有効(0)か、それ以外(`u64::MAX`)」に正規化してから `consensus_params_id` を計算する。edition の高さもこの visitor が書き換える(`for_each_fence`、`identity.rs:3645`、edition 1 以降。edition 0 は fence ではない)。

**正規化で `u64::MAX` になった edition は、中身ごと identity から外す**(`normalize_values_a_scheduled_fence_drags_with_it`、`identity.rs:651`、`drop_unscheduled_for_identity`)。外さないと `write_value_into` が edition の番号・高さ・root を書き、次の edition を持つ build と持たない build の identity が違い、handshake が配備日に拒否する — rolling update が配備日の分断になる。外すので、旧新バイナリは edition の切替まで同じ identity を共有する。genesis で有効な edition 0 は残る(異なる genesis カタログは別のネットワーク)。

`consensus_schedule_id`(報告用で gate ではない)は**全 edition** の番号・高さ・root を書く(`identity.rs:1960`)。次の edition を持つ build、持たない build、別の中身の edition を持つ build は、切替前から handshake の警告で区別できる。raw の `consensus_params_id` も全 edition を含む(`identity.rs:5239`)。

### 7.3 fork id(`fork_id_v1.rs`)— 切替前は警告、切替後は拒否

edition の高さは `fence_schedule_v1` に入る(visitor 経由)。さらに `fork_id_gate_fences_v1` に edition の高さを OR した(`fork_id_v1.rs:282`)。edition は `palw_model_tier_v1` の値の中にあるので、`palw_fences_v1` は fence 自身の高さしか名指さないからである。結果:

* 切替 H の前: edition を持たない peer は `DisagreeBeforeAnyFence`(警告)。両者は今あるブロックについて一致している。
* H 以後: `DisagreePastFence { fired_through: H }`(拒否)。旧バイナリのノードは、黙って旧重みの arm を伸ばすのではなく、名前付きで切り離される。
* H より下を同期中の新バイナリの新ノードは、H を越えた新バイナリのノードから見て同じ schedule の接頭辞であり保持される。

### 7.4 `palw_model_registry_v1`

registry の lifecycle(Registered … Active … Held)は階層と独立である(§6.2)。registry の RPC 行は `state` と `modelTier` / `modelTierPermille`(ADR-0179 の wire v4)を別に報告する。

### 7.5 ADR-0175・ADR-0179 との統合

* ADR-0179 の `ApprovedModels(h)` は、**このカタログ連鎖そのもの**である(genesis = edition 0)。階層の API(`palw_model_tier_v1`、`_at_v1`、`_of_model_v1`、`palw_model_tier_of_v1`、係数・重み・cap)は呼出元から見て不変である。`PalwTierSourceV1::AnnualFork` は `index` ではなく `edition` を持つ。
* ADR-0175 の不変性は §5.4 の追加のみ規則として連鎖の検証に入っている。

### 7.6 P2P handshake — 新しい field を割り当てない

* p2p の protocol version は 108 のまま(`protocol/flows/src/flow_context.rs:89`)。`VersionMessage` は field 1–16 を使っており(`protocol/p2p/proto/p2p.proto:178-225`)、汎用の拡張 map はない。
* カタログ root と次の有効化は、**既存の field ですでに伝わる:** `consensusScheduleId`(14)は全 edition の root と高さを含み、`forkIdNext`(16)は次の fence 高さ(次の edition を含む)を運ぶ。新しい field を足しても非合意の重複にしかならないので、**本RFCは p2p に何も割り当てない**。readiness の測定は RPC で行う(§9)。

### 7.7 RPC / CLI

* 新しい RPC op は割り当てない。`getPalwModelRegistry`(op 186)の応答 `GetPalwModelRegistryResponse` を wire version 4 にし、末尾に `annualCatalog: RpcPalwAnnualCatalogStatus`(scheduled、fence DAA、active、edition と root、次の edition / 有効化 DAA / root、edition の一覧(SCHEDULED / ACTIVE)、kernel 上限、`readiness = "off-chain"`)を足した(`rpc/core/src/model/message/palw_operator_reads.rs:2235`、`:2357`)。version 3 以前の node からは「未 schedule」として読む。
* gRPC: `GetPalwModelRegistryResponseMessage` の message 内 field 41 と、新 message `RpcPalwAnnualCatalogStatus` / `RpcPalwAnnualCatalogEdition`(`rpc/grpc/core/proto/rpc.proto:2291-2363`)。`RpcRequest`/`RpcResponse` の oneof 番号(≤ 1307、1330–1331)は使わない。
* service は Params と tip DAA から純粋に組み立てる(`rpc/service/src/service.rs:2402`)。
* CLI: `misaka palw registry` が「annual catalog (RFC-0019)」の節を表示する(`misaka/apps/cli/src/palw_registry.rs:52`)。
* **ローカル LLM の動作は別途 E2E が必要:** registry / catalog の RPC はモデルの追加・階層を示すだけで、実モデルのロードや推論成功を示さない。`misaka-palw-gateway` の loopback `GET /health`(`class_id`)、`GET /v1/models`、`POST /v1/chat/completions` と family worker / 固定 artifact を使い、旧・新 ID を別々に確かめる(§11 R-8)。モデル名の表示やカタログ root の一致だけを推論成功と扱わない。

## 8. リリース手順

### 8.1 署名済みリリース manifest(`misaka/release-manifest/v1`)

`scripts/release/misaka-release-manifest.py` が schema を強制する。必須 field: `schema`、`release`、`network`、`git_commit`、`genesis`、`consensus`(`consensus_params_fingerprint`、`consensus_schedule_id`、`fence_schedule`、`switch_daa`)、`annual_catalog`(`fence_daa`、`editions[]`: edition・activation_daa・previous_catalog_root・root・lifecycle・models)、`lifecycle`、`builds[]`(platform、artifacts: name・sha256・size・reproduced_by[builder, sha256])。任意: `rust_toolchain`、`timeline`、`readiness`(`operational_target_permille`、`consensus_rule: false`)、`components_manifest_sha256`、`signatures`、`notes`。未知の field(たとえば `vote`)は拒否する。

### 8.2 検証(`verify`、運用者が install の前に実行)

| 検査 | 失敗の条件 |
| --- | --- |
| 署名 | `ssh-keygen -Y verify`(namespace `misaka-release`)で有効な署名者が `--min-signers` 未満。署名は manifest から `signatures` を除いた正規 JSON(sort_keys、空白なし、UTF-8)に対する。**未署名・`--allowed-signers` なしは必ず失敗** |
| checksum | `--dir` の成果物の sha256 / size が違う |
| 再現可能 build | 同じ sha256 を報告した独立 builder が `--min-builders` 未満 |
| 指紋 | `RELEASE-INFO.json`(release workflow が各 platform のバイナリの起動時出力から作る)と fingerprint・schedule id・fence schedule が違う、または全 platform で一致が記録されていない |
| 切替 | `switch_daa` が `fence_schedule` にない(バイナリがそこで切り替わらない) |
| カタログ | 連鎖の再計算・検証(§5.3–5.4 と同じ規則)、root の不一致、出荷できない lifecycle、kernel 上限超過 |
| 追加のみ | `--previous` の manifest の連鎖が接頭辞でない、ネットワーク・genesis が違う |
| install 後 | `--node-status` に与えたノード自身の読み(`consensusParamsId`、`consensusScheduleId`、`annualCatalog`)が違う |
| ABORTED | manifest が ABORTED なら install しない |

### 8.3 自動実行しない

道具はバイナリを install・copy・実行しない。未署名のバイナリを自動で実行する経路はない。install の時期は運用者が選ぶ。

### 8.4 署名の運び方

`payload` が署名対象の byte 列を書き、署名者は `ssh-keygen -Y sign -n misaka-release` で署名し、`attach` が manifest に付ける。署名者集合と閾値 k-of-n はユーザー判断(§14 U-3)。

### 8.5 時期案(提案であり検証済みの値ではない)

| 時点 | 内容 | lifecycle |
| --- | --- | --- |
| T-90 | カタログ案と有効化 DAA 案の公開 | PROPOSED |
| T-60 | testnet での混在バイナリ試験(§11 R 系) | TESTED |
| T-30 | stable release(edition と H を持つ) | PREPARED |
| T-14 | readiness の測定(§9) | — |
| T-7 | 中止しない約束 | LOCKED_IN |
| H | 有効化 | ACTIVE |

T-30 で出荷したバイナリはすでに H で切り替わる(§4.2)。LOCKED_IN は合意の状態ではなく、T-7 以後は中止リリースを出さないという約束である。

## 9. readiness の測定 — 投票しない

`scripts/release/misaka-release-readiness.py` が公開 RPC と drill 記録から測る。どの数値も合意の入力ではなく、切替は readiness と無関係に H で起こる。

| 指標 | 定義 |
| --- | --- |
| M1 | 直近 `window_daa` に採用された PALW 計算(void されていない claim、work leaves 加重)のうち、executor bond の運用者のノードが新 schedule を報告している割合 |
| M2 | 検証可能な Panel 容量(fresh な seat-readiness 行、bond × class)のうち、更新済み運用者の割合 |
| M3 | 独立した full node の相互運用: 新バイナリで切替をまたいだ sync・reorg・IBD の成功を記録した運用者の割合(`misaka/interop-record/v1`、drill が書く) |
| M4 | 問い合わせたノードのうち、期待する schedule id、切替高さ、(カタログ切替なら)カタログ root と次の有効化を報告した割合。到達できないノードは別に数える |
| M5 | 更新能力の集中: M1 の更新済み計算における最大運用者の割合と HHI |

* 約 95%(`target_permille = 950`)は**運用目標**であり、合意規則にはならない。manifest の `readiness.consensus_rule` が真なら manifest 検査が拒否する。
* version・schedule id の自己申告は偽装できる。readiness の「更新済み」は SELF-REPORTED と表示し、**M3 の実際の相互運用試験を必須の証拠とする**。

## 10. 互換性 — hard fork と soft fork

* **0.3 → 1.0 の掲載は有効性を広げる**(同じ claim・ブロックに対し、より大きな配分・権利を認める)。したがって、互換が証明されない限り **協調 hard fork** である(ADR-0172 §3 の既定)。旧バイナリは H 以後の配分で新バイナリと異なる state root を計算し、分岐する。
* **有効化前の要件(MUST):** 旧新バイナリは、H より前のすべてのブロックについて同じ有効性判定・同じ state root・同じ fork choice を出す。identity が同じで(§7.2)、fork id が警告にとどまり(§7.3)、`rolling` drill の各段で収束する(§11 R-1)ことで確かめる。
* **有効化後:** 旧バイナリに残ったノードは分岐し得る。fork id がそれを名前付きで拒否し、旧ノードは「同期成功」を偽って報告してはならない(§11 R-3)。**rolling update は「決して fork しない」ことを意味しない。**
* kernel の更新(新しい `kernel_version`)は、それを実装するバイナリを要する協調 upgrade であり、カタログの編集だけでは起こらない(§5.3 の 4)。

## 11. 受入試験

試験 source は書いたが、build も実行もしていない。PASS と表示しない。

| ID | 要件 | 試験 / driver |
| --- | --- | --- |
| A-1 | 正規の符号化と root、Python 道具とのベクトル一致 | `palw_annual_catalog_v1/tests.rs` `the_root_is_the_canonical_encoding_and_matches_the_release_tool`、`misaka-release-manifest.py self-test` |
| A-2 | 連鎖・追加のみ・kernel 上限・上限値の拒否 | `catalogs_chain_by_previous_root_and_only_add`、`palw_model_tier_v1/tests.rs` `an_annual_list_only_adds_and_never_re_roots_a_model` |
| A-3 | リリースをまたいで接頭辞を保つ | `a_release_extends_the_previous_release_as_a_prefix`、manifest `--previous` |
| A-4 | `annual_catalog_active_at`、`ApprovedModels(h)` = 和集合 | `the_active_catalog_is_the_highest_activated_edition` |
| A-5 | `Active` ≠ 1.0、`model_tier_permille` | `the_tier_is_catalog_membership_and_never_registry_state`、`only_an_annual_fork_listing_makes_1_0` |
| B-1 | 切替まで旧新が同じ identity、schedule id は報告 | `old_and_new_builds_share_the_identity_until_the_edition_activates` |
| B-2 | fork id: 切替前は警告、切替後は拒否、同期中の新ノードは保持 | `the_fork_id_warns_before_an_edition_and_refuses_a_build_without_it_after` |
| C-1 | 受理 DAA と境界: RED の跨ぎは一つの epoch snapshot で一度だけ | `a_red_block_merged_across_the_activation_is_read_against_one_epoch_snapshot`(純関数)、driver `boundary`(実ノード) |
| C-2 | 分割・多数登録で権利が増えない、0.3 は一度だけ(0.09 でない)、κ | `splitting_capital_or_registering_more_models_never_raises_rights_and_0_3_applies_once`、`c_m_is_applied_once_never_0_09`、`n_registrations_never_raise_the_registered_tier_total` |
| C-3 | 未認証の Final は frontier・settlement・rule E を進めない(edition 0・edition 1 掲載とも) | `palw_rfc0018_v1/tests.rs` `rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt`(単体)、RFC-0018 driver `scripts/drills/rfc0018_dag_drill.py retired` / `attack`(実ノード。Gate C は本RFCの切替と独立なので RFC-0018 の driver をそのまま使う) |
| D-1 | 再起動・IBD・reorg で同じ読み | `the_catalog_status_is_a_pure_function_of_params_and_tip_across_restart_and_reorg`、`the_tiered_roll_is_the_same_after_reorg_ibd_and_restart`、driver `restart` |
| D-2 | カタログのモデルがいなくても BASE-0 / heartbeat が進む、掲載はブロックを gate しない | `a_catalog_naming_absent_models_never_gates_blocks_or_liveness`、driver `liveness` |
| E-1 | lifecycle の名前、出荷できる状態、ノードは SCHEDULED / ACTIVE だけ | `the_lifecycle_names_are_operational_and_only_a_released_edition_ships` |
| E-2 | RPC wire v4 の往復、CLI 表示 | `rpc/core/src/model/tests.rs`(Mock に annualCatalog)、`palw_registry.rs` `rfc0019_the_registry_renders_the_annual_catalog` |
| R-1 | 混在バイナリ(切替前): 同じブロック・state・fork choice | `scripts/drills/rfc0019_rolling_upgrade_drill.py rolling` |
| R-2 | 切替直後に全更新ノードが同じ root と階層を読む | driver `switch`(interop 記録を書く) |
| R-3 | 未更新ノードは同期成功を偽らない、更新集合は旧ノードの block を採らない | driver `stale` |
| R-4 | DAG sibling / RED の境界保持、claim は一度だけ | driver `boundary` |
| R-5 | 再起動・IBD・partition / rejoin・pruning の一貫性 | driver `restart` |
| R-6 | カタログのモデルの miner なしで heartbeat / BASE-0 が切替を越える | driver `liveness` |
| R-7 | **新規ローカル LLM の ID / root は追加のみ。** 旧 genesis / edition 0 と前 release の接頭辞は不変、新 `class_id` は未使用で root は固定。階層 fence が H 前から有効な年次追加試験では、新登録モデルは H 前 300‰、H 後 1000‰、旧 genesis は前後 1000‰。既存 ID / root の再利用・上書き・消失は FAIL。更新ノードで edition / root / tier / state が一致する | **追加予定:** 年次 edition 1 devnet の `model-add` drill。`getPalwModelRegistry`、manifest `--previous`、`palw-class inspect`、R-1〜R-5 の実ノード記録。現行 driver には未実装 |
| R-8 | **実ローカル LLM 推論。** 旧 genesis モデルと追加モデルを別の固定 ID / artifact で起動し、H の前後で実際の text prompt を送る。`/health` の `class_id`、`/v1/models`、artifact root 検査と推論応答 / commitment を対応付ける。H 後も旧モデルは利用可能、新モデルは新 ID で回答し、他 ID への暗黙の fallback はしない。報酬対象を主張する場合は実 claim の受理・Gate C 認証も別途確認する。未起動・artifact 不在・ID 不一致・空回答・未観測なら PASS にしない | **追加予定:** `local-llm` E2E drill(実 gateway / worker + RPC / claim 記録)。現行 driver には未実装 |
| M-1 | manifest: 署名・checksum・再現・指紋・カタログ・追加のみ・未署名は失敗 | `misaka-release-manifest.py self-test`(実行済み)と、使い捨て鍵による write → payload → sign → attach → verify と改竄 2 種の失敗(実行済み、§15.4) |
| M-2 | readiness: M1–M5、void / 窓外の除外、到達不能を別計上、合意効果なし | `misaka-release-readiness.py --self-test`(実行済み) |

driver は何も arm しない。切替高さを schedule しないバイナリでは BLOCKED になる。状態は `OBSERVED_CONSISTENT` / `BLOCKED` / `FAIL` だけで、承認ではない。

**R-7 / R-8 は設計上の受入条件であり、driver と E2E 実行証拠を追加するまでは未検証(BLOCKED)。既存の `switch` だけを通してローカル LLM 追加を PASS としてはならない。**

## 12. 最初の開発段階 — 同じカタログでの rolling upgrade(testnet-12 DAA 14,000)

ユーザー方針 6・7 により、最初の実使用は**カタログを変えない**切替である。testnet-12 の DAA 14,000 更新がそれに当たる。

1. **同じカタログ:** `palw_model_tier_v1` を DAA 14,000 に schedule し、`catalogs` は edition 0 だけ(新モデルなし)。genesis assembly の class はチェーンの事実で 1.0(§5.5)。edition 0 にそれらを並べるかどうかは U-5。
2. **切替前(〜13,999):** 14,000 は fence 高さなので、旧新バイナリは同じ identity(§7.2)を持ち、fork id は警告だけ(§7.3)。`rolling` drill で各段の収束を記録する(R-1)。
3. **運用:** 署名済み manifest(`switch_daa = 14000`、`annual_catalog.fence_daa = 14000`、edition 0)を配り、運用者は `verify` してから自分の時期に install する。T-14 相当の時点で `misaka-release-readiness.py` を回し、M1–M5 を報告する(計画上の更新期限の目安は DAA 13,700)。
4. **切替(14,000):** 更新済みノードは同じ root と階層を読み(R-2)、未更新ノードは fork id で切り離される(R-3)。
5. **出荷バイナリでの drill:** 計画の順序どおり、出荷するバイナリそのもので 14,000 をまたぐ drill(`switch`・`stale`・`restart`・`liveness`。`boundary` は複数モデルの sibling を作れる devnet で)を行う。
6. **その後で** 0.3 → 1.0 の年次掲載(edition 1)と kernel 更新を加える。それは別の LOCKED_IN 手順を経る。

道具の対応: manifest と readiness は「カタログ edition の切替」と「flag day の fence 切替(カタログ不変)」の両方を同じ field で扱う(`switch_daa` は schedule 上の任意の fence 高さ、カタログ root の期待値は任意)。

**期限(10-18)までに未完の項目:** §14 の判断待ちと、§15.5 の compile リスクの解消(validation lane の build)。drill の実行は計画の段階 3・4(10-18〜20)で、本 lane では行っていない。

### 12.1 次の段階 — ローカル LLM のモデル ID を**追加のみ**で有効化する年次 edition drill(2026-10-16 追加)

この段階の「更新」は、**旧 genesis モデルを新しい重みや ID で上書きすることではない**。edition 0 / 既存 genesis の登録・root / 過去の claim / position / 市場は不変とし、新モデルに新規 `class_id` と固定 `artifact_root` を与え、edition `e ≥ 1` に追加する(§5.4、ADR-0175)。利用者は新 ID を**明示的に選択**できるが、旧 ID を黙って新 ID へルーティングしない。

1. **準備(H 前):** 旧 edition を接頭辞として固定し、別 ID / root の新モデルを登録・artifact / tokenizer / family worker のロードを準備する。drill 用 devnet では階層 fence を H より前に有効化し、新 ID が登録 tier 300‰、旧 genesis が 1000‰ であることを読む。新 kernel が必要なら対応コードの事前出荷と独立の協調 upgrade が必要で、カタログだけで未知の kernel は動かせない。
2. **Solana 型の段階的展開:** edition と H を固定した schedule を持つリリースを事前配布し、H 前は旧・新ノードが同じブロック・state root・fork choice になることを R-1 で確かめる。**主な検査対象はモデル ID / catalog の有効化**だが、MISAKA の schedule はバイナリに固定されるため、事前のソフトウェア展開を省略することはできない。readiness は運用上の測定のみ。
3. **有効化(H):** H 前は新 ID が年次掲載されたと誤認されず、H 後に edition `e` が ACTIVE となり、`ApprovedModels` に新 `(class_id, artifact_root)` が**増えるだけ**であることを全更新ノードで確認する。配分の 1000‰ は H 以後に初めて行う allocation roll から効く(§6.4)。旧 ID / root / 階層は保持される(R-7)。旧 schedule ノードの fork-id 拒否は R-3 で検証する。
4. **ローカル LLM 実動:** 旧・新モデルを別個の固定 artifact と worker で実行し、H 前後に実際の prompt を投げる。gateway `GET /health` の `class_id`、`GET /v1/models`、`POST /v1/chat/completions` の非空回答 / commitment と、ノードの登録 `(class_id, artifact_root)` を照合する。H 後は**新モデルで回答でき、旧モデルも回答し続ける**。モデル名だけの表示・mock・別モデルへの fallback は合格にしない。報酬を伴う claim を検証するときは Gate C も別途確認する(R-8)。
5. **再起動・再同期・リリース検査:** H 後の restart / IBD / reorg でも新旧両モデルの階層と旧状態が再現され、署名済み manifest のカタログが前版の接頭辞を保持することを確認する。root 不一致・再割当て・暗黙の ID 置換・新モデルが使えない場合は FAIL、実モデルの fixture が準備できない場合は BLOCKED と記録する。

**区別:** testnet-12 DAA 14,000 の最初の drill は §12 のとおり **edition 0 のまま・新モデルなし**であり、R-7 / R-8 の合格証拠にはならない。R-7 / R-8 は**次の実モデル追加 edition から必須**。実施前に driver とモデル別の再現可能な fixture / 検査ログを実装する。


## 13. 本RFCで決めない値

| 名前 | 設計値(未承認) | 意味 |
| --- | --- | --- |
| 時期 T-90 / T-60 / T-30 / T-14 / T-7 | 提案のとおり | §8.5 |
| `target_permille` | 950(運用目標) | readiness の目標。合意規則ではない |
| `window_daa` | 720 | M1 の窓 |
| `--min-signers` / 署名者集合 | 2 / 未定 | manifest の k-of-n |
| `--min-builders` | 2 | 独立した再現 build の数 |
| 更新期限の余裕 | testnet-12 は DAA 13,700 目安 | 切替前の運用期限 |
| `PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1` | 1 | このバイナリが実装する kernel |
| `PALW_ANNUAL_CATALOG_MAX_MODELS_PER_EDITION_V1` / `_MAX_EDITIONS_V1` | 1,024 / 64 | 上限(政策目標ではない) |
| `c`(登録 tier 係数)/ `κ`(登録 tier 上限) | 300‰ / 300‰ | ADR-0179 の設計値 |
| edition の高さを allocation epoch 境界に揃えるか | SHOULD | §6.4 の 6 |
| testnet-12 edition 0 の中身 | 空、または genesis class を明示 | U-5 |

## 14. ユーザー判断が必要な事項

* **U-1** 中止の扱い: T-30 の stable release が H を持つ(PREPARED、本RFCの推奨)か、edition を LOCKED_IN の時点まで出荷しない(展開期間が 7 日になる)か(§4.2、§8.5)。
* **U-2** 時期 T-90 … H と readiness の目標値(§13)。
* **U-3** manifest の署名者集合と閾値 k-of-n、独立 builder の数。
* **U-4** 掲載する年次 edition の中身(モデル・root・kernel version・plan id)と H。
* **U-5** testnet-12 の DAA 14,000 で edition 0 に genesis class を明示するか(機能は同じ、root が変わる)。
* **U-6** `palw_model_tier_v1` の有効化(ADR-0179 の値 c・κ の承認と測定値記録)。本RFCは有効化 gate に「署名済み manifest と、切替をまたぐ混在バイナリ drill の記録」を加える(§16)。

## 15. 実装(休眠、未build、未試験)

### 15.1 fence と割り当て

| 種類 | 割り当て | 確認 |
| --- | --- | --- |
| fence | **新規なし。** `palw_model_tier_v1`(ADR-0179)の値 `catalogs` | identity の collapse・正規化(`identity.rs:651`)、visitor(`:3645`)、schedule id(`:1960`)、params id(`:5239`)、fork id gate(`fork_id_v1.rs:282`)、validation(`palw_model_tier_v1.rs:325`)、mirror(`PalwModelTierMirrorV1::catalogs`)。全 preset `None` |
| RPC | 新 op なし。`GetPalwModelRegistryResponse` wire version 4(`annualCatalog`)、新 struct `RpcPalwAnnualCatalogStatus` / `RpcPalwAnnualCatalogEdition`(各 wire version 1) | op 番号 ≤ 244 / 255 は不変 |
| gRPC | `GetPalwModelRegistryResponseMessage` の message 内 field 41、新 message 2 つ | `RpcRequest` / `RpcResponse` の oneof 番号(≤ 1307、1330–1331)は不変 |
| p2p | なし(protocol version 108 のまま、`VersionMessage` field は 16 まで) | §7.6 |
| object tag / kernel inner / aux table / funded action / v39 stage action | なし | grep で使用中の範囲(tag 164–171・230・240–242、inner 63–71・73–75、aux ≤ 136 と 137 以降、funded ≤ 46、stage 41–79)に触れていない |

### 15.2 module とファイル

* `consensus/core/src/palw_annual_catalog_v1.rs`(新規)— 型、正規の符号化と root、連鎖の検証、`extends`、`annual_catalog_active_at`、`model_tier_permille`、`palw_catalog_tier_reference_daa_v1`、lifecycle と binary state、ノードの status。試験 `palw_annual_catalog_v1/tests.rs`。
* `consensus/core/src/palw_model_tier_v1.rs` — `genesis` / `annual` list をカタログ連鎖に置き換え(API 不変)。試験 `palw_model_tier_v1/tests.rs` と `palw_rfc0018_v1/tests.rs` を連鎖へ移した。
* `consensus/core/src/config/params/identity.rs`、`consensus/core/src/fork_id_v1.rs`、`consensus/core/src/palw_model_registry_v1.rs`(doc)、`consensus/core/src/lib.rs`。
* `rpc/core/src/model/message/palw_operator_reads.rs`、`rpc/core/src/model/tests.rs`、`rpc/grpc/core/proto/rpc.proto`、`rpc/grpc/core/src/convert/message.rs`、`rpc/service/src/service.rs`、`misaka/apps/cli/src/palw_registry.rs`。
* `scripts/release/misaka-release-manifest.py`、`scripts/release/misaka-release-readiness.py`、`scripts/drills/rfc0019_rolling_upgrade_drill.py`(新規)。

### 15.3 driver

`scripts/drills/rfc0019_rolling_upgrade_drill.py`: `rolling`(R-1)、`switch`(R-2)、`stale`(R-3)、`boundary`(R-4)、`restart`(R-5)、`liveness`(R-6)。plan は役ごとに `old_argv` と `new_argv`(同じ app dir)を持ち、P2P は driver 所有の loopback link(RFC-0018 drill の `DelayedLink` を import)、判定は全て安定した sink での公開 wRPC。`switch` と `restart` は readiness M3 の入力となる `misaka/interop-record/v1` を書く(`--interop-out`)。

**2026-10-16 追加要求(未実装):** 年次モデル追加 drill は、R-7 の `model-add` と R-8 の `local-llm` を driver / fixture に実装してから行う。固定入力は新旧 `class_id` / `artifact_root`、対応する worker / artifact、edition / root / H、再現可能な prompt、RPC と gateway の loopback endpoint、期待 tier、実測ログ。現行コードの `rolling` / `switch` 成功を R-7 / R-8 の成功とみなさない。

### 15.4 本 lane が実行したもの

* `python3 -I scripts/release/misaka-release-manifest.py self-test` と `python3 -I scripts/release/misaka-release-readiness.py --self-test`(標準ライブラリだけ、ネットワークなし)。
* 使い捨ての ed25519 鍵 2 本で write → payload → `ssh-keygen -Y sign` → attach → verify(2-of-2)、成果物 1 byte の改竄と署名後の schedule 改竄がともに失敗することを確かめた。
* Rust 側の試験ベクトル R0・R1・EMPTY は、この Python 実装で計算した値である。Rust 実装との一致は validation lane の build で初めて確かめられる。

### 15.5 compile 上のリスク(未build)

* `PalwModelTierFenceV1` / `PalwModelTierMirrorV1` の field 変更(`genesis`・`annual` → `catalogs`)。grep で既知の構築・参照箇所(試験 2 ファイル、`fork_id_v1.rs` の probe は `at()` を使う)は更新済み。
* `PalwTierSourceV1::AnnualFork { index }` → `{ edition }`。
* `for_each_fence` 内の `visit(&mut catalog.activation_daa)`(`visit` は `&mut impl FnMut(&mut u64)`)。
* 試験での `&peer.fired.as_bytes()` の `&[u8; 32]` → `&[u8]` 変換、`TESTNET_PARAMS` を値で使う `armed()`。
* `GetPalwModelRegistryResponse` の新 field: struct literal は service・Mock・gRPC 変換で更新済み、他は `..Default::default()`。
* `deserialize!(RpcPalwAnnualCatalogStatus, reader)` の単一 struct 形と、gRPC の `Option::transpose`。
* `blake2b_simd` は `consensus/core` の既存依存(`palw_model_lines_v1.rs` が使用)。

## 16. 有効化 gate

~~~text
ACTIVATION_ALLOWED(palw_model_tier_v1 とその各 edition)
    = ユーザーの有効化判断(U-6)と §13 の値の承認
      AND ADR-0179 の gate(c・κ の測定値記録、配分 fence と Gate C が同じ高さ以下)
      AND 単体試験(§11 A〜E)を validation が実行して合格
      AND 出荷するバイナリそのもので rolling / switch / stale / restart / liveness を実行し、収束を記録(R-1〜R-3、R-5、R-6)
      AND 年次 edition(0.3→1.0 の掲載)を含む切替では boundary(R-4)も記録
      AND 新規ローカル LLM を追加する edition では、新規 ID/root の追加のみ・旧 genesis 保持を実ノードで観測(R-7)
      AND 同 edition では旧・新 ID の実ローカル推論/commitment を H 前後で検証し、必要な claim の安全性を確認(R-8)。未実装・未実行は BLOCKED
      AND 署名済み manifest が verify を通る(署名・checksum・再現・指紋・カタログ・追加のみ)
      AND readiness M1〜M5 をユーザーへ報告済み(目標は運用判断の材料であり、合意の条件ではない)
~~~

## 17. 参照

* [PRINCIPLES](../PRINCIPLES.md)(§8、§9、§0 の 2026-10-15 改定)、[ユーザー判断 2026-10-15 §7b・§7c](../design/palw/user-decisions-2026-10-15-adr-rfc16-rfc17.md)、[testnet-12 DAA 14,000 リリース計画](../design/palw/t12-daa14000-release-plan-2026-10-15.md)
* [ADR-0172 §3](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)、[ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)、[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)、[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)、[ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md)
* [RFC-0016](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)、[RFC-0017](0017-model-eligibility-examination-and-renewal.md)、[RFC-0018](0018-dag-parallel-multi-model-consensus.md)
* Agave(参照のみ、複写・実行なし): [feature-set](https://raw.githubusercontent.com/anza-xyz/agave/master/feature-set/src/lib.rs)、[runtime bank](https://raw.githubusercontent.com/anza-xyz/agave/master/runtime/src/bank.rs)、[cli feature](https://raw.githubusercontent.com/anza-xyz/agave/master/cli/src/feature.rs)、[RELEASE.md](https://raw.githubusercontent.com/anza-xyz/agave/master/RELEASE.md)、[Feature Gate Activation Guidelines](https://github.com/anza-xyz/agave/wiki/Feature-Gate-Activation-Guidelines)、[Backward compatibility policy](https://docs.anza.xyz/backwards-compatibility)
