# Hisui 2025.3.3 から Sora Archive Compositor への移行

このドキュメントでは、以下のバージョン間の移行方法を説明します。

- 移行元：Recording Composition Tool Hisui 2025.3.3
- 移行先：Sora Archive Compositor 2026.1.0

このドキュメントは上記のバージョン間の差分を記録したスナップショットです。
移行先より新しいバージョンの変更については [`CHANGES.md`](../CHANGES.md) を参照してください。

なお、[レガシー版 Hisui からのマイグレーションガイド](https://github.com/shiguredo/hisui/blob/2025.3.3/docs/migrate_hisui_legacy.md) は、C++ 版のレガシー Hisui から Rust 版 Hisui への移行を対象とした別のドキュメントです。

## 互換性の概要

Sora Archive Compositor は、Hisui 2025.3.3 の Sora 録画合成機能を引き継いでいます。
以下の 5 つのサブコマンドは引き続き利用できます。

- `compose`
- `inspect`
- `list-codecs`
- `tune`
- `vmaf`

レイアウト JSONC の基本構造、`compose`、`inspect`、`list-codecs` の標準出力 JSON、`compose` の統計情報 JSON は互換です。
一方で、移行時には主に以下の変更への対応が必要です。

- バイナリ名と環境変数名の変更
- `tune` のオプション、試行回数の意味、探索履歴形式の変更
- `vmaf` のオプションと出力項目の削除
- FDK-AAC 共有ライブラリの指定方法の変更
- Cargo フィーチャーと Rust の最小サポートバージョンの変更
- H.265 の MP4 出力に使用するサンプルエントリーの変更
- ログの時刻表現と色付けの変更
- `pipeline` の除外と配布方法の変更

## 最短の移行手順

1. `hisui` バイナリを `sora-archive-compositor` バイナリへ置き換える
2. コマンドラインやスクリプト内の `hisui` を `sora-archive-compositor` に置き換える
3. `HISUI_*` 環境変数を `SORA_ARCHIVE_COMPOSITOR_*` 環境変数へ置き換える
4. `tune` を利用している場合は、`--study-name` を `--name` に変更し、`--trial-count` には既存履歴を含む目標の合計試行回数を指定する
5. `tune` を利用している場合は、`optuna.db` を引き継がず、新しい JSON Lines 形式で探索を開始する
6. `vmaf` を利用している場合は、`--vmaf-output-file` の指定と `vmaf_output_file_path` の参照を削除する
7. FDK-AAC を利用している場合は、共有ライブラリのパスをコマンドライン引数または環境変数で指定する
8. Cargo でビルドしている場合は、Rust 1.98 以降を使い、`libvpx` フィーチャーの指定を外す
9. `pipeline` を利用している場合は、ワークフローを別のツールまたは独自実装へ移す
10. Docker イメージを利用している場合は、[配布物](#配布物) を確認し、Docker イメージが提供されるまではビルド済みバイナリまたは自前でビルドしたバイナリへ切り替える
11. H.265 の MP4、VMAF の JSON、ログを処理する連携先がある場合は、出力形式の変更に対応する

## バイナリ名とコマンド

プロジェクト名、Cargo パッケージ名、バイナリ名が変わりました。

| 項目 | Hisui 2025.3.3 | Sora Archive Compositor 2026.1.0 |
|---|---|---|
| プロジェクト名 | Recording Composition Tool Hisui | Sora Archive Compositor |
| Cargo パッケージ名 | `hisui` | `sora-archive-compositor` |
| バイナリ名 | `hisui` | `sora-archive-compositor` |
| バージョン表示 | `hisui 2025.3.3` | `sora-archive-compositor 2026.1.0` |
| リポジトリ | `github.com/shiguredo/hisui` | `github.com/shiguredo/sora-archive-compositor` |

たとえば、次のコマンドはバイナリ名だけを置き換えて実行できます。

```console
# Hisui 2025.3.3
$ hisui compose /path/to/archive/RECORDING_ID/

# Sora Archive Compositor
$ sora-archive-compositor compose /path/to/archive/RECORDING_ID/
```

`inspect`、`list-codecs`、`compose`、`vmaf`、`tune` のサブコマンド名は変わっていません。
変更されていないオプションについては、[関連ドキュメント](#関連ドキュメント) を参照してください。

また、Hisui 2025.3.3 にはないサブコマンドとして `generate-archive` が追加されています。
これはダミーの録画データを生成するコマンドで、実録画がなくても `compose` や `tune` を試すために利用できます。
詳細は [generate-archive コマンド](command_generate_archive.md) を参照してください。

## 環境変数

Hisui 2025.3.3 から引き継がれた環境変数は、接頭辞が `HISUI_*` から `SORA_ARCHIVE_COMPOSITOR_*` に変わりました。
古い環境変数名は Sora Archive Compositor では利用できません。

| Hisui 2025.3.3 | Sora Archive Compositor | 対象 |
|---|---|---|
| `HISUI_LAYOUT_FILE_PATH` | `SORA_ARCHIVE_COMPOSITOR_LAYOUT_FILE_PATH` | `compose`、`vmaf` |
| `HISUI_OPENH264_PATH` | `SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH` | `compose`、`inspect`、`list-codecs`、`tune`、`vmaf` |
| `HISUI_THREAD_COUNT` | `SORA_ARCHIVE_COMPOSITOR_THREAD_COUNT` | `compose` |
| `HISUI_SYNC_CHANNEL_SIZE` | `SORA_ARCHIVE_COMPOSITOR_SYNC_CHANNEL_SIZE` | `compose`、`inspect`、`vmaf` |

`HISUI_MAX_CPU_CORES` に対応する環境変数はありません。`--max-cpu-cores` とともに削除されました。
詳細は [`tune` の変更点](#tune-の変更点) と [`vmaf` の変更点](#vmaf-の変更点) を参照してください。

`SORA_ARCHIVE_COMPOSITOR_SYNC_CHANNEL_SIZE` は隠し設定で、デフォルト値は `10` です。
`inspect` では、`--decode` の有無にかかわらず利用されます。

FDK-AAC 用の `SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH` については、[FDK-AAC の利用方法](#fdk-aac-の利用方法) を参照してください。
ログの色付けを無効にする `NO_COLOR` については、[ログ形式](#ログ形式) を参照してください。

## `compose` の変更点

既存の `compose` オプションは、環境変数名を除いてそのまま利用できます。
FDK-AAC を有効にしたビルドでは、共有ライブラリのパスを指定する `--fdk-aac` オプションが追加されています。

標準出力 JSON と `--stats-file` で保存する統計情報 JSON のスキーマは互換です。
H.265 の MP4 出力については、[入力ファイルと出力ファイル](#入力ファイルと出力ファイル) を参照してください。

## `tune` の変更点

`tune` は外部の Optuna を使う方式から、Sora Archive Compositor に組み込まれた NSGA-II を使う方式へ変わりました。
Python と `optuna` 実行ファイルは不要です。

### オプションと試行回数

| 項目 | Hisui 2025.3.3 | Sora Archive Compositor |
|---|---|---|
| 探索名 | `--study-name` | `--name` |
| `--trial-count` の意味 | 今回追加する試行回数 | 既存履歴を含む目標の合計試行回数 |

たとえば、100 回の試行が完了した履歴に対して `--trial-count 150` を指定すると、追加で 50 回試行します。

`--max-cpu-cores` (`-c`) と環境変数 `HISUI_MAX_CPU_CORES` / `SORA_ARCHIVE_COMPOSITOR_MAX_CPU_CORES` はありません。指定すると未知オプションとして拒否されます。

### 探索履歴

探索履歴の保存形式とファイル名が変わりました。

| Hisui 2025.3.3 | Sora Archive Compositor |
|---|---|
| `<tune-working-dir>/optuna.db` | `<tune-working-dir>/<name>.jsonl` |
| Optuna の SQLite データベース | 1 トライアルを 1 行で記録する JSON Lines |

`optuna.db` は引き継げません。
移行後は JSON Lines 形式で探索を新たに開始してください。

探索中は多重起動を防ぐ `<name>.lock` も作成されます。
中断によってロックファイルが残った場合は、次回起動時に自動で回収されるため、手動で削除する必要はありません。
Hisui 2025.3.3 のデフォルト作業ディレクトリは `ROOT_DIR/hisui-tune/`、探索名は `hisui-tune` です。
Sora Archive Compositor のデフォルト作業ディレクトリは `ROOT_DIR/tune/`、探索名は `tune` です。
既存の `hisui-tune/` は自動では読みません。

各トライアルディレクトリには `vmaf-output.json` が作成されなくなりました。
`layout.jsonc` と `metrics.json` は引き続き作成され、評価用の `reference.yuv` と `distorted.yuv` は評価後に削除されます。

### ログと子プロセス

起動時の `INFO` ブロックに含まれるキー名が変わりました。

| Hisui 2025.3.3 | Sora Archive Compositor |
|---|---|
| `optuna storage:` | `trials file:` |
| `optuna study name:` | `name:` |
| `optuna trial count:` | `target total trials:` |

古いキー名を CI や監視で検出している場合は、新しいキー名に変更してください。
Optuna が標準エラー出力へ出力していたログ行も出力されません。
Optuna のログ行を成功条件として検出している場合は、その条件を削除してください。

各トライアルの評価では、実行中の Sora Archive Compositor バイナリから `vmaf` サブコマンドを起動します。
`PATH` 上に `hisui` バイナリを配置する必要はありません。

## `vmaf` の変更点

VMAF の計算は Sora Archive Compositor に組み込まれました。
外部の `vmaf` 実行ファイルは不要です。
外部の `vmaf` が出力していたバージョン情報などのログ行も出力されません。

以下のオプション、出力項目、生成物がなくなりました。

- `--vmaf-output-file` オプション
- `--max-cpu-cores` (`-c`) オプションと `HISUI_MAX_CPU_CORES` に対応する環境変数
- 標準出力 JSON の `vmaf_output_file_path`
- 中間生成物の `vmaf-output.json`

標準出力 JSON の以下の VMAF スコアは引き続き出力されます。

- `vmaf_min`
- `vmaf_max`
- `vmaf_mean`
- `vmaf_harmonic_mean`

`reference_yuv_file_path` と `distorted_yuv_file_path` も引き続き出力されます。

## FDK-AAC の利用方法

FDK-AAC の共有ライブラリを読み込む方法が変わりました。

| Hisui 2025.3.3 | Sora Archive Compositor |
|---|---|
| ビルド時にシステム標準パスの共有ライブラリへ動的リンク | 実行時にユーザーが指定したパスから共有ライブラリをロード |

Sora Archive Compositor を `--features fdk-aac` でビルドした場合でも、システム標準パスは探索されません。
`compose` で FDK-AAC の AAC エンコードを利用するには、共有ライブラリのパスを指定する必要があります。
`list-codecs` で FDK-AAC を利用可能なエンコーダーとして表示する場合も、共有ライブラリのパスを指定してください。

共有ライブラリのパスは、次のいずれかの方法で指定します。

- `--fdk-aac` オプション
- `SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH` 環境変数

```bash
sora-archive-compositor compose \
  --fdk-aac /path/to/libfdk-aac.so \
  /path/to/archive/RECORDING_ID/
```

```bash
SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH=/path/to/libfdk-aac.so \
  sora-archive-compositor compose /path/to/archive/RECORDING_ID/
```

Sora Archive Compositor の `fdk-aac` フィーチャーは Ubuntu 向けです。
Hisui 2025.3.3 を macOS で `--features fdk-aac` によりビルドしていた場合は、
デフォルト構成で自動的に有効になる Apple Audio Toolbox の AAC エンコードへ切り替えてください。
ビルド手順については [FDK-AAC を使った AAC エンコードを有効にする場合](build.md#fdk-aac-を使った-aac-エンコードを有効にする場合) を参照してください。

## レイアウト JSONC と探索設定

Hisui 2025.3.3 と Sora Archive Compositor では、レイアウト JSONC の外側のスキーマは同じです。
ただし、`*_encode_params` と `*_decode_params` で指定できるパラメーターには互換性のない変更があります。
既存のレイアウトを移行する場合は、以下の差分を確認してください。

指定可能なパラメーターの範囲と意味は、[エンコードパラメーター](layout_encode_params.md) と [デコードパラメーター](layout_decode_params.md) を参照してください。
Sora Archive Compositor は、未知のエンコードパラメーターを無視し、`ignored unknown ... encode param: ...` 形式の警告ログを出力します。

### libvpx と nvcodec のエンコードパラメーター

libvpx と nvcodec では、Hisui 2025.3.3 から公開 JSON キーは増減していません。

### OpenH264 のエンコードパラメーター

OpenH264 では、`entropy_coding` が `entropy_coding_mode` に変わり、値も真偽値から `"cavlc"` / `"cabac"` に変わりました。
互換性のため、従来の `entropy_coding` も引き続き利用できます。
`"entropy_coding": false` は `"entropy_coding_mode": "cavlc"`、`"entropy_coding": true` は `"entropy_coding_mode": "cabac"` と同じ意味です。

### SVT-AV1 のエンコードパラメーター

SVT-AV1 では、以下のパラメーターが追加されました。

- 品質と速度：`aq_mode` / `sharpness` / `rtc` / `tune` / `intra_refresh_type` / `screen_content_mode` / `lossless` / `ac_bias` / `level_of_parallelism`
- レート制御：`vbr_min_section_pct` / `vbr_max_section_pct` / `mbr_over_shoot_pct` / `recode_loop` / `starting_buffer_level_ms` / `optimal_buffer_level_ms` / `maximum_buffer_size_ms`
- GOP とフレーム構造：`sframe_dist` / `sframe_mode` / `sframe_qp` / `sframe_qp_offset` / `gop_constraint_rc` / `multiply_keyint`
- スーパーレゾリューションとリサイズ：`superres_mode` / `superres_denom` / `superres_kf_denom` / `superres_qthres` / `superres_kf_qthres` / `superres_auto_search_type` / `resize_mode` / `resize_denom` / `resize_kf_denom`
- フィルタリング：`tf_strength` / `enable_variance_boost` / `variance_boost_strength` / `variance_octile` / `variance_boost_curve` / `film_grain_denoise_apply` / `adaptive_film_grain`
- 量子化と高度な設定：`enable_qm` / `min_qm_level` / `max_qm_level` / `min_chroma_qm_level` / `max_chroma_qm_level` / `max_tx_size` / `enable_mfmv` / `enable_dg` / `avif` / `startup_mg_size` / `startup_qp_offset` / `luminance_qp_bias` / `qp_scale_compress_strength` / `extended_crf_qindex_offset`
- HDR：`color_primaries` / `transfer_characteristics` / `matrix_coefficients` / `color_range` / `chroma_sample_position` / `mastering_display` / `content_light_level`

以下のパラメーターは廃止されました。
指定しても無視され、警告ログが出力されます。

- `pred_structure`
- `pin_threads`
- `target_socket`
- `enable_tpl_la`
- `force_key_frames`
- `recon_enabled`
- `encoder_bit_depth`
- `encoder_color_format`
- `profile`
- `level`
- `tier`

`encoder_color_format` は単純に名称を置き換えられません。
新しい `color_format` を利用し、値も移行してください。
指定可能な値は `"i420"` / `"i42010"` です。

`enable_dlf_flag`、`enable_tf`、`fast_decode`、`enable_restoration_filtering` は真偽値から整数値に変わりました。
これらのパラメーターでは、従来の真偽値も互換性のために受け付けます。
`cdef_level` の整数型は `i8` から `i32` に変わり、真偽値も受け付けます。

`intra_period_length` に `-1` は指定できません。
1 以上の値を指定するか、省略してデフォルト値の `300` を利用してください。

### Video Toolbox のエンコードパラメーター

`data_rate_limits` が追加され、H.264 / H.265 のデータレート上限を指定できるようになりました。
`allow_frame_reordering` は、Hisui 2025.3.3 では設定に反映されませんでしたが、Sora Archive Compositor では H.264 / H.265 の設定に反映されます。
`use_parallelization` は廃止され、指定しても無視されます。
`allow_open_gop` は H.264 では廃止され、H.265 では引き続き利用できます。
内部 API では `prioritize_speed_over_quality` が名称変更されましたが、レイアウト JSONC では従来の `prioritize_speed_over_quality` を引き続き利用できます。

### nvcodec のデコードパラメーター

すべての nvcodec デコーダーに `reconfigure_enabled` が追加されました。
既定値は `false` です。

### デフォルトレイアウトと探索空間

`layout-examples/compose-default.jsonc` の数値は Hisui 2025.3.3 から変更していません。
エンコード / デコード設定には、廃止されたパラメーターの削除と nvcodec デコーダーへの `"reconfigure_enabled": false` の追加だけを反映しており、既定値のチューニングは実施していません。

Hisui 2025.3.3 の `tune` 用レイアウトと探索空間をそのまま引き継がず、Sora Archive Compositor の `layout-examples/` と `search-space-examples/full.jsonc` を基に移行してください。
Sora Archive Compositor の探索空間は、廃止されたパラメーターと指定できない値を除外し、探索対象に含めるパラメーターの範囲を現在の実装に合わせています。

レイアウト全体については [レイアウト機能](layout.md) と [レイアウト JSON の仕様](layout_spec.md) を参照してください。

## 入力ファイルと出力ファイル

Hisui 2025.3.3 と Sora Archive Compositor は、H.265 の MP4 入力で `hev1` と `hvc1` の両方を利用できます。

H.265 の MP4 出力に使用するサンプルエントリーは、`hev1` から `hvc1` に変わりました。
出力ファイルを処理するシステムがサンプルエントリーを判定している場合は、`hvc1` を受け入れるように変更してください。

`compose`、`inspect`、`list-codecs` の標準出力 JSON と `compose` の統計情報 JSON はスキーマ互換です。
ただし、`list-codecs` の `engines[].build_version` は依存クレートの更新に伴って数値が変わる場合があります。

`tune` と `vmaf` の生成物については、[`tune` の変更点](#tune-の変更点) と [`vmaf` の変更点](#vmaf-の変更点) を参照してください。

## Cargo フィーチャーとプラットフォーム

### Rust の最小サポートバージョン

ビルドに必要な Rust のバージョンは 1.90 から 1.98 に変わりました。
Rust 1.98 以降を利用してください。

### `libvpx` フィーチャー

Hisui 2025.3.3 では `libvpx` がデフォルトフィーチャーでした。
Sora Archive Compositor では `libvpx` フィーチャーがなくなり、`libvpx` が常に有効になりました。

- `--features libvpx` を明示していた場合は指定を外す
- `--no-default-features` で `libvpx` を無効にしていた場合は、無効化できなくなったためビルド構成を見直す

`--features libvpx` を残すと、存在しないフィーチャーの指定として Cargo のビルドが失敗します。

### `nvcodec` フィーチャー

`nvcodec` フィーチャーは同じ名前で引き続き利用できます。

### `fdk-aac` フィーチャー

`fdk-aac` フィーチャーは同じ名前で引き続き利用できますが、Sora Archive Compositor では Ubuntu 向けです。
共有ライブラリの指定方法については [FDK-AAC の利用方法](#fdk-aac-の利用方法) を参照してください。

## ログ形式

ログは引き続き標準エラー出力へ出力されます。
ログ 1 行の時刻表現は、プロセス起動後の経過秒から ISO 8601 UTC のマイクロ秒精度の絶対時刻に変わりました。
モジュールパスの接頭辞も、`hisui` から `sora_archive_compositor` に変わりました。

```text
# Hisui 2025.3.3
0.123456 [WARN] hisui::module - message

# Sora Archive Compositor
2026-07-30T12:34:56.123456Z [WARN] sora_archive_compositor::module - message
```

標準エラー出力が端末の場合は、ログ行がログレベルに応じた ANSI 色で表示されます。
環境変数 `NO_COLOR` を設定すると色付けを無効にできます。

```bash
NO_COLOR=1 sora-archive-compositor compose /path/to/archive/RECORDING_ID/
```

## 利用できない機能と配布物

### `pipeline` サブコマンド

Hisui 2025.3.3 の実験的な `pipeline` サブコマンドは、Sora Archive Compositor には移植されていません。
`pipeline-examples` と `plugin-examples` に含まれていたサンプルも利用できません。

`sora_source.py` と `sora_publish.py` は `pipeline` 用のプラグイン例であり、独立した CLI サブコマンドではありません。

### 配布物

このドキュメントの更新時点では、Sora Archive Compositor 2026.1.0 は未リリースです。
以下の配布物と経路は利用できません。

- GitHub Releases の 2026.1.0 向けビルド済みバイナリ
- `Dockerfile`
- `canary.py`
- `ghcr.io/shiguredo/sora-archive-compositor` の Docker イメージ
- crates.io の `sora-archive-compositor` パッケージ

2026.1.0 のリリースまでは、[ビルド方法](build.md) のリポジトリ指定またはローカルリポジトリの手順に従ってビルドしてください。
リリース後の提供状況は、[README](../README.md) と [GitHub Releases](https://github.com/shiguredo/sora-archive-compositor/releases) で確認してください。

## 関連ドキュメント

- [Sora Archive Compositor を利用してみる](usage.md)
- [ビルド方法](build.md)
- [`sora-archive-compositor compose` コマンド](command_compose.md)
- [`sora-archive-compositor generate-archive` コマンド](command_generate_archive.md)
- [`sora-archive-compositor inspect` コマンド（開発者向け）](command_inspect.md)
- [`sora-archive-compositor list-codecs` コマンド](command_list_codecs.md)
- [`sora-archive-compositor tune` コマンド](command_tune.md)
- [`sora-archive-compositor vmaf` コマンド](command_vmaf.md)
- [レイアウト機能](layout.md)
- [レイアウト JSON の仕様](layout_spec.md)
