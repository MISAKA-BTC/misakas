# MISAKA Fact Settlement Layer(FSL)設計書 v0.3(統合版)

**版:** Consolidated Draft v0.3 — Evidence-Anchored Fact Settlement on MISAKA L1
**日付:** 2026-06-10
**コード接地版:** [ADR-0021](adr/0021-fact-settlement-layer.md)(本文書がその source design)
**構成:** v0.1(コア設計)+ v0.2(信用獲得要件)+ 外部レビュー対応(COI/Sybil の entity 化、経済保証の再定義、evidence 再現可能性、correction 独立化、下流 override 規律、Predicate DSL 正式化、Shadow 方法論強化、二製品分割)を単一文書に統合する。
**位置づけ:** MISAKA L1(UTXO/DAG)+ EVM レーン([EVM 設計書 v0.4](misaka-evm-design-v0.4.md))+ DNS finality overlay(ADR-0009/0010/0011)上のサブシステム。consensus 変更は ML-DSA-87 検証 precompile のみ。
**監査トレーサビリティ:** 外部レビュー指摘との対応は §28 に全件記録する。

## 0. 結論
作るべきものは prediction market ではなく **Fact Settlement Layer** である: 命題(Claim)を機械可読 Predicate DSL で厳密に定義し、証拠(Evidence)を第三者再計算可能な forensic package として恒久保存し、entity 単位で COI を統制した裁定機構で確定(Settle)し、確定済み事実をリスク分解情報付きの API として提供する。予測市場・保険・DeFi 清算・RWA・AI エージェントはすべて下流統合である。

中核原理は 4 つである。
- **曖昧性は裁定段階ではなく作成段階の欠陥として潰す。** Predicate DSL(§5)が market rule compiler として機能し、Strategy 型の「事象の発生時刻 vs 確認可能時刻」問題を claim 作成時に構造的に排除する。
- **裁定階層の機構化。** `証拠 > 評判 > 専門性 > ステーク > 投票` をエスカレーションラダー L0–L4 として実装する。投票は最終手段であり、void(裁定不能)エスケープハッチを常設する。evidence 引用のない票は無効である。
- **検証可能な説明責任。** verdict は第三者が同一結論を再計算できる evidence package を伴い(§6)、裁定参加は entity-bound credential で one-entity-one-seat を第三者監査可能にし(§10)、経済保証は分解されたリスク情報として常時公開する(§12)。
- **信用は主張ではなく実績で獲得する。** 採用経路は Evidence Graph API(Product A)を先行させ、方法論を固めた Shadow Resolution の公開実績の上に Settlement Adapter(Product B)を載せる(§19–§20)。

正直に明記すべき限界: **oracle problem は解決できない。構造化できるだけである。** 本レイヤーが確定するのは形而上の真実ではなく「証拠トレイル付きでプロトコルが裁定した結果」である(§2.3 R1)。

## 1. 市場環境と領域定義(2026-06 時点)
### 1.1 既存 resolution stack の現状
```
価格・決定論市場:   Chainlink が獲得済み。Polymarket は 2025-09 に Data Streams +
                    Automation を統合し、5/15 分市場で数十億 USD 規模を自動決済。
                    subjective 領域への拡大も表明済み — 時間窓は有限。
イベント事実市場:   UMA Optimistic Oracle(bond 標準 $750 pUSD、challenge 2h、
                    2 回目 dispute 後に DVM token vote)。構造的危機にある:
  - 2026 年の紛争市場 1,150 件超(2025 通年超)。2026-04 だけで $1B 超の
    230 契約がトークン投票へ。
  - WSJ 調査(2026-05): 紛争市場の投票の過半が上位 10 wallet。投票者の
    約 60% が Polymarket アカウントに連結。紛争の約 1/5 で裁定対象市場に
    金銭的利害を持つ投票者が参加。
  - Strategy BTC 売却市場($60M+): 売却執行は期限内、8-K 開示は期限翌日。
    「執行時刻か公開確認時刻か」が文言上未定義のままトークン投票で No 確定。
    取引開始後の Additional Context が confirmation 基準に寄ったことが
    「遡及的ルール変更」と受け止められた。
  - Barron Trump 市場: プラットフォームが oracle 裁定と異なる返金対応を
    行った事例 — oracle の「拘束性」は contract 外の運用で迂回され得る。
  - UFO 市場(2025-12): 証拠なき whale 投票で Yes 確定。
```
### 1.2 FSL の領域定義
```
取りに行く領域: event facts — 企業アクション(売却・買収・決算)、規制 filing、
  上場/廃止、公式記録(スポーツ・選挙管理・統計局)、マクロ指標。
  「機械可読な一次資料が存在するが、価格 feed では決済できない」事実。
取りに行かない領域: 価格 feed(Chainlink が構造的に優位)/ 主観命題(FD6 で受理拒否)
```

## 2. 目的・非目的・トップリスク
### 2.1 目的
- 機械検証可能な命題の確定を、単一運営者・単一 oracle トークン・匿名常設投票に依存せず行う。
- 結論ではなく、第三者が再計算可能な証拠 package を on-chain commitment として残す。
- 確定事実を finality セマンティクス(latest/safe/finalized)とリスク分解情報付き API として提供する。
- 裁定の COI を entity 単位で統制し、その統制自体を第三者監査可能にする。
- 既存インフラ(DNS overlay の stake/attestation/slashing、EVM レーン、DAG の高頻度包含)を再利用する。
### 2.2 非目的
- 新規 L1 の構築(FD1)。
- 主観的・解釈依存の命題の裁定(template gating で受理段階から排除)。
- 賭博プラットフォームの運営(市場は下流統合)。
- 「真実を投票で決める」こと(stake 加重投票は L3 最終ラウンドのみ、void 併設)。
- 価格 feed 領域での Chainlink との正面競争。
### 2.3 トップリスク
- **R1: Oracle problem の構造的限界。** 保証は「指定された証拠と手続きに基づく裁定が改竄不能・監査可能・再計算可能である」ことに限る。「truth」を無条件に謳わない。
- **R2: Schelling game への賄賂(p+ε)・大口支配・Sybil。** 緩和は entity credential(§10)、commit-reveal、控訴 stake 倍増、void、リスク分解の常時公開(§12)、そして投票到達率自体の最小化。
- **R3: 曖昧な claim 文言。** Predicate DSL(§5)で作成段階から排除する。
- **R4: 検出不能な COI。** on-chain ポジション照合は検出可能な COI の一部にすぎない(実質支配者、CEX/OTC ヘッジ、他人名義、法人関係は不可視)。多層統制(§10)を課すが、残余リスクは開示事項とする。
- **R5: 下流プラットフォームによる事実上の override。** FSL に admin key がなくても、返金・追加配布・再上場・UI 変更で実質 override は可能。統合規約と override_event 公開登録(§14)で対処するが、強制力は契約に依存する。
- **R6: 補償判定の自己裁定。** backstop の独立化(§13)で構造的に分離する。
- **R7: pruning と恒久性の矛盾。** EVM state(persist)+ archival network + pruning point trusted data 併記(§16)。
- **R8: EVM レーンの非 PQ 性との緊張。** PQ 境界の分離(§4.2)。

