# Source-derived Selection duty — 2026-11-11

`d2ec940a7` は、受付済みoriginal frameとnative sourceから、全SourceMaterialGraphを構築せずにSelectionの依存関係を導くadapterを追加した。供給役割数はframeの元pledgeから取り、FullIdsと全row tileを各役割分保持する。既存のFinish/duty IDをそのまま再現し、既存の署名・裁定・再検査へ渡す。新しいwireや予約の権限は作らない。[source/binary/log digest](summary.json)。

core/RPCのlibrary test buildは4分30秒で成功。対象テスト2件が成功した。native Selectionの1件はgreedy・seeded sampling・V4 penaltyの正常と不正、二つのtileをまたぐ正常ケースの計7ケースを実行した。別の幾何テストは1/2 tile sourceの先頭・中間・末尾targetで全旧依存先と一致する。新adapterの依存先・相対/絶対ID・resource/profile bindingは旧graphを列挙したdutyと全fieldが一致する。

新adapterから作ったdutyを既存の署名付きnative interpreterへ渡し、独立した従来primitiveのproof IDと判定との一致を確認した。FullIds欠損、必要なcommitted/beat tile欠損、scope・署名・clock・鍵の不一致は拒否される。依存先配列のnative ownerは必要量ちょうどで成功、1 byte不足で資源エラーになり、解放後の予約量は0へ戻る。

これは受付済みfixtureを使った裁定adapterの試験であり、受付前のcomplete plannerではない。descriptorのownerは依存先配列だけを所有し、資料・program fill・proof clone・再検査scratchを代替しない。Batch Selection notice、実際の予約・有料搬送・DA/default回復、全branchのquote/fold共通計算と公開profileは未完。公開quote・受付・Final・main pushの成功として扱わない。

文書検査は既存の930 issue occurrences、971 missing-file、2 missing-heading、RFC歴史検査7件・ADR歴史検査89件で失敗し、前段階から増えていない。baselineは更新していない。
