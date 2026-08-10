# MISAKA Compute Token Program 設計書 v0.1

**副題:** 検証済み LLM 計算を裏付けとするネイティブ資産の発行 — SPL 型 Token Program と Compute-Backed Emission

**版:** v0.1 Draft
**日付:** 2026-08-10
**対象ソース:** `main` 00d1294 系列 + `dns-partition-fix-t10`（`VltEpochSnapshot` / `vlt_voting_snapshot` 実装中）、commit `2221e8a` 時点
**対象:** ネイティブ資産台帳（Token Program）、計算量裏付け発行（emission）、VLT との境界、hard-fork、段階導入
**関連:** ADR-0024（Verified LLM Token-Weighted BFT）、ADR-0020（Selected-Parent EVM Lane）、ADR-0013 Addendum C.2（consensus 側 UTXO mint 副作用）、ADR-0022（pruned IBD overlay snapshot）、`docs/misaka-base-3lane-execution-design-v0.1.md`

> 本書の **MUST / MUST NOT / SHOULD / MAY** は規範語として使用する。数値に「候補」と明記したものは testnet 計測後に凍結する。本書承認後、決定記録は次番の ADR として切り出す。

---

## 0. 結論

MISAKA に **プロトコル・ネイティブの資産台帳（Token Program）** を導入し、その第 0 資産として **Token（ticker: TOK）** を定義する。TOK の発行権限は鍵ではなく **consensus の compute-certificate 受理パイプラインそのもの** であり、検証済み LLM 計算量に比例して epoch ごとに発行される。

```text
                    （既存 — ADR-0024, 稼働中）
  LlmJobSpec ─▶ Commitment(0x17) ─▶ sortition ─▶ Certificate(0x14)
                                                      │
                                     Verdicts(0x18) / Challenge(0x15)
                                                      │ challenge window 生存
                                                      ▼
                      x_j = ρ(S_j)·(a·t_in + b·t_out)          … §3.2 正規化
                      X_i(e) = Σ x_j                            … epoch 集計
                                │
             ┌──────────────────┴──────────────────┐
             ▼ （既存）                             ▼ （本設計 — 新規）
   C_i(E) = Σ d_τ·X_i(E−τ)                reward_i(E) = R(E) · X_i(E) / X(E)
   W_i(E) = min{C_i, λ·B_i}                        │
   → DNS finality 投票重み                          ▼
   （非譲渡・decay あり）                  Token Ledger へ TOK を貸記
                                          （譲渡可能・decay なし・投票権ゼロ）
```

一文で言えば: **「ハッシュを掘ってコインを得る代わりに、検証可能な LLM 計算を行ってネイティブトークンを得る」**。PoW の block reward に対応する位置付けであり、§6 の Job fee / execution reward（受注対価）の復活では **ない**（→ §6.1, §11）。

成立条件は次の 5 点である。

1. **投票権と貨幣の分離。** TOK 残高は `W_i(E)` に一切入らない（MUST NOT）。投票重みの源泉は従来どおり `C_i(E)`（＋ `λ·B_i` cap）のみ。トークン購入で finality 権力は買えない。
2. **発行はフォーク不変。** emission は `VltEpochSnapshot` と同じ「全競合ブランチが含む block に pin された台帳」から計算し、challenge window 完全閉鎖後にのみ確定する。フォーク上でだけ存在する計算が貨幣化されてはならない。
3. **発行総量は計算量ではなく schedule が決める。** epoch 予算 `R(E)` は固定 schedule。計算量 `X(E)` は分配比率のみを決める。これが difficulty retarget の等価物である（→ §5.2）。
4. **台帳は単一のプロトコル実装。** 残高・転送・burn は consensus store 上の一実装が扱う。ERC-20 のようなコントラクト実装差分は存在しない（SPL と同じ性質）。
5. **有用性は consensus で測らない。** ρ・a・b という客観量のみが報酬に入る。「その計算が誰かの役に立ったか」は orderer / fee market の領分であり、本設計の非目標（→ §11）。

