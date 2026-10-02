AGENT 4 — RED-TEAM VERIFIER.  Read-only probes.  Nothing here modifies the repo or touches a network.

Build (deps are already compiled in ../target):

    cd /private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet/460defeb-125d-4763-9250-04db46ec225a/scratchpad/reward-audit/agent4-redteam
    CARGO_TARGET_DIR=../target cargo build --release

Four binaries, each writing the .txt beside it:

  redteam  -> probe-output.txt        R1..R7: re-derive the four shipped classes; the decode lever;
                                      the tile lever; the worst-case gap; the post-7,101 configuration
                                      (pay on CCU vs WEIGHT on leaves); the dilution channel.
  close    -> close-output.txt        C1..C5: the tile lever's TRUE admissible bound (it is refuted);
                                      C2 CLOSES A1's open lead with a concrete geometry; C3 the one-field
                                      class clone; C4 the free-prompt ceiling; C5 weight per executed MAC-eq.
  ladder   -> ladder-output.txt       every fence height read off `palw_rc_shipped_params()` itself,
                                      plus the court's real refutation cap at each DAA.
  table    -> counterexample-table.txt the operator's four families in the requested columns.

Everything calls only the repo's own functions:
  palw_step::{step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1}
  palw_economic_compute_v1::{palw_job_economic_compute_v1, palw_attempt_economic_compute_v1}
  palw_attempt_v2::palw_attempt_job_v1 (via palw_attempt_economic_compute_v1)
  palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1}
  palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1}
  palw_panel_economy_v1::palw_work_priced_reward_v1
  palw_class_admission_v2::reachable_kernels_v1
  config::params::palw_rc_shipped_params

HEADLINE (from counterexample-table.txt, family (b) and (d)):

  Three declarations of ONE graph, ONE artifact, ONE kernel set, ONE certification, all ADMISSIBLE:

    canonical (63,2)    executed 83,102,171,136 MAC-eq   weight     26,522,176   pay 2,991.68 MSK
    canonical (63,370)  executed 83,102,171,136 MAC-eq   weight    206,141,504   pay 2,991.68 MSK   <- 7.8x weight, identical cost
    canonical (1,432)   executed  1,546,037,392 MAC-eq   weight 12,124,304,640   pay 3,200.30 MSK   <- 457x weight, 1/54 the cost

  Weight per unit of arithmetic ACTUALLY EXECUTED, indexed to the shipped row = 100:
    (63,2) 100   (63,370) 777   (1,432) 2,457,197

  Pay is nearly flat across all three past the 7,101 flag day (ADR-0132 Upgrade C prices on CCU).
  FORK-CHOICE WEIGHT is not, and has no fence at all.
