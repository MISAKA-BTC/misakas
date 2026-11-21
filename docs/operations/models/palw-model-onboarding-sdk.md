# The PALW model-onboarding SDK (`misaka-palw-sdk`)

<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../../governance/qwen-fow1-profile-transition.md)。QF1は登録artifact/scale/tokenizer/StateMapを揃えてMAS1へ進む具体profile。既存ADMISSIBLE/LOWERABLE等のarchitecture出力だけでFOW1採掘PASSとしない。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用定理・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

本書の旧source・型・抽選式・実装状況は旧profileの記録として読む。FOW1/TB1の新規実装には0061/0062の型・purpose・固定点・失敗終端を適用し、旧記述をfallback経路として実装しない。

[TB1の正本](../../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](../../rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](../../governance/palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](../../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

> **版依存の資料です。** 今回CLI・稼働nodeを再検証していません。旧資格・旧数値を現在の設計へ流用せず、[状態](../../STATUS.md) と [画像・動画・改善の当面無効化](../../governance/post-mainnet-scope.md) を先に確認してください。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Current operator entry point (Testnet-11 Relaunch 5f):** use
> `misaka --network testnet-11 model add` to inspect the catalog, then
> `misaka --network testnet-11 model add <catalog-model> --artifact <file>` to run the resumable
> registration and lane-certification flow. The lower-level SDK and `kaspad` flags documented
> below remain the developer interfaces, but are not the recommended first-time operator path.

**One interface every model class passes through.** Adding an LLM to a MISAKA network is four
agreements that must hold at once — a graph the court can walk (whose id IS the class id), an
artifact whose root the chain pins, a canonical job the class is paid per, and an engine that
executes what the graph describes. Before this crate those agreements were kept per lineage, per
consumer: two class tables of different types in `misaka_palw_base0::classes`, three per-lineage
arms in `kaspad`'s backend dispatch, two loops in the panel's registration builder, a magic switch
in the artifact loader. Every new model family meant finding all of them, and an arm forgotten in
one consumer was a class that could register but not produce, or produce but not be judged.

The SDK is the seam:

* **`PalwModelLineageV1`** (trait) — what a lineage must supply: container sniff/load, the frozen
  class table, artifact↔class pairing (shape check + root derivation), the known-weights keys, and
  chain-named `(class_id, artifact_root)` → execution backend.
* **`PalwClassSdk`** (registry) — the only door consumers use: `load_artifact`, `ledger`,
  `registration_candidate`, `preflight_admission`, `build_post_genesis_registration`, `resolve`.
  The kaspad panel, the producer's backend registry and the `palw-class` CLI all hold one of these
  and nothing lineage-specific.
* **`conformance`** — the standard battery, one call (`check_lineage_v1` / `check_sdk_v1`):
  the profile validates, every reference points backwards, both coverage gates certify (kernel ids
  AND per-node shape service), the canonical job counts, fits the worst case and the declared
  context, the court cost derives, and class ids are distinct across the whole ledger. The SDK's
  own tests run it over every built-in lineage, so a new table row is covered the moment it exists.

Consensus rules are untouched: the SDK produces the same `PalwConsensusObjectV2` objects through
the same `palw_post_genesis_registration_v1` / `verify_class_admission_v2` gates, and no ruleset
id, class id or fingerprint moves.

## Adding a new checkpoint of a known lineage (the common case)

No SDK code changes. The class is data:

1. **Freeze the geometry** beside its family's profile module in `kaspa-consensus-core`
   (`palw_qwen25_profile` / `palw_qwen36_profile` / …). A geometry that moved later would silently
   rename a class the chain already registered, so it is a named constant.
2. **Add the table row** in `misaka_palw_base0::classes` (`canonical_classes_v1` for the dense
   container, `qwen36_canonical_classes_v1` for the mmap tier). Take the next unburned rung of
   whatever axis separates siblings (the A16 family uses `n_ctx`; n_ctx 17 is burned — see the
   table's own comments).
3. **`cargo test -p misaka-palw-sdk`** — the conformance battery now covers the new row. A red
   test here is a class the court could not adjudicate; fix it before anything touches a chain.
4. **Convert the weights** with the family's converter (`qwen25-convert`, `qwen36-convert`, …).
5. **Dry-run the pairing and the gate** — no node, no keys, no coin:

   ```
   palw-class inspect   --network testnet-11 /path/to/artifact
   palw-class preflight --network testnet-11 /path/to/artifact --model-id <model-id>
   ```

   `inspect` shows which class the file pairs with, under which root, and why not the others.
   `preflight` runs the real admission gate (`verify_class_admission_v2`) against that network's
   bundle. Its genesis view is static — a live chain may hold more classes — but a REFUSED here
   is a refusal the chain would also give, before any fee is spent.
6. **Register from the host that holds the artifact**, using the current operator flow:

   ```bash
   misaka --network testnet-11 model add
   misaka --network testnet-11 model add <catalog-model> --artifact /absolute/path/to/artifact
   ```

   It resumes from live chain and local state, uses the bonded key selected by the wizard, reads
   live terms, applies the known-weights rule and sibling filters, and reruns admission preflight
   before anything is signed or funded. Developers automating the primitive directly may still
   use `kaspad --palw-class-artifact <file> --palw-register-class <model-id>` with that Bond key;
   the primitive is not the recommended first-time workflow. That `kaspad` must be the ONLY
   process running the bond: since 2026-09-25 it runs the bond's duties, round blocks included,
   and a second process beside a live node double-signs its round permits and is slashed
   (`docs/palw-add-a-model-runbook.md` §5).

## Adding a new model family (a new lineage)

A model expressible with the registered typed primitives can use a data frontend or canonical
TIR directly. It does not need a model-specific consensus profile, official compiler or a new
SDK lineage. `palw-class kernel-preflight <program.tir> --positions N --json` checks canonical
bytes and the node's hypothetical public-prosecution/carrier/block bounds at exactly N positions;
the report also exposes the dormant shipping schedule. It checks neither weights nor source
fidelity/economics, and does not activate a fence. `misaka model onboard verify <class> --artifact
<file>` derives verifier inputs from one root-consistent public row snapshot: sampled policies
rebuild the beacon/evidence and re-read selected leaves; complete-check policies re-execute all
enumerable inputs and artifact leaves. Served roots still require caller chain/state-proof trust.

The custom native-family container integration described below is a separate optional route.
An operation outside the registered semantics needs a versioned shared primitive/kernel extension
with its checker and terminal court. For a native engine/container outside the generic TIR path,
the SDK integration is one impl:

1. Implement `PalwModelLineageV1` in `misaka/palw/sdk/src/lineages/<family>.rs`, delegating to the
   family's own crates (the built-in `dense.rs` / `qwen36.rs` are the worked examples; the
   `TestLineage` in `lib.rs` is the minimal one).
2. Add it to `builtin_lineages_v1` (or compose it at a call site with
   `PalwClassSdk::with_lineage`).
3. Make `conformance::check_lineage_v1` the family's first test — `check_sdk_v1` in the crate's
   tests already runs it once the lineage is in the built-in list.

Nothing else learns the family exists: the panel's registration builder, the producer's resolve
path, the loader's magic dispatch and the CLI all iterate lineages generically. That is the
"must pass through" property — there is no second path a model can take, so the contract cannot
be partially honored.

## The two scars the candidate path encodes

* **Known weights never candidate for a new class** (`registered_weight_keys` vs the chain's
  `registered_artifact_roots`): re-registering weights the chain already has under a fresh id is
  never meaningful, and with two same-shape artifacts loaded it is exactly the 2026-08-28
  mispairing that burned the A16 n_ctx-17 seat.
* **Preflight before signature, always**: `build_post_genesis_registration` refuses to build an
  object the admission gate would refuse, so a hopeless registration never reaches the signer, the
  mempool, or the fee.

## What stays outside the SDK, deliberately

Converting weights (each family's converter binary owns its container), key custody and carrier
funding (the node's), fleet distribution of multi-GiB artifacts, and the economics a registrant
must not choose (share, target, slash value — the chain's terms). The SDK is where a model becomes
a class; it is not a wallet and not a deployment tool.