### 0.1 ユーザー提案式との対応

出発点の提案式と本設計の対応は次のとおり。

```text
提案:   Reward = verified LLM tokens × difficulty / compute quality
本設計: reward_i(E) = Σ_j x_j  ×  R(E)/X(E)
                      └────┘      └───────┘
        x_j = ρ(S_j)·(a·t_j^in + b·t_j^out)
              └─┬──┘  └────────┬─────────┘
   compute quality ρ      verified LLM tokens（prefill/decode 重み付き）
                          difficulty ≡ X(E)/R(E)（自動リターゲット、§5.2）
```

「モデルサイズ・推論難易度・検証コスト」は `ρ(S_j)`（`ModelCostTable`、consensus パラメータ）が担い、「仕事量」は `a·t_in + b·t_out` が担う。difficulty は明示パラメータではなく、**全網の検証済み計算量が増えるほど 1 TOK あたりに要る計算が増える** という比率 `X(E)/R(E)` として創発する。PoW の difficulty adjustment と同型である。

---

## 1. 用語と境界

- **LLM token**: 計算量の単位（prefill `t_in` / decode `t_out`）。資産ではない。
- **Token（TOK）**: LLM token 消費の検証に対して発行されるオンチェーン資産。譲渡・burn 可能。名称・ticker は 2026-08-10 決定（旧仮称 MCT）。
- **Token Program**: 資産の発行・残高・転送・burn を扱う consensus 内の単一実装。SPL Token Program の対応物。コントラクトではない。
- **Token Ledger**: Token Program が維持する `(asset_id, owner) → {balance, nonce}` の台帳。consensus store。
- **emission epoch**: VLT epoch（`attestation_epoch_length_blue_score`、出荷値 100 blue score）をそのまま使う。emission 独自の epoch は定義しない（MUST）。
- **settlement offset `D_settle`**: epoch `E` の報酬が台帳に貸記されるまでの epoch 数（→ §5.3）。

LLM token（計算量）とオンチェーン token（資産）の分離は本書全体の前提である。前者は `vlt.rs` の領分、後者は本書の領分。

---

## 2. 現行実装への接地

本設計はゼロから作らない。以下の稼働中・実装中の機構に接木する。

### 2.1 検証済み計算の測度（既存・完成）

`consensus/core/src/vlt.rs` に §3.2 正規化と epoch 集計が実装済みである。

- `x_j = ρ(S_j)·(a·t_j^in + b·t_j^out)`、`Verify = CanonicalFullReplay` のみ consensus-eligible（v0.1 pin）
- 全演算は `u128` 整数・`VLT_MICRO` 固定小数点。float は重み経路に存在しない
- `ρ` は `ModelCostTable`（最大 `MAX_MODEL_COST_ENTRIES = 16`）の consensus パラメータであり、executor 入力ではない
- `a = prefill_cost_micro`、`b = decode_cost_micro`
- 受理は refutation-dominant。`min_verifier_confirmations` / `min_verifier_refutations`、二相 sortition（commitment 0x17 が beacon より先）、standalone verdict（0x18）
- challenge window（`challenge_window_blocks`）を生き延びた certificate だけが `X_i(e)` に入る

**emission はこの `X_i(e)` を読むだけで、新しい検証機構を一切導入しない（MUST）。** 不正 certificate の排除・slashing（ContradictoryVerification のみ）・入力オンチェーン化（`MAX_JOB_INPUT_BYTES = 8192`）はすべて既存層の性質をそのまま継承する。

### 2.2 フォーク耐性の pin（実装中）

`VltEpochSnapshot`（`vlt.rs:1825`、store は `consensus/src/model/stores/vlt_voting_snapshot.rs`）は「全競合ブランチが含む block に pin された credit 表」を提供する。投票重みがフォーク自身の計算で膨らまないための機構だが、**貨幣発行はこれと同じ表から読む**（→ §5.3）。本設計の実装は voting-snapshot 系列（PR 2: weight-source flip）の完了に依存する（→ §9.5）。

