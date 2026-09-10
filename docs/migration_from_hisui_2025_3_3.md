# Hisui からのマイグレーションガイド

このドキュメントでは、以下のバージョン間の移行方法と相違点を説明します。

- 移行元：Recording Composition Tool Hisui 2025.3.3
- 移行先：Sora Archive Compositor 2026.1.0

Sora Archive Compositor 2026.1.0 の `compose` サブコマンドは、Hisui 2025.3.3 の `compose` サブコマンドとほぼ互換です。
基本的には、バイナリ名と環境変数名を置き換えることで移行できます。
このドキュメントでは、移行時に対応が必要な差分を説明します。

## 注意

このガイドは、Hisui 2025.3.3 と Sora Archive Compositor 2026.1.0 の差分をもとに記載しています。
Sora Archive Compositor 2026.1.0 より新しいバージョンの変更については [`CHANGES.md`](../CHANGES.md) を参照してください。

## `compose` サブコマンドへの移行方法

バイナリ名は `hisui` から `sora-archive-compositor` に変わりました。
コマンドラインやスクリプトでは、バイナリ名を次のように置き換えてください。

```console
# Hisui
$ hisui compose /path/to/archive/RECORDING_ID/

# Sora Archive Compositor
$ sora-archive-compositor compose /path/to/archive/RECORDING_ID/
```

`compose` サブコマンドで利用する環境変数は、接頭辞が `HISUI_*` から `SORA_ARCHIVE_COMPOSITOR_*` に変わりました。
古い環境変数名は Sora Archive Compositor では利用できません。

| Hisui | Sora Archive Compositor |
|---|---|
| `HISUI_LAYOUT_FILE_PATH` | `SORA_ARCHIVE_COMPOSITOR_LAYOUT_FILE_PATH` |
| `HISUI_OPENH264_PATH` | `SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH` |
| `HISUI_THREAD_COUNT` | `SORA_ARCHIVE_COMPOSITOR_THREAD_COUNT` |

FDK-AAC 用の `SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH` については、[FDK-AAC の利用方法](#fdk-aac-の利用方法) を参照してください。

## FDK-AAC の利用方法

FDK-AAC の共有ライブラリを読み込む方法が変わりました。

| Hisui | Sora Archive Compositor |
|---|---|
| `fdk-aac` feature を有効にして自前でビルド | Ubuntu 向けビルド済みバイナリで `fdk-aac` feature を有効化 |

Sora Archive Compositor の Ubuntu 向けビルド済みバイナリでは、FDK-AAC を利用するために自前でビルドする必要はありません。
ただし、FDK-AAC の共有ライブラリは同梱されないため、別途インストールしてください。
`compose` で FDK-AAC の AAC エンコードを利用するには、共有ライブラリのパスを指定する必要があります。

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

Sora Archive Compositor の `fdk-aac` feature は Ubuntu 向けです。
Hisui を macOS で `--features fdk-aac` によりビルドしていた場合は、デフォルト構成で自動的に有効になる Apple Audio Toolbox の AAC エンコードへ切り替えてください。
自前でビルドする場合の手順については、[FDK-AAC を使った AAC エンコードを有効にする場合](build.md#fdk-aac-を使った-aac-エンコードを有効にする場合) を参照してください。

## H.265 の MP4 出力

H.265 の MP4 出力に使用するサンプルエントリーは、`hev1` から `hvc1` に変わりました。
出力ファイルを処理するシステムがサンプルエントリーを判定している場合は、`hvc1` を受け入れるように変更してください。

## ログ形式

ログ 1 行の時刻表現は、プロセス起動後の経過秒から ISO 8601 UTC のマイクロ秒精度の絶対時刻に変わりました。
モジュールパスの接頭辞も、`hisui` から `sora_archive_compositor` に変わりました。

```text
# Hisui
0.123456 [WARN] hisui::module - message

# Sora Archive Compositor
2026-07-30T12:34:56.123456Z [WARN] sora_archive_compositor::module - message
```

標準エラー出力が端末の場合は、ログ行がログレベルに応じた ANSI 色で表示されます。
環境変数 `NO_COLOR` を設定すると色付けを無効にできます。

```bash
NO_COLOR=1 sora-archive-compositor compose /path/to/archive/RECORDING_ID/
```

## エンコードパラメーターとデコードパラメーター

デフォルトレイアウトを利用している場合は、移行に伴う対応は不要です。
レイアウト JSONC で `*_encode_params` または `*_decode_params` を個別に指定している場合は、以下の追加と廃止を確認してください。

主な追加項目は以下のとおりです。

- OpenH264 の `entropy_coding_mode`
- SVT-AV1 のエンコードパラメーター 59 個
- Video Toolbox の `data_rate_limits`
- nvcodec デコーダーの `reconfigure_enabled`

廃止された項目は以下のとおりです。

- SVT-AV1 の `pred_structure`、`pin_threads`、`target_socket`、`enable_tpl_la`、`force_key_frames`、`recon_enabled`、`encoder_bit_depth`、`encoder_color_format`、`profile`、`level`、`tier`
- Video Toolbox の `use_parallelization` と、H.264 での `allow_open_gop`

SVT-AV1 の `encoder_color_format` は `color_format` に置き換えてください。
廃止されたエンコードパラメーターを指定すると、その指定は無視され、警告ログが出力されます。

SVT-AV1 の追加項目は数が多いため、このガイドでは個別に列挙していません。
追加項目を含む個々のパラメーターについては、[エンコードパラメーター](layout_encode_params.md) と [デコードパラメーター](layout_decode_params.md) を参照してください。
