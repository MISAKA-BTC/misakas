# A0 — Positive Court 共通契約 v2 / revision 1

2026-10-31。実装対象はユーザーが指定したR2、履歴番号は [RFC-0044](../../../rfc/0044-positive-court-full-coverage.md) / [ADR-0212](../../../adr/0212-positive-court-without-model-prepossession.md)。既存0042 / 0210は変更しない。

この文書と `misaka/palw/positive-contract/` を担当間の基準とする。A0が共通型・署名domain・識別子・framing・port・旧新切替を管理する。A1〜A7が各自で型や成功条件を再定義しない。ここで定めるのは私設interchange r1であり、native tag、公開profile、activation heightの割当ではない。独立レビューと各実装の統合は別の状態である。

追加接続仕様: [TIR interchange](TIR-INTERCHANGE-R1.md) / [native・funding契約](NATIVE-CONTRACT-R1.md)。既存r1 field順・署名domainは保持し、新しい外側の認証・署名scopeを明示する。

## 1. 基準と変更管理

- workspace: `codex/positive-court-a0`。初期baseは `a5800d2dd5d334e89f7a1ddb6e956aedcbf4adf5`。
- 依存crate: `misaka-palw-positive-contract.workspace = true`。実装クレートへの依存はなく、循環依存を作らない。
- 私設契約のversionは2、revisionは1。既存wire、旧signature domain、旧履歴を再解釈しない。
- field順序・domain・範囲単位・責任の意味を変える場合はA0へ差分を返す。破壊的変更はrevision・magic・署名domain・vectorを一緒に更新する。実測値や保存済みr1 bytesは書き換えない。
- r1は初期EXACT経路の接続基準。native承認・有効化条件をr1への合意だけで満たしたことにしない。

## 2. IDと署名範囲

hashは64-byte BLAKE2b、入力はRust定義順のBorsh。domainをBLAKE2bのkeyとして使う。ハッシュは同一性のため、ML-DSA-87は作成者認証のために使う。計算の正しさの暗号学的証明ではない。

| ID | 入力 | 固定するもの |
| --- | --- | --- |
| `ResultId` | `MinerResultCommit` 全体 | scope、モデル・入力・実行条件・出力・依存・有限資料契約 |
| `ClaimId` | `ResultId`、別domain | 新形式のclaim。旧claim IDでGate Cを借りない |
| `CaseId` | scope、job、execution、Miner principal | 結果の差替え、Panel交代、witness rootやnonceの変更で予算を増やさない |
| `WitnessId` | `PanelWitnessCommit` 全体 | Panel author、結果、計画、試行番号、Panel自身の記録と出力 |
| `RequestId` | `SourceRequest` 全体 | case、契約、元資料root、種類、正規unit範囲。Panel鍵・relay・nonceを含まない |
| `PlanId` | result、profile、derivation ID、必須relation数 | Nodeが独立導出した同じ順序・全範囲 |

`CaseId`は出力を含まない。A4は同じcaseへの異なる結果の二重受付を拒否し、既存予約・要求・試行数を保存する。重なるrequestはIDが異なってもunitの和集合で重複排除する。異なるcase IDを渡しただけで、同じjobの受付上限を迂回できる規則にしない。

Miner署名contextは `misaka/positive/result/v2/r1`、対象は `ResultId`。Panel署名contextは `misaka/positive/witness/v2/r1`、対象は `WitnessId`。要求署名contextは `misaka/positive/request/v2/r1`、対象は `hash(request ID, requester principal)`。principalは既存MISAKAと同じ、2592-byte ML-DSA-87公開鍵のunkeyed BLAKE2b-512。署名は4627 bytes。

Miner結果にstep tree root、全trace、Positive witnessを必須にしない。Panel記録をMinerの署名済みtraceと見なさない。署名確認済み`AuthenticatedResult`はnative受付・資料可用性・計算合格のcapabilityではない。

## 3. 登録と受付の結び付け

A4はnativeに登録されたclass ID、artifact root、program root、tokenizer、実行・選択規則と、現在のnetwork/ruleset/branchから`AdmissionContext`を構築する。候補側の値をそのままcontextにコピーして認証済みと扱わない。