### 2.3 consensus 側 mint の先例（既存）

- ADR-0013 C.2: slashing reporter 報酬を **consensus 副作用 UTXO** として mint（`utxo_diff.rs` の `add_utxo`）
- §6 audit fee: `audit_fee_sompi`（出荷値 50,000,000 = 0.5 KAS）を counted verdict ごとに verifier へ mint（`processor.rs` の `compute_audit_fee_outputs`）

つまり「tx 出力ではない、consensus が直接 mint する価値」の機構と監査経験は既にある。本設計はこれを **別資産・台帳貸記** に拡張する。

### 2.4 subnetwork 帯域と asset_id フック（既存）

overlay 帯 0x10–0x1a（DNS finality + VLT compute）、EVM bridge 帯 0x20–0x22 が使用中。0x20 `EVM_DEPOSIT` の payload は `asset_id` フィールドを **予約済み** であり、将来の多資産 bridge を既に見込んでいる（v0.1 では未使用のまま。→ §7）。

Token Program は新帯域 **0x30–0x33** を取る（→ §4.3）。

---

## 3. 資産モデルの選定

3 案を比較し、**案 B（consensus store 上のアカウント台帳）を採用する**。

| | 案 A: UTXO 多資産化 | **案 B: アカウント台帳（採用）** | 案 C: EVM predeploy |
|---|---|---|---|
| 方式 | `UtxoEntry` に `(asset_id, amount)` を追加（Cardano 型） | consensus store に `(asset, owner) → balance`（SPL/VLT credit 型） | Lane 2 に ERC-20 を配置し F00x で mint |
| 変更範囲 | tx 検証・mass・utxoindex・wallet・P2P 直列化まで全域 | 新 store + 新 subnet 帯 + virtual processor の適用 seam | EVM 側のみ |
| 既存機構との整合 | UTXO 多重集合 commitment を全面改訂 | bond / VLT credit / audit fee と同じ適用・reorg 経路 | ○ |
| 「プロトコルが理解する標準資産」 | ○ | ○ | ×（コントラクト資産。option 1 として却下済み） |
| mint authority = consensus | 可能だが重い | **自然**（§2.3 の先例どおり） | precompile 経由で歪む |
| リスク | 最深部の hard-fork、監査再実施が広範 | 台帳 state の IBD/commitment が新課題（§9.4） | 主権が EVM lane に移る。3-lane 設計の「Base が settlement を持つ」原則に反する |

却下理由の要点:

- **案 C** は「Misaka プロトコル自身が理解する標準資産」という要求そのものに反する。EVM facade はあくまで表示層として将来提供する（§7）。
- **案 A** は最終形として魅力があるが、UTXO 直列化・多重集合 hash・mass 計算・全 wallet を一度に動かす。v0.1 の成果物として過大であり、案 B の台帳を後日 UTXO 多資産へ移送する道も閉じない（§12）。
- **案 B** は本 codebase で bond（0x10）、VLT credit、audit fee が既に通っている「payload 検証 → acceptance 時 store 適用 → reorg 巻き戻し」経路の上に乗る。blockDAG の並行性とも整合する: 二重支払いは mergeset の決定的受理順で最初の 1 件だけが nonce を消費する（§4.4）。

---

## 4. Token Program 仕様

### 4.1 資産 ID 空間

```text
asset_id : 8 bytes
  0x0000000000000000        = TOK（プロトコル予約、Phase A で唯一）
  それ以外                   = CreateMint tx id から導出（Phase B、§4.6）
```

`asset_id = 0` の mint authority は **存在しない**（MUST）。いかなる鍵も TOK を mint できず、発行経路は §5 の emission のみ。これが「PoW coinbase と同じ位置付け」の形式的表現である。

### 4.2 台帳

```text
TokenLedger:  (asset_id, owner) → { balance: u128, nonce: u64 }
TokenSupply:  asset_id → { minted: u128, burned: u128 }
```