## 3. 設計判断(FD)
- **FD1: 新チェーンではなく MISAKA 上のレイヤー。** ロジックは EVM contract、attestor インフラは DNS overlay 再利用、証拠 body は archival network。consensus 変更は FD3 のみ。
- **FD2: 裁定階層をエスカレーションラダー L0–L4 として機構化**(§9)。経済的に「ほとんどの claim は L0/L1 で確定し投票に到達しない」を定常とする(KPI K3)。
- **FD3: ML-DSA-87 検証 precompile(0xF003)を EVM レーンに追加**(§4.3)。唯一の consensus 変更。
- **FD4: 証拠は forensic package の commitment を on-chain、body は archival network**(§6)。attestor の pin 義務 + storage challenge + slashing。
- **FD5: 裁定者選出乱数は attestor commit-reveal beacon**(§11)。prevrandao は完全予測可能のため使用禁止。
- **FD6: claim は審査済み template(Predicate DSL)からのみ生成可能**(§5)。主観命題は構造的に作成不能。predicate_hash は作成時固定で、事後変更経路が存在しない。
- **FD7: 事実の読み出しは finality tag に従う。** 高価値統合は settled_final のみ。
- **FD8: source identity は DNS ドメイン束縛 + ML-DSA-87 キー登録**(§7)。
- **FD9(新): 裁定参加は entity-bound credential を必須とする**(§10)。one-entity-one-seat を ZK/KYC provider 経由で第三者監査可能にする。address clustering は補助情報に格下げする。
- **FD10(新): 経済保証は単一の「保証額」ではなく分解されたリスク情報として定義・公開する**(§12)。posted_bond / slashable_stake / attack_cost_lower_bound / backstop cap / 補償流動性 / exposure 上限を区別し、混同した主張を行わない。
- **FD11(新): 採用製品を二分割する**(§20)。Product A(Evidence Graph API、settlement 権限なし)を先行させ、Product B(Settlement Adapter)はその実績の上に載せる。
- **FD12(新): correction / backstop は通常裁定から独立化する**(§13)。original 裁定参加者・foundation 関係者は自動除外、高額補償は外部 review を要求する。
- **FD13(新): 下流統合は post-facto rule change を禁止する product contract を伴う**(§14)。Additional Context は predicate_hash 不変の範囲に限定し、意味変更は void/refund + 新 claim でのみ行う。
- **FD14(新): evidence は「リンク集」ではなく再計算可能な forensic package とする**(§6)。parser version と抽出 field まで固定する。

## 4. アーキテクチャ配置と PQ 境界
### 4.1 全体配置
```
MISAKA L1 (UTXO/DAG, PQ-safe, ML-DSA-87)
 ├─ DNS finality overlay ─────────── attestor set / stake / slashing(再利用)
 ├─ EVM lane (v0.3/v0.4) ────────── FSL contracts + MLDSA87_VERIFY precompile
 │    ├─ TemplateRegistry           Predicate DSL template(§5)
 │    ├─ ClaimRegistry              claim 生成・状態機械(§8)
 │    ├─ EvidenceAnchor             forensic package commitment(§6)
 │    ├─ SourceRegistry             source identity / reputation(§7)
 │    ├─ EntityCredentialRegistry   裁定参加者の entity 証明(§10)
 │    ├─ ResolutionEngine           L0/L1 optimistic 確定(§9)
 │    ├─ DisputeCourt               L2/L3 裁定・控訴(§9)
 │    ├─ CorrectionCourt            訂正・補償の独立裁定(§13)
 │    └─ FactStore                  確定事実の正規ストア(admin key なし)
 ├─ Archival network ────────────── evidence body / verdict 文書の保存(FD4)
 └─ 消費 chain アダプタ(Polygon 等) ─ Product A API gateway / Product B OOV2 互換
```
### 4.2 PQ 境界(R8 の解決)
```
PQ-safe domain(長期検証が必要なもの):
  evidence package hash:     keyed BLAKE2b-512(domain "FSL_Evidence64")
  evidence graph commitment: BLAKE2b-512 merkle(Hash64)
  attestor / source 署名:    ML-DSA-87
  verdict bundle:            ML-DSA-87 署名 + Hash64 commitment
EVM compatibility domain(経済操作のみ):
  bond / 手数料 / 報酬決済:   EVM native MISAKA / ERC-20(secp256k1 tx)
```
原則: **secp256k1 は「いま資金を動かす権限」にのみ、「将来検証される事実の帰属」には ML-DSA-87 を使う。** なお外部 pitch における PQ の位置づけは耐久性項目であり先頭に置かない(§21)。
### 4.3 MLDSA87_VERIFY precompile
```
address:  0x000000000000000000000000000000000000F003
input:    pubkey(2592B) || message_hash(64B, BLAKE2b-512) || signature(4627B)
output:   1 if valid else 0
gas:      MLDSA_VERIFY_GAS_BASE(初期値 20,000)+ calldata cost(~117k gas が支配的)
```
calibration は ML-DSA-87 verify 実測(verify 63.9–76.5 µs、最遅 portable 基準)に従う。NIST FIPS 204 KAT + 差分 fuzz をテスト必須とする。Ethereum 側の ML-DSA precompile 標準化動向(EIP 提案)は互換性観点で追跡する(FO13)。

