# A7 実LLM harnessのsource固定

2026-11-01。source commit `283385c31f808b4894fbc7a9969215645a97dbe4`。固定baseはRoot `4753ee4beae5e245095954c6ef6e9b96fa05f361`。変更はA7専用testing内の10ファイルで、production変更0。

Rustの明示CPU/capture harness、固定3モデルのプロセスdriver、保存自然文の16token入力、再現可能なdriver guardを実装した。[使用条件と範囲](../../REAL-LLM-REPRO.md)。Pythiaはi64 Q24、Smol2モデルはnarrowのまま、元context8192/8192/2048を維持する。短jobはdecode1、実行位置数は16を予定する。最大context全位置の実行ではない。

3担当の静的レビューで、prompt/context pin、narrow Idxのunsigned decode、bounded JSON読込、dense coverage、自己整合性と独立expected、実leaf payloadによる予約、logits exportの追加保持、owner会計、per-operation work、driverの取消し・厳密な成功判定を確認・修正した。source hashとレビュー範囲は[source-freeze.json](source-freeze.json)に保存した。Rootの後続固定candidate `c85d5a9c9` と、参照した17 APIファイルのbyteが同じこともread-onlyで照合した。後続Root全体でのcompile/runtime成功を意味しない。

## 実施した確認

- `rustfmt --edition 2024 --check` と `git diff --check` が成功。
- 固定sourceのPython pure/mock guard **12件成功**。[原stdout](guards.stdout)、[command/終了状態](guards-command.json)。モデルprocess・Rust buildは0回。RSS parserの値は合成fixtureであり、実測ではない。
- 固定sourceでdriverのdry-runがexit0。3モデルのcheckpoint・artifact・fixture・tokenizer・revision・元context・保存promptを照合し、各モデルのerrorは0、source前後hashは一致。[全manifest](preflight-manifest.json)、[command](preflight-command.json)、[原stdout](preflight.stdout)。状態は `NOT_RUN_BUILD_NOT_REQUESTED`、モデルは `PLANNED_NOT_RUN`。

## 未実行と次の条件

**型検査・Cargo/build・Rust試験・runtime quote・推論・capture roundtripは未実行。** sourceが静的レビュー可能になった段階の証拠である。A0がRoot/A4結合試験中のためA7 heavy buildをholdしており、他担当のtarget/cacheや新cacheを使用していない。

次は統合担当が指定する固定candidate・既存target・限定実行枠でtypecheck/buildし、binary/source対応を保存する。その後3モデルのquote-onlyを先行し、明示した予算で受理できる短jobだけを実行する。quoteの論理上界とfresh-processのwall/CPU/RSSを別記録にする。型・compile不具合やRESOURCE_LIMITも新しい失敗ログとして保持し、成功へ数えない。

同じproducerから作るcaptureは `SELF_CONSISTENCY_ONLY`。独立expected、最大context coverage、新native V2登録・admission・再起動、SecurityCertified/P/creditはそれぞれNOT_RUNである。原artifact・原ログ・旧667件の証拠は変更していない。
