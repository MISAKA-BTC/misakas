# A4 — G14 Court基盤の初回実装

> 本文は初回実装の履歴。続報は[A4共通契約接続](A4-CONTRACT-INTEGRATION.md)を参照。

- 担当branch: `codex/positive-court-a4`
- 元HEAD: `a5800d2dd`
- 作業開始時の既存prototype依存snapshot: `c4749c861`（A4の新規実装とは別commit）
- 共通契約: A0 `a85152edc`を`5597c7967`として取り込み済み（v2 / revision 1）。旧prototypeから共通DTOへのadapterは未接続。
- 有効化設定: 変更なし。本番・native ingressは未接続。

## 変更した責務・ファイル

`consensus/core/src/palw_positive_court_v2/coordinator/transport.rs`:
既存EXACT checkerへの独立した小片搬送。claim全期間で一回だけ追加予約し、順序・範囲・長さ・payload同一性を確認する。部分受理はcoverageを進めず、完成partを既存`submit_batch`へ渡す。Nodeの算術検査に成功して初めてcursorを進める。

relayの変更に新しい署名domainは設けない。同一descriptorの再送は冪等だが、再送と範囲内の無効packetもwire量・packet数を消費する。buffer破棄・Panel交代で消費量を戻さない。破損してdescriptorと一致しない搬送はcandidateを拒否せず、期限は既存caseのexclusive cutoffを使う。

このdescriptorはローカルの搬送メタデータであり、Panel署名や計算認証ではない。内側の証拠は既存prototypeの認証・checkerへ渡す。native導入時はA0のcarrier/提出者認証と結合する必要があり、unsigned network packetにこのローカルAPIを直接公開しない。`AlreadyChecked`は既存coverageのACKであり、再送payloadの認証結果ではない。

`consensus/core/src/palw_positive_court_v2/coordinator/capacity.rs`:
Positive/Fraud別の同時case数・work・搬送量・live bytesの予約台帳。同じcaseで両laneを予約でき、Positiveが満杯でもFraud専用枠を消費しない。重複予約・重複解放は冪等。異なる予約量による上書き、overflow、いずれかの資源超過は全検査後の変更に到達しない。

これは占有予約であり、消費済みworkの会計・資金hold・支払・保持horizonの判定を実装しない。`release`はA6が保持期間終了を確認した後に使い、caseの累積予算をリセットする用途には使わない。

`coordinator.rs`: 専用submodule登録、搬送状態の保持、candidate交代・finish・期限・revoke時の一時buffer解放。
`coordinator_tests.rs`: 実際のtiny TIR証拠による搬送と役割境界の試験。

## 接続したAPI

- `configure_material_transport` — checkerの全試行分に追加してwire予算を予約。受付の予約上限を超えれば変更せず拒否。
- `submit_material_chunk` — 同一witness/leafの連続fragment、重複、直近checked partのACK。
- `material_transport_meter` / `discard_material_buffer` — 消費量の取得、一時領域だけの破棄。
- `CourtCapacityV2::{new,reserve,usage,release}` — 正常/不正Court共用のローカル占有台帳。

## 他担当に必要な変更・未接続

- A0: 取り込み済みの`misaka-palw-positive-contract` revision 1とのadapterを確定する。本変更は既存private prototypeへの追加であり、新共通DTOを旧IDへ再解釈しない。native tag・signature domain・fence・高さは変更していない。
- A1/A3: 完成partは既存EXACTのcone/optional traceを使う。公開の共通`EvidenceFrame`/checker portへのadapterは未接続。全coverage・終端照合は既存checkerが決める。
- A5/A6: `AtomicCourtStore`の単一transaction内でcapacityの提案state、実funding、case、liability、journalを確定する。capacityだけ先に公開しない。資金やjournalの失敗時は提案state全体を捨てる。成功を返す仮adapterは置いていない。
- A6: 追加予約設定、受理fragment、消費された無効packet/再送、buffer破棄、candidate交代を記録・再生する。accepted partだけの既存journalでは搬送消費量を復元できない。再起動・reorg・IBDは未接続。
- A4/A0: occupancy poolと既存G14のnative admission/interval予約との結合、Fraud裁定からの失効遷移は未接続。現在のprivate checkerによる終端不一致と、旧Fraud入口の意味を変更しない。
- A7: 実プロセス・複数Node・負荷/性能・敵対的network ingressは独立検証が必要。

Miner資料応答とPanel witnessの状態分離、Gate C、会計、journalの既存コードは変更していない。native受付・資金・保存を原子的に結合した完成版とは扱わない。

## 実行した試験と結果

- 対象harness: Positive関連52件＋旧TIR裁定/資料2件、計54 PASS / 0 FAIL（91.76秒）。[raw log](../../../evidence/positive-court-a4-2026-10-31/optimized-tests.log)。
- 同branchで`cargo test -p misaka-palw-positive-contract --offline --locked`: 13 PASS / 0 FAIL。[raw log](../../../evidence/positive-court-a4-2026-10-31/common-contract-tests.log)。
- `rustfmt --check` / 実装source・本引継ぎの`git diff --check`: PASS。文書検査: 新規欠落0、既知43。raw log 3本の末尾空行は採取時のまま保存し、全差分のwhitespace検査ではその3件を報告する。

対象module検証は既存consensus-core rlibを使う最適化harnessで行った。元tiny fixtureの不変artifact openingだけをharness内でmemoizeし、算術・checker・合否条件は変えていない。全文workspaceの新規build、native/G14全回帰の成功を意味しない。raw logと使用source hashは[検証記録](../../../evidence/positive-court-a4-2026-10-31/validation.json)を参照。追加試験は分割正常証拠、再送、gap/conflict/overflow、破損、Nodeによる不正part拒否、wire/packet上限、締切、Panel交代、lane共存・隔離・冪等性・提案stateの破棄を対象とする。

残っている仮実装: native成功を返すstubはない。transport headerの192 bytesはprivateの保守的な予約値であり、native codec割当・性能測定・mainnetパラメーターではない。
