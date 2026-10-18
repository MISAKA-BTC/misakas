# Codex への引き継ぎ(2026-10-18)— G14 と RFC18 の未割当て作業

Claude 側では 4 本の lane が動いています。Codex には、**どの lane にも割り当てていない作業だけ**を渡します。下の「触らない範囲」と重ならないようにしてください。

## 0. 必ず読むもの

- `AGENTS.md`(= `CLAUDE.md`):暗号学的な計算証明(ZK・SNARK・STARK・zkVM・zkML・PCS など)は一切使わない。モデル tier は凍結(genesis・年次フォークは 1.0、登録モデルは 0.3、モデル単位の審査はしない)。
- `docs/PRINCIPLES.md` §10〜§12:§10 は slash の損失が不正の利得を上回ること・court が正直な仕事を slash しないこと。§11 は設計凍結(設計変更は ADR に記録)。§12 は画像・動画・モデル改善を mainnet 後へ延期し、当面無効。
- `docs/rfc/0023-unresolved-claim-liability-and-independent-progress.md` と `docs/adr/0186-*.md`
- `docs/audit/palw/rfc18-dag-audit-2026-10-17/assessment_ja.md` と `shared-frontier-sources.md`
- `docs/audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md` の §11〜§13(G14 の現状)

**G14 の目標:** 計算していない claim を出した miner がいても、Panel に正直な 1 人(または Panel 外の bond を持つ検証者 1 人)がいれば、公開資料から客観的に不一致を証明できる。そのうえで、producer の slash、consensus credit と block 権利の拒否、報酬の拒否まで進むこと。報酬が出る全経路で成り立つこと。

## 1. 作業の基準点

- base:`origin/pre`(最新を取得すること)。
- 作業 branch:`codex/rfc23-stage2` と `codex/rfc18-dag-width`(1 タスク 1 branch)。
- push してよいのは自分の branch だけです。main・pre・`integ/*` には push しないでください。統合は Claude の側で行います。

## 2. タスク

### タスク 1 — RFC-0023 段階 2:未解決 claim を局所管理する(別の休眠 fence)

RFC-0023 §4 のとおりです。G14 に直結します(未解決の A を、外部の正直な検証者が後からでも反証できること)。

- 共通 DAG は維持し、次の 3 つを分けて管理します。
  - DAG への記録・共通の順序
  - claim ごとの認証・決済状態
  - 未解決 claim の責任・証拠状態
- checkpoint に、未解決の集合を明示的に拘束します(claim、担保の予約、検出 policy、乱数・登録状態の証拠、court の状態)。Merkle root だけでは足りません。pruning・IBD のあとも、資料を実際に取得して反証を完成できることを要件にします。
- 依存の単位は、claim とその実際の依存関係です(出力を入力に使う、同じ execution root、同じ担保の予約)。モデル ID が同じことは依存とみなしません。
- 不可逆な checkpoint は、取り消されない credit だけで支えます。
- 次をまとめて 1 つの fence で変えます。
  - checkpoint
  - fork choice の frontier の比較(`palw_fork_choice*`)
  - 取引の `settled` の判定(`consensus/core/src/palw_settlement_v1.rs:118–125`)
  - native の `lifecycle_closed`
  - pruning・IBD・証拠の保持
- 段階 1(`palw_unresolved_bound_v1`)はすでに `integ/g14-court-capacity-r2` にあります。その上に積むので、この branch を merge してから始めてください。
- **受入条件(RFC-0023 §6)**
  - 未解決の A に依存しない B が、Final・認証・報酬保留の解除・支払い・`settled` まで A を待たずに進む。
  - A が後で有罪になったとき、影響を受けるのは A と A に依存した claim だけである。
  - pruning・IBD・restart・reorg のあとも、外部の検証者 1 人が A の反証を完成できる。
  - 全ノードが同じ checkpoint・未解決の集合・決済状態に収束する。
  - fence の前は byte 単位で同じ動作をする。
- 実装の詳細は RFC-0023 の §7 に追記してください。

### タスク 2 — RFC18 (i):DAG 幅の schedule を block ごとに読む

- `palw_rfc0018_v1.rs:463–519` の DAG 幅の schedule は、型・ID・構造の検査はありますが、どこからも読まれていません。
- 次の値を、schedule の有効化の高さから block ごとに切り替えて読むようにします。
  - GHOSTDAG の k
  - parent の上限
  - mergeset の上限
  - merge depth
- これらは現在、manager・header の検証・pruning proof の作成と検証・coinbase・tx validator の各所で固定値として使われています。Kaspa 本体の crescendo の `ForkedParam` 化と同じ種類の改修です。
- **値は変えないでください**(schedule にある値をそのまま読む)。
- 試験:切り替えの高さをまたぐ header の検証、template、pruning proof、IBD、reorg。fence の前は byte 単位で同じ動作であること。

## 3. 触らない範囲(Claude の lane が作業中)

| lane / branch | 範囲 |
|---|---|
| `integ/g14-court-capacity-r2` | native v39・text29 の G14、GDN held filer、RFC18 (ii)〜(iv)、RFC-0023 段階 1、court 容量、検出 policy |
| `integ/validation-r2` | RFC07・RFC08・RFC09・RFC10・RFC12、beacon |
| `integ/g14-tir-pricing-r2` | TIR frame、T-N1、RFC02 identity、RFC13・RFC20、genesis モデルの変換と HF |
| `integ/g14-drill` | drill driver、受入試験 A〜G、Valid seat への自動告発、filer の再提出、最終 release |

タスク 1 は段階 1 の上に積むので、`integ/g14-court-capacity-r2` を merge してかまいません。ただし、その branch のファイルを編集する場合は、どこを変えたかを commit message に明記してください。

## 4. 規則

- 新しい規則はすべて休眠 fence の下に置きます(全 preset で `None`。validation は測定記録なしの武装を拒否する)。testnet-12 の identity と指紋の試験は変えないでください(`palw_the_release_did_not_move`、`palw_activation_pool_is_t12_only`、`testnet12_identity_did_not_move`)。
- 新しい object の種類や header の形式は、自分の fence までは「decode できないものと同じ扱い(判定せず通す)」にします。どこかの段で bytes を拒否すると、旧 release と混在した瞬間に fence の前で分裂します。
- ビルド:`CARGO_INCREMENTAL=0`、`-j 3`、対象を絞った試験だけを流します(workspace 全体や clippy は回さない)。ディスクは Claude の lane と共有しているので、`df -h` で空きが 25 GB を切ったらビルドを止めてください。
- 設計を変えた場合は ADR に記録してください(次の番号は `docs/maintenance/number-registry.json` と `docs/adr/` で確認。0183〜0187 は使用済み)。docs を push する前に `python3 docs/maintenance/check_docs.py`(と `generate_indexes.py`)を実行してください。
- git stash は使わないでください(全 worktree で共有されるため)。

## 5. 報告

タスクごとに、実装したもの、試験名と pass の件数、測定値、残りとその理由を、RFC-0023 §7(タスク 1)または RFC18 audit の追記(タスク 2)に書いてください。試験で示していないことを「完了」と書かないでください。
