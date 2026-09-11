# migration_from_hisui_2025_3_3.md の内容を整理・修正する

- Created: 2026-08-03
- Completed: 2026-09-11
- Branch: feature/update-migration-doc
- Polished: {YYYY-MM-DD}
- Updated: 2026-09-09

## 目的

`docs/migration_from_hisui_2025_3_2.md` の比較元を Hisui 2025.3.3 に更新し、`docs/migration_from_hisui_2025_3_3.md` にリネームする。現在の内容は移行に必要な材料を列挙した叩き台にすぎないため、記述の正確さ・構成・過不足を確認して、移行ドキュメントとして利用できる水準に整理・修正する。

## 現状

- `docs/migration_from_hisui_2025_3_2.md` は Hisui 2025.3.2 から Sora Archive Compositor への移行差分を列挙している
- 現在の移行ドキュメントは移行先を Sora Archive Compositor 2026.1.0-canary.0、必要な Rust を 1.95 としている。移行先はリリース直前に更新する 2026.1.0 を前提とし、必要な Rust は現在の `Cargo.toml` に合わせて 1.98 に更新する必要がある
- 互換性の概要、最短の移行手順、CLI、環境変数、レイアウト JSONC、Cargo フィーチャー、FDK-AAC 等を並べた叩き台で、記載内容の精査と全体構成の整理がされていない
- レイアウト JSONC 節は「スキーマは同じ。`*_encode_params` 内の指定可能パラメーターは `shiguredo_*` crate 更新で変わった」と書いているが、追加・廃止・型変化の列挙が不足している
- 「`search-space-examples/full.jsonc` は上記のパラメーター変更に合わせて更新されている」という記述は、調査時点では古かった。探索空間の範囲ずれは `issues/closed/0005-other-tune-default-params.md` 側で直したので、移行ドキュメントではその後の実装に合わせて書き直すこと

### エンコードパラメーター差分 (0005 の調査、比較対象は Hisui 2025.3.3)

移行ドキュメントのレイアウト JSONC 節で書くべき差分。詳細な範囲は `docs/layout_encode_params.md` を参照する。

依存の世代差:

| crate | Hisui 2025.3.3 | SAC | upstream |
|---|---|---|---|
| `shiguredo_libvpx` | 2025.1.0 | 2026.2.0-canary.1 | libvpx v1.15.2 → v1.16.0 |
| `shiguredo_svt_av1` | 2025.1.0 | 2026.2.0 | SVT-AV1 v3.1.2 → v4.2.0 |
| `shiguredo_openh264` | 2025.1.0 | 2026.2.0 | OpenH264 v2.6.0 のまま |
| `shiguredo_video_toolbox` | 2025.1.0 | 2026.2.0-canary.2 | API 再構成 |
| `shiguredo_nvcodec` | 2026.2.0 | 2026.3.0-canary.0 | Video Codec SDK 13.0.19 のまま |

`compose-default.jsonc` の数値は Hisui 2025.3.3 由来のままである。Hisui 2025.3.2 と 2025.3.3 の `compose-default.jsonc` は同一である。廃止キーを消したことと、nvcodec decode に `reconfigure_enabled: false` を足したことが差分である。既定値の tune し直しはしない (0005)。

公開 JSON キーの変化:

- **libvpx**: 公開キーは実質そのまま
- **OpenH264**: `entropy_coding_mode` (`"cavlc"` / `"cabac"`) を追加。旧 `entropy_coding` (bool) は互換のため受け付ける
- **SVT-AV1**: 追加が多い (品質、レート制御、GOP、スーパーレゾリューション、QM、HDR など)。`rate_control_mode` に `"cqp_or_crf"` が加わった
- **SVT-AV1 廃止** (指定すると unknown 警告、無視): `pred_structure` / `pin_threads` / `target_socket` / `enable_tpl_la` / `force_key_frames` / `recon_enabled` / `encoder_bit_depth` / `encoder_color_format` / `profile` / `level` / `tier`
- **SVT-AV1 非互換値**: `intra_period_length: -1` はパースできない。無制限相当はキー省略
- **SVT-AV1 互換**: `enable_dlf_flag` / `enable_tf` / `fast_decode` / `enable_restoration_filtering` / `cdef_level` は旧真偽値も整数として受け付ける
- **Video Toolbox**: `data_rate_limits` を追加。`use_parallelization` は無視。JSON キー `prioritize_speed_over_quality` は維持
- **nvcodec encode**: Hisui 2025.3.3 から公開キーは増えていない
- **nvcodec decode**: `reconfigure_enabled` を追加 (既定 `false`)

