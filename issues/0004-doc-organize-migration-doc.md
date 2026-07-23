# migration_from_hisui_2025_3_2.md の内容を整理・修正する

- Created: 2026-08-03
- Completed: {YYYY-MM-DD}
- Branch: feature/update-migration-doc
- Polished: {YYYY-MM-DD}

## 目的

`docs/migration_from_hisui_2025_3_2.md` の現在の内容は、移行に必要な材料を列挙した叩き台にすぎない。記述の正確さ・構成・過不足を確認して、移行ドキュメントとして利用できる水準に整理・修正する。

## 現状

- `docs/migration_from_hisui_2025_3_2.md` は 308 行で、Hisui 2025.3.2 から Sora Archive Compositor への移行差分を列挙している
- 互換性の概要・最短の移行手順・CLI の変更点・環境変数の変更点・レイアウト JSONC・Cargo フィーチャー・FDK-AAC 等のトピックを並べた叩き台の状態で、記載内容の精査と全体構成の整理がされていない

## 設計方針

- 各トピックの記述をソースコードの実装と照合し、誤り・古い情報・不足を修正する
- 読者が「移行手順」として追えるように全体構成を整理する (概要 → 変更点 → 対応手順 の流れなど)
- 関連する他ドキュメント (build.md / layout 系 / command 系) との記述の整合を確認する

## 完了条件

- 記述がソースコードの実装と一致している (誤った情報が残っていない)
- 移行作業を進める読者が手順として追える構成になっている
