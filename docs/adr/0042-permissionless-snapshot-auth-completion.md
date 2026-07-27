# ADR-0042 — Permissionless snapshot authentication: completion contract

- **Status:** Accepted / **改訂 A1(2026-07-28)で fence 解除** — 本 ADR は **testnet で配線を有効化する**。
  検証は外部レビュー/外部 soak を待たず testnet 実運用に一本化。mainnet preset は不変。詳細は末尾の
  「改訂 A1」。**未了: R1**(可用性のみ、consensus 非破壊)
- **Date:** 2026-07-26(改訂 A1: 2026-07-28)
- **Supersedes / amends:** `docs/adr-permissionless-snapshot-authentication.md`(StopShip)を amend し、
  完成の定義(DoD)と設計上の確定事項を固定する
- **Consumes:** ADR-0041(mainnet は pruned 運用 → permissionless late-join が hard dependency)

## Context

pruned mainnet(ADR-0041)では pruning point より後方から参加するノードに **trustless**
な snapshot import が必要になる。現状は operator-pin を信頼根とする v4 import しか許されず、
chain-derived(permissionless)経路は二層(provenance admission + `Config::palw_permissionless_snapshot_auth`
lever、既定 false)で fenced。crypto core と fenced importer call-site は着地済み
(`consensus/core/src/palw_pruned_frontier.rs`、nodes 2b8139c/82d2330/5bf0a8a/1afedd6)。

**本 ADR 執筆時の再検査で確定した事実(元 ADR の記述を更新する):**
元 ADR は「install-before-verify(durable write が `c==v` 検証に先行する)gap 1」を
未解決の StopShip として記述していたが、**現行コードでは既に解消済み**である。
`prepare_pruning_point_palw_snapshot_import`(`consensus/src/pipeline/virtual_processor/processor.rs:7801`)は
read-only で、chain-derived 経路では `verify_chain_derived_pruning_boundary_from_payload`(:7889)と
全 collision preflight を行って `Prepared…` を返すだけ。実際の `stage_prepared…` + `self.db.write(batch)`
は別関数 `import_pruning_point_palw_snapshot`(:8225-8251)にあり、**検証は書き込みに先行する**。
したがって残る作業は安全性バグの修正ではなく、**信頼根の transport とその検証の統合**である。

## Decision

permissionless snapshot auth の **完成の定義(DoD)を以下に固定**し、4 点すべてを満たすまで
lever は既定 false のまま出荷しない。

1. **信頼根は新しい PoW 暗号ではなく既存の pruning-proof accumulated-work 検証を再利用する。**
   transported Header-v4 bundle(PP selected-parent state を `overlay_commitment_root` で commit する
   子ヘッダ + below-PP support-row ヘッダ commitment)は、**既存の trusted-data / headers-proof パッケージ
   の内側**(または暗号的にそれに束縛された bounded addendum)で運ぶ。独立した PoW validator は作らない。
   → 元 ADR の 1d 選択肢のうち「同一パッケージ内 transport」を確定採用する。
2. **1c 導出等価性を full-lifecycle TestConsensus fixture で証明する。**
   payload 由来の `palw_pruning_payload_paid_work_nullifiers` / `palw_pruning_payload_da_state_root` が、
   live commitment が使う store 由来の `palw_paid_work_window(pp)` / `palw_da_parent_state(pp).state_root()`
   と**一致**することを、**実 batch lifecycle(manifest→leaf-chunk→cert→mint carrier)で coherent に
   構築した状態**の上で assert する。mint 専用の hand-seed 環境(`palw_algo4_env`)は overlay view と
   manifest が pruning 用に coherent でないため不可 — この fixture は ADR-0044 の long-chain harness を
   共有する(keystone)。手で builder を再現する fixture は builder のバグを検出できないため**禁止**。
3. **信頼上のレビュー点は 3 つに限定される** — (a) transported ヘッダが proof-validated 集合の部分集合、
   (b) descendant が正しい first-post-pruning-point child、(c) support-row ヘッダが anti-spam closure に
   一致 — これを独立レビュアーが承認する。