## 5. Predicate DSL(正式仕様)— market rule compiler
### 5.1 必須フィールド
template は次の全フィールドを機械可読に持たなければならない。欠落 template は登録不能とする。
```
predicate:                  型付き述語(対象、metric、比較演算、閾値、単位)
market_end_time:            UTC 必須 + market_end_time_timezone の明示表記
primary_source_rank:        一次 source の優先順位リスト(SourceId)
backup_source_rank:         一次 source 不達時の代替リスト
event_time_basis:           execution / public_confirmation / filing_time
confirmation_time_basis:    何をもって「確認」とするか(filing 受理 / 公式発表 / 双方)
filing_time_basis:          filing の効力時刻の解釈(受理時刻 / 記載された執行時刻)
confirmation_lag_policy:    event が期限内・確認が期限後の場合の扱い
evidence_cutoff_time:       証拠として算入可能な公開時刻の締切
post_cutoff_evidence_policy: 締切後に出現した証拠が「期限内の event」を証明する場合に
                            採用可能か(admissible / inadmissible / L2 判断)
source_conflict_policy:     一次 source 間の矛盾時の優先規則
correction_policy:          source 自身の訂正(訂正 filing 等)の扱い
minimum_source_finality:    source 側の確定度要件(速報 / 確定値)
N/A_policy(void 条件):     裁定不能条件の列挙
bond_class / risk_tier:     §12 の経済区分
related_market_definition:  COI 統制(§10)が参照する関連・相関市場の定義
```
### 5.2 Strategy 型の回帰例(本仕様の存在理由)
```
「{company} は {deadline} までに {asset} を売却するか」
  event_time_basis = execution
  evidence_cutoff_time = deadline + 7d
  post_cutoff_evidence_policy = admissible
    → 期限翌日の 8-K でも、記載された執行時刻が期限内なら Yes。
  event_time_basis = public_confirmation
  post_cutoff_evidence_policy = inadmissible
    → 期限内に確認可能でなければ No。
どちらの解釈かは claim 作成時に固定され、裁定段階の解釈余地が存在しない。
この組合せ網羅を恒久回帰テスト群(F15)として維持する。
```
### 5.3 ガバナンス
template の追加・改定は公開 RFC + 敵対的解釈の事前演習(red-team、§23)を経る。review 記録は archival 保存し、事件発生時に「この曖昧性は審査済みか」を遡及検証できるようにする。曖昧性に起因する void は template defect として別 KPI(K2b)に計上する。

## 6. Evidence Layer — Forensic Evidence Package(FD14)
evidence は第三者が verdict を再計算できる単位でなければならない。リンクと結論だけの evidence は無効とする。
```
pub struct EvidenceNode {
    // 同定
    package_hash:        Hash64,                 // 以下全体の keyed BLAKE2b-512
    source_type:         SEC_filing / issuer_IR / exchange_notice / election_board /
                         sports_official / statistics_bureau / court_record /
                         onchain_proof / news_backup,
    source_id:           SourceId,               // §7 登録 source
    source_authority_rank: u8,                   // template の rank に対応
    // 取得物
    canonical_url:       String,
    archived_refs:       Vec<ArchiveRef>,        // WARC / IPFS / Arweave hash(≥1 必須)
    content_hash:        Hash64,                 // 取得 body の hash
    snapshot_hash:       Option<Hash64>,         // screenshot / PDF render
    tls_notary_proof:    Option<bytes>,          // 取得経路の暗号証明(可能な場合)
    source_sig:          Option<MlDsa87Sig>,     // source 自身の署名(最強の帰属)
    // 時刻の四分離(Strategy 型の核心)
    event_execution_ts:  Option<u64>,            // 事象の執行時刻(資料記載)
    publication_ts:      u64,                    // 資料の公開時刻
    filing_ts:           Option<u64>,            // filing の受理時刻
    effective_ts:        Option<u64>,            // 資料が主張する効力時刻
    retrieval_ts:        u64,                    // FSL 側の取得時刻
    anchored_at:         u64,                    // chain 登録時刻(検証可能)
    // 抽出の再現性
    parser_id:           Hash64,                 // 抽出器の識別(コード hash)
    parser_version:      String,
    extracted_fields:    Borsh bytes,            // 述語評価に使う field の構造化値
    extraction_confidence: u8,
    human_override_flag: bool,                   // 人手修正の有無(修正は別 node)
    parents:             Vec<Hash64>,            // 依存 evidence
}
```
規則:
- **再計算可能性:** 任意の検証者が archived_refs から body を取得し、parser_id/version で extracted_fields を再生成し、predicate に代入して同一 outcome に到達できること。これが evidence の有効条件である。
- parser はオープンソースで versioned とし、parser 更新は新 node を生む(過去 node の遡及変更なし)。human_override は別 node として履歴化する。
- anchoring は commitment のみを EVM state に書き、DAG の ~100ms 包含で PoW 裏付きの存在時刻を与える。body は archival network(attestor pin 義務 + storage challenge + slashing 接続)。
- 結論ではなく証拠を残す。verdict は evidence node 集合の引用を必須とする(§9)。

## 7. Source Reputation Layer
```
pub struct SourceRecord {
    source_id:     Hash64(domain),
    domain:        String,                  // 例: "ir.tesla.com"
    mldsa_pubkey:  MlDsa87PubKey,           // DNS TXT / well-known 束縛(DNS overlay 手法)
    class:         Official / Exchange / NewsAgency / Registered / Anonymous,
    score:         u32,                     // 0–1000、参考値
    history:       OutcomeStats,
}
```
score は確定結果との整合で指数移動平均更新する(初期値: Official 980 / Exchange 950 / NewsAgency 900 / Registered 400 / Anonymous 100)。**score は informative であって authoritative ではない**: L0 許可判定・bond 割引・UI 表示にのみ使い、court は evidence を de novo で判断する(回帰テスト F13)。source キーの rotation/revocation を実装し、L0 にも challenge window を必ず残す(キー漏洩対策)。

## 8. Claim 状態機械
```
Created → Proposed → (challenge window) → Settled(L0/L1)
                   ↘ Disputed → PanelVerdict(L2) → (appeal) → CourtVerdict(L3) → Settled
                                                  ↘ Voided(L4)
Settled / Voided は finalized tag 到達後 Immutable。
Immutable 後の訂正は CorrectionCourt(§13)による Corrected 追記のみ(原記録不変)。
```

