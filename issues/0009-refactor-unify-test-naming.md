# テストファイル・テスト関数の命名規則を統一する

- Created: 2026-08-04
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-unify-test-naming
- Polished: {YYYY-MM-DD}

## 目的

`tests/` 配下のファイル名とテスト関数名の命名規則が 3 系統混在しているのを 1 つの規約に統一する。

## 現状

- ファイル名の混在:
  - `e2e.rs` (サフィックスなし)
  - `reader_webm_test.rs` / `layout_test.rs` / `mixer_audio_test.rs` / `mixer_video_test.rs` (`_test`)
  - `decoder_tests.rs` / `writer_mp4_tests.rs` (`_tests` 複数形)
  - `test_encoder_svt_av1_params.rs` (`test_` プレフィックス)
- 関数名の混在:
  - `src/layout.rs` 内部テストの `test_merge_overlapping_sources_*` 接頭辞
  - `tests/layout_test.rs` の `decide_grid_dimensions_works` の `_works` 接尾辞
  - 他は無装飾

## 設計方針

- 1 つの規約 (例: `*_test.rs` + 接頭辞なし) へ統一する
- 既存の他プロジェクト (hisui 等) の規約があれば合わせる

## 完了条件

- `tests/` 配下のファイル名が 1 つの命名規則で統一されている
- テスト関数名の接頭辞・接尾辞が統一されている
- `cargo test --workspace` が全 pass する