4. **新 v4 re-genesis identity で多ノード pruning/catch-up/reorg soak** を通す。

## Consequences

- lever は既定 false。全 6 preset は archival/closed-network policy を保つ。DoD の 1〜4 を満たすまで
  public late-join は full-history same-genesis 参加に限られる(ADR-0041 の pruned-mainnet 制約と一致)。
- 2(fixture)は ADR-0044 の harness に依存する = **ADR-0044 が先行 keystone**。
- 1(transport)と 4(soak)は multi-node かつ外部レビューを要し、in-session では閉じない。
- 安全性の観点では現行コードは fail-closed(検証が書き込みに先行、誤導出は valid boundary を
  拒否するだけ)であり、lever off の限り regression リスクは無い。

## Definition of done

- [~] transport 統合 + IBD flow が lever on 時に bundle を構築し importer へ渡す
  — **transport と認証は着地、最終 fence は意図的に残置**。実装済: P2P(proto tag 75-78 + v8 専用 flow
  `protocol/flows/src/v8/request_palw_chain_derived_bundle.rs`)、IBD requester(`ibd/flow.rs:1002-1053`)、
  `ConsensusApi` の bundle 引数(`consensus/core/src/api/mod.rs:586`,`:1067`)、importer 分岐
  (`virtual_processor/processor.rs:7925-8045`)、lever の ConfigBuilder(`config/mod.rs:292`)+ CLI flag
  (`kaspad/src/args.rs:428`、拒否4種)。既定 false と「どの preset も有効化しない」は維持(`config/mod.rs:202`、test `:341`)。
  — **未達**: `chain_derived_import_is_wired`(`ibd/flow.rs:1240-1262`)が bundle 認証成功時に必ずエラーを返すため、
  lever on の IBD は現状「必ず失敗」= fail-closed。この fence の解除は下記 SEC-1 の解消と独立レビュー完了が前提。
  — **設計上の重要な訂正(実測)**: 本 ADR の Decision 1「descendant を proof 検証済み集合から選ぶ」は
  **実装不可能**。pruning proof は `future(root) ∩ past(pruning_point)` のみを収集する(`pruning_proof/build.rs:180-241`)
  ため PP より上のヘッダを1つも含まない。support-row も span=32,768(`palw_antispam.rs:55` の window_daa=26,440)に対し
  proof level-0 は `2*pruning_proof_m`=2,000 上限で、レビュー点(a)を字義どおりには充足できない。
  実装は「descendant を**ローカル状態から選び**、転送されたヘッダが**ローカルのものとバイト一致**することを要求する」
  方式に変更した(`ibd/flow.rs:235-256`,`:1129-1167`)。
- [x] 1c full-lifecycle fixture(ADR-0044 harness 共有)green
  — `pruning_processor/palw_lifecycle_e2e.rs:1230-1298` が **3導出すべて**(paid-work nullifiers / DA state root /
  search-availability state root)を実 pruning point 上で live store と等値 assert。本 ADR 本文は 2 導出しか
  約束していないため、**実装が DoD を上回っている**。
- [ ] 3 レビュー点(a/b/c)の独立レビュー完了 — **外部(監査)。本セッションの敵対的検証は代替にならない。**
  参考: セッション内検証では (a)(b)(c)・install-before-verify 順序・lever-off バイト同一性を
  CONFIRMED-SAFE と判定し、設計時に発見した Borsh ハッシュ偽造 hole は
  `palw_bind_transported_header_identity`(`palw_pruned_frontier.rs:527`)で封じた。
- [ ] 新 v4 re-genesis での多ノード soak 完了 — **外部。実機・実時間が必要。**
- [x] install-before-verify 順序の確認(現行コードで既に fail-closed)
  — 再確認済: peer 由来の検査は全て `prepare_pruning_point_palw_snapshot_import`(read-only token を返す)の内側。
  両書込点は staging を次文で行い以後 fail-stop(`processor.rs:8382-8401`、`consensus/mod.rs:1170-1179`→`:1227`)。

