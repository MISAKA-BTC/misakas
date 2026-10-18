# RFC0006・0007・0008・0012 — 同一候補 binary の単一統合受入 campaign

状態: **source 実装、未コンパイル・未実行・未検証**。この文書は受入結果ではない。全コード実装の完了後に検証をまとめて実施する。RFC-0016・0017・ADR は本作業の対象外。

## 実装の境界

- entry: `scripts/drills/misaka-rfc-integration-campaign.py`
- plan / identity / evidence / stage hooks: `scripts/drills/rfc_integration_campaign.py`
- source controls: `scripts/drills/tests/test_rfc_integration_campaign.py`（未実行）
- 既存 `misaka-rfc12-multiprocess.py::MeasuredDrill.run` を **一回だけ**呼ぶ。既存 script は改変しない。
- A / B / JOIN の役割ごとに一つの DB を所有する。scenario ごとに DB・binary・network を交換しない。JOIN の初回空 DB は実 P2P IBD のための役割の初回起動であり、scenario のやり直し用 DB ではない。OS crash / consumer activation の再起動は同じ役割・同じ DB を使う。
- 独立した既存 fork、partition、pruning、fresh pruned P2P join、cold restart、DNS retirement、100k claim と process/file 観測を継承し、その前後に正常/攻撃 scenario を挿入する。

## 先に固定するもの

campaign plan は閉じた version-1 JSON schema。entry は plan、base drill plan、process observation limits、candidate、使用 helper/input/source files の SHA256 と file identity を起動前に固定する。全 node の `argv[0]` は同じ絶対 canonical path の candidate。symlink、別 binary、scenario ごとの appdir、途中の inode/byte 差替えを拒否する。

candidate SHA256 は **実行ファイルの byte identity**。`sourceClaims.revision` と `sourceClaims.buildInputs` は別の申告であり、その binary がその source から生成されたことの attestation として扱わない。driver / base drill の source digest も別に記録する。

最初の子 process より前に、所有 root へ `campaign-freeze.json` と `campaign-once.json` を exclusive create・fsync する。既存 report/root を上書きして再試行しない。成功/失敗/UNKNOWN にかかわらず DB・node log・fixture stdout/stderr・report を残す。

plan の top-level fields:

| field | 内容 |
|---|---|
| `schemaVersion` / `kind` | `1` / `misaka-rfc-integration-campaign-v1` |
| `campaignId` | 一意の lowercase identifier |
| `candidate` | `{path, sha256}`、全 node/restart で同じファイル |
| `sourceClaims` | `{revision, buildInputs}`、申告と実行 byte を混同しない |
| `network` | `{network:"misaka-testnet-12", genesisHash, consensusParamsId}` |
| `drillPlan` | `{path, sha256}`、既存 MeasuredDrill plan、A/B/JOIN 固定 |
| `resourceLimits` | `{path, sha256}`、既存 process observation limits schema |
| `integrityPolicy` | `{fullDigestIntervalSeconds, maximumLogicalFileReadBytes, maximumSingleCheckMilliseconds}`、事前に固定するpin検査の観測budget |
| `pinnedInputs` | `[{path,sha256}, ...]`、fixture interpreter、helper scripts、事前入力等 |
| `scenarios` | 事前固定した正常・攻撃の双方。複数の distinct claim ids、各 scenario は複数 actual nodes |
| `gates` | 下記全 gate から固定 requirement ids への対応 |

node argv の appdir は各 role について `--appdir={A_APPDIR}` / `{B_APPDIR}` / `{JOIN_APPDIR}` 一つだけ。loopback endpoints と `--nodnsseed --outpeers=0` を使う。node の発見/接続は既存 driver の所有 P2P links が管理する。

fixture/scenario command は shell を介さない明示 argv array。executable と既存 script/input file は別途 content pin 必須。build tools、shell/env、inline `-c/-e/--eval/-m` は受け入れない。生成物や fixture stdout は native fact に変換しない。

## scenario schema と hooks

各 scenario の fields は次のとおり。

