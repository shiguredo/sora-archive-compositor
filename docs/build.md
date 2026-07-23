# ビルド方法

## ビルドに必要な依存パッケージのインストール

### Ubuntu の場合

Ubuntu の場合には以下のようにして、ビルドに必要なパッケージをインストールしてください。

```bash
sudo apt-get install -y meson ninja-build nasm yasm build-essential autoconf automake libtool pkg-config cmake clang
```

### macOS の場合

macOS の場合には以下のようにして、ビルドに必要なパッケージをインストールしてください。

```bash
brew install meson ninja nasm yasm cmake automake autoconf libtool pkg-config
```

## Sora Archive Compositor 本体のビルド方法

Sora Archive Compositor は Rust のビルドツールである [Cargo](https://doc.rust-lang.org/cargo/) を使って以下のようにビルドします。

なお、必要な Rust バージョンは [`Cargo.toml`](../Cargo.toml) の `rust-version` を参照してください。

```bash
# crates.io からビルドする場合
cargo install sora-archive-compositor

# リポジトリ指定でビルドする場合
cargo install --git https://github.com/shiguredo/sora-archive-compositor.git

# ローカルに clone してからビルドする場合
git clone https://github.com/shiguredo/sora-archive-compositor.git
cd sora-archive-compositor/
cargo install --path .
```

上のいずれかの方法でビルドした sora-archive-compositor のバイナリは
`$HOME/.cargo/bin/sora-archive-compositor` のようなディレクトリに配置されます。
アンインストールする場合には `cargo uninstall sora-archive-compositor` を実行してください。

### NVIDIA Video Codec を使った H.264 / H.265 / AV1 のデコードおよびエンコードを有効にする場合

CUDA が利用できる環境で、以下のように `--features nvcodec` を指定して Sora Archive Compositor をビルドしてください。
CUDA がインストールされていない環境では、実行時に nvcodec は自動的に無効になります。

```bash
cargo install sora-archive-compositor --features nvcodec

```

#### CUDA Toolkit のインストール

nvcodec 機能を有効にするには、CUDA Toolkit がインストールされている必要があります。

CUDA Toolkit は [NVIDIA の公式サイト](https://developer.nvidia.com/cuda-downloads) からダウンロードできます。

インストール後、`cuda.h` が以下のいずれかの場所に存在することを確認してください：

- デフォルトパス: `/usr/local/cuda/include/cuda.h`
- 環境変数で指定したパス: `$CUDA_INCLUDE_PATH/cuda.h`

デフォルトパス以外に CUDA をインストールした場合は、環境変数 `CUDA_INCLUDE_PATH` を設定してください：

```bash
export CUDA_INCLUDE_PATH=/path/to/cuda/include
cargo install sora-archive-compositor --features nvcodec

```

### FDK-AAC を使った AAC エンコードを有効にする場合

> [NOTE]
>
> `libfdk-aac` は OSI 認定外の独自ライセンスです。
> 配布や商用利用を行う場合は、利用前にライセンス条件を確認してください。

`--features fdk-aac` でビルドした場合でも、実行時には `--fdk-aac` オプション、または `SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH` 環境変数で共有ライブラリのパスを指定してください。
パス未指定の場合、システムデフォルトの探索は行わず AAC エンコードはエラーになります。

共有ライブラリは別途入手する必要があります。Ubuntu では以下のコマンドでインストールできます。

```bash
sudo apt-get install -y libfdk-aac-dev
```

```bash
sora-archive-compositor compose --fdk-aac /path/to/libfdk-aac.so /path/to/archive/
# または
SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH=/path/to/libfdk-aac.so \
  sora-archive-compositor compose /path/to/archive/
```

共有ライブラリのパスは、ディストリビューションやアーキテクチャによって異なるため、インストールされた場所に合わせて指定してください。

ソースからビルドする場合には、上記の共有ライブラリのインストールに加えて、`--features fdk-aac` を指定してビルドしてください。

```bash
cargo install sora-archive-compositor --features fdk-aac

```

なお macOS では `--features fdk-aac` を有効にしたビルドはできません (`shiguredo_fdk_aac` が Linux 限定のため)。
macOS の場合には Apple Audio Toolbox を用いた AAC エンコードがデフォルト構成で自動的に有効になります。

## GitHub Release のビルド済みバイナリ

タグ push により GitHub Release へ添付されるビルド済みバイナリの feature 構成は次のとおりです。

| 対象 | feature |
|---|---|
| Ubuntu x86_64 | `nvcodec,fdk-aac` |
| Ubuntu arm64 | `fdk-aac` |
| macOS arm64 | デフォルト (Audio Toolbox / Video Toolbox) |

- nvcodec は CUDA がない環境では実行時に無効になります
- fdk-aac は実行時に `--fdk-aac` または `SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH` で共有ライブラリを指定してください
- 対象 OS の版数は [README の動作環境](../README.md#動作環境) を参照してください

## ビルド結果の確認方法

`sora-archive-compositor -h` を実行してみてください。

```console
$ sora-archive-compositor -h
Sora Archive Compositor

Usage: sora-archive-compositor [OPTIONS] <COMMAND>

Commands:
  inspect          録画ファイルの情報を取得します
  list-codecs      利用可能なコーデック一覧を表示します
  compose          録画ファイルの合成を行います
  vmaf             VMAF を用いた映像エンコード品質の評価を行います
  tune             映像エンコードパラメーターの調整を行います
  generate-archive ダミーの録画データを生成します

Options:
  -h, --help    このヘルプメッセージを表示します ('--help' なら詳細、'-h' なら簡易版を表示)
      --version バージョン番号を表示します
      --verbose 警告未満のログメッセージも出力します
```