### SEC-1(2026-07-27 敵対的検証で発見、要解消)— paid_work 行の帰属が commitment に入っていない

`reconstruct_selected_parent_state_from_pruning_payload`(`palw_pruned_frontier.rs:352`)は paid-work
**nullifier の重複除去和のみ**を fold する。各 `PalwPrunedPaidWorkBlockV1` の `block_hash` /
`block_daa_score` はいかなる commitment にも入らず、`prepare_pruning_point_palw_snapshot_import` も
`payload.paid_work` に対する store 照合を行わない(prepare 内の `headers_store` 参照は PP ヘッダのみ、
`processor.rs:7976`)。`validate_paid_work` は窓 `[pp_daa−window, pp_daa]` と block_hash の相異のみを課す。

**攻撃(仕事量ゼロ)**: nullifier 和をバイト単位で同一に保ったまま、行の日付を窓内で改竄する。PP における
fold は完全一致するが、`palw_paid_work_window`(`processor.rs:5423-5427`)は `anchor_daa` が前進しながら
`anchor_daa − row.block_daa_score <= walk_bound` で絞るため、数エポック後に被害ノードの paid-work 窓が
ネットワークと乖離し、異なる `selected_parent_palw_state_root` を導出して**正直なブロックを
`BadOverlayCommitment` で拒否 = 恒久 desync**。operator-pin provenance は digest がこのバイト列を覆うため
**影響を受けない**(chain-derived 固有)。

`verify_chain_derived_pruning_boundary` の doc が言う「転送 payload の**いかなる**改竄も比較で落ちる」は
**偽**。正しい表現は「selected-parent `state_root` が覆う**フィールドの**改竄は落ちる」。

#### SEC-1 の解消(2026-07-27)— CONFIRMED-CLOSED、ただし残余3件

**採用: Option A**(`prepare_pruning_point_palw_snapshot_import` 内での store 照合)。
**Option B(帰属を fold に入れる)は consensus 破壊として実証・棄却**: fold の live 対応物
(`processor.rs:5247-5265`)は `palw_paid_work_window` の `HashSet<Hash64>` を使い、**型からして帰属を持たない**。
`block_hash`/`block_daa_score` を state に入れれば `state_root()` → `overlay_commitment_root` が変わり、
**全 Header-v4 ブロックが body ルール(`utxo_validation.rs:1420-1443`)で落ちる**。加えて payload は空行を保持するが
`palw_paid_work_store` は空行を省くため、行単位 fold は pruned joiner と full node で一致すらしない。

実装した束縛は**2つとも必須・fail-closed・chain-derived 限定**:
1. `row.block_hash` のローカルヘッダが存在し `daa_score == row.block_daa_score`(`palw_pruned_frontier.rs:533-535`)
2. ローカル `palw_paid_work_store` の記録が(ソート後)`row.job_nullifiers` と一致(`:536-542`)
ヘッダ単独では不十分(別の実在ブロックへ nullifier を付け替えれば union は不変)なため、2 が必要。
ローカルデータ不在は **refusal であり skip ではない**(`:530-532`)。
呼び出しは `verify_chain_derived_pruning_boundary_from_payload` の**後**、staging の**前**(`processor.rs:8106`)。
`stage_prepared_...` は HEAD とバイト同一(関数本体 diff で確認)。

**consensus 非破壊を確認**: `overlay_commitment_root` に入る全関数を HEAD と diff し、
`palw_paid_work_window` / `versioned_overlay_commitment_root` / `compute_overlay_snapshot` /
`validate_palw_pruning_snapshot` / `palw_da_parent_state` / `palw_search_parent_state` /
`stage_prepared_...` は **IDENTICAL**。

