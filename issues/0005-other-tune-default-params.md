# tune の結果に基づくデフォルトパラメータの決定

- Created: 2026-08-04
- Completed: {YYYY-MM-DD}
- Branch: feature/tune-default-params
- Polished: {YYYY-MM-DD}

## 目的

tune 実行ワークフローで tune を実行し、結果が溜まったら `layout-examples/compose-default.jsonc` のデフォルトパラメータを決定する。**tune 結果が十分に溜まるまでは保留**する。

## 現状

- 各エンコーダーの指定可能パラメーターは最新版へ追従済み
- `layout-examples/compose-default.jsonc` の既定値は hisui 2025.3.2 由来のままで、新規公開されたパラメーターの既定値は未検討
- `search-space-examples/full.jsonc` の探索空間は更新済み
- tune の入力となるダミー録画データは `generate-archive` で生成できる
- tune 実行ワークフロー (`tune.yml`) で tune を手動実行できる

## 設計方針

- tune 実行ワークフローで主要コーデック (VP9 / H.264 (openh264) / AV1 (svt-av1) / NVENC / VideoToolbox) の tune を実行し、パレートフロントを収集する
- パレートフロントの結果から、合成時間と VMAF 平均のバランスを考慮してデフォルトパラメータを決定する
  - 判断基準の例: 「VMAF 平均 90 以上で最も合成時間が短い解」など。実際の収集結果を見て基準を確定し、実施記録に残す
- 決定したデフォルトパラメータを `layout-examples/compose-default.jsonc` に反映する
- 既定値の変更は出力メディアの内容を変えるため、後方互換的な注意と目視確認を行う

## 完了条件

- 主要コーデックで `tune` を実行し、パレートフロントが収集されている (実施記録あり)
- 収集結果に基づいてデフォルトパラメータが決定され、`layout-examples/compose-default.jsonc` に反映されている (判断基準と採用した解を実施記録に残す)
- 既定値変更後の出力メディアが破損せず、主要コーデックで再生できることを目視確認した結果が実施記録に残されている
- `CHANGES.md` の `## develop` に既定値変更のエントリが追記されている

## 依存

- tune 実行ワークフローで tune 結果が溜まること。**結果が溜まるまでは保留**
