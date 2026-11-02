# DAA 18,850 のテスト固定値の再計算

本体 commit `31d451ce6a5a65350514c812d58c2036e1d01739` の後続となる test / evidence のみの更新。
`release.json` と SDK の現在値は本体 commit に含まれる。実装・consensus rule・binary の変更は含まない。

既存 `scripts/release/t12-repin.sh` / `t12_repin.py` の harvest 形式と既存 constructor を使用し、
現在 shipped t12 の pin を持つ 17 integration module と、旧値の直接検索では見つからない
4 module の派生 twin を確認した。更新値はビルドしたテストが出力した値。
現在の shipped 値をすべての派生値へ一律置換していない。

- shipped params `bbb4ad66…` / schedule `5a84a8d7…` と旧短縮 assert 2 箇所を更新。
- without-cap / without-maturity / without-horizon / without-attribution /
  without-attribution-and-deadline / EVM-parent / mainnet-values-parent /
  IR-off shipped・drill の各値を実計算して更新。
- 古い経路を明示的に作る 6 箇所は、ModelOne を無効にする際にその後続の retention fence も無効にする。
  top-level のみを消して stale active mirror を残す手法は使わず、既存の mirror sync を維持する。
- t12 launch / release v1–v7 / pre-ModelOne と、他 network の歴史 pin は保存。

最終結果は **108 件成功・失敗 0 件・既存の timing harness 1 件 ignored**（66.52 秒）。
[最終実行ログ](retention-18850-repin-final.log)と[前後値・対象・SHA の記録](retention-18850-test-repin.json)を参照。
一時的に既存 21 module を[一つの runner](retention-18850-repin-runner.rs.txt)へまとめてリンクし、
既存テスト本体を実行した。runner は production / integration source に追加していない。
記録の runner を `consensus/core/tests/retention_18850_repin_probe.rs` として一時配置すれば同じ構成になる。

使用した warm target は既存 `g14-18400-deploy/target/ci-gates`、jobs=2、incremental=0、
test debug=0、core opt-level=0、`RUST_MIN_STACK=67108864`。
core library の変更、genesis / layout の再生成、全 integration suite の実行、fleet への適用はこの検証に含まない。
ライブでの新しい実モデル claim の受理・panel=1・Final については
[本体の検証記録](retention-18850.md)の pending 状態を維持する。

## consensus harness の追加監査

`consensus/src/pipeline/virtual_processor/tests/t12_round_lane_e2e.rs` の historical harness は
public-genesis G14 fences と ModelOne を意図的に無効にする。実際の params と比較対象 expected の
2 箇所で、後続の retention fence も同時に無効にし、直後の既存 mirror sync を維持した。
この追加ファイルは source / diff review のみで、上の 108 件には含めない。
この変更後の kaspa-consensus の compile / runtime は実行していない。