```json
{
  "id": "normal-native-claim",
  "kind": "normal",
  "rfc": [6, 7, 8, 12],
  "stage": "fixture.bootstrap.after",
  "roles": ["A", "B"],
  "claims": ["<事前に固定した128桁 lowercase native claim id>"],
  "commands": [],
  "observationTimeoutSeconds": 60,
  "requirements": [
    {
      "id": "normal.claim-a",
      "kind": "native-claim-observed",
      "role": "A",
      "claimId": "<同じclaim id>",
      "expected": {
        "classId": "<128桁>",
        "jobId": "<128桁>",
        "producerBond": "<128桁transaction id>:<u32 index>",
        "expectedConvicted": false,
        "maximumRecordBytes": 1048576
      }
    }
  ]
}
```

正常系全体で少なくとも二つの distinct work claim を必要とし、各正常 claim は二つ以上の実 node で `native-claim-observed` を宣言する。上の短い例は B 側と二つ目の claim を省略した schema 説明であり、単独の実行 plan には使えない。未使用の claim id を一覧に足して複数 claim と主張できない。攻撃系も claim ごとに submission/import/adjudication の三種を全て必要とする。

上記は schema 説明であり、実行可能な署名 work / plan ではない。例示した observation timeout / record byte cap は consensus 設計値や承認済み測定値ではない。実行 plan では必要な finite observation limits を事前に記録する。

hook:

- `fixture.bootstrap.before/after`
- `fixture.fork.before/after`
- `fixture.partition.before/after`
- `fixture.prune.before/after`
- `milestone.shared-bootstrap`（実 kernel consumer activation を設定した場合のみ）
- `milestone.fork-convergence`
- `milestone.partition-healed`
- `milestone.source-body-pruned`
- `milestone.fresh-pruned-join`
- `milestone.cold-pruned-join`
- `run.after`

JOIN を使う scenario は `fresh-pruned-join` / `cold-pruned-join` / `run.after` だけ。まだ存在しない process の検証を宣言させない。`run.before` は root/binary identity の記録 hook であり、node scenario を配置できない。hook は一回だけ消費し、未到達の scenario は UNKNOWN で残す。

各 hook・scenario・launch・command・run の前後に full SHA と plan/DB ownership を確認する。hot probe は symlink/inode/size/mtime を照合し、事前固定した interval ごとにも full SHA を再読する。`pinVerificationCosts` に full/metadata の区別、実測時間、論理 file read bytes を全て保存し、事前固定 budget 超過で止める。これは物理 disk IO や RSS の上限を証明したものではない。

各 hook・scenario・launch・command・probe の前後に byte/plan/DB ownership を確認する。network / genesis / params は全期間不変。sink / pruning point / DAA / import epoch は **各 native read の前後で coherent bracket** を要求し、fork/pruning で実際に進んだ値を保存する。sink/import を campaign 全期間同じ値に固定したと誤って主張しない。launch 直後にまだ RPC がない interval は fresh context を捏造せず、後の実 RPC probe と launch label を対応させる。 正常な JOIN import の途中は pending として観測し、network identity の不一致と混同しない。stable/completed import は authority collector の前後に必須。sink/import が遷移中なら finite deadline まで再観測し、binary/plan/root/network identity の差替えは専用エラーで再試行せず中断する。

## actual-only の native evidence

実装済み collector は `native-claim-observed`。所有 node の `getPalwKernelClaim` を `getBlockDagInfo` / `getPalwNodeStatus` の前後で囲み、exact claim/class/job/producer、DAA、ledger/aux roots、bounded record/header を確認する。`state=Final`、`receiptCount`、`settlement="paid"` から typed selected-verifier approval や fee settlement を推定しない。

以下の capability は、対応する native 観測 consumer がまだ公開されていないため **UNKNOWN**。任意 JSON path、stdout の PASS、WAL、manifest、古い RPC の lifecycle label で埋めない。

- `selected-verifier-approval`
- `native-fee-settlement`
- `reader-router-court`
- `all-model-cost-boundaries`
- `independent-source-availability`
- `cumulative-soundness-review`
- `128-bit-review`
- `adapter-interoperability`
- `all-rewards-once`
- `material-all-path-bounds`
- `restart-reorg-atomicity`
- `market-accounting`
- `attack-submission` / `attack-import` / `attack-adjudication`

attack scenario は actual command と、この三種の独立した native fact requirements を全て事前宣言する。送信しなかった・検証 node に import されなかった・RPC/command が timeout したことを「防御成功」にしない。native capability を追加する場合は、候補 binary の実 API/exporter → exact receipt/claim/context → driver の固定 collector を別途接続する。plan から自由な「native と自称する JSON reader」を注入する仕組みはない。