## 9. エスカレーションラダー(FD2)
- **L0: Automated source resolution。** template が許可する場合のみ。優先 source(Official/Exchange、score 閾値以上)の ML-DSA-87 署名付きデータで述語評価が決定的なら、短縮 challenge window 経過後に自動確定。L0 でも evidence package と challenge window は必須(撤廃禁止)。
- **L1: Optimistic resolution。** `propose(claim_id, outcome, evidence_root, bond)`。evidence_root は EvidenceAnchor 登録済み package のみ参照可能。challenge window は `max(CW_BASE(risk_tier), finalized 到達時間 × 2)`。異議は bond × 1.5 で L2 へ。
- **L2: Attestor panel。** beacon 選出(§11)の panel(初期 7 名、entity credential 必須、§10 の COI 統制)。verdict bundle は outcome + 引用 evidence(空は無効票)+ reasoning hash + ML-DSA-87 署名。過半数で確定。
- **L3: 控訴 court。** stake 倍増で受理、juror 数は控訴ごとに 2 倍+1。最終ラウンドは commit-reveal の stake 加重 Schelling game — **投票に依存する唯一の地点であることを仕様に明記**。控訴で覆った場合、前ラウンドで最終結果側に投じた少数派へ追加報酬(「証拠が強い少数派」の incentive-compatible な実装)。
- **L4: Void。** void 条件成立、quorum 不達、または僅差(< VOID_MARGIN)は Voided 確定: bond は手数料控除後返還、下流は N/A 決済。曖昧命題への裁定強行をしない。
- **賄賂・大口攻撃の緩和:** commit-reveal、控訴 stake 倍増、選出予測不能性、void margin、リスク分解の常時公開(§12)。残余リスク(十分大きな賄賂は最終ラウンドを破り得る)は開示。

## 10. 裁定参加者の資格と COI 統制(FD9)
### 10.1 Entity-bound credential(one-entity-one-seat)
K6(panel 内投票力 < 1/3)は entity 識別なしには証明不能である。address ベースの統制を廃し、次を必須とする。
```
pub struct AdjudicatorCredential {
    entity_id_commitment:    Hash64,        // 公開身元ではなく実体一意性の commitment
    credential_provider:     ProviderId,    // 認定 ZK/KYC provider(複数、法域分散)
    jurisdiction:            ISO country,
    stake_owner_attestation: Hash64,        // stake の実質保有者の宣誓 commitment
    beneficial_owner_attestation: Hash64,   // 受益者宣誓
    related_wallet_commitments: Vec<Hash64>,// 関連 wallet の自己申告 commitment
    conflict_disclosure_commitment: Hash64, // claim 別 COI 申告に紐づく
}
```
- provider は「同一実体に複数 credential を発行していない」ことの監査を受ける(provider 自体の認定・取消は governance)。公開 KYC は要求しない — 要求するのは**第三者監査可能な one-entity-one-seat** である。
- credential 偽装(複数席の保持)の発覚は全席 slashing + 永久除外 + 当該 entity が関与した過去 N 件の verdict review trigger。
- address clustering・on-chain ポジション照合は**補助的検出手段**に格下げし、唯一の COI 検出手段としない(R4)。
### 10.2 COI 規律(多層)
```
申告:     claim ごとに、対象市場・related_market_definition(§5.1)該当市場・
          関係当事者性を申告。申告は selection 前に commitment、selection 後に開示。
除外:     申告利害は beacon 選出の除外条件。
blackout: panel 選出から verdict finality まで、assigned claim と関連・相関市場での
          取引を禁止(credential の related wallets を含む)。違反は slashing。
事後照合: on-chain ポジション連結の検出 bounty(補助)。
review:   未申告 COI 発覚は slashing に加え、当該裁定者の過去 N 件の verdict review。
公開:     検出件数を K5 として公開。検出不能な COI の残余リスクは開示文書に明記。
```

## 11. 乱数 beacon(FD5)
```
epoch e: 各 attestor が commit = blake2b_512("FSL_Beacon64" || epoch || secret) を前半に提出、
         後半に reveal(未 reveal は減点、連続で slashing)
beacon(e) = blake2b_512(concat(sorted revealed secrets))
選出:     weighted_sample(eligible credentials, stake, seed = beacon(e) || claim_id)
          beacon(e) は epoch e+1 の選出にのみ使用(last-revealer bias の遮断)
```

## 12. 経済設計 — リスク分解と保証規律(FD10)
### 12.1 概念の分離(旧 K9 の矛盾の解消)
「経済保証額 ≥ 下流決済額」という単一比は、bond(攻撃の担保)と補償能力(誤裁定の救済原資)と攻撃コスト(買収の下限費用)を混同しており、α=0.5% の bond と両立しない。v0.3 では次を別個に定義し、混同した対外主張を禁止する。
```
posted_bond:                 提案者/異議者が積んだ担保
slashable_panel_stake:       当該裁定に参加した credential の slashing 対象 stake 合計
attack_cost_lower_bound:     現裁定段階を覆す最小費用の見積(stake 買収 + 控訴経路)
backstop_per_fact_cap:       誤裁定時の per-fact 補償上限
backstop_available_liquidity: backstop fund の現在流動性
max_downstream_exposure:     本 fact への依存決済の許容上限(下記 K9a)
```
### 12.2 保証規律(新 K9 群)
```
K9a: downstream_exposure ≤ 公開された補償能力
     min(backstop_per_fact_cap, backstop_available_liquidity 比例配分)を超える
     依存決済を統合規約で禁止する。
K9b: attack_cost_lower_bound ≥ β(risk_tier) × downstream_exposure
     β 初期値: tier-A(filing/公式記録)1.5 / tier-B(複合事実)3.0 /
     tier-C(高争点)5.0。
K9c: posted_bond + slashable_panel_stake ≥ γ(risk_tier) × downstream_exposure
     γ 初期値: tier-A 0.05 / tier-B 0.15 / tier-C 0.30。
K9d: K9b/K9c を満たせない claim は受理拒否、または multi-oracle 必須
     (2-of-3 構成)とフラグする。
```
`MIN_BOND = α × 想定決済額(α=0.5%)` は **低額・低争点の tier-A L1 市場に限定**する。tier-B/C は K9b/K9c が支配する。
### 12.3 リスク情報 API
`misaka_getFactSecurityInfo` を廃止し、分解表示に置換する。
```
misaka_getFactRiskInfo(claim_id) -> {
  value_at_risk_declared,            // 作成時申告の想定決済規模
  max_downstream_exposure_allowed,   // K9a の上限
  posted_bond,
  slashable_stake_at_risk,
  estimated_attack_cost_lower_bound, // 算定式の version 付き(F14 で再現検証)
  backstop_per_fact_cap,
  backstop_available_liquidity,
  finality_tier,                     // latest / safe / finalized / immutable
  challenge_window_remaining,
  escalation_level_reached,          // L0–L4
  unresolved_correlation_risks,      // 同一 event に依存する他 fact 群の連鎖 exposure
}
```
`unresolved_correlation_risks` は重要である: 同一 filing に依存する複数 fact が同時に誤る相関を、独立 fact の保証として二重計上してはならない。
### 12.4 手数料・分配・slashing
```
収入:   claim 作成料(risk_tier 別)/ resolution 手数料 / Product A API 課金(§20)
分配:   敗者 bond → 50% 勝者 / 30% 裁定参加 / 20% treasury(backstop・archival 補助)
slashing: COI 違反、credential 偽装、blackout 違反、storage challenge 失敗、
          beacon 未 reveal 連続、控訴で覆った多数派(段階的)
stake:  FSL 裁定 stake は DNS overlay bond と分離(裁定紛争の L1 finality への波及遮断)
```