class IDの文字列一致だけで、候補が別に書いたartifact/programの値を許可しない。Nodeは登録program・layoutから必要範囲を導出し、認証openingを登録artifactに結び付ける。job/executionの正規identity・input・Miner割当・依存・funding consent・資料契約・受付時刻の適格性もA4が元の受付情報と照合する。`AdmissionContext`は期待job/execution/Miner/inputも固定する。Minerがexecution nonceを変えて同じjobのcase予算を作り直せないよう、A4の正規割当から解決する。モデル品質や取得可能性のチェーン審査を追加するものではない。

旧/新の受付形式は信頼されたrulesetから決める。Positive対象で旧形式を選んで回避できない。既存の旧claimは既存規則のままとし、旧必須fieldを空にして新形式に流用しない。

## 4. 資料要求・応答

r1の`SourceKind`は原資料 `ArtifactInventory` / `Input`のみ。unitは既存G14 inventory/promptの正規単位。新しい資料Merkle木や独自課金エンジンを作らない。`MinerSource.respond`の引数は認証済みdutyのみで、要求を別引数で差し替えられない。元資料のrootだけで応答完了にはせず、A4が実資料と既存openingを検査する。

範囲は半開区間`[first,end)`。A4はsource root/kind/contractを含む正規unitの和集合を全Panel・全試行で共有する。送信済みbytesはproofや再送も含めて別に加算し、同じ資料を再請求して予約・課金枠を再作成しない。

`MinerServiceContract`はMinerの結果署名に含まれる有限同意。全Panel合計のround、unique bytes、transmitted bytes、追加処理、保持bytes/期限、fee、witness試行数を固定する。実原資holdが存在するという証明ではない。最悪時の資料供給がこの枠に入らなければ、A2の事前委任を確保するかA4で受付を拒否する。

Minerへwitness構築、全trace再構築、projection計算を要求するAPIは用意しない。任意checkpoint/replayの同意profileはr1では未実装で、別の事前署名契約をA0へ追加するまで受理しない。

PanelのACKを応答完了の必要条件にしない。適格な資料はNodeへの直接搬送でも完了を記録する。Panelが証拠生成を放棄しても、既に完了したMiner応答を未提出へ戻さない。未応答の責任判定は成立済みの有限義務・通知・供給機会・期限から別途行う。私的RPC timeoutだけでMinerを有罪にしない。

## 5. 検査・生成API

共通portの定義は `ports.rs`。成功を返すdefault実装はない。

| port | 所有者 | 要件 |
| --- | --- | --- |
| `NodeChecker` | A1 | 登録programからplan導出、既存G14の副作用のない局所検査、全relationと状態接続・全語彙decode・終端を検査 |
| `MinerSource` | A2 | 認証済み有限dutyに対する原資料応答だけ |
| `PanelBuilder` | A3 | 再実行中にcapture、記録commit、分割証拠構築。正常/不正の提出と再開を計算合格から分離 |
| `AtomicCourtStore` | A4+A5+A6 | 受付・予約・資金・責任・保存を一体で確定/取消 |
| `CreditGate` | A5 | 現branchのrooted claimから全信用条件を読む。呼出側booleanを受け取らない |
| `Recovery` | A6 | 信頼されたanchorから履歴と全予算・会計を再構築 |

`CoverageDescriptor`はA1が導出する。`require_next`はcase/result/witness/planと連続範囲だけを検査し、新cursor候補を返す。算術の検査や`PositivePassed`生成は行わない。A1は局所`NoFaultFound`やPanelの`Clean`を全体合格へ読み替えない。

初期profileは`ExactStreamV1`。`FullRelationDirectV1`は型上の識別だけ保持し、binding/plan受付で拒否する。seed・反復・誤受理上界等が未定義のまま有効にしない。

## 6. 三つの独立した進捗と帰責

| 系統 | 状態 | 作用 |
| --- | --- | --- |
| Miner資料 | NotRequested / Pending / Complete | 証拠完成・計算合格とは別。warm PanelならNotRequestedで進める |
| Panel証拠 | Building / Committed / Checking / Complete / Rejected / Incomplete | 偽の中間値・不正構成はPanel証拠の拒否 |
| claim | ResultCommitted / Pending / PositivePassed / ClaimRefuted / EndedUnverified / Revoked | Nodeの検査事実からA4が遷移させる |

