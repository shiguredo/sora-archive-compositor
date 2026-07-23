# crates.io 公開方針を決定し Cargo.toml の publish 設定を整備する

- Created: 2026-08-04
- Completed: {YYYY-MM-DD}
- Branch: feature/other-prepare-cratesio-publish
- Polished: {YYYY-MM-DD}

## 目的

OSS 公開に備えて、crates.io への公開の有無を決定し、Cargo.toml の publish 設定と公開メタデータを整備する。

## 現状

- Cargo.toml に `publish = false` が設定済み (公開方針の最終決定は未了)
- crates.io の `sora-archive-compositor` は未登録 (404)
- docs/build.md は `cargo install sora-archive-compositor` を案内しているが、現時点で実行不可能
- `exclude = ["docs/**", "testdata/**"]` のため、publish すると issues/ 全ファイルがクレート tarball に同梱される
- `description = "Sora Archive Compositor"` の 1 語のみで、keywords / categories / homepage 等のメタデータが未整備

## 設計方針

- 公開直前まで着手しない。今は crates.io 公開の有無を決めない
- crates.io に公開しない場合は `publish = false` を設定する
- 公開する場合は以下を整備する
  - `exclude` に `issues/**` (および AGENTS.md / CLAUDE.md 等) を追加
  - description / keywords / categories を整備
  - docs/build.md の `cargo install` 手順が実行可能になるタイミングと整合させる

## 完了条件

- crates.io 公開の有無が決定され、Cargo.toml に `publish = false` または公開用メタデータが設定されている
- publish 時に issues/ が tarball に同梱されない
- docs/build.md のインストール手順が現実と整合している
