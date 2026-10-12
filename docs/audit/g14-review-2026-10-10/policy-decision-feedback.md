# Policy decision feedback — 2026-10-11

These are Codex recommendations requested by the user, not a recorded user approval, shipping activation, or G14 completion claim. Reference: integration `105e2ab8bd596bbca0cee4974a40d8eca8c10f5c`, RFC-0012 policy proposal §4/§10, BUDGET §2/P-1/P-9/P-10/P-11 and ECON §5b. ADR-0177 D7 remains: the public verifier holds the correctly registered model/tokenizer/config.

## Recommended decisions

1. **RFC-12 drill policy:** D=3, unique mature work W=6,331,093,440 pwu, operator concentration=400 permille. Class concentration=1000 while fewer than three REAL classes contribute mature work; reconsider 600 after a measured census and a prospective policy update. Treat W as a starting floor requiring a live mature-work census, not a production economic derivation. Operator IDs do not prove independent economic entities. If the threshold cannot be met, safe stays null; do not silently relax it.
2. **Horizons:** retain the acceptance-relative 5,400-DAA maturity rule and the existing max with later claim retirement. Do not shorten to Final+1,000 or Final+600. Keep the separate legacy DNS evidence horizon at fence+10,080 DAA. New G14 staged/EXEC paths must derive maturity from their actual reversal, DA, court and hold horizons; 5,400 is not a universal upper bound. Never count work whose effect can still be reversed.
3. **Fee-only Round:** select ExecutionCap with capital-proportional Round rights charged at allocation. Do not count these rights as B_max blocks; B_max bounds reward-bearing blocks. Market fees stay outside R_max in v1, but expected fee gain remains in the economic incentive calculation. No refund or right renewal on skipped/failed execution, and no uncapped fee-only route.
4. **Budget window W (distinct from RFC-12 work W):** use 400 DAA only for a drill whose admitted profiles satisfy L<=400 and the per-span/slot bound. The interim feasible range is 280..436 DAA at150s/DAA (upper546 at120s), not a proved range for all G14 profiles. Longer staged liability may leave no feasible W under the current 65,536 bound: close that parameter/algorithm conflict before activating those profiles; do not inflate W or shorten liability without derivation.
5. **G14-L:** accept only the explicitly specified bounded session/lane distinction as a prospective design proposal: 34 lifetime sessions, up to64 lanes/session, up to128 extra lanes/reservation,128-DAA join window, per-lane rent/exposure, deadline and retention checks. Do not accept it as proof of full G14. The kernel16-unit FileDemand quota and native legacy36-lane first round have different routes; a direct implementation conflict between them was not established. Keep the kernel guard and independently verify native acquisition scope, lost-round reacquisition schedule, retained RAM admission and maximal-profile terminal completion. Preserve existing quota guards until that is done.
6. **Rule E and DNS retirement:** implement an explicit opt-in salted-drill-only arming path for the **current ADR-0178 exclusive-past fork-choice rule**. Require a distinct genesis/network/parameter identity and E active no later than retirement; leave shipping presets unarmed. This does not restore any ADR-0177-withdrawn model-acquisition obligation. Before qualifying the route, fix remote L2's hardcoded None and prove identical node/client decisions, DNS-zero behavior, partition/rejoin, restart/pruned IBD, and maturity under late conviction/default. A v1 light-client leaf lacking E's authenticated inputs must refuse certification, rather than silently compare under the old rule. Preserve the integration settlement lock discipline. Making a route available does not approve shipping activation.

## 現時点の判断（統合担当への回答）

| 項目 | 推奨判断 | 条件 |
| --- | --- | --- |
| RFC-12 | 初回 drill は D=3、work W=6,331,093,440 pwu、operator 上限400‰、class 上限1000‰ | live census による妥当性確認は未了。閾値未達なら safe=null。3つ以上の REAL class に成熟 work があることを測定した後、別の policy 更新で class 上限600‰を再評価する |
| 成熟と horizon | 受理後5,400 DAAを維持し、さらに遅い retirement・反転可能期限まで待つ。legacy DNS evidence は fence+10,080 DAA | 新しい G14 の DA/default/court/hold 期限を測定し、5,400を超える経路は成熟も延長する。Final+1,000/+600へ短縮しない |
| fee-only Round / market fee | fee-only は別の有限 ExecutionCap、market fee は v1 の R_max 外 | 権利割当時に課金し、失敗・未実行で権利を回復させない。fee 利得は抑止条件へ含める。fee 額そのものの上限を証明したとはしない |
| budget W | 400 DAAを drill の初期値とする | work W とは別。最大 liability・包含・slot/span 上限を満たす profile に限定し、全域の本番値は資源計算後に決める |
| G14-L の34 | 条件付き採用。34 lifetime sessionsと、有限で課金される並行 lane を区別した新規 contract として固定する | 64 lanes/session、128 extra lanes/reservation、128-DAA join window等の上限・徴収・期限・再取得を検証する。既存 guard を読み替えだけで外さない。有限というだけでは1 verifierの完走証拠にならない |
| Eと退役の arming | salted drill 専用の opt-in 経路を作る | ADR-0178のEを退役までに有効にし、通常presetは休眠。remote clientのNone固定と不足した証拠形式を解消してからdrillを有効と評価する |

### RFC-12 提案書の規則 E 記述の照合

Current `Params` contains `palw_fork_choice_rule_e_v1` (ADR-0178), includes it in the identity/schedule fingerprints, and validates its ordering with DNS retirement. `misaka/palw/remote/src/miner.rs` still supplies `rule_e: None`; `l2.rs` already refuses an explicitly active E because the v1 leaf lacks its inputs. The withdrawn model-availability gate and the current exclusive-past comparator are separate. RFC-0012 policy proposal §12.8's prior statement that the field is absent was inconsistent with this integrated source and has been corrected. These are source observations, not an executed salted-drill result.

The registry in `docs/design/palw/remaining-rfc-integration-matrix.md` is authoritative for future allocations. Reconcile the integration branch periodically and keep source-qualified execution logs; a source merge or a finite planner alone is not evidence of terminal prosecution.