- `owner` は既存 overlay と同じ ML-DSA-87 鍵ハッシュ表現（validator id と同系の `Hash64` 形式）。EOA 的な「アカウント作成」手続きは不要 — 初回貸記で行が生まれる（SPL の ATA 相当は不要とする）。
- 単位: `10^8` atomic / 1 TOK（sompi 慣習に一致）。
- 保存則（MUST・不変条件）: 全 `(asset)` について `Σ balance = minted − burned`。consensus テストではこれを supply-conservation suite として常時検証する（EVM lane の F002 供給保存テストと同格の扱い）。

### 4.3 命令セットと subnetwork

| subnet | 命令 | Phase | payload 要旨 |
|---|---|---|---|
| 0x30 | `TokenTransfer` | A | version, asset_id, from_pk, to, amount, nonce, sig |
| 0x31 | `TokenBurn` | A | version, asset_id, owner_pk, amount, nonce, sig |
| 0x32 | `TokenCreateMint` | B | version, params(decimals, supply_cap, mint_authority), sig |
| 0x33 | `TokenMintTo` | B | version, asset_id, authority_pk, to, amount, nonce, sig |

- 各 payload は専用 BLAKE2b domain separator で署名される（`misaka-tkn-v1/transfer` 等。VLT payload と同じ流儀、MUST）。
- 搬送 tx は通常の base-coin tx であり、**手数料は base coin で払う**。v0.1 で TOK による fee 支払いは導入しない（MUST NOT。fee market の変更は別設計）。
- payload 検証は他 overlay 帯と同じく isolation validation（stateless）→ acceptance 時 stateful 適用の二段（MUST）。

### 4.4 リプレイ防止と並行性

- `nonce` は `(asset_id, owner)` ごとの単調カウンタ。署名対象に含まれ、適用時に `stored_nonce + 1` と一致しない payload は **無効として無視**（tx 自体は有効、token 効果ゼロ。EVM lane の skip-class と同じ扱い）。
- blockDAG 上の同時 transfer は mergeset の決定的受理順が先着 1 件を選ぶ。carrier tx id への束縛ではなく nonce 方式を採るのは、wallet が payload を事前署名してから fee/UTXO を差し替えられるようにするため。
- 束縛タイミング（§9.2 の埋没 fold に従う）: オペの台帳効果は、受理 block が reorg horizon を超えて埋没した時点で確定する。nonce・残高の判定もその fold 時点の台帳に対して行う（MUST）。

### 4.5 投票権との絶縁（再掲・規範）

`TokenLedger` の残高はいかなる読み出しでも `effective_voting_weight` / `W_i(E)` に入らない（MUST NOT）。逆も然り: `C_i(E)` は譲渡できない。両者の唯一の接点は §5 の「同じ `X_i(E)` から計算される」ことだけである。

### 4.6 Phase B: 一般 mint（SPL 完全対応）

Phase A の台帳・転送・burn がそのまま使われ、`CreateMint`（asset_id = carrier tx id の `Hash64`、衝突フリー・レジストリ不要）と authority 付き `MintTo` を解禁する。これにより「ユーザー発行トークンも単一共通実装」という SPL 対応が完成する。freeze / clawback authority は導入しない（非目標、§11）。Phase B の activation は Phase A と独立の DAA fence とする（SHOULD）。

---

## 5. Compute-Backed Emission

### 5.1 報酬式

epoch `E` の executor `i` への発行量:

```text
reward_i(E) = ⌊ R(E) · X_i(E) / X(E) ⌋        X(E) = Σ_i X_i(E)
```

- `X_i(E)` は challenge window を生存した certificate の §3.2 credit そのもの。**decay（d_τ）は掛けない** — decay は「古い計算で今日投票させない」ための投票側の性質であり、既に発行済みの貨幣を減価させる理由はない（PoW と同じ非対称）。
- 端数は切り捨て、`Σ reward_i ≤ R(E)`。残余は発行しない（供給は schedule を上回らない、MUST）。
- `X(E) < min_network_compute` の epoch は **発行なし・繰越なし**（MUST）。過疎網で 1 job が epoch 予算を独占する bootstrap 攻撃を塞ぐ。既存 `VltParams::min_network_compute` を共用する。

