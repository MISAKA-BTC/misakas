# A7 class-analysis wrapper policy v2案

原run/scriptは変更せず、次prefix向けの別案を作成した。**モデル再実行はしていない。** 原runの[独立audit](../class-analysis-first-run-audit-20261102/README.md)はPythia complete、両Smol incompleteのまま保持する。

- wrapper: `/tmp/positive-a0-class-analysis-run-policy-v2-20261102.py`
- SHA-256: `c597f3cfe5cae8aaa76653310d5e55e2827866d4f7cf366d725772715dd8edcb`
- mock test: `/tmp/test_positive_a0_class_analysis_run_policy_v2_20261102.py`
- test SHA-256: `5caa0e3fb1a140a5f01cd41de7772ca880754c7321855a760e455f8e25c7e8e4`

同名のsource・testをこのdirへverbatim保存した。import時の実行を防ぎ、新しい `--out` を必須にする。Rootの元source `4e6724e32ac2cbde9f7d021ee87cf8f7c05f5218`、build record、binary、モデル・元context、u64MAX diagnostic leaf cap、court both、180秒/モデル・2GiB polled group RSS・512MiB disk停止policyは固定。現Rootや別candidateへ自動で切り替えない。これは次prefixの修正案であり、今回の同モデル再実行用ではない。

## 修正した判定

caseのcompleteには、停止理由なし、取消し未検知、exit0、strict schema/scope/完了status、error fieldなし、leader回収済み、live/zombie両方なし、source/binary安定を全て要求する。正常終了とpolicy停止が競合しても停止を成功へ昇格しない。cleanupが確実なら失敗rawもhashを保存するが、残存writerやcleanup不確実の場合はrawをfinalizeしない。

取消しはRSS/disk等に上書きされない。poll/wait/stream context終了、次spawn前、最後のcancellation context終了後にも確認する。取消し後に次modelへ進まず、final source検査中の例外も取消しを優先する。

途中はRUNNINGを保持し、for終了後にexact3モデル・各case条件・最終source/binary/HEAD安定を照合してaggregateを確定する。部分集合や重複case、final source変更はall completeにならない。

## 検証範囲

Python unittest/mock **16件成功**。[原stderr](positive-a0-class-analysis-policy-v2-mock-20261102-v2.stderr.log)、[command/source/log/protected hash記録](positive-a0-class-analysis-policy-v2-mock-20261102-v2.json)。停止と正常終了の競合、late cancel・同時RSS、spawn前取消し、cleanup失敗/zombie、identity不一致、errorを隠すsuccess report、最終aggregateを検査した。既存実0/2/2 JSONはread-only再分類し、同じglobal incompleteを確認した。

実モデルprocess・Cargo/buildは0回、target/modelファイルは未読。原wrapperとRoot raw全ファイルのbefore/after SHAは一致。mock値は物理RSS・モデル性能・native資格の実測ではない。RSSは引き続きpoll停止policyで、hard boundの保証ではない。

mock再現は保存したwrapper/testを上記の別名pathに置き、`python3 -B /tmp/test_positive_a0_class_analysis_run_policy_v2_20261102.py`。元Root JSON snapshotのread-only参照が必要。wrapper本体を実行してモデルを再走査する必要はない。
