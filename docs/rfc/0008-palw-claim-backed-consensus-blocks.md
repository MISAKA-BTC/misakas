# RFC-0008: Claim-backed PALW consensus blocks — LLM work as the chain, not a side lane

Status: Draft (design only; no activation height, consensus fingerprint, or testnet rule change)

Date: 2026-10-03

## 0. 要旨

目標は「LLM 実行の件数が多い Explorer」ではなく、**検証可能で重複しない LLM 計算を含むブロックが、通常時の selected chain と BLUE の主な担い手になること**である。1 つの bounded job/session を claim で開き、その計算を PALW-TIR の決定的な境界で複数の work slice に分け、各 slice の microclaim を持つブロックを合意候補にする。heartbeat と PALW-BASE-0 は停止時の復旧用とし、通常時には有効な LLM ブロックの着色・報酬・時計を奪わない。ただし liveness のための heartbeat を無条件に無効化しない。

これは現在の algo-10 model-execution round block を BLUE に変更する提案ではない。現行の round block は既に Final になった claim の credit を使った fee-only の実行レーンであり、常に RED、非 selected-parent、DAA 非進行である（[ADR-0125](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)）。新たに main に入った [ADR-0168](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) は将来の fence で canonical な E-BLUE を提案するが、round はなお selected parent にならず、blue score/DAA を進めず、weight も元の claim の予算内で分配する。したがって E-BLUE 化だけでは新しい LLM 計算 block によるチェーンは生まれない。本 RFC は**別の、chain-eligible な work-slice block 種別**を提案する。既存の execution lane は高速な取引実行の役割を保つ。

提案は安全性と供給能力の未解決条件を含む。RFC-0008 を merge しても規則は有効にならない。実装・fork choice・時計・経済・裁定の仕様と実測に基づく、別々の fence が必要である。

## 1. 現状と問題の切り分け

現行 T12 の通常 block は bonded PALW attempt、BASE-0、heartbeat を含む。attempt の合意参加と状態更新は別経路であり、モデルを登録し panel を ready にしても、その model attempt が BLUE になるとは限らない。`ghostdag_k=1` の着色と floor との競合、推論時間、producer の hold が BLUE 率を決める。さらに、[ADR-0142](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) の時計では、実モデル attempt と heartbeat は同じ clock advancement の代替にはなっていない。

運用側が報告した P2 観測（本 RFC の checkout からは再実測していない）では、DAA 300 の窓で 8k attempt 72 本のうち BLUE 1、RED 71 で、RED の競合相手は全て floor だった。8k の到着は 0.19 本/120 秒 slot、間隔 p95 は 17.3 slot だった。floor を排除しても、その供給量なら空 slot の大半を heartbeat が埋める。よって問題は少なくとも二つある。

1. **適格な LLM work の BLUE 化**: floor が通常時に DAG の着色を妨げないこと。報酬 fold で floor を無視するだけでは不十分で、無効なら GHOSTDAG 前に拒否しなければならない。
2. **LLM work の供給密度**: 実モデルの work block が slot を十分埋めること。floor を止めても、producer/panel/検証の能力が 0.19 本/slot のままなら「LLM 主体」にはならない。

「recent blocks に E block が多い」は解決の証拠ではない。現行の E block は selected chain、consensus BLUE、DAA に参加しない。ADR-0168 の E-BLUE が将来有効になっても、その BLUE は consensus blue score と selected-parent 資格を意味しない。以下で **share** と言うと、別記しない限り合意対象 block の selected-chain/consensus BLUE share を指す。

## 2. 目的と非目的

### 目的

- 通常時は、各 chain-eligible block に新規の、重複しない PALW 計算 slice を対応させ、実モデル block が selected chain と BLUE の過半数、供給が足りる運用では 90% 以上を占められる設計にする。
- 1 bounded claim/session から複数の有用な LLM 計算 block を生成する。ただし「1 回の推論を N 回の仕事として支払う」ことはしない。
- 模型停止、panel 停止、ネットワーク分断の際は、heartbeat と必要な場合の floor でチェーンの進行を継続する。
- 既存の 120 秒 slot と `ghostdag_k=1` を初期案では維持し、変更が必要なら別の安全性評価を要求する。
- BLUE、Finalized/licensed work、報酬、DAA を別々に計測し、見かけ上の block 数で成功としない。

### 非目的

- 任意の一般的な LLM API 呼び出しを、既存の再現性・裁定要件を満たさず合意 work として扱うこと。
- heartbeat を永久停止すること、または未検証の header class だけで floor/heartbeat を抑止すること。
- Final claim の permit や既に得た credit を複数の BLUE block に複製すること。
- [ADR-0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) の「推論そのものを ticket にするか」という未決問題を本 RFC で決着させること。初期案は既存の work-ticket lottery を前提とする。

