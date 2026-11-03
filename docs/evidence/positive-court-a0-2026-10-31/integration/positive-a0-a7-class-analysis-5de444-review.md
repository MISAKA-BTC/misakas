# A7 class-only analysis source review

対象 source `5de444ecc225f6cfe7a226a24def8d8891c07e9a`、evidence `35ffd08c80ef034f251a10de30b5ff5799be04da`、base `07d7eaebad0021164476c6289d8ccbbcbc45862c`。独立 read-only static review。

**結論: source/evidence の選択 cherry-pick 後、型検査・ビルドへ進める。追加 blocker なし。実行・モデル登録・全範囲成功の承認ではない。**

新 binary 547行と使用条件97行のみ。production/dependency/Cargo差分なし。source SHA `7ae2bc09cc03050b579a782a133ccb2e36420a86b8c58dc17729f26d87b718d7`。freeze 内の2 sourceと9 API、evidence manifest の2 hashを照合。9 APIはRoot `919309e93`ともbyte一致。

- 原3モデルのfixture SHAをJSON decode前に検査し、元artifact全体SHAを長さ固定でstream検査。program/layout/tokenizerは元fixtureと完全照合。weight inventory rootは再計算せずfixture宣言値からclass IDを再計算する。この制限をJSON/文書へ明記している。再計算されたnative rootとして扱わない。
- 元layout最大位置数8192/2048は保持。既存canonicalはP1023/255、D2、位置1024/256、token budget8193/2049。別longest sizing probeはP1/D8192又は2048/max_context_tokens=u32::MAXで、CPU短probe・canonical・job権限と混同しない。
- narrowはV1、wideはstrict StepSpaceV2 + typed sizingV2。Multiproof、固定work cap 2^26、stop_above=None。commit `(block,node)` 全集合・重複・GEN checkpoint不在を照合。layer occurrencesは別keyへ水増ししない。
- 同courtのelement/range両方が完走した時だけ全bounds tupleを比較する。work counterの一致やcross-court一致、独立モデルoracleを主張しない。cap/overflow/error/不完全count/不一致ではexit2又はerrorとなり、未測定partial workを偽の値で埋めない。
- coarseとmax(coarse,typed)、actual terminal/carrier/chunk/ladders/F7/rulesはNOT_CHECKED。固定root one-carrier比較は別欄。root fit=falseでもデータ解析完了exit0はあり得るが、これはdocのexit0定義と一致し、admission成功ではない。
- share/target/slash/activation/bond/signature/実Params等が無いのでconstructorとfull gateをNOT_RUNに保つ。signature/bondを捏造せず、登録object/native state/creditを生成しない。
- bounded header parserのlocal quoteだけで全startup/RSSを保証しない。fixture/JSON/decoded program/StepSpace/inventory/read maps/cache/exportは別ownerで、外部timeout/RSS guardが実行前に必要。

出力はcreate_newで既存input/reportを上書きしない。処理前後のfile/path metadataとfixture SHAを再検査する。外部停止時は最終JSONが書かれないため、rawとprocess wrapperの失敗記録を別途保持する。

このreviewではCargo/typecheck/build/rustfmt/test/model実行・実artifact再hash・weight inventory再計算を行っていない。owner evidenceのstaticチェックや以前のCPU成功を新binaryへ流用しない。source-base文字列は履歴baseを示し、今後の実buildでは実Root source/binary/Paramsを別pinする。既存CPU harnessの成功schema/argvとは異なるため取り違えない。
