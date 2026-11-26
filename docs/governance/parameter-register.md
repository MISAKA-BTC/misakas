# 現行parameter register

この表は設計値と明示的な未定値を示します。公開設定・古い台帳のparameter・実装状態を変更しません。旧ネットワークの値を新profileへ自動移植しません。

| Key | 値・式 | 単位 | 正本 |
| --- | --- | --- | --- |
| `allocation_alpha` | 3/2 | exponent | [仕様](../rfc/0012-bonds-budgets-and-slashing.md) |
| `common_model_coefficient` | 1 | coefficient | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `admission_routes` | BFT_DECISION only | route | [仕様](../rfc/0007-bft-model-admission.md) |
| `bft_n` | 3*f+1; f>=1 | operators | [仕様](../rfc/0007-bft-model-admission.md) |
| `bft_q` | 2*f+1 | distinct signatures | [仕様](../rfc/0007-bft-model-admission.md) |
| `gate_c_policy` | 対象profileで明示固定 | policy | [仕様](../rfc/0011-gate-c-and-consensus-credit.md) |
| `g14_panel_seats` | 1 | selected seat | [仕様](../rfc/0008-g14-panel-and-verification.md) |
| `existing_batch_notice_daa` | a+3120 | DAA | [仕様](../rfc/0009-court-and-claim-final.md) |
| `existing_batch_hard_daa` | a+10000 | DAA | [仕様](../rfc/0009-court-and-claim-final.md) |
| `fow1_ticket_bits` | 256 | bits | [仕様](../rfc/0004-fow1-execution-and-ticket.md) |
| `fow1_target_range` | 0 <= theta <= 2^256 | integer | [仕様](../rfc/0004-fow1-execution-and-ticket.md) |
| `tb1_field` | 2^521-1 | field modulus | [仕様](../rfc/0006-tb1-randomness-and-release.md) |
| `qf1_processed_positions` | <=8192 | token positions | [仕様](../rfc/0005-qf1-qwen3-execution-profile.md) |
| `qf1_generated_limit` | 1..1024 | decisions | [仕様](../rfc/0005-qf1-qwen3-execution-profile.md) |
| `qf1_readout_vectors` | 37 | vectors per decision | [仕様](../rfc/0005-qf1-qwen3-execution-profile.md) |
| `qf1_readout_bytes` | 303104 | bytes per decision | [仕様](../rfc/0005-qf1-qwen3-execution-profile.md) |
| `common_carrier_slot` | 1000 | milliseconds | [仕様](../rfc/0014-exec-schedule-and-capacity.md) |
| `common_carrier_limit` | 1 | canonical carrier maximum | [仕様](../rfc/0014-exec-schedule-and-capacity.md) |
| `mwa1_frontier_target` | 600 | seconds | [仕様](../rfc/0013-model-work-accounting.md) |
| `mwa1_plan_epoch` | 120 | seconds | [仕様](../rfc/0013-model-work-accounting.md) |
| `mwa1_target_and_price` | 対象profileで事前固定 | target / common work price | [仕様](../rfc/0013-model-work-accounting.md) |
| `moe3_position_decimals` | 8 (1 POSITION = 100000000 atomic units) | decimal places | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_purchase_permille` | 700 nominal; B=D+J | nominal permille of one coinbase budget C | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_lp_misaka_permille` | 200; A=floor(200C/1000) | permille of C | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_lp_position_fraction` | ell=floor(q*J/B), for B>0 | POSITION atomic units | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_miner_position_residual` | m=q-ell | POSITION atomic units | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_direct_misaka_reward` | 0 direct MISAKA to Miner; Panel/Gate C P=5% and BFT F=5% are MISAKA | MISAKA atomic units | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_normal_owner_fee_permille` | 10 with creator; 0 for canonical creator-less genesis | permille of gross normal AMM trade | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_normal_burn_fee_permille` | 50 (unchanged) | permille of gross normal AMM trade | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_internal_fee_permille` | 0 (unchanged) | permille | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_model_coefficient` | 1.0 for every model | coefficient | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_registered_tier_cap` | removed for MOE3 | origin-specific allocation cap | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_market_eligibility` | unique market per model, including genesis | eligibility | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_model_management_scope` | all models, including genesis | governance scope | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_fork_designation` | no new privileged fork class; same addition procedure | classification | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_burn_pause_scope` | all models including genesis and legacy fork | 対象・状態遷移 | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_burn_resume_authorization` | native registrant authentication OR dedicated MAD2 BurnResume approval; prescribed resume burn in both routes | 認証・支払述語 | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_pause_state_migration` | preserve canonical PAUSED/sequence/previous operation; drain old pending transactions under old policy | 移行規則 | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `bmr1_bft_coinbase_permille` | 50; F=floor(50C/1000) | permille of the same coinbase budget C | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_retirement_lp_misaka_burn_permille` | 1000 | permille of target actual MISAKA reserve | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_retirement_registration_bond_burn_permille` | 1000 | permille of target registration responsibility lots | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_retirement_mining_bond_burn_permille` | 1000 | permille of target MINING lots | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_unsettled_mining_reward_forfeit_permille` | 1000 | permille of unsettled target mining reward | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_verification_bond_bft_burn_permille` | 0 | permille of all target Panel/Gate C verification lots | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_legitimate_verification_reward_preserved` | true; preserve legitimate MISAKA payments | payment rule | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_retirement_bounty` | 0 | MISAKA or other assets | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_role_lot_isolation` | required; no double pledge or role change while liable | collateral identity | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `moe3_distribution_basis` | one target coinbase budget, split exactly once | MISAKA atoms | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_miner_purchase_residual` | C-F-P-A-J; nominal 500 permille plus all currency remainder | MISAKA atoms for Miner POSITION purchase | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_lp_position_purchase_permille` | 200; J=floor(200C/1000) | permille of C for LP POSITION purchase | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `moe3_panel_gatec_misaka_permille` | 50; P=floor(50C/1000) | permille of C paid in MISAKA | [仕様](../rfc/0016-position-market-and-rewards.md) |
| `bmr1_court_verification_reward_permille` | 490; E=floor(490S/1000) | permille of actual recovered MISAKA S in individual Court slash | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_court_burn_residual` | S-E; nominal 510 permille plus rounding remainder | MISAKA atoms recovered by Court | [仕様](../rfc/0017-model-stop-and-retirement.md) |
| `bmr1_court_reward_once_no_self` | no offender self-slash reward; one reward per offence | recipient and idempotence rule | [仕様](../rfc/0017-model-stop-and-retirement.md) |

単位、guard、採択/提案状態は[parameters.json](parameters.json)へ同じ値で記録します。未設定の本番定数は実測・予算・故障前提を伴う明示決定を要します。
