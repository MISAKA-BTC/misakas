# RFC-0051 TKR1 production wiring checklist

2026-11-11。採択済みの [RFC-0051](../../../rfc/0051-funded-preclaim-tokenizer-registration.md) と [ADR-0219](../../../adr/0219-funded-preclaim-tokenizer-registration.md) の実装チェックリスト。

| 段階 | 状態 |
| --- | --- |
| TKR1 wire、UTXO fee handoff、予約 book、native fold、journal・snapshot 検査 | core / SDK 実装済み。確定 fold の費用照合を含むローカル回帰を実施 |
| 設定・新規 Open cutoff・header gate・ノード UTXO handoff と受理順 | 隔離した実ノードで署名済み Open/Part/Finish/重複が通過。出荷 preset は未設定 |
| coinbase・テンプレート・Round の焼却反映、work scanner 実行境界 | 接続コードと段階3のローカル回帰30件が通過。公開適格性の測定は未完了 |
| RPC・CLI | unsigned Open quote / accepted status の回帰22件通過。prepare/send CLI と通常 UTXO funding API 実装済み。予約枠・署名検査通過、live CLI broadcast は未検証 |
| mempool / peer の local work 制限 | 接続済み。関連25件と既存 peer 2件が通過。実 TKR1 peer 通信は未検証 |
| 復元と物理資源 | restart / replay / reorg / 認証済み PALW sidecar の復元を検査済み。全 P2P pruned IBD・最大負荷の native/history 測定は未完了 |
| 公開 T12 の有効化、同一 Studio TFP1 の G14 quote・対話・Final・ブロック生成 | binding の受理証跡あり。最終 quote 観測は拒否。公開 profile の測定・採択・有効化と quote 以降は未達 |

