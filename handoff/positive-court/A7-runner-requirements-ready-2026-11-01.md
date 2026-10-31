# A7 — 固定候補回帰のrunner準備（2026-11-01）

Pythonのguard/config/process試験は **26件成功・失敗0**。Rustや全runnerの成功件数ではない。[生ログ・source hash](../../testing/positive-court-a7/evidence/runner-guards-complete-2026-11-01/result.json)と[12コマンド・必須集合](../../testing/positive-court-a7/evidence/runner-guards-complete-2026-11-01/inventory.json)を保存した。初期16件の確認記録も別に保持する。

専用 `positive_court_fence_v2` とA7 `pipeline` は、それぞれ8件以上とliteralな期待8関数名を単一summary内で必須とする。ignored・別target・部分summaryの合算で代替しない。最終suite検査は、normal execution自体の欠落、no-runだけ、失敗entry、target指定のないentryによる代替も拒否する。全件0の通常コマンドも失敗する。要件と実成功名をmanifest v4へ保存する。

対象は共通契約、8用途会計・Node、Miner、4種類のPanel/source target、15種類のTIR/Node/Panel/実Court target、専用fence/source/lifecycle/旧TIR、core Positive単体、旧G14 sharedの12コマンド。A6 common journalの試験はA4 integration内のmoduleも含む。core unitは同じharnessを2filterで再利用する。source wrapperとremote wrapper等の同じfixtureの反復を、独立scenarioの数とは扱わない。

既存 `CARGO_TARGET_DIR` を尊重し、新しいcache directoryは作らない。指定した既存 `librocksdb.a` と環境値を開始時・終了時に照合し、SHAはstreamingで計算する。unitでは実cacheを変更せず、一時directoryによる不足・改変・環境変更・symlink差替えを検査した。source snapshotはconsensus全体とdatabaseも含む。

次に同じA7 worktreeで既存branchを保存し、A0固定 `79c3f9bca` から新branchへ切り替え、runner固有差分を選択する。productionが同sourceと一致し、試験がA7自身のsourceだけを読むことを確認してから回帰を開始する。A0の後続候補や歴史的243/300等は合算しない。