`PositivePassed`は全必須範囲＋正しい終端＋Miner署名結果の一致。`ClaimRefuted`はNodeが独立に検査した正しい終端とMiner署名結果の不一致。Panelの偽証拠だけではMinerを罰しない。期限切れ・不足は`EndedUnverified`であり正常・不正へ推定しない。

これらのenum/DTOは公開するが、デシリアライズした状態値に裁定・支払権限はない。A1/A4/A5の認証済み内部型は各実装がprivate fieldを持つassociated typeとして作る。保存済みPassedフラグからcapabilityを生成しない。

## 7. 資源・資金・状態の原子的境界

1. `prepare_admission`で全分岐のresource quote・有限義務・実資金の署名と残高・保持/rollback条件を照合する。失敗や放棄はdurableなcase/債務/引落しを残さない。
2. `commit_admission`の一つのstorage transactionでcase、資源予約、資金hold、責任、journal/undoを確定する。どれかの失敗は全体取消。commit時にも現在のbranch・state世代・clock・資金・同意・容量を再検証する。同じcase＋同じResultのretryは同じreceiptへ収束し、異なるResultは拒否する。
3. `apply_transition`でNodeの検査事実、資料応答、全Panel予算、試行、coverage、料金・返金・責任を一体で更新する。A5の既存money engineを呼び、A4に独自会計を作らない。
4. A6のrollback/replayは同じbranchの全項目を対象にする。支払済みunitとhold、Miner完了、Panel交代履歴を含め、再起動で上限・料金枠を増やさない。

既存`reservation_readiness_v1`が実原資・全分岐契約の不足で受理不能なら、flagの変更や仮の`Ok`で通さない。`PreparedAdmission`は受付成功を意味せず、実装A4+A5+A6の一体試験で境界を確認する。

## 8. 信用条件と公開網

新claimの成功報酬、work receipt、DAG重み、Rule E、決済、prefix、依存claimは、A5の同じrooted判定を使う。必要条件は現branchのPositive全被覆合格、正しい新result binding、依存条件、現Gate C、未解決適格fraudなし、成熟・責任・資料搬送・資金条件。Positive合格はGate Cの代替ではない。

A0 crateはnative ingress・native row・支払・合格capabilityを実装しない。`require_native_activation()`は常に`NativeAdapterUnassigned`を返す。公開preset、activation height、旧readerを変更していない。各担当がこの拒否を成功へ変えたり、別入口で回避したりしない。

## 9. 私設bytesと署名vector

`wire.rs`のcommit magicはASCII `PC2R1DTO`、続いてlocal kind byte（Result=0 / Witness=1 / Request=2）、続いてflat Borsh。commit bodyは4096 bytes以下、各fieldの順は`messages.rs`が正本。Vecを含まない固定schemaを使い、別型・旧wire・切断・余剰bytesを拒否する。この4096は私設codecの上限であり、新しい公開networkパラメーターではない。

証拠magicは`PC2R1EVD`。headerはcase/result/witness/plan（各64 bytes）、first relation（u64）、count（u64）、payload bytes（u32）。magic込み284 bytes。整数はlittle-endian。payloadは長さ一致を確認した借用sliceで、`EvidenceFrame`のfieldは非公開としbounded constructor/decodeを必須にする。Nodeの現在のpart capも検査する。内側は既存G14形式をA1/A4が別途bounded preflightする。framingだけでinner evidenceを認証したことにしない。

独立Pythonの`struct.pack`と`hashlib.blake2b`から作った [vector](contract-vectors-r1.json) をRustのBorsh bytes・各IDと照合する。これはA0 codec/署名試験であり、A7の演算期待値・実Node・実LLM試験の代替ではない。

## 10. 統合の完了条件

[担当と受入基準](WORKSTREAMS.md) に従い、小さい同一candidateの実プロセス結合から始める。旧G14無効時の回帰、新claimの正常・共謀・責任分離・全consumer拒否・原子的失敗・restart/reorg/IBD・実LLM三者費用を、同じ統合commitで確認する。

文書の統合先はRFC-v2-0014 / ADR-v2-0038、演算・資料認証はRFC-v2-0002。基準HEADには元workspaceの未コミットv2統合資料が含まれないため、それらの原本文・digest台帳を勝手に再生成していない。R2の新番号と元ZIPは本branchで参照できる状態に保つ。