**残余(いずれも lever 既定 off + fence により現時点で到達不能):**
- **R1(consensus 非破壊だが残存)**: `job_nullifiers == []` の行は、ヘッダを持つ窓内の任意ブロックに対して
  受理される(行集合を selected chain に束縛するものが無い)。空行は `palw_paid_work_window` に何も寄与しないため
  **desync は起きない**が、被害ノードの永続 snapshot が非正準化し、それを**そのまま再配信**する
  (`processor.rs:7879`)ため `payload_digest` が変わり、operator-pin を使う下流ピアが同期を拒否する
  =「IBD source として毒化する」原語。
  **重要**: 現行の出荷構成では全 preset が `palw_algo4_accept = false` のため `palw_paid_work_store` は
  一度も書かれず、**あらゆる正直な snapshot の全行が空**。したがって本チェックは**現状ほぼ何も制約していない**。
  algo-4 支払いが実際に発生して初めて load-bearing になる。e2e fixture の union も空のため、
  実証は pure unit test 側にのみ存在する。
- **R2(DoS、Finding 3 と同型)— 解消済(2026-07-27)**: `bind_chain_derived_paid_work_attribution` の冒頭で
  `payload.paid_work.len() > MAX_PALW_PRUNING_PAID_BLOCKS` を**どの store lookup よりも先に**拒否するよう変更。
  容量確保(`Vec::with_capacity`)も意図的にこのチェックの後段へ置いた(ピア指定長からの確保自体が
  攻撃者主導の最初の仕事量であるため)。
- **R3(誤診を招く拒否メッセージ)— 解消済(2026-07-27)**: store 不在と「空」の同一視をやめた。
  `palw_paid_work_store` の行は pruning でヘッダより先に消えうるため、
  **不在 + 非空の主張**は「ローカルにデータが無く判定不能」として、その旨を明示した専用メッセージで拒否する
  (fail-closed は維持、ただし**ピアの不正ではなくローカルの欠落**であると述べ、operator-pin 経路を案内する)。
  不在が空と等価なのは行自身が空のときのみ(store は空行を省くため)。

~~**この fence は R1-R3 の扱いを決め、独立レビュー(外部)と多ノード v4 soak(外部)が完了するまで解除してはならない。**~~
→ **改訂 A1(2026-07-28)により置換。下記を参照。**

---

## 改訂 A1(2026-07-28)— fence を解除し、検証を testnet に一本化する

**決定**: 解除条件から「独立レビュー(外部)」と「多ノード v4 soak(外部)」を**前提から外す**。本 ADR は
以後 **配線を有効化する**。検証は外部工程を待たず **testnet 上の実運用に一本化**する。

**理由**: 外部レビューと外部 soak は本リポジトリ内で完了させられない工程であり、待つ限り lever は
永久に fenced のまま、実装が正しいかを知る手段も得られない。testnet は本番価値を持たないネット
ワークであり、そこで実際に動かすことが最も速い検証経路である。

**受容するリスク(明示)**: 残余のうち **R2・R3 は 2026-07-27 に解消済**。残るのは **R1 のみ**で、その
実害は上記のとおり **consensus 分岐ではない** — 空の `job_nullifiers` 行が chain に束縛されないため
被害ノードの永続 snapshot が非正準化し、**そのノードが IBD 配布元として使えなくなる**(operator-pin
を使う下流が同期を拒否する)。`palw_paid_work_window` には何も寄与しないので desync は起きない。
**可用性の劣化であり consensus の安全性ではない**。testnet 上でこれを受容する。

**R1 が不活性でなくなる点への注意**: R1 が「到達不能」とされた根拠は全 preset の
`palw_algo4_accept = false` であった。testnet-200 は `--palw-enable-algo4` を付けて運用しており、
**この前提はすでに成立していない**。R1 は理論上の残余ではなく、algo-4 支払いが発生した時点で実際に
到達可能な欠陥として扱うこと。R1 の解消(行集合を selected chain に束縛する)は本 ADR の未了作業。

**mainnet への非適用**: 本改訂は **testnet に限る**。mainnet preset の
`palw_requires_peer_allowlist` と lever 既定値は変更しない。mainnet で同じ解除を行う場合は R1 の
解消を前提とし、別途 ADR で判断すること。
