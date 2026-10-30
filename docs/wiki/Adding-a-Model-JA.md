# モデルを追加する(testnet-12 / 現行 main)

「既存の class を使う」ことと「新しい class をチェーンに追加する」ことは別の作業です。詳しい手順の正本は [palw-add-a-model-runbook.md](https://github.com/MISAKA-BTC/misakas/blob/main/docs/palw-add-a-model-runbook.md) です。

## Existing class

```bash
misaka --network testnet-12 model list
misaka --network testnet-12 palw registry
misaka --network testnet-12 model status --help
misaka --network testnet-12 mining setup
```

class ID、lifecycle の状態、ready seat、artifact root は、いまのチェーンが返す値を正とします。過去のネットワークの ID や fingerprint をコピーしないでください。

testnet-12 の genesis にある class:

- **Floor(BASE-0)**: artifact 不要。常に `Active`。
- **`Qwen/Qwen2.5-1.5B/graph-v7@8192`(8k)**: `.palwart` artifact(1,799,359,436 bytes、inventory root `88096dc1…`)と `.palwmanifest` sidecar が必要。seat には 3.5 GiB 以上の memory share が必要。
- **`Qwen/Qwen2.5-1.5B/graph-v7@2097152`(2M)**: 公開時点では規則で閉じている(`ClassDeadlineUnmeasured`)。

Qwen3.6 の hybrid 行(held map)は、登録と認証はできても、いまの build では attempt が prefill の位置 15 で必ず失敗します。そのため testnet-12 の genesis には入っていません。

## artifact を作る(8k の例)

```bash
cargo build --release --bin qwen25-convert --bin palw-class
qwen25-convert /path/to/Qwen2.5-1.5B-Instruct --a16 --n-ctx 8192 --out qwen25-1.5b-a16-8k.palwart
palw-class manifest --network testnet-12 qwen25-1.5b-a16-8k.palwart          # .palwmanifest sidecar を書く
palw-class manifest --network testnet-12 --check qwen25-1.5b-a16-8k.palwart  # 一致しなければ exit 1
```

変換元は `Qwen/Qwen2.5-1.5B-Instruct`(`model.safetensors` の SHA-256 `dd924a11b4c220f385b51ffa522daea7c9f3d850e31b162bb5661df483c6d3ee`)です。sidecar は artifact の隣に置いてください。root には、ファイル全体の digest と graph ごとの operand-inventory root の 2 種類があり、登録されるのは inventory root です。手で書き写さず、sidecar を使います。

## Catalog model

この build の catalog にあるモデルは、`misaka model add` が入口です。

```bash
misaka --network testnet-12 model add
misaka --network testnet-12 model add <catalog-model> --artifact /absolute/path/to/artifact
misaka --network testnet-12 model add --help
```

引数なしで実行すると catalog を表示します。モデル名を指定すると、登録、family drill、lane certification などを、チェーンの現在の状態から再開できる手順として実行します。catalog にない構成でも、ADR-0108 の class manifest 経路(`misaka model add --manifest <file>`)なら build を変えずに登録できます。

**動いている node の横で `kaspad --palw-register-class …` を起動しないでください。** この process は class が登録されても終了せず、同じ bond の 2 つ目の process になって slash の原因になります。`misaka model add` を使うか、動いている node をそのフラグ付きで起動し直してください。

## 登録した class の lifecycle

testnet-12 では permissionless の model registry が genesis から有効です。登録した class は、次の順に状態が進みます(各 span の境界で判定)。

| 状態 | claim を受け付けるか | 次に進む条件 |
|---|---|---|
| `Candidate`(登録された class) | いいえ | **admission audit** が jury を選ぶ。jury は floor の母集団から選ばれた operator 5 人で、そのうち過半数(3 人)が class を持っていれば `Prefetching` |
| `Prefetching`(genesis の行はここから) | いいえ | 別々の operator の seat 7 つ以上が、新しい possession 証明と十分な空き collateral を持つと `Probation` |
| `Probation` | derived admission の 50‰ | probe claim 10 本が失敗なしで `Final` になると `ActiveLimited` |
| `ActiveLimited` | 100‰ | lifecycle の安定した 3 step で `Active` |
| `Active` | 1,000‰ | — |
| `Held` | いいえ | ready seat が十分に戻り、過負荷でなくなると `Probation` |

- **admission audit は testnet-12 では 100 DAA ごと**(120 秒の cadence で約 3 時間 20 分)に行われます。audit の直後に登録した class は、seat がすぐ揃っても最長でこの期間待ちます。短くするフラグはありません(ADR-0147)。
- **登録した本人は自分の class を進められません**(ADR-0145 §7)。登録者**以外**の operator の seat が ready になる必要があるので、seat を他の人に頼めることが前提です。
- `Candidate` や `Prefetching` の class でも block は作れますが、その claim は work として拒否され、報酬も weight もありません。producer は inference の前にこれを確認して止まります(`E-MODEL-NOT-ADMITTING`)。
- ready seat が 5 未満になるか、receipt window が過負荷になると `Held` に戻ります。

登録後の class は、catalog に載っていなくても chain-registered-class の仕組みで動きます。testnet-12 ではこれが常に on なので、フラグは要りません(`--palw-chain-classes` は testnet-12 では非推奨で、`=false` でも止まりません)。class は、その artifact を読み込んだ node でだけ実行されます。

## New model family

catalog にないアーキテクチャは、ファイルを置くだけでは class になりません。次が必要です。

1. 決定論的な整数の artifact 形式
2. canonical な graph/profile と、class ID の導出
3. 到達しうる kernel すべての裁定(adjudication)の網羅
4. artifact root、tokenizer、runtime の結び付け
5. conformance test と、独立した実装との差分試験
6. code review と PR
7. チェーン上での class の登録と lane certification

[model onboarding SDK](https://github.com/MISAKA-BTC/misakas/blob/main/docs/palw-model-onboarding-sdk.md) と、現行の `misaka model add --help` を優先してください。

## Production and verification

```bash
misaka --network testnet-12 mining setup --model <catalog-name-or-class-id> --artifact /path/to/artifact
misaka --network testnet-12 verifier setup --model <catalog-name-or-class-id> --artifact /path/to/artifact
```

producer と verifier は同じ artifact のバイト列を検証できなければなりません。artifact root や tokenizer の結び付けが違う場合は起動しません。`--palw-verify-class-manifest` を付けると、起動時にすべての sidecar を導出し直し、一致しなければ起動を拒否します(2M で artifact 1 つあたり約 135 秒、8k はそれより短い)。

## Collateral

必要な collateral は、class と現在の規則から CLI が導出します。testnet-12 では claim ごとに escrow + weight を予約するので、同時 1 本あたりの目安は Floor 約 6,402 MSK、8k 約 6,452 MSK、2M 約 125,888 MSK です。それでも Wiki に固定の額を書き写さず、`mining setup` と `bond status` の表示を使ってください。登録済みの Bond には collateral を追加できず、同じ key では再登録できません。