### 5.2 発行 schedule `R(E)`

```text
R(E) = R0 >> ⌊E_a / H⌋        E_a = E − emission_activation_epoch
```

半減ステップ型（整数・決定的）。総供給上限は `Σ = R0 · H · 2`（幾何級数）で閉じる。

候補値（testnet 計測と供給政策決定後に凍結。**数値はすべて候補**）:

- epoch ≈ 100 blue score ≈ 100 秒 → `H = 315,360`（約 1 年）
- `R0 = 500 TOK/epoch` → 初年 ≈ 1.577 億 TOK、総上限 ≈ 3.15 億 TOK

difficulty の等価物は `X(E)/R(E)`（1 TOK を得るのに要する検証済み計算量）。参加計算が増えるほど自動的に上がり、明示 retarget アルゴリズムは不要である。

### 5.3 確定（settlement）とフォーク不変性

```text
epoch E 終了 ─▶ challenge window 閉鎖 ─▶ snapshot pin ─▶ E + D_settle で台帳貸記
```

- `D_settle ≥ max( credit_delay_epochs, ⌈challenge_window_blocks / epoch_length⌉ + 1 )`（MUST）。候補: この下限そのもの。
- 貸記は `VltEpochSnapshot` と同じ **pin 済み credit 表** から読む（MUST）。pin は全競合ブランチが含む block なので、どのブランチで計算しても `reward_i(E)` は同一値になる — **coinbase 成熟のような追加の maturity は不要**（mint がそもそもフォーク不変であるため）。reorg 時の巻き戻し・再適用は他 store と同じ virtual-diff 経路。
- challenge window が settlement より先に閉じることを `D_settle` の下限が保証するので、**受理後に mint を取り消す経路は存在しない**（clawback なし、MUST NOT）。fraud は「mint 前に落とす」の一本で守る — これは「slashing は ContradictoryVerification のみ・受理時のみ」という既存の証明可能性原則と整合する。

### 5.4 貸記先

`reward_i(E)` は executor `i` の validator id と同形式の owner キーに貸記する。報酬受取先を別鍵に向けたい場合は Phase A の `TokenTransfer` で移す（coinbase → 支払いの通常フロー）。certificate 側に payout 先フィールドを足す案は、certificate の署名メッセージを太らせるため v0.1 では見送る（MAY、§12）。

---

## 6. VLT・既存経済との関係

### 6.1 これは execution reward の復活ではない

job は self-originated であり、orderer も Job fee も存在しないという設計判断は **変更しない**。emission は「仕事の対価」ではなく「証明可能な計算に対する通貨発行（seigniorage）」である。PoW の block reward が「誰かのためのハッシュ計算の対価」ではないのと同じ位置付けであり、§6 の Job fee / execution reward を導入しないという既存決定と両立する。

### 6.2 一つの測度、二つの消費者

`X_i(E)` は投票（decay あり・非譲渡・bond cap あり）と貨幣（decay なし・譲渡可・cap なし）の両方の源泉になる。帰結:

- **セキュリティ予算が生まれる。** これまで検証済み計算の見返りは投票権のみで、計算コストの金銭回収経路がなかった。emission は PoW における block reward と同じく、finality set の計算供給を直接ファイナンスする。
- **計算 farming の動機も増える。** ただし票側は `λ·B_i`（bond cap）と `credit_delay` が従来どおり抑え、貨幣側は「掘れば儲かる」が意図された性質である。
- **verifier 側の負荷が増える。** cert 数の増加は committee 負荷と audit fee 発行を比例させる（→ §8.11）。

### 6.3 audit fee との相互作用（要再検討事項）

