# A7 初回class-only解析の独立audit

Root固定source `4e6724e32ac2cbde9f7d021ee87cf8f7c05f5218` の初回原本をread-onlyで監査した。モデル再実行・Cargo・target/archiveへのアクセスは行っていない。[audit](audit.json)、[Root manifest snapshot](root-manifest.snapshot.json)、[build snapshot](root-build.snapshot.json)。

| モデル | canonical / worst葉数 | court-off element / range | court-on element / range | data結果 |
|---|---:|---|---|---|
| Pythia70 | 224026 / 2208768 | 全点完了・bounds一致 | 全点完了・bounds一致 | complete、exit0 |
| Smol135 | 5173266 / 42244608 | work cap拒否 / 全点完了 | 全点完了・bounds一致 | incomplete、exit2 |
| Smol360 | 9195742 / 74473472 | work cap拒否 / 全点完了 | 全点完了・bounds一致 | incomplete、exit2 |

全体 `CLASS_DATA_INCOMPLETE` は正確。cap拒否はexact error `the close sizing exceeds its work cap`、部分workはnullのまま。全ケースはwork cap `1<<26`、stopNone。canonical・最長probe、原class/program/layout/tokenizer/SHA、scopeを照合し、元context8192/8192/2048を保持している。

court-onのtyped最大terminal close/root claimはPythia648069/89128B、135M685643/57712B、360M1080999/61680B。rootの固定100000B format比較をterminal・protocol admissionへ流用しない。coarse/M、実court・ladder・fee、constructor/full pure gate・native・creditは未検証を維持する。

12 rawファイルのbyte/hash、build/runの2452 source記録、binary記録を照合した。binaryは6259216B、SHA `60a84e2f4597724b24d46d98f87ddb8f232f45af0a0ab8222d35c8a363d98f2b`。A7のbinary確認はbuild/run/Cargo artifact記録間の整合で、実binary・archiveを独立に読んだ認定ではない。

原wrapperには停止理由・cleanup・安定性をcomplete判定へ接続しない競合と、取消しの見落とし/上書きの余地がある。**今回の3件は全てstop理由なし、live/zombieとも空、source/binary安定**で、偽完了の実発生は確認していない。初回raw/runnerの内容を変更せず、このmixed結果を保持する。修正案は別versionとPython/mock検査で検証し、同モデルの再実行は行わない。

RSSはDarwinの全process寿命のbyte観測。0.2秒pollによる2GiB停止policyはhard RSS保証ではなく、coldやモデル推論性能の結果でもない。Root側wrapper自体のexit0も、3分析caseすべて成功という意味ではない。
