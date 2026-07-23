# tune 系モジュール (tune / tune_storage / tune_rng) のテストを追加する

- Created: 2026-08-04
- Completed: {YYYY-MM-DD}
- Branch: feature/test-add-tune-tests
- Polished: {YYYY-MM-DD}

## 目的

`src/tune.rs` / `src/tune_storage.rs` / `src/tune_rng.rs` にテストを追加し、NSGA-II 最適化の進行管理・永続化・乱数生成の品質を保証する。

## 現状

- `src/tune_nsga2.rs` には 7 本の入念なテストがあるが、`tune.rs` / `tune_storage.rs` / `tune_rng.rs` には `#[cfg(test)]` が皆無
- テストされていないロジック:
  - `src/tune.rs`: `Tuner` の ask / tell / tell_fail / pending 管理、`get_best_trials` の更新検出
  - `src/tune_storage.rs`: `TrialRecord` の DisplayJson → TryFrom ラウンドトリップ、`load_trials` の「最終行のみ破損を許容して再開」ロジック、`LockGuard` の stale ロック奪取 (プロセス生死判定)
  - `src/tune_rng.rs`: `gen_range_i64` のモジュロバイアス回避・閉区間処理、rejection sampling
- 特に `load_trials` の最終行スキップは「追記中の異常終了」を正常系として扱う微妙な設計で、中間行破損との線引きがテストで固定されていない

## 設計方針

- プロジェクト規約に従い、PBT で実現できるものは PBT で書く (`tune_rng` の分布の一様性・全域レンジ等)
- `tune_storage` は一時ファイルを使って load / append / ロックの回帰テストを書く
- モックやスタブは使わない

## 完了条件

- `Tuner` の ask / tell / tell_fail / pending 管理のテストがある
- `TrialRecord` の JSON ラウンドトリップのテストがある
- `load_trials` の最終行破損復旧・中間行破損エラーのテストがある
- `LockGuard` の stale ロック奪取 (PID が死んでいる / 生きている両ケース) のテストがある
- `gen_range_i64` の分布・境界のテストがある
- `cargo test --workspace` が全 pass する
