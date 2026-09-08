# テストファイル・テスト関数の命名規則を統一する

- Created: 2026-08-04
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-unify-test-naming
- Polished: 2026-09-08

## 目的

`tests/` 配下のファイル名と、テスト関数名（`tests/` および `src/` 内 `#[cfg(test)]`）の装飾が複数系統混在しているのを、`shiguredo-rust` の命名規約に合わせて 1 つに統一する。

## 現状

### ファイル名（`tests/`）

4 系統が混在している。

| 系統 | 現行ファイル |
|---|---|
| サフィックスなし（モジュール非対応） | `e2e.rs` |
| `*_test.rs` | `layout_test.rs`, `mixer_audio_test.rs`, `mixer_video_test.rs`, `reader_webm_test.rs` |
| `*_tests.rs`（複数形） | `decoder_tests.rs`, `writer_mp4_tests.rs` |
| `test_*.rs`（`shiguredo-rust` 準拠側） | `test_arg_utils.rs`, `test_encoder_svt_av1_params.rs`, `test_scheduler.rs`, `test_video.rs`, `test_video_sample_entry_from_frame.rs` |

hisui の `tests/` も `test_*` / `*_tests.rs` / `e2e.rs` が混在しており、単一の他プロジェクト規約には頼れない。本リポジトリでは `shiguredo-rust` を正とする。

### テスト関数名の装飾

無装飾が大半だが、次が残っている。

- `test_` 接頭辞: `src/layout.rs` の `test_merge_overlapping_sources_*` / `test_duplicate_region_name`、`src/json.rs` の `test_parse_*`、`tests/e2e.rs` のヘルパー `test_simple_single_source_common`
- `_works` 接尾辞: `tests/layout_test.rs` の `decide_grid_dimensions_works` / `decide_required_cells_works` / `assign_sources_works`
- `_test` 接尾辞: `tests/reader_webm_test.rs` の `webm_audio_reader_test` / `webm_video_reader_test`、`tests/decoder_tests.rs` の `single_track_resolution_change_nvcodec_test`

同ファイル内でも装飾ありと無装飾が同居する（例: `src/layout.rs` の `load_layout_jsons` は無装飾）。

## 設計方針

### 採用する規約（確定）

`shiguredo-rust` に従う。hisui の現状や本文の旧例（`*_test.rs`）には合わせない。

1. **モジュールに対応する単体テストファイル**は `tests/test_<module>.rs` とする（`src/<module>.rs` に対応）
2. **特定モジュールに対応しないテスト**（例: `e2e.rs`）には `test_` / `prop_` プレフィックスを付けない
3. **テスト関数名**は英語・無装飾とする（`test_` 接頭辞、`_works` / `_test` 接尾辞を付けない）
4. テスト専用ヘルパーも同じ無装飾ルールに揃える（例: `test_simple_single_source_common` → `simple_single_source_common`）

### 変更対象

- `tests/` 直下の `*.rs` ファイル名のリネーム（上記規約に合わせる）
- `tests/**` および `src/**` の `#[cfg(test)]` 内にある、装飾付きの `#[test]` 関数名・テスト専用ヘルパー名のリネーム

### リネーム対応（ファイル）

| 現行 | 変更後 |
|---|---|
| `layout_test.rs` | `test_layout.rs` |
| `mixer_audio_test.rs` | `test_mixer_audio.rs` |
| `mixer_video_test.rs` | `test_mixer_video.rs` |
| `reader_webm_test.rs` | `test_reader_webm.rs` |
| `decoder_tests.rs` | `test_decoder.rs` |
| `writer_mp4_tests.rs` | `test_writer_mp4.rs` |
| `e2e.rs` | 変更なし（モジュール非対応） |
| `test_arg_utils.rs` ほか既存の `test_*.rs` | 変更なし（既に準拠） |

### 本 issue でやらないこと

- テスト内容・アサート・フィクスチャの追加や変更（リネームに伴う参照更新のみ）
- `pbt/` / `fuzz/` の新設や命名整備（現状ディレクトリが無い。必要なら別 issue）
- テストの役割分担（PBT 化など）の見直し

## 完了条件

- `tests/` 直下のファイル名が上記の採用規約に従っている（`*_test.rs` / `*_tests.rs` が残っていない）
- `tests/` および `src/` の `#[cfg(test)]` から、`test_` 接頭辞・`_works` / `_test` 接尾辞付きのテスト関数名・テスト専用ヘルパー名が無くなっている
- `cargo test --workspace` が全 pass する