128-bit は短い実験で観測できたと主張しない。レビュー・累積失敗確率・参加率・取得可能性・費用の前提を別途閉じる必要がある。暗号学的計算証明やモデル単位の審査を追加しない。

## 全体 gate

`gates` には次を全て含め、右の kind の固定 requirement ids だけを参照する。弱い claim identity を approval gate に流用できない。

| gate | 必須 evidence kind |
|---|---|
| `rfc06-model-boundaries-cost` | `all-model-cost-boundaries` |
| `rfc06-routing-court` | `reader-router-court` |
| `rfc06-100k-pruned-p2p` | `base-drill-mechanics` |
| `rfc07-independent-sources` | `independent-source-availability` |
| `rfc07-income-cost` | `native-fee-settlement` |
| `rfc07-cumulative-soundness` | `cumulative-soundness-review` |
| `rfc07-128bit-review` | `128-bit-review` |
| `rfc07-adapters` | `adapter-interoperability` |
| `rfc07-selected-approval` | `selected-verifier-approval` |
| `rfc08-all-rewards-once` | `all-rewards-once` |
| `rfc08-material-bounds` | `material-all-path-bounds` |
| `rfc08-restart-reorg` | `restart-reorg-atomicity` |
| `rfc12-fork-ibd-prune-dns` | `base-drill-mechanics` |
| `rfc12-crash-atomicity` | `restart-reorg-atomicity` |
| `rfc12-market` | `market-accounting` |

`base-drill-mechanics` は actual MeasuredDrill が戻り、全 owned launch の RSS/file/RPC observation が predeclared bounds を満たした場合だけ PASS。fixture が同名 fact を返すことはできない。

全必須 scenario、全 gate、base drill、resource observations が既知 positive の場合だけ campaign `PASS` / exit 0。それ以外は UNKNOWN または FAIL / 非ゼロ。report の `securitySoundnessMeasured` と `releaseApproval` は false のまま。process mechanics の成功を無条件の security guarantee にしない。

## 実行順序（現時点では実行禁止）

1. RFC06/07/08/12 の残りの code・consumer・native observation capability を先に全て完成する。
2. 一括検証 phase で source controls を含む必要な検証を実施し、受入用 candidate を一回 build する。
3. binary SHA256、source/build claims、base drill plan、観測 limits、actual signed-work / attack inputs、scenario/claim ids、全 gate を固定する。
4. `--execute` なしで freeze report を保存する場合は node を起動せず UNKNOWN / exit 2。これは実受入ではない。
5. 全実装完了後、同じ凍結 plan で次の **一回**の campaign を実行する。

```text
python3 scripts/drills/misaka-rfc-integration-campaign.py \
  --plan /absolute/campaign.json \
  --root /absolute/new-owned-campaign-root \
  --out /absolute/new-campaign-report.json \
  --execute
```

現時点で上記コマンド、Python tests/compile、build、node、measurement は一切実行していない。

## 残件

V6 paid-bootstrap用のown API/sidecar/collectorを追加した。既存のexact paid claim観測flagに加え、明示共通source-paid profileとV6 policy fileを要求する。V5とV6は別directory/schemaで保持し、restartで両方のexact stale fileを無効化する。新collectorのsourceもcandidate campaignの固定対象に含める。

追加の構造観測kindは`native-paid-bootstrap-observed`、`native-paid-bootstrap-quorum`、`native-paid-bootstrap-source-graph`。expectedはenrollment/context/checkerSnapshot/policy/binding/bootstrapEpochだけを固定できる。current native graphとreceipt structureを観測するもので、上記必須gateのkindは変更していない。full selected approval、独立性/取得・市場条件、履歴署名、実coinbase/決済/全報酬一回精算をこれらで代替しない。全て未実行である。

- 上記未公開 capability の native API/exporter と fixed collector の接続。未実装のまま PASS にしない。
- 実 artifact・claim・攻撃 transaction による署名済み finite command inputs と full model/cost inventory。
- binary/context drift、same DB crash、未import attack、command timeout、missing approval、累積 soundness / 実収支 / 全報酬一回精算の actual negative/positive campaign cases。
- 一括 phase での source control 実行・構文検査・candidate build・一回統合受入。source の追加だけで RFC 完了としない。