現行 audit fee は counted verdict ごとに 0.5 KAS を **base coin で** mint する。emission が cert 数を増やすと、これは base coin の無上限インフレ経路になる。v0.1 では挙動を変えない（稼働中の VLT 設計に触れない）が、次のいずれかを v0.2 で決める（MUST-revisit）:

1. epoch あたり audit fee 総額に cap を置く
2. audit fee の財源を `R(E)` 内（TOK 建て）へ移し、base coin の発行を止める — verifier と executor が同じ発行 pie を分ける PoW 型に揃う

**解決済み（2026-08-11）**: `docs/misaka-audit-emission-v0.2-design.md` が本節を閉じた。上の 2 案はどちらも却下（cap は繁忙 epoch の検証飢餓、bps 分割は committee 較正のたびに誤価格）し、**統一仕事量 emission** を採る — counted verdict を「判じた cert の x_j と同量の仕事」として `R(E)` の同一比例分配に含め、base-coin audit fee は `tkn_activation` fence で停止。実装は PR #62 の後続 PR。

### 6.4 基軸コインとの関係

TOK は基軸コインと **別資産** である（ユーザー要件）。fee は base coin のまま、変換・市場価格形成はプロトコル外。基軸コイン自体を LLM 計算で発行する hybrid issuance 案は、既存供給 schedule・halving 監査を全て再実施することになるため採らない（却下を明記）。

---

## 7. EVM lane との関係（v0.1: なし）

v0.1 の TOK は Base の資産であり、EVM lane からは見えない。拡張パス（別設計、MAY）:

- 0x20 `EVM_DEPOSIT` の予約済み `asset_id` を有効化し、TOK を Lane 2 へ deposit
- Lane 2 側は ERC-20 ABI facade（predeploy）+ F004 系 precompile で台帳残高を鏡映
- 逆方向は F002 withdraw の資産対応版

これにより「EVM の ERC-20 として振る舞う TOK」を後付けできるが、正本は常に Base の Token Ledger である（3-lane 設計の settlement 原則）。

---

## 8. セキュリティ・経済分析

| # | 脅威 | 対策 |
|---|---|---|
| 1 | フォーク上の計算で mint | pin 済み snapshot + `D_settle` 下限（§5.3）。フォーク不変が構成的に成立 |
| 2 | transfer/burn のリプレイ | per-(asset,owner) nonce + domain 分離署名（§4.4） |
| 3 | 同一計算の二重申告 | 既存 VLT 層の job/certificate 一意性をそのまま前提とする（MUST: 同一 `job_spec_id` は一度しか credit されない）。emission は独自の受理判定を持たない |
| 4 | 無価値計算の量産（self-origination） | 意図された性質。計算は実費であり、式が支払うのは計算量のみ。fee 補助・対価は存在しないので「タダ乗り」経路がない |
| 5 | モデル級数裁定（ρ の誤較正） | `ModelCostTable` は emission 下では **金融政策** になる。保守的な初期表 + hard-fork による定期再較正を運用要件とする（SHOULD）。較正誤差は「最も割の良い級数に採掘が集中する」に留まり、供給総量は `R(E)` が守る |
| 6 | prefill 詰め込み（`a·t_in` 稼ぎ） | `a ≪ b` の較正 + `MAX_JOB_INPUT_BYTES = 8 KiB` が `t_in` を構造的に制限（既存） |
| 7 | sybil 分割 | `reward` は `X_i` に線形なので分割は中立。sortition・bond 要件は既存のまま |
| 8 | 過疎網での安価な独占 mint | `min_network_compute` floor で epoch ごと不発行（§5.1） |
| 9 | verifier 買収による偽 credit | 既存の refutation-dominant 受理・二相 sortition・ContradictoryVerification slashing に乗る。ただし emission は買収の**金銭的動機を増やす**ため、`verifier_committee_size` / `min_verifier_confirmations` は emission activation 前に再較正する（MUST、§10） |
| 10 | mint 後の不正発覚 | 受理境界で防ぐ（`D_settle` 前に window 閉鎖）。matured な貨幣の clawback は導入しない — 可換性を守る |
| 11 | cert 量産による audit fee（base coin）インフレ | §6.3 の MUST-revisit。v0.1 では testnet 計測で規模を出す |
| 12 | 貨幣による投票権購入 | 構成的に不可能（§4.5 の絶縁）。TOK をいくら集めても `W_i` はゼロのまま |
| 13 | 効率ハードウェアへの集中 | PoW の ASIC 力学と同型。緩和は ρ 表の級数設計（多様なモデル級数を eligible に保つ）に限る。完全な平準化は非目標 |

