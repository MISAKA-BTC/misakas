# DAA 18,850 candidate withdrawn; replacement at DAA 20,000

Status: **source candidate; not deployed or released by this record**. The original ModelOne single-seat activation at DAA **18,640** is unchanged. Only the later `palw_model_panel_one_retention_v1` activation is rescheduled to **20,000**; its acceptance-derived retention algorithm is unchanged.

The unshipped 18,850 candidate is withdrawn from the next official release. The running fleet passed that boundary before deployment, so shipping it now would apply changed retention admission to existing history. The [2026-11-01 live-history audit](../t12-fork-2026-10-31/retention-18850-live-audit-20261101.md), its [machine-readable result](../t12-fork-2026-10-31/retention-18850-live-audit.json), [29 actual claims](../t12-fork-2026-10-31/retention-18850-live-claims.json), and retained [`retention-18850-live-headers.json.gz`](../t12-fork-2026-10-31/retention-18850-live-headers.json.gz) record changed outcomes for 29 actual Qwen claims under the old candidate. That audit is an independent incident result; the ID probes below do not reproduce it. This note does not authorize replaying existing history with new rules or replacing a live DB.

All `retention-18850*` raw evidence, source snapshots, logs and candidate IDs remain historical records of the withdrawn, unreleased candidate. They do not certify the replacement candidate. In particular, the old 108-test result must not be reported as a new 20,000 test run.

| Fingerprint | Deployed 18,640 / retention absent | Withdrawn 18,850 candidate | New 20,000 candidate |
|---|---|---|---|
| params | `edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189` | `bbb4ad66e54695b04f136742902794a6d93aa4cec178e80c185cb58b1c3050a2` | `253d22bbc32fbcf3f23e9333524435478a5a7b87ac9a360de6513dde4cdc5248` |
| schedule | `4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04` | `5a84a8d700cae89fad2fb6316cf2033d7c80cecede29e7ccfa00eb3088e8679a` | `ec5f56fa48cdeafb3a6fc953c02a2dcd53f0f54025fe7243a87972884a47eb5e` |
| identity | `5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5` | same | same |

The [identity probe](retention-20000-identity-probe.rs.txt) and [actual derived output](retention-20000-identity-derived.log) call existing generic consensus fingerprint functions with the public retention field set to 20,000. They also check inactive at19,999/active at20,000, ModelOne at18,640, and retention `None` / `never()` collapsing to the deployed EDF/4024 values. A previously built core Rlib was used to avoid rebuilding dependencies on a disk-constrained workstation; these are **ID derivations, not new-source test results**.

The [twin probe](retention-20000-twin-probe.rs.txt) copies the existing seven counterfactual constructors and the existing IR-off shipped/drill constructors. Its [18,850 control output](retention-20000-old-candidate-control.log) reproduces the seven previous twin pins, and its [20,000 output](retention-20000-twins-derived.log) supplies the new pins. Existing identity pins and other-network pins are retained. [Derivation details](retention-20000-twin-derivation.json) record both sets without altering the earlier evidence.

New-source verification is pending: config4+state5, existing route9/identity, the 108 shipped/twin integration assertions, and the existing node rider tests. Deployment and release require their actual new-source logs plus a fresh rollout-margin review. The observed external DAA19,019 leaves981 DAA to20,000 at that observation; it is not a guarantee of remaining margin at deployment.

Canonical-chain policy is awaiting the user’s choice between retaining the current G14-at18,640 lineage and inheriting the older18,400 lineage with a future G14 migration. This20,000 source candidate assumes the currentG14 lineage. It is not authority to choose that lineage, deploy a production fork or publish a final release before the answer.