- [ ] **設定と入口**。候補ブロックの DAA で TKR1 専用の activation と policy を解決する。未設定のネットワークは既存の挙動と fingerprint を維持する。設定の形状、fence visitor、ruleset と consensus params ID に、設定時だけ値を組み込む。header-context の入口で fence を確認し、前段で候補ブロック内の TKR1 carrier 全件と期限処理の作業量を集計する。未知版・重複・拒否される carrier も作業量に含める。[設定検証](../../../../consensus/core/src/config/params/validation.rs#L271) · [設定 fingerprint](../../../../consensus/core/src/config/params/identity.rs#L1985) · [header-context 検査](../../../../consensus/src/processes/transaction_validator/tx_validation_in_header_context.rs#L73) · [TKR1 scanner](../../../../consensus/core/src/palw_tokenizer_registration_carriage_v1.rs#L244)
- [ ] **UTXO handoff と受理順**。実 UTXO と先行控除から handoff を準備し、認証済み親 root、受理点、policy、txid、受理順に束縛する。Batch と同じ共有 lease の形式で読み取り用 handoff を渡し、仮実行で同じ fee budget を消費しないようにする。既存 lifecycle・Batch・TKR1 と raw assignment を候補仮実行と確定 fold で同じ順に処理する。[UTXO handoff の既存入口](../../../../consensus/src/pipeline/virtual_processor/utxo_validation.rs#L1410) · [Batch の共有 lease](../../../../consensus/core/src/palw_court_shared_material_intake_v1/storage_rent.rs#L147) · [受理順の組み立て](../../../../consensus/src/pipeline/virtual_processor/processor/palw_accept.rs#L3615) · [確定 fold](../../../../consensus/core/src/palw_state_v2.rs#L15713)
- [ ] **新規 Open の一回焼却**。既存の候補 fee preview → UTXO replay → observed rehearsal → 確定 fold の順を使う。preview は受理順付きの一時状態で実行し、新規 Open の挿入に成功した取引だけ、exact row の charge と charge ID を fee plan に記録する。replay では親、受理点、取引順、実 fee と先行控除を照合する。observed と確定 fold で同じ結果を照合し、coinbase の collectible fee から一度だけ差し引く。Open 重複・拒否、Part、Finish の追加焼却額はゼロ。Finish が実パースに入り失敗したときは、拒否理由とともに試行スロット消費を確定 fold と journal に残す。[候補 preview](../../../../consensus/src/pipeline/virtual_processor/processor/palw_accept.rs#L681) · [UTXO replay](../../../../consensus/src/pipeline/virtual_processor/processor.rs#L4295) · [observed rehearsal](../../../../consensus/src/pipeline/virtual_processor/processor.rs#L3024) · [Open fee と拒否処理](../../../../consensus/core/src/palw_tokenizer_registration_fold_v1.rs#L33)
- [ ] **テンプレート・Round・復元**。virtual template の fee preview と coinbase に同じ fee plan を渡す。Round の予算判定では、各候補 body を直前の受理済み prefix から UTXO と native の一時状態で処理する。焼却後の collectible fee で予算判定し、受理時は両方を反映し、拒否時は両方を破棄する。既存の全体 replay は受理順の変化を拒否するため、Round の採否が変わる場合も検証する。期限処理と snapshot 検査の作業上限をそれぞれの入口で適用する。[virtual preview](../../../../consensus/src/pipeline/virtual_processor/processor.rs#L4439) · [template coinbase](../../../../consensus/src/pipeline/virtual_processor/processor/template.rs#L981) · [Round 予算判定](../../../../consensus/src/pipeline/virtual_processor/utxo_validation.rs#L1156) · [Round template 見積もり](../../../../consensus/src/pipeline/virtual_processor/processor/evm.rs#L608) · [snapshot 検査](../../../../consensus/core/src/palw_state_v2/chain_state.rs#L4475)
- [ ] **RPC・CLI と通し検証**。参照系 RPC で認証済み親に基づく Open 見積もりと、受理後に確定した Part・Finish の枠および進捗を区別して返す。送信は既存の transaction RPC を使う。テンプレートから実ブロック受理まで、Open の一回焼却、拒否された Finish の試行消費、Part の標準 mass、再編・復元を検証する。公開 T12 では元の Studio claim ID と同じ ID で G14 quote 以降を照合する。[既存 RPC の例](../../../../rpc/service/src/service.rs#L4194) · [CLI の例](../../../../misaka/palw/gateway/src/bin/rail.rs#L1818)

2026-11-11 更新: transport と新規 Open cutoff を分離した。受付停止後も既存契約の Part / Finish と同一 Open の再送を継続できる。TKR1 専用 network domain は TIR allocation の解決と独立させ、受理順はフィルター前の完全な accepted transaction ordinal と照合する。Finish の意味上の拒否でも、実パースに入った試行の消費を仮実行と確定 fold の両方に残す。

候補 scanner は物理 carrier の重複・不完全入力も含めて四次元の費用を checked 加算する。親に book がある場合、または最初の Open が book を作り得る場合に、最大五回の期限処理を覆う費用を一回計上する。予約 book と quote は、その期限処理分を控除した同じ実効 work 容量を使う。段階3で scanner を候補の UTXO 判定前へ接続した。段階5では snapshot の work envelope、予約表の件数・geometry と session/Part の件数を、map 再構築・row hash・cross-row 検査の前に制限した。外側の bounded import と物理資源の検査は引き続き必要である。

段階3で新規 Open の追加焼却額を UTXO 集計、virtual template と Round 収入判定へ接続した。候補ごとに TKR1 仮状態を一つ保持し、body ごとに journal の開始位置を記録する。Round 拒否時は次の body が読む前に行・派生 index を逆順で復元する。受理時の費用を observed rehearsal と確定 fold に渡して照合する。これはノードでの通し検証完了を意味しない。公開 profile・有効化・同一 Studio claim の quote 成功を、このコードやローカル回帰の結果から主張しない。

段階3の容量境界は、送出元の一つの合法な body が後続候補すべてを容量超過にしないことも要求する。`maximum_source_blocks = M` は network の通常 mergeset 上限履歴の最大値と Round lane 上限の和以上とする。候補全体の四次元 service budget から最大五回の期限処理を覆う work を一回控除し、各次元を M で整数除算した枠を、各 source body と予約 book の共通上限にする。source body は親不明の Finish も同じ quote 算式の最大値で計上する。受信時の body 検証とテンプレート初回選択・再補充で適用し、候補全体は親から取得した quote で再集計する。送出元 DAA が transport activation 前の payload は、後から受理されても TKR1 として扱わない。これは Open cutoff とは別の境界で、既存契約の Part / Finish の受付は cutoff 後も継続する。

Finish の raw assembly と deterministic derivation は、実 parent の借用、parent root、受理点、policy、Open ID、試行 slot と EOG digest に結び付けた候補内 cache で共用する。資格・pins・slot・親 Parts の照合は毎回行う。成功 row と deterministic な入力拒否だけを予約済み RAM に保持し、local resource failure は cache しない。期限処理は TKR1 専用の pre-object 一回に限定し、既存 staged sweep から TKR1 行を除外する。

TKR1 transport が有効になった候補から、通常 PALW object・Batch・TKR1 をフィルター前の受理済みトランザクション順で並べる。候補に TKR1 carrier がない場合にも同じ順序を使い、carrier を持たない派生 object は先頭に置く。activation 境界で従来の通常 object のグループ順が変わる場合を通し検証に含める。

数値 profile の測定には、各 source body の標準 transaction mass、最大 mergeset、複数の仮実行・確定処理で繰り返す bookkeeping、cache を含む native RAM、snapshot、undo/redo を含める。M による容量分割や parser の上限値を、そのまま公開性能認定として使わない。


段階3レビュー修正: fee 仮実行と observed rehearsal は、TKR1 carrier ごとに全 PALW state を複製しない。所有権と native reservation を移して共通 writer を呼ぶ。取り消す body の old row は journal から移し、forward 時の予約を逆適用完了まで保持する。その後に journal 領域を捨て、body 開始時の予約量まで戻す。変更のない重複・拒否 carrier の一時予約も返す。実際に変更した行の保守的な保持上限は維持し、公開 profile の peak RAM 測定に含める。

同じ候補には isolated fee projection、任意の Positive fee-plan rehearsal、任意の isolated replay、observed rehearsal、確定 fold の最大五回がある。source scanner と Open quote はこの共通上限を使い、bookkeeping は最大二回の body 逆適用も計上する。Finish の raw assembly / deterministic parser だけは候補内 cache で一回とし、署名・EOG・保存 Part の再ハッシュは各回の費用に残す。5 / 2 / 1 の係数を policy fingerprint に含め、経路を増やす変更では同時に見直す。ここで公開機材・単価・activation の採択は行っていない。

段階3の機能回帰: registration core 19、Finish cache 1、native ownership 1、既存 Batch 順序 4、Batch 1、Output 1、node 受理順序 1、Round fee 1、SDK 1 の計30件が成功した。core/node は opt-level 0 の機能検査で、Batch は既知のテスト stack 条件として 64 MiB を指定した。実ブロックの template → UTXO → coinbase 受理と reorg/IBD の通し試験は未完了。

段階4の参照 RPC と CLI の形状・番号・状態区分は [読み取り接続記録](rfc-0051-read-rpc-2026-11-11.md) に記載する。公開での成功とは区別する。

段階5の[実ノード試験と関連24件](../../../evidence/palw/funded-tokenizer-registration-phase5-node-regression-2026-11-11.json)が成功した。既存の予約規則で Open/Part/Finish/同一 Open 再送を通常 carrier として処理し、実 UTXO と coinbase に新規 Open の一回焼却を反映した。隔離設定の機能試験であり、再起動・再編・pruned IBD と公開数値 profile の適格性は、この結果に含めない。

追加の[復元回帰25件](../../../evidence/palw/funded-tokenizer-registration-phase5-recovery-regression-2026-11-11.json)では、同じ DB の再起動・別ノードの block replay・Finish 前からの実分岐を確認した。再起動は root/status/UTXO、replay は root/status の一致、再編は Open の charge ID・Part・予約枠を維持した Pending への復帰を検査した。pruned IBD 転送、実 Round の登録費用拒否、公開性能測定は未完了。

[Round の追加試験](../../../evidence/palw/tokenizer-registration-round-sender-regression-2026-11-11.json)では、実署名済み Round 本文の Open を焼却後の collectible fee で判定した。1 sompi 超過時に本文の TKR 行・UTXO・RoundFee income が残らず、ヘッダー permit は既存規則どおり消費され、同じ Open 再試行に全額の burn が発生した。既存のテスト用 V2 budget 上限を使う機能検査であり、公開 arming に必要な V3 profile の検証とは区別する。

[認証済み PALW sidecar の復元試験](../../../evidence/palw/tokenizer-registration-pruned-sidecar-regression-2026-11-11.json)が通過した。Part 受理点の予約 book・session・raw Part を取り込み、同じ Finish 取引の受理と Complete、後続各ブロックの状態 root 一致を確認した。取り込み未完了時には厳格な status RPC が拒否することも検査した。試験は検証済み UTXO を保持し既存の epoch-zero 完了処理を使う構成であり、P2P handshake と UTXO stream 全体の pruned IBD 試験ではない。ノード2件と CLI1件が成功した。

`ee879a1d6` で TKR1 の local work 計上を通常 mempool 受付と peer relay に接続した。認証済み状態や parser を読む前に、有効 policy の最大 source cost を使い、不正・重複・上限超過も含めて peer と process の別枠に計上する。枠は monotonic clock で補充し、policy ID の変更で残枠をリセットしない。上限到達は retryable とし peer への不正判定にしない。burst/refill は既存 H1 のローカル運用値を参照し、未測定の公開容量や合意値にはしていない。[関連25件と既存 peer 2件](../../../evidence/palw/tokenizer-registration-local-admission-regression-2026-11-11.json)が成功した。実 MiningManager で有効化前の従来扱い、上限超過の早期拒否、資金付き Open の受付と重複拒否を検査した。TKR1 の P2P ネットワーク通し試験はこの結果に含めない。

`8d6d68b00` の[受理済み親状態を用意した native workload 試験](../../../evidence/palw/tokenizer-registration-native-workload-functional-2026-11-11.json)で、実 tokenizer の2件×33 Part と、8 MiB raw の2件×128 Part が成功した。両 session が全 raw を保持する状態から snapshot を復元し、第二の Finish で Complete、第一は未試行のまま Superseded となる。各 carrier の fee 仮実行・巻き戻し、確定 fold、journal undo/redo の root 一致も確認した。最大 raw ケースの snapshot は18,188,793 bytes、累積 journal は21,705,513 bytesで、どちらも Borsh の論理サイズである。macOS/opt-level 0 の機能結果であり、物理 RocksDB 容量や最低要件 Linux 機材の性能認定ではない。測定用 policy は2 session・1 sourceで公開 T12 の M=780 profile ではなく、実 tokenizer ケースの class/artifact も明記した fixture を用いる。公開対象 claim の受付とは区別する。

`13b755c4a` の[pruning import session 回帰](../../../evidence/palw/tokenizer-registration-pruning-utxo-session-regression-2026-11-11.json)で、以前の verified UTXO 保持・stable marker 手動完了を本番の取り込み処理に置き換えた。donor の Part 受理点の past UTXO 集合を復元して header MuHash と一致させ、複数チャンクを staged UTXO set へ投入した。部分 stream と早過ぎる完了は拒否され、全量の streamed/staged commitment 検査と session 完了後にだけ厳格な status RPC が応答する。復元した Part から元の Finish 取引が受理され、各 root と Complete が一致した。ノード2件・CLI1件が成功。P2P 転送、trusted-history proof の交換、自然な pruning point 更新は対象外である。
