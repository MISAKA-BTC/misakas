# A7 Core8dの限定解除

A0 Rootが[read-only map](../core8d-readonly-review-20261102/a7-core-8d-readonly-owner-map-20261102.json)のSHA `641b695cf77800e12a0b77cda4350da44e88def87cbd2e4c719fe9fd3cc17241` と8d rlib/rmetaの**exact2パスのみ**を承認した後に実施した。Rootは独立にstat/inode/nlink/fullSHA、旧Cargo JSON、.d/FP、source差とRust arを照合した。

実行直前にmap・source/HEAD・historical log・.d/FP hash・旧Git/current3input SHA・全artifact SHA/stat/fstat/realpath/nlink1を再確認し、Cargo/rustc/linkerなし・lsof emptyを確認した。map観測HEAD a25から承認HEAD c045への17変更は全A7evidenceのみで、Core差分はない。

削除したのは共有cacheの `libkaspa_consensus_core-8d23c7aaac2eb958.rlib` と `.rmeta` のみ。0bbは除外を維持し、stat/inode/mtime/ctime/nlinkをpostcheckした。.d/FP hashも保持。実行binary/test executable/raw/source/lock/registry/native CPP/A6 cache/他unitを操作せず、圧縮・build・model実行・新targetもない。

| 観測 | 空き bytes |
|---|---:|
| unlink直前 | 3932598272 |
| exact2 unlink直後 | 4416729088 |
| 後続postcheck | 4408860672 |

直後の差は484130816B。測定時は4GiB（4294967296B）を満たした。後続のディスク使用で空きは変わるため、Rootが再開前にactual freeを再確認する。scopeは追加していない。

[元receipt](a7-core-8d-unlink-receipt-20261102.json)を不変で保持し、[source HEAD差の補足を含む最終receipt](a7-core-8d-unlink-receipt-20261102.final.json)を別ファイルに保存した。最終SHAは `6b72ea1d877a2f88c759b4f790b0458de812c5df0cf275c7fbf06183dcaef5d6`。Rootへ一度伝えたpostcheck数値は実receiptの4408860672Bに訂正済み。

旧生成時のlibrary content SHA未記録という限定は維持する。この中間物解除は、過去試験やnative/credit資格の追加根拠ではない。
