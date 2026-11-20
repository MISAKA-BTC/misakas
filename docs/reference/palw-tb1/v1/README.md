# PALW-TB1の上流参照fixture

[RFC0061](../../../rfc/0061-palw-internal-threshold-beacon.md) / [新profileの移行](../../../governance/palw-tb1-fow1-transition.md)。

添付のPython参照モデルを内容変更なしで収録する。完全BGW、PQ暗号、native合意、DB、holder選出、空batch、consumer別永続消費、追加SCHEDULE/ROLE/ADMISSION、FOW1関数定理を実装・証明するものではない。FIELD=2^127−1やdeterministic randomはfixture専用で、RFC0061のp=2^521−1を用いる本番suiteではない。実suiteへ接続してはならない。

`python3 test_beacon_model.py`は29件の参照検査とvectorを生成する。`python3 bgw_certificate_check.py`は別の有限体上の103,152ケースを検査する。二つを合算して暗号安全性試験件数にしない。元のREADMEはUPSTREAM_README.txt、原本全体は改訂記録のupstream ZIPへ保持した。
