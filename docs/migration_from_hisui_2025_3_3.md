# Hisui 2025.3.3 からのマイグレーションガイド

Sora Archive Compositor は [Hisui のバージョン 2025.3.3](https://github.com/shiguredo/hisui/releases/tag/2025.3.3) から派生したツールです。

Sora Archive Compositor 2026.1.0 は、Hisui 2025.3.3 とほぼ互換のインターフェースを提供しているため、
基本的には、コマンドのバイナリを置き換えるだけで、そのまま利用できます。

ただし、バイナリ名の変更や、依存ライブラリの更新に伴う非互換な変更もいくつかあるため、
このドキュメントでは、移行時に対応が必要となる可能性がある差分についてを説明します。

## 注意

このドキュメントは、Hisui 2025.3.3 と Sora Archive Compositor 2026.1.0 の差分をもとに記載しています。
Sora Archive Compositor 2026.1.0 より新しいバージョンの変更については [`CHANGES.md`](../CHANGES.md) を参照してください。

## 録画合成コマンド（`compose`）の移行方法

バイナリ名は `hisui` から `sora-archive-compositor` に変わりました。
コマンドラインやスクリプトでは、バイナリ名を次のように置き換えてください。

```console
# Hisui
$ hisui compose /path/to/archive/RECORDING_ID/

# Sora Archive Compositor
$ sora-archive-compositor compose /path/to/archive/RECORDING_ID/
```

`compose` コマンドで利用する環境変数は、接頭辞が `HISUI_*` から `SORA_ARCHIVE_COMPOSITOR_*` に変わりました。
以下の環境変数は、新しい名前に変更する必要があります。

- HISUI_LAYOUT_FILE_PATH (SORA_ARCHIVE_COMPOSITOR_LAYOUT_FILE_PATH に変更)
- HISUI_OPENH264_PATH (SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH に変更)
- HISUI_THREAD_COUNT (SORA_ARCHIVE_COMPOSITOR_THREAD_COUNT に変更)

## FDK-AAC の利用方法

### Ubuntu 向けビルド済みバイナリでの FDK-AAC の扱い

Hisui の Ubuntu 向けビルド済みバイナリでは、`fdk-aac` feature が無効になっていました。
そのため、FDK-AAC を利用するには、`fdk-aac` feature を有効にして自前でビルドする必要がありました。

一方、Sora Archive Compositor の Ubuntu 向けビルド済みバイナリでは、`fdk-aac` feature が有効になっているため、自前ビルドは不要です。
ただし、FDK-AAC の共有ライブラリ自体は同梱されていないため、別途インストールしてください。

### FDK-AAC の共有ライブラリを読み込む方法の変更

Hisui では、`fdk-aac` feature を指定してビルドすると、システムの FDK-AAC 共有ライブラリが自動で読み込まれました。

Sora Archive Compositor では、次のいずれかの方法で共有ライブラリのパスを明示的に指定してください。

- `--fdk-aac` オプション
- `SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH` 環境変数

## H.265 の MP4 出力形式

合成結果を H.265 でエンコードして MP4 に出力する場合に、MP4 内で H.265 映像を表す形式が `hev1` から `hvc1` に変わりました。
H.265 映像はどちらの形式でも表現できますが、Apple 系のプレイヤーでは `hvc1` しかサポートしていないことが多いため、互換性を高めるための変更です。

## ログ形式

ログ出力のフォーマットには互換性はありません。

具体的には、ログメッセージの時刻表記が「プロセス起動後の経過秒」から「ISO 8601 UTC のマイクロ秒精度の絶対時刻」に変わりました。
また、メッセージに含まれる Rust のモジュールパスの接頭辞も、`hisui` から `sora_archive_compositor` に変わりました。

```text
# Hisui
0.123456 [WARN] hisui::module - message

# Sora Archive Compositor
2026-07-30T12:34:56.123456Z [WARN] sora_archive_compositor::module - message
```

標準エラー出力が端末の場合は、ログ行がログレベルに応じた ANSI 色で表示されるようにもなりました。
この機能は、環境変数 `NO_COLOR` を設定すると無効にできます。

```bash
NO_COLOR=1 sora-archive-compositor compose /path/to/archive/RECORDING_ID/
```

## エンコードパラメーターとデコードパラメーター

依存ライブラリの更新に伴い、エンコーダーおよびデコーダーで利用可能なパラメーターにも変更があります。

Hisui でデフォルトのパラメーターを用いていた場合、移行のための設定変更は不要です。
レイアウト JSONC で `*_encode_params` または `*_decode_params` を個別に指定している場合は、以下の追加と廃止を確認してください。

主な追加項目は以下のとおりです。

- OpenH264 の `entropy_coding_mode`
- SVT-AV1 のエンコードパラメーター多数 (59 個)
- Video Toolbox の `data_rate_limits`
- nvcodec デコーダーの `reconfigure_enabled`

SVT-AV1 の追加項目は数が多いため、このドキュメントでは個別に列挙していません。
SVT-AV1 やそれ以外のパラメーターの詳細については、[エンコードパラメーター](layout_encode_params.md) と [デコードパラメーター](layout_decode_params.md) を参照してください。

廃止された項目は以下のとおりです。

- SVT-AV1 の `pred_structure`、`pin_threads`、`target_socket`、`enable_tpl_la`、`force_key_frames`、`recon_enabled`、`encoder_bit_depth`、`encoder_color_format`、`profile`、`level`、`tier`
- Video Toolbox の `use_parallelization` と H.264 用の `allow_open_gop`

SVT-AV1 の `encoder_color_format` を指定していた場合は、`color_format` に置き換え、値を `"i420"` または `"i42010"` に変更してください。
また、`intra_period_length` に `-1` は指定できないため、1 以上の値に変更してください。

廃止されたパラメーターを指定すると、その指定は無視され、警告ログが出力されます。
