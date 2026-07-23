# Sora Archive Compositor を利用してみる

まずは Sora Archive Compositor を使って録画データの合成をしてみましょう。

Hisui 2025.3.2 から移行する場合は、[Hisui 2025.3.2 から Sora Archive Compositor への移行](migration_from_hisui_2025_3_2.md) を参照してください。

## ビルドして合成する

現時点ではビルド済みバイナリおよび Docker イメージは提供していません。
[ビルド方法](build.md) に従ってソースからビルドしてください。

Sora Archive Compositor には録画ファイルの合成を行うための [compose](command_compose.md) コマンドがあります。
Sora が録画ファイルを保存したディレクトリを指定して、このコマンドを実行すると合成が始まります。

```console
# 録画ディレクトリを確認
$ ls 録画ファイルの配置ディレクトリ/
report-{ RECORDING_ID }.json
archive-{ CONNECTION_ID }.json
archive-{ CONNECTION_ID }.mp4
...

# 合成を実行
$ sora-archive-compositor compose 録画ファイルの配置ディレクトリ/

# 合成結果ファイルを確認
$ ls 録画ファイルの配置ディレクトリ/output.mp4
```

## 映像・音声の構成やエンコード設定を指定して合成する（レイアウト機能）

Sora Archive Compositor にはレイアウトという機能があり、そちらを利用することでより自由な合成が可能です。

レイアウトは JSON 形式のファイルで定義し、
`compose` コマンドの `--layout-file` 引数で指定します。

```bash
# レイアウトファイルを指定して合成を実行
./sora-archive-compositor compose --layout-file レイアウト.jsonc 録画ファイルの配置ディレクトリ/
```

`--layout-file` 引数が省略された場合は [デフォルトのレイアウト](../layout-examples/compose-default.jsonc) が使用されます。
デフォルトでは、録画データの映像がグリッド状に並べられます。
より複雑な構成での合成も可能ですので、[レイアウト機能のドキュメント](./layout.md) を参考にして、ぜひ試してみてください。

また、レイアウトファイルを使うことで、合成時に使用するエンコードコーデックやエンコードパラメーターを変更することも可能です。
これらを指定することで、用途に応じて、変換時間と画質のどちらを優先するか、などを細かく制御できます。

指定方法の詳細は [エンコードコーデックやパラメーターの指定](layout_encode_params.md) のドキュメントをご参照ください。

また [tune](command_tune.md) コマンドを利用することで、エンコードパラメーターの自動調整を行うことができます。

## 利用可能なコマンド一覧

Sora Archive Compositor は `compose` 以外にもいろいろなコマンドを提供しています。

利用可能なコマンドとドキュメントは以下になります。目的に合った項目を参照してください。

- [`compose`](command_compose.md) - Sora が保存した録画ファイルを合成するためのコマンド
- [`list-codecs`](command_list_codecs.md) - 利用可能なコーデックの一覧を表示するコマンド
- [`tune`](command_tune.md) - 映像エンコードパラメーターの最適化を行うコマンド
- [`vmaf`](command_vmaf.md) - 録画ファイルの品質評価（VMAF スコア計算）を行うコマンド
- [`inspect`](command_inspect.md) - 録画ファイルの詳細情報を取得するコマンド
- [`generate-archive`](command_generate_archive.md) - ダミーの録画データを生成するコマンド
