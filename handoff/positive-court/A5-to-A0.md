# A5 → A0: R2 接続に必要な共有契約

初期基準: A0 `a85152edc` + TIR annex `d615f62b6`。A5基盤 `56ef2750a`。
2026-10-31更新。A5内の3agent並列実装から、正式A1 terminal、実payer署名/actual output、
既存escrow checker、固定anchor scope検証を接続した。phase3候補は `1341730c7`、証拠は `4b52c23b1`。
その後A0 aux151型 `327393e8e` とP statement DTO `069d54007` を取り込み、data-only row readerの準備を完了した。
native owner・P実抽選Certified・受付/lockの未実装を成功へ変えていない。

後続: source dueのfirst unique計上は `c0d21e2f1` で具体化した。
new-version Positive fundsのlock分類/spend算術/CAS候補は
[採用案](A5-positive-funds-proposal.md) とnode `dues` / `native_funds` を参照する。
同候補のno-default node全lib 44件、all-targets Clippy、独立レビュー記録はphase5 evidenceに保存した。
actual native writer・mempool/blockの呼出し・同一DB batchの接続と有効化は別に残る。

## 採用済みA1共通adapter

A1 workspaceの `positive_contract.rs` はA0 annexの `tir::exact_derivation_id()`、
`greedy_selector_id()`、`provenance_id(result)`、`ExactPart`（16-byte prefix）へ更新された。
A5は先行snapshot `a2e32cb46` と正式A1 `ef8e4e87d` の一致を確認し、optional登録 `b3a61454c` も採用した。
A1 `50473e607` のauthor-authenticated packet wrapperも採用し、実terminalと署名込みmeterを検査した。
移行前のterminalはA5のcanonical plan照合で拒否し、旧derivationを新IDへaliasしない。

## nativeで必要な共有契約・割当

1. 新Resultのnative受付/保存binding。aux151の固定型とdata-only core readは `327393e8e` で採用済み。
   `ClaimId = result.claim_id()` とcase/result/scope/role/output/dependencies、登録期待値・A1導出planを
   実origin/accepted prefixへ固定するnative resolverは残る。旧claim rowを借りない。
2. positive-requiredを受付時rulesetから解決するversioned入口と、旧claim不変の切替条件。
   A0の `require_native_activation()` 拒否は、条件が割当・検証されるまで維持する。
3. A1のローカル全検査terminalとA4の現在裁定を結ぶopaque capability。復元時はA6が履歴を再検査し、
   保存済みPassed/署名/root/任意boolで生成しない。A4のsource残義務をPositive完了で消さない。
4. 新result/P subjectに結び付いたversioned attestation DTOは `069d54007` で採用済み。
   実draw/P value/getter/foldとactual `Certified` は未実装。DTO署名認証やPublicCheckCertifiedで代用せず、
   既存の正直割合の前提を維持し、旧Reproduced/旧S/K/Vの意味を変えない。
5. dependencies commitmentの正規preimageとexport/state/root/位置/model/tokenizer/layoutの解決API。
   全祖先の信用保留・取消しを同じ現viewで評価し、独立claimへ伝播させない。
6. 明示署名付きFundingScope/consentはA0 `42a60b840`、FundingRow型は `327393e8e` で採用済み。
   実原資/lock、正規schedule/policy、source/完成scope/carrier料金、capital、返金/責任を同じ現在stateへ
   結ぶnative ownerは残る。旧TFO1 `execution_root` をPanel witnessや新Resultへ再解釈して転用しない。
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
  この循環はA0 `42a60b840` のFundingScope projectionで解決済みで、A5は実payer署名/actual outputを検査した。
  実UTXO/lock・新result/case・現在viewを同一native受付で結ぶownerはなお必要である。
  A5はこの未実装を任意boolや架空の署名済み原資で埋めない。

## A5側の実装接続口

- leaf `PositiveCreditGate<RootedCreditView>` と全7用途の `CreditConsumers`。
- node `TirCreditView` は実A1 `CheckedTerminal` と認証済み新resultだけを保持し、A0 canonical planを照合。
- leaf `FeeSettlementProjection` はprivate fieldsを持ち、現在のbook世代・同じchecked transitionからのみ生成。
- node `check_existing_escrow_fee_spend` は実Transactionを既存 `palw_escrow_qualified_spend_v1` に渡す。
  principalを住所へcastせず、実ML-DSA公開鍵からnative address payloadを導出する。
  私設proposalであり、既存のscript/UTXO/native受付を代替しない。