Hisui 2025.3.3 は H.265 MP4 入力の `hvc1` サンプルエントリーに対応済みなので、`hvc1` 入力対応は移行差分から外す。H.265 MP4 出力のサンプルエントリーが `hev1` から `hvc1` に変わる点は移行差分として残す。

## 設計方針

- 各トピックの記述をソースコードの実装と照合し、誤り、古い情報、不足を修正する
- 比較元を Hisui 2025.3.3、移行先を Sora Archive Compositor 2026.1.0 とし、必要な Rust を 1.98 に更新する
- 移行ドキュメントを `docs/migration_from_hisui_2025_3_3.md` にリネームし、`README.md`、`docs/usage.md`、`skills/sora-archive-composer/SKILL.md` の参照を更新する
- 読者が移行手順として追えるように全体構成を整理する (概要 → 変更点 → 対応手順 の流れなど)
- 関連する他ドキュメント (`docs/build.md` / layout 系 / command 系) との記述の整合を確認する
- エンコードパラメーター節は上記の棚卸しを実装と照合して本文へ落とす。詳細な範囲・意味は `docs/layout_encode_params.md` へ誘導し、移行ドキュメント側は差分と非互換だけを書く
- `compose-default.jsonc` の数値は Hisui 2025.3.3 由来のままであること、tune で作り直していないことを書く
- ビルド済みバイナリと Docker イメージの案内は `issues/0001-add-dockerfile-and-canary.md` および `issues/0003-add-release-workflow.md` の実装後、crates.io の案内は `issues/0010-other-prepare-cratesio-publish.md` の方針確定後に、リリース時点の提供状況と照合する

## 完了条件

- 記述がソースコードの実装と一致している (誤った情報が残っていない)
- `docs/migration_from_hisui_2025_3_3.md` へのリネームと、リポジトリ内の参照更新が完了している
- 比較元が Hisui 2025.3.3、移行先が Sora Archive Compositor 2026.1.0、必要な Rust が 1.98 になっており、移行先の版数に Canary 表記が残っていない
- 移行作業を進める読者が手順として追える構成になっている
- `hvc1` 入力対応が移行差分から外され、H.265 MP4 出力の `hev1` から `hvc1` への変更は記載されている
- エンコードパラメーターの追加、廃止、リネーム、型変化、`intra_period_length: -1` の非互換が本文に書かれている
- 探索空間についての記述が、直した後の `search-space-examples/full.jsonc` と矛盾していない
- ビルド済みバイナリ、Docker イメージ、crates.io の案内がリリース時点の提供状況および関連ドキュメントと一致している

## 解決方法

移行ドキュメントを `docs/migration_from_hisui_2025_3_3.md` にリネームし、リポジトリ内の参照を更新した。
内容は `compose` コマンドの移行を中心に再構成し、バイナリ名と環境変数の変更、FDK-AAC の利用方法、H.265 の MP4 出力形式、ログ形式、エンコードおよびデコードパラメーターの主な差分を記載した。

詳細なパラメーター一覧は既存のパラメータードキュメントへ委ね、通常の移行に不要な開発用コマンド、隠し設定、ビルド設定、配布物の説明は対象外とした。
Video Toolbox の `allow_frame_reordering` は Hisui と同様に指定値を無視して常に無効とし、移行差分として扱わないように実装とドキュメントを揃えた。

Hisui 2025.3.3 と現在の実装を照合し、移行時に利用者が確認すべき機能差分に内容を整理できたため closed にする。