---

## 9. 実装スケッチ

### 9.1 store

- `TokenLedgerStore`（`consensus/src/model/stores/token_ledger.rs`）: `(asset_id, owner) → {balance, nonce}`
- `TokenSupplyStore`: `asset_id → {minted, burned}`
- `EmissionSettlementStore`: `epoch → settled marker + Σ paid`（冪等な再適用と reorg 巻き戻しのため）
- `database/src/registry.rs` に prefix 追加（`vlt_voting_snapshot` と同じ手順）

### 9.2 適用 seam（v0.1 改: 埋没 fold 方式 — PR 2 実装で確定）

- stateless 検証: `tx_validation_in_isolation` の subnet dispatch に 0x30/0x31 を追加（`validate_token_*_payload` → `TxRuleError::InvalidTokenPayload`）。**帯域の受理自体は他 overlay 帯と同じくリリース協調**（旧ビルドは `SubnetworksDisabled` で per-tx 拒否）であり、consensus fingerprint の pin 更新＝次回 flag day に含める。fence が守るのは効果の束縛のみ
- stateful 適用: **undo 機構を持たない埋没 fold** に確定した。当初案の「virtual-diff 経路（reorg 巻き戻し diff）」は採らない。台帳は「`max_reorg_horizon_blocks` を超えて埋没した selected-chain block の accepted 0x30/0x31 を acceptance 順に畳む」append-only fold であり、reorg が触れ得る block を fold が読まないので巻き戻しという状態が構成的に存在しない — credit accumulator が finalized epoch だけを write-once する論法の transfer 版
  - 代償は束縛遅延 ≈ reorg horizon（10 bps で ~30 秒）。支払いのファイナリティ遅延として受容（MUST 文書化、wallet UX は §9.3）
  - fold cursor（次に処理する chain index）と settlement cursor（次に検討する epoch）を singleton で永続化。shadow 区間は cursor だけ進めて効果を書かない = shadow 期のオペは**恒久に** void（fork 時の遡及 mint / 遡及 transfer を構成的に排除）
  - 同一 commit 内の fold と settlement は **一つの staging view を共有**（別々に WriteBatch へ書くと同一口座 key の後勝ちで先の delta が消えるため）
- emission settlement: `settle_token_emission` が **finalized credit 行（`DbVltCreditStore`）だけ**を epoch 順に読む。`has_settlement` で冪等、`D_settle` は `is_coherent_with_vlt` が finalization 埋没深度以上に拘束。credit 行が永遠に現れない過去 epoch（pre-shadow 史）は空 settlement を記録して cursor を前進（停滞防止）

### 9.3 RPC / index / wallet

- RPC: `getTokenLedgerEntry`, `getTokenSupply`, `getEmissionInfo(E)`（read）。書き込みは通常の `submitTransaction`
- indexer: `evm-indexer` は対象外のまま。native 資産は utxoindex 系列の軽量 index を別途（v0.1 では RPC 直読で可）
- wallet: Phase A は CLI（`misaka-cli`）のみ対応で足りる

### 9.4 台帳 state の同期（未決・リスク）

Token Ledger は UTXO 多重集合 commitment の外にある。pruning window 内は overlay tx の再適用で再構成できるが、pruned IBD には ADR-0022（EVM overlay snapshot）と同型の snapshot 供給が要る。v0.1 実装の最重量課題として §12 に残す。header への ledger root commitment 追加は v0.1 では行わない（hard-fork 面積を抑える）。

