# `sora-archive-compositor generate-archive` コマンド

`sora-archive-compositor generate-archive` コマンドは、Sora 録画形式のダミー録画データを生成するためのコマンドです。

このコマンドで生成したデータは、`compose` による合成や `tune` によるパラメーター調整を試すために利用できます。

## 使用方法

```console
$ sora-archive-compositor generate-archive -h
ダミーの録画データを生成します

Usage: sora-archive-compositor ... generate-archive [OPTIONS] OUTPUT_DIR

Example:
  $ sora-archive-compositor generate-archive /path/to/output/

Arguments:
  OUTPUT_DIR ダミー録画データの出力先ディレクトリを指定します

Options:
  -h, --help                         このヘルプメッセージを表示します ('--help' なら詳細、'-h' なら簡易版を表示)
      --version                      バージョン番号を表示します
      --verbose                      警告未満のログメッセージも出力します
      --connection-id <ID>           archive JSON の connection_id を指定します
      --resolution <WxH>             出力解像度を指定します [default: 1280x720]
      --frame-rate <N>               フレームレートを指定します [default: 30]
      --start-time <SECONDS>         録画の開始時刻を指定します [default: 0]
      --duration <SECONDS>           録画の長さを指定します [default: 10]
      --seed <N>                     映像パターンの乱数シードを指定します (未指定の場合はランダムに決定されます)
      --codec <CODEC>                出力映像のコーデックを指定します (VP8 / VP9 / H264 / H265 / AV1) [default: VP9]
      --openh264 <PATH>              OpenH264 の共有ライブラリのパスを指定します (H264 エンコードに使用) [env: SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH]
      --resolution-change <TIME:WxH> 録画の途中で解像度を変更します (複数回指定可能)
```

生成の進捗状況は、プログレスバーで標準エラー出力に表示されます (標準エラー出力がターミナルでない場合には表示されません)。

## 実行例

以下のコマンドは、`/path/to/output/` ディレクトリに `alice` という connection_id のダミー録画データを生成します。

```console
$ sora-archive-compositor generate-archive /path/to/output/ --connection-id alice --duration 10
```

生成されるファイルは以下の 2 つです。

- `archive-alice.json`: Sora 録画形式のメタデータ
- `archive-alice.mp4`: `--codec` で指定したコーデック (デフォルトは VP9) でエンコードされた録画映像

`archive-alice.json` の内容:

```json
{
  "connection_id": "alice",
  "format": "mp4",
  "audio": false,
  "video": true,
  "start_time_offset": 0,
  "stop_time_offset": 10
}
```

## 生成される映像

このコマンドが生成する映像は、実ユースケースを模した図形のみの映像です。

- シードで決まる色相の暗い固定背景 (ウェブ会議用途を想定し、背景色は時間変化させない)
- 横方向に並ぶ円が上下に動くウェーブパターン
- リサージュ曲線風に動く半透明の円 (バウンドする円)
- 中央周囲を周回する円群
- 左上に connection_id がピクセルアート (5x7 ドット) で表示される
  - ドットの大きさは画面の縦サイズに合わせて自動で調整される
  - 背景は暗い固定色のため、黒半透明の背景は使わず白文字で直接表示される

映像パターンは `--seed` オプションの値で決まるため、同じシード・同じオプションで実行すれば同じ映像が生成されます。
未指定の場合には、システム時刻を基にランダムなシードが決定されます (使用されたシードは `--verbose` で確認できます)。

## オプション

- `--connection-id <ID>`: archive JSON の connection_id を指定します
  - 未指定の場合には自動生成されます
- `--resolution <WxH>`: 出力解像度を指定します
  - デフォルト: `1280x720`
- `--frame-rate <N>`: フレームレートを指定します
  - デフォルト: `30`
- `--start-time <SECONDS>`: 録画の開始時刻 (秒) を指定します
  - `archive-*.json` の `start_time_offset` に反映されます
  - デフォルト: `0`
- `--duration <SECONDS>`: 録画の長さ (秒) を指定します
  - フレーム数は `duration × frame-rate` で決まります
  - `archive-*.json` の `stop_time_offset` は `start-time + duration` になります
  - デフォルト: `10`
- `--seed <N>`: 映像パターンの乱数シードを指定します
  - 未指定の場合には、システム時刻を基にランダムに決定されます
- `--codec <CODEC>`: 出力映像のコーデックを指定します
  - 指定可能な値: `VP8` / `VP9` / `H264` / `H265` / `AV1`
  - デフォルト: `VP9`
  - エンコードに利用できるエンジンは `list-codecs` コマンドで確認できます
- `--openh264 <PATH>`: OpenH264 の共有ライブラリのパスを指定します
  - `H264` を指定する場合に必要です (`SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH` 環境変数でも指定できます)
- `--resolution-change <TIME:WxH>`: 録画の途中で解像度を変更します
  - 複数回指定できます。`TIME` は録画開始からの経過秒で、`--duration` 未満の範囲内です
  - 単調増加かつ現在の解像度と異なる値のみ指定できます
  - 解像度の変更点ではエンコーダーが新しい解像度のものに切り替わり、変更点のフレームはキーフレームになります

## 途中で解像度が変わる録画データを生成する

実際の録画データは途中で解像度が変わることがあり得ます。`--resolution-change` オプションで、そのような録画データのダミーを生成できます。

以下のコマンドは、15 秒の録画データを生成し、5 秒目から 640x360、10 秒目から 1280x720 に解像度を変更します。

```console
$ sora-archive-compositor generate-archive /path/to/output/ --connection-id alice --duration 15 --resolution-change "5:640x360" --resolution-change "10:1280x720"
```

## 出力コーデック

出力映像のコーデックは `--codec` オプションで指定できます。デフォルトは VP9 です。

libvpx は静的リンクされるため、VP8 / VP9 のエンコードに追加のライブラリは不要です。

H264 を OpenH264 でエンコードする場合には、共有ライブラリのパスを `--openh264` オプションか `SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH` 環境変数で指定してください。

```console
$ sora-archive-compositor generate-archive /path/to/output/ --connection-id alice --codec H264 --openh264 /path/to/libopenh264.dylib
```

なお、VP9 は QuickTime Player で再生できないため、確認には ffplay / VLC などの VP9 対応プレイヤーを使用してください。

音声は含まれません (`archive-*.json` の `audio` は `false` になります)。

## 複数のソースを生成する場合

複数のソースが必要な場合には、`--connection-id` を変えてコマンドを複数回実行してください。

```console
$ sora-archive-compositor generate-archive /path/to/output/ --connection-id alice
$ sora-archive-compositor generate-archive /path/to/output/ --connection-id bob
$ sora-archive-compositor generate-archive /path/to/output/ --connection-id carol
```

生成したディレクトリをそのまま `compose` や `tune` の入力として指定できます。

## 生成したデータを合成する

生成したダミー録画データは、レイアウトファイルを用意して `compose` サブコマンドで合成できます。

以下の例は、3 人の参加者を 1 画面に並べるレイアウトです。

```jsonc
{
  "audio_sources": [],
  "video_layout": {
    "main": {
      "cell_width": 640,
      "cell_height": 360,
      "max_columns": 3,
      "video_sources": ["archive-*.json"]
    }
  },
  "video_codec": "VP9",
  "video_bitrate": 2000000,
  "frame_rate": 30,
}
```

レイアウトファイルを保存したら、`compose` を実行します。

```console
$ sora-archive-compositor compose -l /path/to/layout.jsonc /path/to/output/
```

`/path/to/output/output.mp4` に合成結果が出力されます。
