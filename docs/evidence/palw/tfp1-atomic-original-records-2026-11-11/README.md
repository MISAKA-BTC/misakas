# TFO3受付・リンク・履歴の一括保存

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前のgoalターンは状況報告のみでno progress。今回、現在の差分を再確認し、三行保存と逆再生の検査を修正した。Studio同一claimの公開G14受付・Final・当選時権利消費・対応blockは **NOT_PROVEN**。最後の公開観測を今回の実測として再利用しない。

## 実装

[専用writer](../../../../consensus/core/src/palw_kernel_route_fold_v1.rs)は、PublicOriginal受付198・TFP1リンク204・履歴境界206を一つのチェックポイントで保存する。新規保存は三行とも存在しない場合だけ。三行とも完全一致なら冪等、部分状態は修復せず拒否する。新規保存には実際のordered foldが保持するprivate prefixを要求し、claim、実carrier、selected parent、受理点、書込み前root、journal境界を照合する。

[準備owner](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/native_link.rs)は、最初の書込み前に同じ署名済みcompanion・実UTXO履歴・予約入力を認証し、所有した値をframe/escrow書込み後に消費してリンクを完成させる。非zero reservation digestだけで予約済みと扱わず、完全native plannerによる実予約の認証を別途必要とする。[198の保存照合](../../../../consensus/core/src/palw_positive_contract_v2.rs)は、途中状態の古いLastPointではなく明示された今回の受理点を使う。

[prefix capture](../../../../consensus/core/src/palw_state_v2/objects.rs)のroot計算は、carriage/workspaceを型付き資源予約に計上してから行う。これは繰返しroot計算のCPU上限や全profileの費用計算の完成を意味しない。

差分の順再生・逆再生では、204または206が現れたとき、連続する198→204→206の新規挿入組であること、original/carrier/reservation/claim/sourceが一致すること、履歴のcutが自身の198挿入より後でないことを検査する。逆再生でroute header自体が消える場合も構造検査を省略しない。旧単独198は引き続き許容するため、204/206を両方取り除いた任意の差分の真正性までこの検査だけで保証しない。通常のnative deltaと期待state rootの認証は引き続き必要。

## 回帰と制限

[回帰コード](../../../../consensus/core/src/palw_state_v2/tests/native_writers_v1.rs)に、部分状態・prefix欠落/別root・escrow不整合による途中失敗の巻戻し・LastPoint更新前の保存・三行の冪等性・片側を落とした順/逆再生・headerなしの構造検査・不正な履歴cut・snapshot復元・資源不足を記述した。[準備側](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original.rs)には受理点差替え、zero reservation、frame/escrow未設置、二段階と既存delegateの一致を記述。**WRITTEN_NOT_RUN**。Rust実行は[採択済み通し実装計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)の後段に置く。実行結果はvalidation.jsonに分離する。

writerのrollbackは明示的に資源計上した全stateコピーであり、限定された行だけのundoへの置換は残る。三行だけの原子性を、claim/frame/book/escrow全体の原子性やproduction intakeの完成と数えない。実受付からの呼出しは未接続で、未対応profileの拒否は維持する。

## 次の本番接続

現在コードから、quoteとintakeで共用する完全なnative profile resolverが不足していると確認した。`PublicOriginalAdmissionV1`/`PublicOriginalReservationV1`の構築はfixtureにあり、productionの権限源にはなっていない。

- 固定chain scope、literal prompt、実executor bond、モデル/Source root、HF descriptor、初期状態・依存、manifest上限、費用・保持期限・全枝予算をnative入力から導く。署名済みcompanionの自己申告をpolicyにしない。
- 現drawn予約に、claim固有資料の初期供給、outsider/defense、資料要求以外の計算不一致、default/recoveryを含む実際の予約・consumerを接続する。旧モデル全量供給を復活させない。
- exact union packet IDsを持つServiceClaimBinding、CourtServiceRowV3、対応context、escrowと三行writerをFreePromptCommittedの同じチェックポイントへ接続する。
- 旧op248にはliteral prompt・unsigned companion/manifest intent等が不足する。既存wireの意味を変更せず、版を区別したquoteと受付の同一計算を接続する。
- Nodeの実候補filterとPMR dispatcherを全prefix再演・実UTXO料金へ接続する。署名、既存責任、物理ordinal、fee投影、選出承認を維持する。

配備・署名送信・公開probeは行っていない。HF取得可否の認定・多数決・モデル重み全量配信義務は追加していない。
