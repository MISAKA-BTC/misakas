# 初回資料要求の署名・予約枠・有料受理行の接続

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

Studioの同一claimから公開受付・Panel検証・Finalを経て、当選時の権利消費と対応ブロック生成へ進む目標を継続する。[前段の初回要求予約](../initial-manifest-request-booking-2026-11-11/README.md)に、native受理準備と二行の専用writerを接続した。公開T12の受理や生成権消費を実測したものではない。

## 接続した処理

[受理準備](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/manifest_request.rs)はliteral paired TFP1のrooted link、受理済みoriginalとdrawから、要求者のbond・principal・正規順位を解決する。署名済みManifestRequestを既存のnative検証へ渡し、claim・Source・順位別物理scope・予約packet・実行DAA・費用を照合する。署名前共通termsとpaired originalのobligation一致、初回要求のreadinessと締切も確認する。

PMR1のfee handoffへ実carrier blockを追加した。accepting block、DAA、blue score、実txid、accepted ordinalとともに、native readerから得た要求のoriginを照合する。source blockを要求本文やHTTPから採用しない。新profileの完全な料金policyと供給義務・inspection scopeをnative履歴から解決するproduction callerは、まだ未接続である。料金callbackは内部policy用であり、wireの自己申告額を受理する入口ではない。

[有料受理行](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/manifest_paid.rs)をPALW aux **205**へ追加した。同じ空間の既存割当を調べ、旧table・wire IDを置き換えていない。固定738 bytes、keyは初回要求packet IDの64 bytes。claim・Source・物理scope・original・CaseId・origin・要求digest・charge purpose・金額を保持する。packet IDとcharge purposeは正規導出値と照合する。サービス予約表の供給記録は保持し、有料受理行が順位の一回使用を記録する。

[専用writer](../../../../consensus/core/src/palw_kernel_route_fold_v1.rs)はpending case（aux 199）と有料行（205）を同じcheckpointで保存する。key/encoding・map/index・二つのjournal枠を事前予約し、実feeの追加割当を最後の失敗可能な処理に置く。途中拒否は二行とjournalを戻す。同じ二行の再実行は追加chargeゼロ、pending行だけがある場合は有料受理へ昇格させず拒否する。205は汎用writerから書けない。

rollback用の全状態copyは試行中だけのnative leaseにした。失敗ごとに全状態copyの予約をbuilderへ残さない。ただし全状態copy自体を有界な局所undoへ置き換えること、失敗処理を含む本番費用・資源上限の確定は残る。外側のRound/bodyが後から拒否される場合には、受理行だけでなくfee handoffも同じ候補として破棄する必要がある。

205を固定幅wire preflight、shape検査、snapshotの相互参照検査、deltaのinsert-only検査へ接続した。forward insertはaccepting block/DAA/blueがdeltaの実行点と一致しなければ拒否する。元のinsertを逆適用するrollbackは許す。初回予約の保持下限にも199と205の両方を計上した。この固定body下限だけで索引・undo等の完全な費用を覆うとはしない。

## 回帰コードと検査範囲

[既存の実署名paired original/draw fixture](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original.rs)から受理準備を呼び、実UTXO差額からのcharge、署名改変・未rooted companion・予約表欠落・費用不足の拒否を記述した。[writer回帰](../../../../consensus/core/src/palw_state_v2/tests/native_writers_v1.rs)は同一/再構築handoffの再実行、最後のfee割当失敗、旧pendingのみ、汎用writer拒否、delta forward/revert、snapshot復元を対象にする。

このfixtureの物理bookと料金は明示的な試験入力であり、新resource termsからbookをrooted保存するproduction coordinatorを通した試験ではない。署名前後のterms同値・予約失敗は別のpreclaim回帰コードで扱う。fixtureの存在を通し成功としない。Rust build/testは[作業計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)どおり未実行。整形・差分・文書参照検査のみ実行する。

## 次の接続と公開状態

PMR1を本番accepted native orderへ載せ、source block/txid/ordinalを実UTXO受理列から引き渡す必要がある。preview・observed・actualの有料受理結果を照合し、成功chargeをcollectible feeとRound body採否へ接続する。後続のcase継続・解決は205を再読込して実有料受理を確認し、未支払caseを資料取得・承認・Finalの代わりにしない。旧pendingの存在だけでも、未解決caseとしての成功保留は維持する。

完全TFO3 coordinator、native料金・供給policy、局所undo、後続処理・Panel承認・Final・生成権の各consumer、公開候補の通し検証は未完。今回は公開照会・送信・配備なし。最後の保存済み公開観測は[DAA21239のG14_ROUTE_ABSENT(FpTokenizer)](../selection-native-notice-2026-11-11/README.md)。同一claimの公開受付・当選時権利消費・対応blockは **NOT_PROVEN**。Minerのモデル全量配信やHF取得可否認定は追加していない。