## 13. Correction / Backstop の独立化(FD12)
### 13.1 訂正の独立裁定
「補償判定を FSL claim として自己裁定する」構造(v0.2)は廃止し、CorrectionCourt に分離する。
```
- T-CORRECTION は correction-special panel(専用 credential pool)で裁く
- original verdict に参加した全 juror/attestor/proposer は自動除外
- foundation / treasury の関係 entity も除外(credential の関係宣誓で執行)
- 発動条件: Immutable 確定後の決定的後発証拠(訂正 filing、裁判記録、公式訂正)
- 処理: FactStore へ Corrected レコード追記(原記録は不変 — 履歴付き訂正)
- 補償: parametric 化可能な部分(訂正 filing の存在等、機械判定可能な条件)は
  自動支払い。閾値超の高額補償は external review committee または
  2-of-3 external oracle(他 oracle 併用)の承認を要求
- 補償拒否も含め全件 post-mortem 公開(K12)
```
backstop fund が大きくなるほど「誤りを認めると fund が減る」逆インセンティブが生じるため、この独立性は信用設計の必須要素である(R6)。
### 13.2 Backstop fund
```
資金源: treasury 配分 + resolution 手数料の一部
公開:   backstop_available_liquidity / per-fact cap を misaka_getFactRiskInfo で常時公開
規律:   補償実績・拒否実績の全件公開。overturn(K1)が目標域である限り fund は
        積み上がり、それ自体が品質の公開証明になる
```

## 14. 下流統合規約と override 規律(FD13)
FactStore に admin key がないこと(§22.1)は必要条件だが十分条件ではない。下流は返金・追加配布・再上場・UI 変更で事実上の override が可能である(Barron Trump 型)。統合規約(product contract)で次を課す。
```
- 高価値決済は settled_final のみをトリガーとする(FD7)
- downstream_exposure ≤ misaka_getFactRiskInfo の公開上限(K9a)
- market 作成後の Additional Context は predicate_hash を変えない範囲に限定する
- 意味の明確化が必要になった場合、元 market を void/refund し新 claim として再作成する
  (遡及的ルール変更の経路を product として持たない)
- 下流が FSL verdict と異なる支払い・返金・補償を行う場合、override_event として
  FSL に公開登録する義務を負う。override_event は当該統合の採用スコアに反映され、
  「FSL-settled」ラベルの使用条件(契約定義)に接続する
- void(N/A)時の決済規則を市場規約に事前定義する
- 市場文言は predicate_hash の完全展開文書と一致させる(独自要約の禁止)
- Corrected レコードの監視と補償請求手続の事前合意
```
override の強制力は契約と公開圧力に依存する(R5)。FSL が保証するのは「override が隠せないこと」である。

## 15. API / RPC
```
EVM(コントラクト読み出し):
  FactStore.getFact(claim_id) -> (status, outcome, settled_at, risk_summary)
  FactStore.getFactAt(claim_id, block)
Native RPC:
  misaka_getFact(claim_id, tag)               // latest / safe / finalized
  misaka_getEvidenceGraph(claim_id)           // forensic package の DAG
  misaka_getVerdictTrail(claim_id)            // L0–L4 全裁定記録と署名
  misaka_getFactRiskInfo(claim_id)            // §12.3 のリスク分解
  misaka_getFslMetrics()                      // §18 KPI(raw data から第三者再計算可能)
  misaka_subscribeFacts(filter)               // websocket push
  misaka_verifyFactProof(claim_id, proof)     // chain 非接続検証(state proof + PQ 署名束)
  misaka_recomputeVerdict(claim_id)           // §6 の再計算手順の参照実装
status: proposed / disputed / settled_latest / settled_safe / settled_final /
        immutable / voided / corrected
```

## 16. Pruning と恒久性(R7)
```
EVM state(FactStore / registries / commitments): persist — pruning 非対象
evidence body / verdict 文書 / review 記録:        archival network(pin 義務 + challenge)
長期出口: epoch ごとの FactStore state root + verdict 署名束を
          EvmPruningPointTrustedData(EVM 設計書 §12.2)に併記。
          archive 全滅時も「確定結果 + 帰属署名」は再検証可能
```