## 3. 提案する合意単位

### 3.1 Root claim は「全計算の報酬」ではなく、bounded session の約束

producer は、承認済み class、canonical job 入力、最大 decode 長/計算量、PALW-TIR graph と重み、slice 境界計画、初期状態 commitment、data-availability commitment、bond、対象 clock window を含む root claim を出す。root は全推論の実行済み証明として扱わず、全量の PWU/CCU・報酬・fork-choice weight を一括で与えない。境界は IR の position/layer/checkpoint に対応し、producer が任意に極小化したり、成功した出力を見てから引き直したりできない。

root に結び付いた各 work-slice block は、少なくとも `(root_id, slice_index, predecessor_boundary_root, result_boundary_root, canonical_range, class_id, job_id, DA_root, proof_or_challenge_commitment, producer_authorization, bond_reference)` を確定する。次 slice は直前の canonical boundary からのみ進む。同じ `(root_id, slice_index)` の work credit は一つの履歴で一回だけ使える。分岐上の再使用と private-fork grinding については §7 の gate を満たすこと。

slice は実際の tensor 計算の**非重複の区間**である。費用は [RFC-0002](0002-palw-tensor-ir.md) の canonical IR cost に基づく。`total_credited_CCU <= canonical_job_CCU`、各区間の credited range は互いに非重複、root と slice と既存 round block の間で同じ CCU に二重の PWU/発行/票を与えない。標準 slice は固定 work target `W` 相当を目指し、端数の許容範囲と区間数上限は class ごとのコスト上限・DoS 予算から決める。正確な `W` と slice 数は実測前に固定しない。[ADR-0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) の「1 block が買う work unit」の経済を維持する。

「1 claim から複数 block」とは、1 session の中で**それぞれ異なる区間を計算・開示・検証する**という意味である。同じ全 trace を複数 header に分割表示するだけでは、有用な新規計算も物理的な実行時刻も証明できない。決定的推論が本当にその秒に計算されたことは、trace commitment 単独からは証明できない。この限界を spec と UI に明記する。

### 3.2 新しい work-slice lane

新 block 種別は既存 algo-10 execution round block とは別の識別子と domain separation を持ち、選択親、BLUE、DAA の候補になる。既存の attempt / round の wire format や解釈を黙って変えない。parent/job/ticket の binding、per-slice 対象 class と署名の検証、header/body commitment、最大サイズ、重複禁止は versioned consensus rules とする。

chain-eligible であっても header の外見だけで Final work と同じ fork-choice weight を与えない。[ADR-0069](../adr/0069-e2e-adjudicability-is-the-price-of-weight.md) に従い、DA、検証、裁定可能性、Final の各段階に応じて安全な frontier/weight を定義する。既存の `safe_frontier` と header blue-work hint の二重意味を混同しない。検証前に連鎖できる子の数、未決算 work の合計、裁定期限、reorg で巻き戻る範囲に上限を置く。

slice の正しさは optimistic に licence → challenge → Final と進めるか、早い verification certificate を必須にするかを実装仕様で選ぶ。いずれの場合も、依存する後続 slice は先行 slice の不正で同時に無効化され、Final とした state/発行を後から巻き戻す状況を許さない。実装上、検証能力が追いつかない状態で BLUE/DAA のみを大量に増やすことを禁止する。

### 3.3 裁定・検証の処理能力

複数 slice は panel、licence、court、DA の負荷を増やす。[RFC-0006](0006-palw-layer-sharded-panels.md) の layer/position sharding と [RFC-0007](0007-palw-verification-certificates-and-algebraic-checks.md) の batched receipts/代数的チェックは候補となるが、両 RFC は Draft である。採用しない場合も同等の throughput、安全性、censorship 耐性を実測で示す。単に producer を増やして verification backlog を後ろに押す案は認めない。

### 3.4 Kernelと確率的検査の境界（2026-10-06）

新しいslice profileの通常検証は[RFC07 Part V](0007-palw-verification-certificates-and-algebraic-checks.md)・
[RFC11 §15](0011-permissionless-model-and-long-context-onboarding.md)の小さいencoded/algebraic constraint検査とする。
slice全体の再実行を通常経路に要求せず、異常時だけ有界exact courtへ局所化する。
[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)のactive Kernel・plan・suiteを
root/slice/receiptにbindし、未対応演算はKernel更新まで拒否する。VM代替経路は設けない。
各sliceの初期・終端stateとpredecessorを被覆し、session全体の誤受理確率を合成する。同じ証拠やchallengeを
複数blockに載せても独立な検査回数とは数えない。正のreceipt、DA、challenge window、§7の未確定weight上限を
維持し、軽量化を根拠に無検証のBLUE/rewardを増やさない。

