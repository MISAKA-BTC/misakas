# 全モデル共通の管理

[RFC-0007](../../rfc/0007-bft-model-admission.md)と[RFC-0016](../../rfc/0016-position-market-and-rewards.md)を適用する。

全モデルの係数は1.0。originは認証された由来として保存し、genesis・後日追加・旧forkに経済上の特別区分を設けない。モデルごとに一意な市場を持ち、BFTの管理・停止・正規解除・廃止の対象とする。genesisの差はcreator不在である。

新規モデル追加はBFT YES/NOだけ。kernel互換判定、0.3での無審査採用、年次forkだけの1.0昇格を入口にしない。市場の存在は採掘資格でも個別claim認証でもない。

既受理claimの署名・元期限・責任を保ち、旧係数で確定した報酬を遡及再計算しない。新policyへの移行と公開有効化は別に定める。