## 17. 失敗→機構マッピング(信用主張の根拠表)
| 実例 | 失敗の構造 | FSL の機構的封殺 |
| --- | --- | --- |
| Strategy BTC 売却($60M) | 執行時刻と確認時刻の判定基準が文言上未定義。事後の文言追補が遡及変更と受け取られた | Predicate DSL の `event_time_basis` / `confirmation_lag_policy` / `evidence_cutoff_time` / `post_cutoff_evidence_policy` 必須化(§5)。predicate_hash 固定で遡及変更経路が原理的に不存在。下流の Additional Context も §14 で predicate 不変範囲に拘束 |
| 上位 10 wallet が投票過半 | 常設トークン投票 = 資本集中が裁定力 | beacon 選出 panel + **entity credential による one-entity-one-seat**(§10)+ panel 内投票力 < 1/3 の第三者監査可能性。投票は L3 最終のみ + void 併設 |
| 紛争の 1/5 に利害投票者 | COI 規律の不在 | 申告 / 選出除外 / blackout window / 関連市場取引禁止 / 事後照合 bounty / verdict review trigger の多層統制(§10.2)。検出不能 COI の残余は開示 |
| 証拠なき whale 投票(UFO) | 票が意見であり証拠と切断 | evidence 引用なき票は無効。verdict は再計算可能な forensic package を引用(§6) |
| bond $750 vs $60M | 保証と決済の 5 桁乖離、かつ「保証」概念の未分解 | リスク分解(§12.1)+ K9a–K9d + exposure 超過 claim の受理拒否 / multi-oracle 強制 |
| Barron Trump 型 override | oracle 外の運用で実質 override 可能 | FactStore に admin 経路なし + 下流 override_event の公開登録義務(§14)。「覆したこと」が構造的に隠せない |
| 2h window での高額確定 | finality 前の確定扱い | challenge window ≥ risk_tier 別下限、Immutable は finalized 到達後のみ |

## 18. 定量的信用目標(KPI、misaka_getFslMetrics で常時公開)
```
品質:
K1   overturn rate(Corrected 発行率)                       ≤ 0.1% / 年
K2   void rate                                              ≤ 2%
K2b  template defect 起因 void(別計上)                      公開、減少傾向を要求
K3   L3 投票到達率                                           ≤ 0.5%
K4   evidence trail 公開・再計算可能率                        = 100%(構造保証、F23 で検証)
K5   COI 違反検出件数                                        公開、slashing 執行率 100%
分散性:
K6   panel 内単一 entity 投票力(credential ベースで証明)     < 1/3
K7   上位 10 entity の年間裁定関与割合                        < 33%
K8   attestor / credential provider の法域分散                ≥ 5 法域、単一 < 40%
経済・運用:
K9a–K9d                                                     §12.2 の通り
K10  time-to-resolution(L0)                                ≤ 10 分 + challenge window
K11  time-to-resolution(L1 無紛争)                          ≤ template 規定
K12  post-mortem 公開率(紛争・void・訂正・補償拒否の全件)    = 100%、14 日以内
```

## 19. Shadow Resolution Program(方法論強化版)
### 19.1 プログラム定義
mainnet 稼働後 12–18 ヶ月、Polymarket/UMA の全 event 系市場(価格系除外)を並行裁定し、公開スコアボードを運用する。**ただし評価方法論を先に固定する** — 方法論なき一致率の宣伝は「都合の良い不一致だけを見せている」と看做され、信用設計を毀損する。
### 19.2 評価方法論(必須手続)
```
1. claim 化は market close 前に行い、predicate_hash を timestamped commitment として
   chain に登録する(後出し claim 定義の禁止)
2. FSL verdict は UMA final 確定前に公開、または commit-reveal で commitment を先行登録
   し UMA final 後に reveal する(カンニングの構造的排除)
3. 不一致案件は third-party gold-standard review(独立レビュアー、利害宣誓付き)に付す
4. accuracy に算入するのは「後発の決定的証拠で確定検証可能になった案件」のみ
5. 述語が両義的で gold standard が成立しない案件は accuracy から除外し、
   template defect(K2b)として別 KPI 計上する
6. FSL 側が誤った案件も同一詳細度で post-mortem 公開する
```
### 19.3 成功条件(Product B 展開の前提)
並行裁定 1,000 件以上、K1–K5 目標域、§19.2 手続下での不一致案件の正答実績、および Strategy 型・filing 型・公式記録型の高品質不一致 post-mortem 100–300 件。

## 20. 採用経路 — 二製品分割(FD11)
### 20.1 Product A: FSL Evidence Graph API(先行)
```
対象:   UMA 投票者 / トレーダー / アナリスト / market creator / 報道
権限:   settlement 権限なし(依存ゼロ = 採用障壁最小)
提供:   claim 化された市場述語の機械可読解釈、forensic evidence package、
        再計算手順、リスク情報、紛争中市場のリアルタイム evidence feed
収益:   API 課金(高頻度利用)+ 無料公開層(スコアボード)
狙い:   UMA 紛争の中で FSL の evidence graph が引用される状態 =
        事実上の上流化。Shadow Program(§19)と同一基盤
```
### 20.2 Product B: FSL Settlement Adapter(後続)
```
対象:   実際に market を settle する統合(新興予測市場 → tie-breaker → primary)
形式:   CTF / OOV2 互換 surface(requestPrice / proposePrice / disputePrice / settle)
        + Polymarket 実態への適合範囲を仕様化: CTF・UMA Adapter・Neg Risk・
        pUSD collateral・bulletin/clarification workflow・indexer・
        resolution timeline UI との整合を統合要件として明示する
        (「互換アダプタで統合変更ゼロ」とは主張しない)
配信:   Phase A: N-of-M attestor relayer(ML-DSA-87 + 宛先 chain secp256k1 二重署名、
        配信異議 window 付き)— 信頼仮定を明示開示
        Phase B: MISAKA light client proof による trustless 検証
要件:   §12 の保証規律、§13 の補償、§14 の統合規約、監査(§23)、法務
段階:   Tier 0(shadow)→ Tier 1(新興 primary)→ Tier 2(2-of-3 tie-breaker /
        参照 evidence 源)→ Tier 3(event facts primary)
```

