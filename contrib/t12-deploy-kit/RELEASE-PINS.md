# Testnet-12 release pins

The re-pin tool checks and updates these copies. The original release records are in Git at
[`d28cd043d`](https://github.com/MISAKA-BTC/LLM-chain/tree/d28cd043db55bd95dd4343b19700b5859fbf4685/docs/history/networks/testnet-12).

**cc5a7dd0 での暫定値（出荷値ではない）**

## 3. Fleet identity

```sh
EXPECT_FP=edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189
EXPECT_GENESIS=a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23
PREMINE_TXID=5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669
# schedule id 4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04（fence schedule "750, 1000"）
# rule manifest digest 9def81a1c56c02d5d1f9d24c5ddbe78d6f7598b31928ab6f388a2a074f14b4d4d4cf8c2245666bc13b9c91688c106efe428e8643075b68b40c4820adb4201d8d
```

## 4. Drill identity

drill salt `5353…53`（例示用。実際の drill の salt ではない）の drill genesis は `1678f353…70349`。

## 新 genesis

| Value | Pin |
| --- | --- |
| genesis hash | `a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23`（`f6cc9576…`、`d73dbf44…` を置き換え。`d73dbf44…` との違いは community 表の 17 行目だけ） |
| hash_merkle_root | `5ef04d1b9a6cb09e728a970d2e0b75a0142ecacc4a373c9b719e47d6a0d2058cb048fa240e13f1bbb094dd4e195919ea041db2ee338f21de34364f885317f805`（初回配備と同じ。coinbase marker `misaka-palw-t12` も同じ） |
| utxo commitment | `6ee51571a65164fb60dad8428e4a0f42584e8453bf6d029038459efe9279b265ad470ee5c42f2d90450ce37824418590b30fe36193cc37d2038fcf75ecb84eed` |
| premine txid | `5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669`（`premine_txid_for(testnet-12)`。index は従来どおり: collateral 0〜7、main wallet 40、fee float 41〜48） |
| community txid | `e3d638e58827755bdc78b362b499495c9607275b50a3ec2e1941f4ea75d38eda1c010f7796c637bd0b343d811a729291869c4658f36d6ca8d959baf8ed85064b`（`testnet12_community_txid()`、index 0〜16） |
