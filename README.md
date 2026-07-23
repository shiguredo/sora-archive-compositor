# Sora Archive Compositor

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![GitHub Actions](https://github.com/shiguredo/sora-archive-compositor/actions/workflows/ci.yml/badge.svg)](https://github.com/shiguredo/sora-archive-compositor/actions/workflows/ci.yml)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white)](https://discord.gg/shiguredo)

## About Shiguredo's open source software

We will not respond to PRs or issues that have not been discussed on Discord. Also, Discord is only available in Japanese.

Please read <https://github.com/shiguredo/oss> before use.

## 時雨堂のオープンソースソフトウェアについて

利用前に <https://github.com/shiguredo/oss> をお読みください。

## 概要

Sora Archive Compositor は WebRTC SFU Sora 向けの録画合成ツールです。

Sora が出力した録画ファイル (MP4 または WebM) を合成し、単一の MP4 ファイルとして出力します。

もともと [Recording Composition Tool Hisui](https://github.com/shiguredo/hisui) の Sora 録画合成機能として実装されていた部分を、独立したツールとして切り出したものです。

Hisui 2025.3.2 から移行する場合は、[Hisui 2025.3.2 から Sora Archive Compositor への移行](docs/migration_from_hisui_2025_3_2.md) を参照してください。

## 特徴

- レイアウト機能
  - JSON 形式のレイアウトファイルで、映像・音声の配置とエンコード設定を細かく指定可能
  - グリッドレイアウトや Picture-in-Picture 等の複雑な配置に対応
- 多様なコーデック・エンコーダー対応
  - 映像: VP8 / VP9 (libvpx), AV1 (SVT-AV1 / dav1d), H.264 (OpenH264 / video_toolbox / nvcodec), H.265 (video_toolbox / nvcodec)
  - 音声: Opus, AAC (FDK-AAC / audio_toolbox)
- エンコードパラメーターの自動調整
  - NSGA-II による多目的最適化で、合成時間の最小化と映像品質 (VMAF スコア) の最大化を両立するパラメーターセットを探索
- VMAF を用いた映像エンコード品質の評価
- マルチスレッド合成
- ダミー録画データの生成
  - 合成やパラメーター調整の動作確認用

利用可能なコーデックは環境・ビルドオプションにより異なります。実際の一覧は `list-codecs` コマンドで確認してください。

## 使い方

現時点ではビルド済みバイナリおよび Docker イメージは提供していません。
[ビルド方法](docs/build.md) に従ってソースからビルドしてください。

より詳しい利用手順は [Sora Archive Compositor を利用してみる](docs/usage.md) を参照してください。

### 合成を試す

まずは録画データを用意します。ここでは `generate-archive` コマンドでダミーの録画データを生成します。

```console
# ダミー録画データを生成
$ sora-archive-compositor generate-archive /path/to/sample/ --connection-id alice --duration 10

# 録画ディレクトリを確認
$ ls /path/to/sample/
archive-alice.json
archive-alice.mp4

# 合成を実行
$ sora-archive-compositor compose /path/to/sample/

# 合成結果ファイルを確認
$ ls /path/to/sample/output.mp4
```

Sora が録画ファイルを保存したディレクトリを `compose` コマンドの引数に指定すると、`output.mp4` が生成されます。

### コマンド一覧

Sora Archive Compositor は `compose` 以外にもいろいろなコマンドを提供しています。

| コマンド | 説明 | ドキュメント |
|---|---|---|
| `compose` | Sora が保存した録画ファイルを合成する | [command_compose.md](docs/command_compose.md) |
| `list-codecs` | 利用可能なコーデックの一覧を表示する | [command_list_codecs.md](docs/command_list_codecs.md) |
| `tune` | 映像エンコードパラメーターの最適化を行う | [command_tune.md](docs/command_tune.md) |
| `vmaf` | 録画ファイルの品質評価 (VMAF スコア計算) を行う | [command_vmaf.md](docs/command_vmaf.md) |
| `inspect` | 録画ファイルの詳細情報を取得する | [command_inspect.md](docs/command_inspect.md) |
| `generate-archive` | ダミーの録画データを生成する | [command_generate_archive.md](docs/command_generate_archive.md) |

### レイアウト機能

Sora Archive Compositor にはレイアウトという機能があり、JSON 形式のレイアウトファイルでより自由な合成が可能です。

```bash
# レイアウトファイルを指定して合成を実行
sora-archive-compositor compose --layout-file レイアウト.jsonc 録画ファイルの配置ディレクトリ/
```

`--layout-file` 引数が省略された場合は [layout-examples/compose-default.jsonc](layout-examples/compose-default.jsonc) が使用され、録画データの映像がグリッド状に並べられます。

レイアウトファイルでは、映像ソースの配置に加えて、合成時に使用するエンコードコーデックやエンコードパラメーターの指定も可能です。これらを指定することで、用途に応じて変換時間と画質のどちらを優先するかなどを細かく制御できます。詳細は [レイアウト機能](docs/layout.md) のドキュメントを参照してください。

また、[`tune`](docs/command_tune.md) コマンドを利用することで、エンコードパラメーターの自動調整を行うこともできます。

## サンプル

Sora Archive Compositor リポジトリには、合成やパラメーター調整の参考となるレイアウトファイルを同梱しています。

- [layout-examples/](layout-examples/) - 合成用・パラメーター調整用のレイアウトファイル例
  - `compose-default.jsonc` - デフォルトの合成レイアウト
  - `tune-*.jsonc` - コーデック・エンコーダー毎のパラメーター調整用レイアウト例
  - `vmaf-default.jsonc` - VMAF 評価用のレイアウト
- [search-space-examples/](search-space-examples/) - パラメーター調整の探索空間定義ファイル例

## ドキュメント

| ドキュメント | 内容 |
|---|---|
| [usage.md](docs/usage.md) | 利用手順の概要 |
| [build.md](docs/build.md) | ビルド方法 |
| [layout.md](docs/layout.md) | レイアウト機能 |
| [migration_from_hisui_2025_3_2.md](docs/migration_from_hisui_2025_3_2.md) | Hisui 2025.3.2 からの移行 |
| [CHANGES.md](CHANGES.md) | 変更履歴 |

## 対応 Sora

- WebRTC SFU Sora 2025.1 以降

## 動作環境

- Ubuntu 26.04 x86_64
- Ubuntu 26.04 arm64
- Ubuntu 24.04 x86_64
- Ubuntu 24.04 arm64
- macOS 26 arm64
- macOS 15 arm64

### macOS の対応バージョン

直近の 2 バージョンをサポートします。

### Ubuntu の対応バージョン

直近の LTS 2 バージョンをサポートします。

## サポートについて

### Discord

- **サポートしません**
- アドバイスします
- フィードバック歓迎します

最新の状況などは Discord で共有しています。質問や相談も Discord でのみ受け付けています。

<https://discord.gg/shiguredo>

### バグ報告

Discord の `#sora-tool-faq` チャンネルへお願いします。

## ライセンス

Apache License 2.0

```text
Copyright 2026 Takeru Ohta (Original Author)
Copyright 2026 Shiguredo Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

## OpenH264

<https://www.openh264.org/BINARY_LICENSE.txt>

```text
"OpenH264 Video Codec provided by Cisco Systems, Inc."
```

## NVIDIA Video Codec SDK

<https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/index.html>

<https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/license/index.html>

```text
“This software contains source code provided by NVIDIA Corporation.”
```

## H.264 (AVC) と H.265 (HEVC) のライセンスについて

**時雨堂が提供する Sora Archive Compositor のビルド済みバイナリには H.264 と H.265 のコーデックは含まれていません**

### H.264

H.264 対応は [Via LA Licensing](https://www.via-la.com/) (旧 MPEG-LA) に連絡を取り、ロイヤリティの対象にならないことを確認しています。

> 時雨堂がエンドユーザーの PC / デバイスに既に存在する AVC / H.264 エンコーダー / デコーダーに依存する製品を提供する場合は、
> ソフトウェア製品は AVC ライセンスの対象外となり、ロイヤリティの対象にもなりません。

### H.265

H.265 対応は以下の二つの団体に連絡を取り、H.265 ハードウェアアクセラレーターのみを利用し、
H.265 が利用可能なバイナリを配布する事は、ライセンスが不要であることを確認しています。

また、H.265 のハードウェアアクセラレーターのみを利用した H.265 対応のツールを OSS で公開し、
ビルド済みバイナリを配布する事は、ライセンスが不要であることも確認しています。

- [Access Advance](https://accessadvance.com/ja/)
- [Via Licensing Alliance](https://www.via-la.com/)
