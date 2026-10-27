# lane GC-E2E — Gate C (SecurityCertified) の実ノード E2E(2026-10-27)

Branch `claude/gatec-e2e`(base `claude/g14-panel1-17000` f82d8db94)。計算の暗号学的証明は使わない(PRINCIPLES §8)。抽選された検証者の署名承認を、正直な割合の前提の下で用いる。

## 実施したこと
新しい試験 `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/gatec_e2e.rs`。kernel world(aux 136)に model tier と Gate C を fence 0 で入れた。policy は TEST-ONLY の m=5/k=3 で、承認値 75/41 ではない。harness のカードは 8 枚で、設計値の窓では数日かかるため。
- **(a) `gatec_e2e_chain_claim_is_certified_only_by_the_drawn_verifiers` — ok**
  - card 0 が floor attempt を出す(LegacyPanelAttempt REAL の chain claim)。Panel bind → Valid の seat quorum を 0x4b carrier で送る → Final。
  - Final で `'C'` 行が commit され、未決のまま。Final だけでは `palw_independently_work_certified_v1` は false。
  - 後の seed anchor で抽選され、producer は抽選に入らない。抽選された bond 全員が mempool 経由で `Reproduced` を出す。
  - 起訴窓のあと `Certified` になり、fork choice・settlement が読む述語が true になる。replay node と root が一致した。
- **(c) `gatec_e2e_chain_claim_shortfall_is_not_certified_and_slashes_nobody` — ok**
  - 抽選された 1 bond が `Unavailable` を出し、残りは沈黙する。
  - 期限後に `NotCertified` となり、述語は false のまま(fork choice・settlement への寄与 0)。
  - 全 8 bond の collateral と slashed は変わらない。Gate C の失敗だけでは slash しない。replay も一致した。
- lane CERT の kernel 経路 2 試験も、修正後に ok(合計 4 passed)。

## 発見と修正(休眠・fence 未満で byte 同一)
**DL-1 の deadline index のずれ(debug assert で node が panic)。**
- `arm_retirement` は Gate C の retirement floor(`final + certification lifetime`)を claim-local settlement が有効なときだけ掛けていた。DL-1 と deadline sweep(`terminal_deadline_at_v1`)は常に掛ける。
- その結果、Gate C が支配する chain claim の Final は、認証が決まる前の時刻に retire する予定で armed されていた。試験では index が 3142、DL-1 が 4742。
- 修正: floor を常に掛ける(`transition_builder.rs` の `arm_retirement`)。Gate C が支配しない claim では恒等なので、出荷網では不変。

## chain claim の失敗時の扱い(現行コードの確認)
issuance 由来の保留報酬は払い出されない(burn)。poster への返金はない。floor attempt に poster はいない。kernel 経路の返金・burn 分割は lane CERT の範囲。

## 未完(大きいもの)
1. **(d) node duty が `'C'`/`'V'` に無い。** kaspad の Gate C drawn verifier duty(`kaspad/src/palw_kernel_roles.rs`)が走査するのは kernel の `'K'` 行だけ。chain claim(LegacyPanelAttempt・FP family)と owning39 `'V'` について、抽選された node が自動で attest する経路はない。fleet では `'C'`/`'V'` の claim は誰も attest しないので、常に NotCertified になる。consensus 側は動く(上記)。node 側の実装が必要。
2. **(b) FP family(FpSelection TIR class)の E2E** は未作成。`'C'` 行と sweep は同じ経路で、違いは FP receipt tail(`fp_receipt_tail_daa`)だけ。
3. **嘘の claim → negative attestation と court での有罪、偽 attestation の検出・除外、応答窓の失効、reorg、restart、pruned IBD** は、`'C'` では未試験。kernel `'K'` では reorg と IBD を lane CERT が、`'V'` では偽 attestor への課金を owning39 G-N2 が試験済み。
4. 承認値 m/k=75/41 での実ノード走行は未実施。9 枚以上の bond と長い窓が要る。

## 再現
`cargo test -p kaspa-consensus --lib -- gatec_e2e g14_cert_kernel_route`(RUST_MIN_STACK=67108864)。