- 新node crateはcore/exec→accounting→coreの循環を避け、`node → core + exec + leaf` に分離。
  workspace/lockfileはA0 `1eecf492a` / `6aeb4c42c` で登録済み。元の登録patchは提案履歴として保持する。
- `rows::PositiveCreditRows` はactual core `PositiveRowsV2::read/bundle` と共通Witness/Coverage validatorを使用する。
  missing/malformed/link/authを分離し、成功しても `SignedCreditInputs` というdata-only結果に留める。
  `NativeRowCreditGate` は全7用途で同じread failure、または8 native ownerのtyped missingを返す。
  原ログとsource hashは [phase4検証記録](../../misaka/palw/positive-accounting/evidence/phase4-native-rows/README.md) を参照する。

## 全native readerの具体的な未完了条件

全readerは上記native bindingがあるPositive armから**同じ**A5 gateを呼ぶ必要がある。
共通Positive必要条件は用途別の**既存entitlementとのAND**であり、既存条件を置き換えない。
`authorize_entitlement(binding, usage)` はdefault拒否。native ownerはactual origin・paid/mining契約から解決し、
rootは要求用途、祖先はDependencyを照合する。poster-funded paid FinalのB/F/R=0を維持し、
Positive/PだけでDagWeight/WorkReceipt/Rule Eを作らない。TIR adapterも同じportへ委譲する。
privatefixtureのAll/集合はnative権利resolverではない。反例・実A1経路の結果はphase9 evidenceに保存した。
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

## 採用済みFundingRowへの三勘定接続

A0 `327393e8e` の `rows::FundingRow` は **fee / capital / surplus** を別の `BalanceLeg` として保持する。
この節はnative ownerへの接続要件であり、新しいwriterや資金authorityを追加するものではない。
phase3の実署名・実carrier出力preflightは、まだこの三勘定rowを生成しない。

```text
actual output / initial_locked = 100
basis.required_fee            =  27  → fee.initial = 27
basis.required_capital        =   0  → capital.initial = 0
excess                        =  73  → surplus.initial = 73
```

上の数値は説明用であり、採用feeパラメーターではない。`fee.initial` はpayerの最大同意額でも実output全額でもなく、
nativeが正規preimageから導出した `basis.required_fee` と一致させる。三legの初期額のchecked sumを実output額と照合し、
`initial_locked <= consent.max_locked`、`fee.initial <= consent.max_fees` を別々に満たす。
capitalありは現在のA5 fee-only preflightで明示拒否している。capitalと他claim責任の解放を料金返金から推定しない。

例えば上のcaseが、提供済みsourceサービス10・明示契約burn2を精算し、残りを返還可能な状態で未検証終了した場合は次になる。

```text
fee:     initial 27 = paid 10 + burned 2 + refunded 15 + remaining 0
capital: initial  0 = paid  0 + burned 0 + refunded  0 + remaining 0
surplus: initial 73 = paid  0 + burned 0 + refunded 73 + remaining 0
aggregate refund 88 = fee refund 15 + surplus refund 73
```

private bookの集計 `HoldTotals.initial = 100` や `refunded = 88` を、そのまま `FundingRow.fee` へコピーしてはいけない。
その代入は `fee.initial == basis.required_fee` とfee legの保存則を破る。
同様に、まだsource10だけを支払った時点のprivate残額90は、nativeではfee残額17とsurplus残額73に分かれる。
surplusの返還時機はnativeの正規返金条件から解決し、料金holdと同じ扱いで自動解放しない。
現在のprivate preflightが実outputを一つのholdとして照合することを、canonical三勘定のnative受付実装と数えない。

capの役割も分ける。`Service.budget.fee` は全Panel・全予定source unitの資料料上限、
`Consent.max_fees` はCourt全feeのpayer上限、`Basis.required_fee` は全分岐から再導出する必要feeである。
`Case.usage.fee` へPanel完成料やcontractual burnを資料料として混入させない。
policy/scheduleのdigestやpayer署名だけで料金表を認可せず、nativeが正規のrecipient・額・burn・返金条件を解決する。

一回払いは、実精算event、`FundingRow` の三勘定と `current_outpoint / last_settlement`、
適格source unitの `ServiceReceiptRow.settlement`、source予算、資源・責任、journal head、実UTXO diffを
同じnative checkpointとDB batchへ確定して成立する。完成scope/carrierの支払をsource receiptへ偽装しない。
reorgでは同じbefore-imageから全要素と支払順・dedupeを戻し、古いprivate projectionや保存済みsettlement参照を再認可しない。
rowの保存則・typed read・P署名だけでは、この原子的精算が行われたことにならない。
