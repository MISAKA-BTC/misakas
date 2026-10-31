# A5 → A0: R2 接続に必要な共有契約

基準: A0 `a85152edc` + TIR annex `d615f62b6`。A5基盤 `56ef2750a`。
2026-10-31。A5内で3agentを並列実装中。未割当native境界は成功へ変えていない。

## 先に必要なA1互換修正

A1 workspaceの `positive_contract.rs` はA0 annexの `tir::exact_derivation_id()`、
`greedy_selector_id()`、`provenance_id(result)`、`ExactPart`（16-byte prefix）へ更新された。
A5はA1のowner sourceをhash固定snapshot `a2e32cb46` として取り込み、実 `CheckedTerminal` tokenを使う試験を進めている。
移行前のterminalはA5のcanonical plan照合で拒否する。旧derivationを新IDへaliasしない。
正式A1 commit採用時はsnapshotのsource hashと照合する。

## nativeで必要な共有契約・割当

1. 新Resultのnative受付/保存binding。`ClaimId = result.claim_id()` と、case/result/scope/role/output/dependencies、
   登録class・A1導出plan・全必須relation数を固定し、旧claim rowを借りないnative resolver。
2. positive-requiredを受付時rulesetから解決するversioned入口と、旧claim不変の切替条件。
   A0の `require_native_activation()` 拒否は、条件が割当・検証されるまで維持する。
3. A1のローカル全検査terminalとA4の現在裁定を結ぶopaque capability。復元時はA6が履歴を再検査し、
   保存済みPassed/署名/root/任意boolで生成しない。A4のsource残義務をPositive完了で消さない。
4. 新resultに結び付いたGate Cのversioned attestation対象とrow lookup。既存抽選承認・正直割合の前提を維持し、
   旧Reproduced/旧claim IDの意味を変えない。
5. dependencies commitmentの正規preimageとexport/state/root/位置/model/tokenizer/layoutの解決API。
   全祖先の信用保留・取消しを同じ現viewで評価し、独立claimへ伝播させない。
6. payerの明示署名付きfunding consentと実原資の新result/caseへのnative binding。
   旧TFO1 `execution_root` をPanel witnessや新Resultへ再解釈して転用しない。
   source unit/完成scope/carrierのcanonical fee key、署名済み全fee cap、refund/burn条件を解決する。
7. A4/A5/A6の原子的境界: case/資源/資金/有限source義務/attempt/dedupe/支払/undoの同一transaction。
   現branch/state世代/時刻/実escrow before-rowをcommit直前に再照合し、部分成功を残さない。
8. 新Positive service leg/markerを既存once-only台帳に割り当てる場合のID。
   既存leg IDや署名domainをA5独断で再利用しない。Node生成fee projectionはnative spend権限ではない。

### 既存funding bodyへ直接接続できない具体的な理由

- `palw_tir_funded_original_envelope_v1.rs:76` は旧FundingTermsのclaim/job/executionを旧FPの
  `claim_id()` / pledge / producerのtrace execution rootへliteral一致させる。Panel witnessへ意味変更できない。
- `palw_funded_original_coordinator_v1.rs:227` のproduction schedule resolverは現在 `Err(Schedule)`。
  `palw_funded_escrow_v1.rs:410` のproduction fee schedule resolverは `None`。
- さらに、旧FundingTerms.idを新serviceの `funding_consent` として単純流用すると、
  `new result.id → service_contract.id → funding_consent(旧FundingTerms.id) → FundingTerms.claim → new result.claim_id → new result.id`
  の循環identityになる。現在のprivate bridge fixtureのconsentはopaque Digestで、旧FundingTermsへの署名接続を検証したものではない。
  A0は新funding body/参照方式を決め、循環を作らずpayer同意・実UTXO・新result/caseを同一native受付で結び付ける必要がある。
  A5はこの未割当を任意boolや架空の署名済み原資で埋めない。

## A5側の実装接続口

- leaf `PositiveCreditGate<RootedCreditView>` と全7用途の `CreditConsumers`。
- node `TirCreditView` は実A1 `CheckedTerminal` と認証済み新resultだけを保持し、A0 canonical planを照合。
- leaf `FeeSettlementProjection` はprivate fieldsを持ち、現在のbook世代・同じchecked transitionからのみ生成。
- node `check_existing_escrow_fee_spend` は実Transactionを既存 `palw_escrow_qualified_spend_v1` に渡す。
  principalを住所へcastせず、実ML-DSA公開鍵からnative address payloadを導出する。
  私設proposalであり、既存のscript/UTXO/native受付を代替しない。
- 新node crateはcore/exec→accounting→coreの循環を避け、`node → core + exec + leaf` に分離。
  A0へ返すworkspace登録差分を用意する。

## 全native readerの具体的な未完了条件

全readerは上記native bindingがあるPositive armから**同じ**A5 gateを呼ぶ必要がある。
現在はnative新claim入口がないため、下記を接続済みと宣言していない。

| 用途 | 入口 | 未完了条件 |
| --- | --- | --- |
| 発行/coinbase/model/PWU/worker-user成功報酬 | `palw_state_v2.rs`, `palw_state_v2/transition_builder.rs`, `palw_security_certification_fold_v1.rs` | 新result binding・全coverage・Gate C等の同一gateを、Final/held rewardの**支払前**に読む |
| kernel escrow成功報酬 | `palw_kernel_route_fold_v1.rs`, `palw_security_certification_fold_v1.rs` | 新Positive claimと既存kernel claimの対応・原資/一回精算をnative rowで固定 |
| native reward/receipt mint-spend | `palw_native_mining_fold_v39.rs`, `palw_native_receipt_v39.rs`, `palw_rfc0018_v1.rs` | 新形式のcertificationを読んでmint/spendを再認可。carrier自己資格化を拒否 |
| safe/live/provisional weight・復算/retire/取消 | `palw_rfc0018_v1.rs`, `palw_weight_cap_v1.rs`, `palw_state_v2/chain_state.rs`, `palw_state_v2/transition_builder.rs` | 未完了・取消し・祖先holdを加算/復算/取消しの全箇所で同じ数値へ反映 |
| Rule E | `palw_fork_choice_rule_e_v1.rs`, `palw_fork_choice_rule_e_leaf_v3.rs` | 未検証Positiveを参加資本へ入れず、同一predicateでleaf/復算を一致 |
| frontier/depth/anchor/native settlement/EVM | `palw_security_certification_v1.rs`, `palw_settlement_v1.rs`, `palw_native_settlement_v1.rs`, `palw_claim_local_v1.rs`, `palw_state_v2/evm_replay_settlements_owned_v2.rs` | 根拠claimと全祖先の現credit-readyを確認し、reorgで同journalから取消し |
| prefix/state再利用/依存 | `palw_tir_prefix_state_v1.rs`, `palw_prefix_family_state_v1.rs`, `palw_claim_local_v1.rs`, SDK `lineage.rs` | export identityと依存commitmentを照合。rootだけでtrusted inputへ昇格しない |
| RPC/SDK表示 | `palw_user_job_views_v1.rs`, `palw_user_status_v1.rs`, `palw_user_views_v1.rs` | 保存flagではなく同じnative gateの結果・未検証/依存hold理由を表示 |

検証はprivate単体・実checker・既存escrow engineを区別し、native全consumer・実process・A6復旧が
同一candidateで接続されるまでR2全体の完了とはしない。本番有効化は変更しない。
