# A1: retained trained cohort native carrier test source

2026-11-02。native 登録の source freeze `0a5d835aa` の後続作業。
**3件の opt-in ignored test source であり、実行結果ではない。** A1 の Cargo / build / test /
target / cache / 大型 artifact 読取は HOLD。原 fixture や artifact の field を書き換えていない。

入力は `testing/positive-court-a1/inputs/hf3-native-registration-pins-20261102.json`。
Root が指定した過去 manifest と小さい登録 fixture から identity を抽出した。
manifest 自体、fixture、既存 inventory の SHA を pin するが、過去の CPU self-consistency や
class sizing を新しい native 登録成功の証拠として使わない。

| 固定モデル | 原 profile | 原 max_context |
| --- | --- | --- |
| HuggingFaceTB/SmolLM2-360M @ f8027fd0 | narrow tiled I32 | 8192 |
| HuggingFaceTB/SmolLM2-135M @ 93efa2f0 | narrow tiled I32 | 8192 |
| EleutherAI/pythia-70m @ a39f36b1 | wide Q24 I64 | 2048 |

3件とも checkpoint_interval=65536 / h_tile=64 を保持する。同じ private isolated court2 Params
で判定し、Smol2件は共通 selector の旧 v10 経路を通す。program / layout / tokenizer /
artifact root / class ID は pin と元 fixture の両方へ照合する。モデル入替え、再変換、context
短縮、clip / scale、court/work/fee/mass 上限の緩和は行わない。

## 実行すると検査する経路

1. 固定 cohort / fixture / prior inventory の identity を検査。欠損や空 case は失敗。
2. SDK が元 PALWTIR artifact を開き、実 inventory root を算出。別の streaming SHA256 pass
   で全ファイル identity を確認。元 program / layout / profile / context と一致照合。
3. 既存 private isolated genesis と full Params validation を使う。public arming-record の検証
   成功とは扱わない。genesis 割当で裏付けられた実 Active bond と fee-float UTXO を使う。
4. 実 chain の registration terms / DAA で SDK builder と選択された full gate を呼ぶ。
   既存 protocol の share0 を使い、正しい bond key で既存 tag61 message を ML-DSA 署名する。
   署名以外の元 class / canonical / target / slash / share field を変更しない。
5. 実 processor の object 検証で native 拒否文言を記録。通常 UTXO spend と input 署名を作り、
   署名後の compute / transient / storage mass と production relay rate に基づく fee を検査。
6. production 設定の MiningManager の標準検査・挿入、Standard template、carrier 収録、
   accepting block の accepted-transaction store、class / full schema1 row、bond burn / exposure、
   実 input 消費と change UTXO、registration status、aux141 origin を照合。
7. 別 node replay と同一 DB 再開で root / row / origin / UTXO と persisted delta root を照合。

genesis Active bond は post-genesis の新 bond 作成ではない。通常 carrier の原資・署名・受理を
純粋 gate に dummy field を渡すだけの確認と区別する。登録 status は weightless Registered
であり、wide claim/court の利用可否、credit、モデル単位の審査や昇格を意味しない。

## 実行待ちと結果の読み方

`pipeline::virtual_processor::tests::tir_wide_registration_e2e::hf3` の3テストはそれぞれ独立。
親の実行 grant 後、既存 Root test binary / build policy に従って1モデルずつ実行する。
テスト名は `retained_smollm2_360m_native_registration`、
`retained_smollm2_135m_native_registration`、`retained_pythia_70m_native_registration`。
ignored test を実行しなかった通常 test suite の成功を、この3件の成功として数えない。

stderr の `A1_HF3_STAGE` / `A1_HF3_RESULT` JSON を raw log ごと保存する。stage の error text /
SDK admission code を保持し、拒否したモデルの test は失敗する。以降の stage を実行して
いない場合は、未実行として読む。他モデルは別テストなので独立に診断できる。

資源費用は未実測。全 artifact SHA と SDK inventory の読取、SDK full admission、native
processor 診断、本 pipeline と replay による admission があり、pipeline が再評価する回数も
実測で記録する。3件を並列で走らせず、Root の占有 slot と制限に従う。test source は独自の
RSS hard limit、cold-cache 条件、artifact startup の事前メモリ認証を提供しない。
forward / token 実行、原最大位置の実行網羅、Panel / Node の資源測定、新 Assignment / P /
claim credit / public activation は範囲外。full gate が元値で拒否した場合はその原因を記録する。