## 4. BLUE・floor・heartbeat の役割

### 4.1 通常時の floor 排除は GHOSTDAG より前

通常時に floor block を報酬 fold だけで skip すると、block は DAG に残って実モデルを RED にし得る。floor を reserve とする規則を有効化するなら、header admission の**GHOSTDAG 着色より前**に、当該 branch の過去に基づく決定的な有効性判定を置く。必要なのは class 文字列だけでなく、bond、資格、ticket、claim/slice の一意性と必要な証拠を含む**認証済みの productive-work 事実**である。無資格な RED header を撒いて floor を長時間止める経路を作らない。

この判定は現在の virtual tip や node-local mempool/producer 観測に依存できない。selected parent と mergeset（BLUE/RED の両方）の扱い、競合する分岐、IBD、pruning proof、並行 header 到着順について同じ結果になる必要がある。現行実装の full stateful attempt admission は header 着色後にあるため、判定を前に移すには必要な state witness/commitment または二段階の DAG 受け入れ仕様が要る。これを解かずに「header class を見て floor を拒否」は採用しない。

floor の Idle/Probe/Normal や timeout の定数は別途仕様化する。RED の productive attempt を観測するだけで通常状態を無期限に延長せず、BLUE または検証済み work の進行でのみ延長する。Probe、cooldown、復旧時間は adversarial な RED spam と遅い正当な producer の双方で検証する。この RFC はその実装済みを主張しない。

### 4.2 心拍は空 slot の安全網

[ADR-0140](../adr/0140-the-heartbeat-is-the-emergency-generator.md) の heartbeat は誰も仕事を出さない場合の permissionless な liveness path として残す。モデルの意図・wall-clock の「無活動」は合意可能な証拠ではないので、合意が単純に「モデルが活動中なら heartbeat は無効」と判断してはならない。通常時の producer policy、適切な fee/reward、相競合する model block の安全な選択によって heartbeat が空 slot だけを埋めるようにする。heartbeat が有効であることと、BLUE の大半を占めることを同一視しない。

### 4.3 model-backed clock

時計を model work で運ぶなら、**認証済みで安全な work-slice block が 120 秒 slot を一回進める**ことを新たに定義する。heartbeat は運用上は空 slot の fallback とするが、後からモデルが到着したという未来の事実で heartbeat を遡及的に無効にしてはならない。一つの slot を model と heartbeat が二重に進めたり、遅れて到着した slice で過去の空 slot を一気に埋めたりしない。timestamp、future bound、selected-parent/mergeset 間の tie-break、reorg、difficulty/DAA window を数式と例で規定する。[ADR-0142](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) の clock semantics を変更するため、単なる producer 設定ではなく独立した合意 fence と test vector が必要である。

floor の拒否と model-backed clock は分離して段階導入する。前者だけでは heartbeat 比率は下がらず、後者だけでは floor による RED 化は直らない。heartbeat に比べて LLM work を安全な BLUE に選ぶ fork-choice/着色の一般化が必要なら、`ghostdag_k` を不用意に上げるのではなく、攻撃模型と [ADR-0058](../adr/0058-palw-merged-work-is-counted.md) に整合する別仕様として検証する。

## 5. 供給能力は合意定数では作れない

たとえば平均 0.19 本/slot の実モデル work しか生成できなければ、競合が無くなっても約 0.81 の slot はモデルに埋められない（到着の burst/hold があるので実測値はさらに異なる）。「モデル 90%」は floor/heartbeat の重みを変えるだけでは達成できない。複数 producer、複数 session の並列化、短い work slice、温まったモデル、panel の同時検証、backpressure の低減を同時に進める。1 session を複数 slice にしても、**同じ一台が逐次計算するだけでは計算能力は増えない**。

容量計画は平均件数よりも `P(slot に少なくとも 1 有効 slice が届く)`、連続空 slot 長、推論・検証・licence の p50/p95/p99、panel queue、work の Final 化率で行う。比較する分母は raw DAG block、selected chain、BLUE、Finalized productive work でそれぞれ明示する。UX 上の 1 秒更新は別問題であり、120 秒の合意 slot を 1 秒に短縮したことにはしない。

## 6. 変更が必要なコード境界（設計対象）

