# RFC07: selected approval を持つ最初の source の接続

状態: **UNCOMPILED / UNEXECUTED。新契約と抽選 mechanics の source 境界。native authority・receipt・beacon consumer は未接続。公開 activation や数値の承認ではない。**

## 現状の停止条件

旧 `FinalPathV1::PanelIndependent` は「Panel に依存しない Final」を意味し、OPV の window expiry から作られていた。恒久方針により、expiry や local replay は抽選検証者の承認を代替できないため、この event 生成は停止した。一方、現 V1/V3 collector は `PanelAssignment` に対して `PanelLicensed` を一律に拒否する。この二つを維持したまま、最初の paid/onboarding source が自然発生するとは扱わない。

承認済み source を旧 OPV path へ再ラベルする変更や、collector の circularity guard を削除する変更では解消しない。新しい version の bootstrap provenance と selected-approval source path/policy が必要である。RFC0016/0017/ADR の実装は対象外。

モデル単位の審査・資格・昇格を source の条件に追加しない。モデル二階層は凍結した [PRINCIPLES §9](../../PRINCIPLES.md#9-モデルの二階層とモデル審査の不採用2026-10-15ユーザー凍結前提) に従う。ここで扱う承認は全階層に必要な claim 単位の認証であり、登録モデルの 0.3 / 1.0 を変更しない。

## 新契約の境界

`misaka/palw/challenge/src/bootstrap_epoch_v1.rs` に独立した epoch 契約を置く。現時点で `Verified` な authority を返す関数、receipt の承認、source eligibility の変更は提供しない。

契約は genesis/ruleset/policy、凍結 population root、native anchor、その固定位置、epoch と有効期間を束縛する。人口を凍結した位置より後に anchor があり、anchor より後に epoch が始まる。native consumer は数値の大小だけでなく、これらが own canonical chain の実 checkpoint/registry/locked stake であることを検査しなければならない。caller が渡した header/hash/roster をそのまま authority としない。

最初の entropy の候補は、**当該 bootstrap cohort の仕事で採用が決まらない native anchor** とする。native PoW header を使う場合も、hash があることは予測不能性や偏りの小ささを示さない。採掘者の withholding/grinding、fork 選択、利用可能な hashpower、再試行の費用を明示して後続検証する。循環を除いた anchor を own state で認証できなければ `Unavailable` のままにする。LLM 計算の暗号学的証明は使用しない。

## 抽選と相関

cohort は epoch ごとに一度決まり、仕事・claim・retry の ID は抽選入力に入らない。人口は native bond ID 順の正規形で、各 weight は own frozen locked stake から算定する。operator ID や異なる bond を独立した運営主体の証拠としない。

slot ごとに総 weight 上の整数 ticket を取り、同じ検証者を再び選ぶことを許す。これは stake 重みを持つ voting slot であり、異なる人や独立した再計算の数ではない。鍵を分割しても総 ticket mass は増えない。ただし、鍵分割後に同じ seed が同じ signer を返すという不変性は主張しない。population は entropy より前に固定する。

modulo の偏りを避けるため整数 rejection を用い、attempt 上限を使い切った場合は固定 epoch の抽選を拒否する。別 anchor/claim ID に切り替えて都合のよい cohort を採用しない。これは宣言した entropy 仮定の下での抽選であり、entropy の仮定自体の検証ではない。

同じ cohort の全仕事に対する不正通過は相関する。`maximum_source_works` を native admission で消費し、一回の cohort compromise を各仕事で新しい独立試行のように計算しない。期限までに承認を得られなくても自動で初期 cohort を再抽選しない。

現 source は選択結果 Vec の実型・最大 slot 数から予約量を導き、allocation 前の callback が失敗した場合は capacity error を保持する。population の encoded Vec や program/class population は複製しない。選択結果は予約 owner を保持する。これは logical request の境界であり、physical RAM/worker/DB の完成を示さない。

## 接続する native consumer

以下が揃うまで bootstrap を有効化しない。

1. version/fence と native epoch row。public default は None。own canonical anchor、凍結前の registry/locked-weight root、政策の一致を認証する。
2. epoch を初回に固定する journal、source work 件数 cap、期限、reorg/restore/import の同一性。後から違う roster/anchor を同じ epoch と扱わない。
3. work の実 commitment、material scope、complete checker と source profile の固定。claim を変えて cohort を選び直させない。
4. signer/slot/epoch/work/statement/期限を束縛する承認 receipt。slot の再利用を拒否し、同じ signer の複数 slot を独立者の証明としない。報酬を重複発生させない。
5. own accepted quorum と historical settlement からだけ作る新 source event。旧 `PanelIndependent`/`PanelLicensed` の wire/ID を変更しない。
6. consumer から source・cohort・anchor への依存関係を認証する新 collector/policy。対象 profile やその依存先が source approval の seed/validity を決める cycle を拒否する。
7. 実 source 生成 → checker receipt → native quorum → beacon → 後続 assignment の private worker/harness。bootstrap を経ず synthetic Final を投入して代用しない。

## 後続検証

source control は canonical Borsh/hash framing、異なる network/policy/population、固定 epoch の再現、rejection 上限、allocation 拒否、owner 寿命、鍵分割時の coalition ticket mass を対象とする。すべて未実行。

正直な locked-weight の割合、cohort/slot 数、quorum、epoch/anchor/window、source 件数 cap、検閲・取得可能性、資本・報酬・grinding の収支は未承認である。ユーザー指定どおり実装後の一括検証で決める。mechanics-only の test fixture 値を承認済み parameter に昇格しない。
