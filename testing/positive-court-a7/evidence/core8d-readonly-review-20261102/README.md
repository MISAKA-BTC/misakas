# A7 Core cacheの所有・stale調査（read-only）

対象は共有cache `/Users/wata/.cache/misaka-positive-a1-target/debug/deps/` のCore Rust中間物だけ。**削除・圧縮・build・新target・model実行は行っていない。** 実行binary、test executable、raw/source/lock、registry、native CPP/RocksDB、現在A6 cache、他unitは保護する。

## 限定候補と除外

`libkaspa_consensus_core-8d23c7aaac2eb958.{rlib,rmeta}` の2パスはA7旧executionに直接対応する。旧7ddの01.log:416に共有cacheでのfresh:false生成、後続c2eの01.log:370にfresh:true参照があり、終了済みc2e phase・固定source/lock/toolchain/profile記録を保持している。合計484106330B。Rootレビュー用の限定候補であり、unlinkの承認ではない。

`0bbd599cd27ef842` はA7 source-originを示すが、共有cacheで生成したexecutionを特定できないため除外する。旧local targetの同名cleanupとはbytes/hashが異なり、現在の所有根拠として流用しない。

[8d限定map](a7-core-8d-readonly-owner-map-20261102.json)にfull SHA、before/after stat/fstat、nlink1・realpath、旧log/source/current Git、.d/FP、使用観測を収録した。[詳細map](metadata-stale-map.json)では正規化されたcompiled inputs881件中25件が現在sourceと異なる。無条件のcrate root `consensus/core/src/lib.rs` もold SHA1462…からcurrent7704…へ変化し、現在A7のCoreはbase07d7と同じ。

## 観測の限界

4ファイル計968986293Bの現在SHA/statは安定、A1参照inode/sizeと一致した。ただし削除候補は8dの2ファイルだけ。旧生成時のlibrary content SHAは記録されておらず、現在bytesの完全な遡及認証はできない。.d/FPはsource commitを単独で固定しないため、実行ログ・固定sourceの差を併記した。

親SHA reader終了後のps/lsofで使用中buildや対象open/mmapは観測されなかった。最初の並列lsofで見えたPython71236は、この調査自身のSHA readerである。これは将来のCargo再利用を排除せず、別途承認された操作の直前にも再確認が必要。

ファイルbytesやstat blocksはAPFSで実際に回復する空き容量の保証ではない。rlib/rmetaは固定source/lock等から再ビルドする中間物で、保持済みtest executableや原結果の代替ではない。今回の証拠は回収の実行・承認と分離して保持する。