| 境界 | 現状の責務 | RFC-0008 の変更 |
| --- | --- | --- |
| `consensus/core/src/pow_layer0.rs` | attempt、heartbeat、round の種別と header work | versioned work-slice block・domain・work 上限を追加。algo-10 round は維持 |
| `consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs` と `processor.rs` | 基本的な header/parent/PoW 検査の後に GHOSTDAG 計算 | branch-local な productive/floor 資格と slice の最小証拠を着色前に検証。stateful 証拠の運び方を仕様化 |
| `consensus/src/processes/ghostdag/protocol.rs` | round を RED/非 selected-parent に固定し、通常 block を着色 | 新種別だけを chain-eligible にする。k=1 で競合・同時到着・reorg を test vector で固定 |
| `consensus/src/processes/difficulty.rs`、`consensus/core/src/palw_clock_cursor_v1.rs` | clock cursor / DAA advancement と work window | 二重 tick しない model-backed slot + heartbeat fallback の導入 |
| `consensus/core/src/palw_fork_choice.rs`、`consensus/core/src/palw_work_target_v1.rs` | safe frontier / work target | slice ごとの検証済み非重複 work のみを重み・経済に算入。仮証拠による private-fork 増幅を防止 |
| `consensus/src/pipeline/virtual_processor/processor.rs` | stateful admission / licence / fold | root と slice の one-use 台帳、依存関係、失格・裁定・報酬 conservation を追加 |
| `kaspad/src/palw_producer.rs` | 親に bind した job template と attempt 生産 | session/slice scheduler、複数 model producer、stale/reorg の再計算・破棄、hold の計測 |

上表は変更を提案する境界であり、関数・行番号まで凍結する実装指示ではない。既存の合意規則と versioned hash/fingerprint、P2P/RPC の相互運用性を監査して、別の実装仕様に落とす。

## 7. 安全性を満たすまで未解決の項目

1. **precomputation と fork reuse**: deterministic な LLM 出力を別 parent/header に署名し直すだけで多数の chain block を作れるなら、物理的な新規仕事は増えない。branch/slot binding と再利用禁止をどう両立し、ユーザー向け出力の意味を壊さないかを定義する。既存の ticket lottery と safe frontier を残しても、cheap private-fork・header flooding・long-range sync を simulation で棄却すること。
2. **header 時点の資格**: 遅い stateful admission より先に GHOSTDAG を動かす現在の経路で、虚偽の class/root/slice が floor 抑止や着色に影響しない証拠を作る。pruned node と IBD node も同じ判定をすること。
3. **optimistic Finality**: slice 失格時に依存子、取引、発行、DAA をどこまで無効化するか。未検証の深さ・価値・期間の上限と challenge/court の throughput を確定する。
4. **経済と Sybil**: 一つの job を分割・複製・複数 identity に移しても同じ CCU から追加の報酬、share、重みを得られないこと。単一 session/運営者が slot を囲い込まず、異なるモデルも参加できること。
5. **liveness**: producer と panel が同時に止まっても heartbeat（必要なら floor）が clock を進めること。悪意ある RED spam だけで reserve を止められないこと。

これらは「実装時に何とかする」項目ではなく、合意 fence を armed にする**前提条件**である。一つでも未証明なら pilot を shadow mode に留める。

## 8. 段階導入と合格基準

1. **観測**: 実際の block 色、RED の競合原因、モデル別 accepted/BLUE/Final 件数、空 slot、遅延、queue、外部 floor 比率を chain data から再現可能にする。P2 の数値は再測定する。
2. **shadow replay**: 本番の規則を変えず、過去の DAG と遅延注入した producer で floor gate、slice、clock、fork choice を再生する。モデル停止・再開・reorg・pruning・IBD の結果を全 node で一致させる。
3. **isolated pilot**: 新しい work-slice fence のみを testnet/drill で有効にし、最初は小さく固定した slice 数と outstanding depth に制限する。旧 node との切替、invalid block、duplicate slice、裁定、panel 不在を試す。
4. **reserve と clock の独立した pilot**: floor の header-stage admission と model-backed clock をそれぞれ別の fence/test vector/fingerprint で導入し、組み合わせも試す。活性化高さは readiness と replay に基づき別途決める。
5. **拡大**: 供給/検証能力が確認されてから複数 producer、複数 model、slice 数を増やす。target は「通常時の selected-chain model BLUE share 90% 以上」だが、平均でなく長い窓・低供給時間帯も公表する。異常時の停止後復旧時間、誤 BLUE/重複 reward ゼロ、Final backlog の有界性も同時に合格条件とする。

必要な property test には、同じ root/slice の再利用、境界の飛越し・重複、別 branch での replay、外部 floor miner の旧ソフト、RED-only 攻撃、panel が停止したままの producer 連射、遅い正当な producer、clock の二重 tick/逆行、failure 後の selected-chain/Final 整合性を含める。

## 9. 判断

本 RFC は **「claim を中心にした bounded session → 複数の検証可能な microclaim/work-slice block → model-backed selected chain」**を次の設計方向として提案する。同時に、見かけだけの algo-10 BLUE 化、full claim credit の N 倍配布、heartbeat の停止、header class の自己申告による floor 拒否は採用しない。§7 の安全性と §8 の供給能力を実証するまでは Draft のままにする。