### 9.5 実装順序

1. 前提: `dns-partition-fix-t10` 系列の `VltEpochSnapshot` 完了 + PR 2（weight-source flip）着地
2. Phase A store + payload 検証 + 適用 seam（emission なし、activation INERT）
3. emission settlement + supply-conservation suite
4. testnet: shadow 計測（§10）
5. Phase B（CreateMint/MintTo）は別 PR 系列

---

## 10. 段階導入と activation

既存の二段 fence 慣行に従う（MUST）:

```text
tkn_shadow_activation_daa_score   … 台帳・emission を計算しログするが、貸記しない
tkn_activation_daa_score          … 貸記開始（hard fork）
emission_activation_epoch         … R(E) の E_a 原点
```

- 出荷 default は全網 INERT（`VltParams::INERT` と同じ流儀）。module 追加自体は consensus 変更にならない
- 制約: `tkn_activation ≥ vlt_shadow_activation`（credit 機構が動いていない網に emission は定義されない、MUST）。**v0.1 改**: 当初は weight fence を下限としたが、settlement が読むのは shadow fence から動く finalized credit 行であり、weight fence（§6 活性化状態機械）への結合は 2026-08-10 の devnet 実走で「activation 待ちの overlay に emission が同伴停止する」ことが判明したため shadow fence に緩和した。金は票の活性化と独立
- shadow 期間で凍結する数値: `R0`, `H`, `D_settle`, `min_network_compute`（emission 用途での妥当性）, audit fee 方針（§6.3）, committee 再較正（§8.9）
- 供給保存 suite（§4.2）と skip-class 相当（nonce 不一致 = 無視）の e2e を activation 前提条件とする

---

## 11. 非目標

- **orderer / job marketplace / Job fee** — job は self-originated のまま。有用性・需要の価格付けは将来の fee market 設計
- **有用性スコアの consensus 算入** — 主観量は式に入れない（§0 成立条件 5）
- **UTXO 多資産化** — 案 A として却下（§3）。将来の移送は妨げない
- **EVM facade / bridge** — §7 の拡張パスに委ねる
- **freeze / clawback / 管理者権限** — governance-free 原則のまま
- **TOK による fee 支払い** — fee market 変更は別設計
- **基軸コインの hybrid issuance** — §6.4 で却下

---

## 12. 未決事項

| # | 事項 | 期限 |
|---|---|---|
| 1 | 資産名・ticker | **決定（2026-08-10）: Token / TOK** |
| 2 | `R0` / `H` / `D_settle` の凍結値（TBD 維持を 2026-08-10 確認。§5.2 の数値は引き続き候補例示） | testnet shadow 後 |
| 3 | audit fee の財源 | **設計済み（2026-08-11）**: 統一仕事量 emission — `misaka-audit-emission-v0.2-design.md`。実装は後続 PR |
| 4 | Phase B（一般 mint）の activation 時期 | Phase A 安定後 |
| 5 | pruned IBD 向け台帳 snapshot 方式（ADR-0022 準拠の詳細） | Phase A 実装中 |
| 6 | certificate への payout 先フィールド追加（§5.4 の MAY） | 需要を見て |
| 7 | 台帳 root の header commitment（light client 対応） | 将来 fork |

---

## 付録 A. 一枚図

```text
 掘る対象         PoW: nonce 探索            MISAKA: 検証可能な LLM 計算
 検証             hash 1 回                  CanonicalFullReplay（決定的再実行）
 仕事の測度       hashrate                   x_j = ρ·(a·t_in + b·t_out)
 difficulty       target 再調整              X(E)/R(E)（自動）
 発行             coinbase tx                epoch settlement → Token Ledger 貸記
 成熟             coinbase maturity          pin 済み snapshot（フォーク不変なので不要）
 資産             基軸コイン                  TOK（第 0 ネイティブ資産、投票権なし）
 権力             なし（PoWは票を持たない）    C_i(E) 経由のみ（TOK 残高は無関係）
```