## 21. 競争ポジショニングと対外 pitch 序列
差別化の本体は PQ でも独自チェーンでもなく、**market predicate の machine-checkable 化と、evidence/verdict の再計算可能性**である。対外 pitch は次の順序とする。
```
1. No retroactive rule changes(predicate_hash + immutable rule + 下流規約)
2. Reproducible evidence(forensic package + archive + parser version)
3. COI-resilient adjudication(entity-bound panel + blackout + slashing)
4. Public track record(方法論固定済み shadow scoreboard)
5. Economic accountability(リスク分解 + backstop + exposure 規律)
6. Easy migration(CTF/OOV2 互換 surface — 適合範囲明示)
7. Long-term cryptographic durability(PQ 署名 — 耐久性項目として最後)
```
Chainlink の event/subjective 領域拡大は公言されており時間窓は有限である。Product A の公開開始(F-M7')を全 milestone の最優先とする。

## 22. Security considerations
- **S1: 検出不能 COI(R4)。** §10 の多層統制後も、実質支配者・OTC ヘッジ・法人関係は完全には検出できない。残余リスクは開示文書と risk_tier の β/γ に織り込む。
- **S2: evidence 捏造。** anchor は存在時刻のみ証明。真正性は source 署名 > 複数独立 source の相互裏付け > 単独無署名の順で court が評価。取得経路は archive 併用 + 可能なら TLS notary。
- **S3: L0 source キー漏洩。** L0 にも challenge window 必須(撤廃禁止)、rotation/revocation 実装。
- **S4: beacon 操作。** reveal 義務 + epoch 分離。attestor set 過半の買収は DNS overlay 全体の前提崩壊であり依存として明示。
- **S5: spam claim。** 作成料 + template gating + 同一述語の canonical claim への正規化。
- **S6: credential provider の腐敗。** provider 複数化・法域分散(K8)・監査・取消手続。単一 provider 依存の禁止。
- **S7: 相関リスク。** 同一 event 依存の fact 群の連鎖 exposure を unresolved_correlation_risks で公開(§12.3)。
- **S8: 規制。** FSL は事実確定 API であり賭博提供者ではないが、法域別評価を運営文書として整備(設計書範囲外と明記)。

## 23. 監査・検証プログラム
```
- contract 監査 ≥ 2 社・公開(ClaimRegistry / DisputeCourt / CorrectionCourt / adapter)
- 形式検証: 状態機械到達可能性、bond 保存則、admin 経路不存在(F24 と二重化)
- MLDSA87_VERIFY: FIPS 204 KAT + 差分 fuzz
- 経済監査: attack_cost_lower_bound 算定式と K9 群の独立検証
- credential provider 監査: one-entity-one-seat の発行プロセス検証
- bug bounty 常設(裁定迂回・COI 検出回避・credential 偽装を最高等級)
- 公開 red-team: template の敵対的解釈コンテストを追加サイクルに組込み
```

## 24. MVP スコープ
```
Phase 1(MVP): Product A + 決定論 claim の L0/L1
  T-EARN(決算)/ T-FLOW(ETF 流入)/ T-LIST(上場・廃止)/
  T-SPORT(公式スコア)/ T-CHAIN(on-chain 指標)/ T-MACRO(統計指標)
  Shadow Program(§19 方法論込み)同時開始。L2 は手動運用、L3/L4 は仕様凍結
Phase 2: entity credential 本稼働、L2/L3/L4 完全稼働、CorrectionCourt、archival challenge
Phase 3: Product B(Tier 1 統合 → tie-breaker)、Phase B 配信(light client proof)
全 Phase で受理しない claim: 主観命題、意図・解釈、終結判定なき継続事象
```

## 25. Test plan(統合)
```
基盤・状態機械:
F1  template 外 claim の作成が構造的に不可能
F2  evidence 引用なき L2/L3 票が無効
F3  L1→L2 遷移と bond escrow
F4  控訴で覆った際の前ラウンド少数派(最終結果側)への報酬分配
F5  void 確定と bond 返還
F6  commit-reveal(reveal 前不可視、未 reveal 罰則)
F7  beacon の epoch 分離(last-revealer が選出を操作不能)
F8  MLDSA87_VERIFY の FIPS 204 KAT 一致 + 差分 fuzz
F9  storage challenge 失敗 → slashing 接続
F10 fact reorg(settled_latest 巻き戻り、settled_final 不変)
F11 misaka_verifyFactProof の chain 非接続検証
F12 自己言及 claim の class 制限
F13 source score が裁定入力に不使用(de novo 回帰)
Predicate DSL / evidence:
F15 Strategy 型回帰群 — event_time_basis × post_cutoff_evidence_policy の全組合せで一意裁定
F16 predicate_hash 固定 — 作成後の文言変更が全経路で不能
F23 misaka_recomputeVerdict — 第三者が archived body + parser から同一 outcome を再計算
F25 evidence 時刻四分離(execution/publication/filing/effective)の独立検証
F26 parser version 更新が過去 node を変更しない(新 node 生成のみ)
COI / entity:
F17 panel 単一 entity < 1/3 が credential ベースで常時成立(K6)
F18 未申告 COI の検出 → slashing + verdict review trigger
F27 credential 偽装(多席)発覚 → 全席 slashing + 過去 verdict review
F28 blackout window 中の関連市場取引検出 → slashing
経済 / 訂正:
F14 attack_cost_lower_bound 算定の第三者再現
F19 Corrected 追記(原記録不変、両 proof 可能)
F29 K9a–K9d 違反 claim の受理拒否 / multi-oracle フラグ
F30 CorrectionCourt の除外規則(original 参加者・foundation 関係 entity の自動除外)
採用 / 配信:
F21 OOV2/CTF 互換フローの end-to-end 決済(Neg Risk / pUSD 含む適合範囲)
F22 relayer N-of-M 閾値と配信異議 window
F31 shadow 方法論 — market close 前 commitment、UMA final 前 reveal の時系列強制
F24 admin 経路不存在の網羅的否定テスト(形式検証と二重化)
F32 override_event 登録と採用スコア反映
```

## 26. Implementation milestones
```
F-M0:  MLDSA87_VERIFY precompile(fork 同梱判断は FO1)
F-M1:  Predicate DSL / TemplateRegistry / ClaimRegistry / EvidenceAnchor(forensic 版)/
       FactStore(admin 経路なし)
F-M2:  L0/L1 ResolutionEngine + SourceRegistry + parser 基盤(open source / versioned)
F-M7': **Product A 公開(最優先)** — Evidence Graph API + 方法論固定済み shadow
       scoreboard。Strategy 型・filing 型・公式記録型の不一致 post-mortem を
       100–300 件規模で蓄積する
F-M3:  EntityCredentialRegistry + provider 認定 + attestor panel(L2)+ beacon
F-M4:  DisputeCourt(L3/L4)+ blackout 執行 + 控訴経済
F-M5:  CorrectionCourt + backstop fund + parametric 補償
F-M6:  misaka_getFactRiskInfo / misaka_getFslMetrics / 監査・red-team 第 1 サイクル
F-M8:  Product B(OOV2/CTF アダプタ + Phase A relayer)— §19.3 成功条件の充足後
F-M9:  Phase B 配信(light client proof)
```

## 27. Open decisions
```
FO1:  precompile fork の v0.4 M10 同梱 vs 独立
FO2:  evidence retention の無期限 vs 階層化(コスト実測後)
FO4:  treasury / governance(timelock 形式)
FO5:  scalar/categorical の L3 投票方式(中央値 vs 範囲)
FO8:  shadow 期間の claim 作成料の treasury 負担範囲
FO9:  β/γ/α の初期値検証と risk_tier 境界(経済監査と連動)
FO10: 2-of-3 構成での他 oracle 不一致時の公開プロトコル
FO11: backstop 対象統合の範囲
FO12: red-team bounty 等級
FO13: Ethereum 側 ML-DSA precompile 標準化(EIP 提案)との互換追跡
FO14: credential provider の初期認定基準と取消手続
FO15: external review committee の構成(高額補償の承認主体)
```

## 28. 外部レビュー対応表
| レビュー指摘 | v0.3 での解決 |
| --- | --- |
| 1. K9 と bond の矛盾(0.5% bond で 100% 保証) | §12: 概念の六分解、K9a–K9d、risk_tier 別 β/γ、α は tier-A L1 限定、misaka_getFactRiskInfo への置換、超過 claim の受理拒否 / multi-oracle 強制 |
| 2. COI が on-chain 可視利害に偏重 / K6 が entity 識別なしに証明不能 | §10: entity-bound credential(one-entity-one-seat、ZK/KYC provider、第三者監査可能)、blackout window、関連市場取引禁止、verdict review trigger、related_market を template 定義、clustering の補助格下げ。検出不能 COI は R4 として残余開示 |
| 3. admin key なしでは platform override が消えない | §14: Additional Context の predicate 不変拘束、意味変更は void/refund + 新 claim、override_event 公開登録 + 採用スコア + ラベル使用条件。「FSL が保証するのは override が隠せないこと」と限定を明記 |
| 4. evidence graph の再現可能性不足 | §6: forensic package(時刻四分離、parser_id/version、extracted_fields、archive 必須、TLS notary、human_override 履歴化)、misaka_recomputeVerdict 参照実装、F23/F25/F26 |
| 5. backstop の自己裁定 | §13: CorrectionCourt 分離、original 参加者・foundation 関係 entity の自動除外、parametric 補償、高額は external review / 2-of-3、補償拒否の post-mortem 義務 |
| A. Predicate DSL 正式化(post_cutoff_evidence_policy 等) | §5.1 の必須フィールド全採用 + Strategy 回帰例(§5.2)+ F15 恒久回帰群 |
| B. entity ベース Sybil 統制 | §10.1(FD9) |
| C. 経済保証の再定義 | §12(FD10) |
| D. correction 独立化 | §13(FD12) |
| E. shadow 評価方法論 | §19.2: close 前 commitment、UMA final 前 reveal、gold-standard review、検証可能案件のみ算入、template defect の別 KPI(K2b) |
| 採用経路の二製品分割 / 「統合変更ゼロ」の楽観修正 | §20(FD11): Product A 先行、Product B の適合範囲(CTF/Neg Risk/pUSD/bulletin/indexer)明示 |
| pitch 序列(PQ を先頭にしない) | §21: PQ は第 7 項目。差別化の本体は predicate compile と再計算可能性 |

## 29. References
- Polymarket Documentation, Resolution / Contracts(UMA OO、bond $750、2h window、Additional Context、UMA Adapter)。https://docs.polymarket.com/concepts/resolution / https://docs.polymarket.com/resources/contracts
- Polymarket/uma-ctf-adapter(CTF resolution adapter 実装)。https://github.com/Polymarket/uma-ctf-adapter
- Galaxy Research, Strategy Sold Bitcoin in May. Polymarket Says It Didn't.(2026-05 紛争の一次整理)。https://www.galaxy.com/insights/research/strategy-bitcoin-sale-polymarket-resolution-dispute-may-2026
- WSJ 調査の報道(2026-05、投票集中・COI 比率)。https://cryptobriefing.com/polymarket-dispute-resolution-scrutiny/
- Polymarket × Chainlink(2025-09、価格系自動決済と subjective 拡大方針)。https://www.prnewswire.com/news-releases/polymarket-partners-with-chainlink-to-enhance-accuracy-of-prediction-market-resolutions-302555123.html
- UMA 紛争プロセス解説・Barron Trump 事例。https://polymarkets.co.il/en/guide/uma-disputes/
- Vitalik Buterin, The P + epsilon Attack。https://blog.ethereum.org/2015/01/28/p-epsilon-attack
- NIST FIPS 204(ML-DSA)。https://csrc.nist.gov/pubs/fips/204/final
- Kleros Oracle / Reality.eth(分散仲裁の先行例)。https://kleros.io/oracle/
- MISAKA EVM 設計書 [v0.4](misaka-evm-design-v0.4.md)、ADR-0005/0009/0010/0011

## 30. 最終設計判断(v0.3)
1. FSL の競争領域は event facts であり、差別化の本体は predicate の machine-checkable 化と evidence/verdict の再計算可能性である。PQ 恒久性は耐久性項目として保持するが pitch の先頭に置かない。
2. 曖昧性は Predicate DSL により作成段階で潰す。predicate_hash 固定により、FSL 側にも下流側にも遡及的ルール変更の経路を残さない。
3. 裁定参加は entity-bound credential による one-entity-one-seat を必須とし、K6/K7 を第三者監査可能な主張にする。検出不能な COI の残余は開示する。
4. 経済保証は六分解したリスク情報として公開し、K9a–K9d の規律を満たせない claim は受理拒否または multi-oracle 強制とする。単一の「保証額」という対外主張を行わない。
5. 訂正・補償は CorrectionCourt に独立化し、自己裁定構造を排除する。
6. 下流統合には override_event 公開登録を含む product contract を課す。FSL が保証するのは「override が隠せないこと」である。
7. evidence は第三者が同一 verdict を再計算できる forensic package のみを有効とする。
8. 採用は Product A(Evidence Graph API + 方法論固定済み shadow scoreboard)を先行させ、Product B(Settlement)はその公開実績の上に載せる。
9. 投票は L3 最終ラウンドにのみ存在し、void エスケープハッチを常設する。evidence 引用なき票は無効である。
10. すべての紛争・void・訂正・補償拒否は 14 日以内に post-mortem を公開し、FSL 自身の誤りも同一詳細度で公開する。
