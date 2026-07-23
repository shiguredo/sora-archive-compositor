# リリース用 GitHub Actions ワークフロー (release.yml) を追加する

- Created: 2026-08-03
- Completed: {YYYY-MM-DD}
- Branch: feature/add-release-workflow
- Polished: {YYYY-MM-DD}

## 目的

ビルド済みバイナリの公開経路を作る。タグ push で GitHub Release を作成し、Ubuntu / macOS 向けバイナリを添付する。

## 現状

- `.github/workflows/release.yml` を追加済み (実装中の検証待ち)
- タグ push で GitHub Release 作成 → Ubuntu / macOS バイナリのアップロード → Slack 通知
- Docker イメージのジョブは未追加 (0001 の Dockerfile 移植後)
- crates.io publish ジョブは未追加 (`publish = false` / 0010 の方針決定後)

## 設計方針

- hisui / sora-rust-sdk / raw-player-rs の `release.yml` を参考にする
  - sora-rust-sdk: `ubuntu-slim` での Release 作成、Ubuntu 24.04 / 26.04 × x86_64 / arm64 の matrix、`fail-fast: false`、`gh release upload --clobber`、workflow レベルの `permissions`
  - hisui: x86_64 での CUDA + nvcodec、tar.gz パッケージング、macOS バイナリ、Slack 通知
- タグ push をトリガーとする
- ubuntu ビルドの feature 構成:
  - ubuntu x86_64: `--features nvcodec,fdk-aac`
  - ubuntu arm64: `--features fdk-aac`
  - macOS: デフォルト構成 (Audio Toolbox / Video Toolbox)
- 対象 OS は README の動作環境に合わせる (Ubuntu 26.04 / 24.04、macOS 26 / 15)
- Docker イメージのジョブは 0001 の Dockerfile 移植後に追加する
- Slack 通知は ci.yml と同様 (`sora-tools` / `failure_and_fixed`)

## 依存

- 0001 (Dockerfile 移植) 完了後に Docker イメージジョブを追加する
- ubuntu バイナリの feature 構成は上記の設計方針に合わせること。docs と release.yml でずれが起きないよう相互に参照する

## 完了条件

- タグ push で GitHub Release が作成される
- ubuntu / macOS のビルド済みバイナリがリリースにアップロードされる
- ubuntu バイナリで fdk-aac が有効になっている (x86_64 は nvcodec も有効)
- ビルド構成が上記の設計方針と一致している (docs と release.yml のずれがない)

## 実装メモ

- Ubuntu x86_64 の CUDA は `setup-cuda-toolkit` で導入する (24.04 / 26.04 とも `13.3.1`)
- fdk-aac の共有ライブラリは実行時ロードのため、リリース資産には同梱しない
