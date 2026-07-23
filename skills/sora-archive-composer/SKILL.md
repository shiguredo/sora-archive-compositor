---
name: sora-archive-composer
description: Sora の録画アーカイブを合成・確認・調整するための利用者向けスキル。compose による MP4 合成、レイアウト JSON による配置とエンコード設定、list-codecs による環境確認、generate-archive によるダミー録画生成、tune によるパラメーター最適化、vmaf による品質評価を行うときに使う。
---

# Sora Archive Compositor 利用者スキル

## 概要

Sora Archive Compositor は WebRTC SFU Sora 向けの録画合成ツールである。
Sora が出力した録画ファイル (MP4 または WebM) を合成し、単一の MP4 ファイルとして出力する。

利用前に `https://github.com/shiguredo/oss` を読み、質問や相談は Discord のみで行う。
バグ報告は Discord の `#sora-tool-faq` チャンネルへ送る。

## いつこのスキルを使うか

- Sora の録画ディレクトリから `output.mp4` を作りたいとき
- レイアウト JSON で映像配置やエンコード設定を変えたいとき
- 実行環境で使えるコーデックを確認したいとき
- ダミー録画で動作確認したいとき
- 合成時間と画質を両立するエンコードパラメーターを探したいとき
- VMAF スコアでエンコード品質を評価したいとき

開発・デバッグ目的の `inspect` 以外は、すべてこのスキルで対応できる。
`inspect` は開発者向けであり、利用者支援では原則として使わない。

## 前提

- 入力は Sora が保存した録画ディレクトリである。`ROOT_DIR` と呼ぶ。
- `ROOT_DIR` には `archive-{ CONNECTION_ID }.json` と対応する `.mp4` または `.webm` が入る。
- ソース JSON には `connection_id`、`format`、`audio`、`video`、`start_time_offset`、`stop_time_offset` が含まれる。
- メディアファイルのパスは、JSON の拡張子をメディア形式に置き換えたものとして解決される。
- `ROOT_DIR` の外を参照するパス指定はエラーになる。
- バイナリは Releases から取得するか、`docs/build.md` の手順でビルドする。
- Docker イメージは 2026.1.0-canary.0 では提供していない。

## 最短フロー

```console
# ダミー録画データを生成する
$ sora-archive-compositor generate-archive /path/to/sample/ --connection-id alice --duration 10

# 合成を実行する
$ sora-archive-compositor compose /path/to/sample/

# 合成結果を確認する
$ ls /path/to/sample/output.mp4
```

実録画を使う場合は、生成手順を飛ばして `compose 録画ファイルの配置ディレクトリ/` を実行する。
デフォルト出力は `ROOT_DIR/output.mp4` である。

## コマンド一覧

| コマンド | 用途 | 詳細ドキュメント |
|---|---|---|
| `compose` | 録画ファイルを合成する | `docs/command_compose.md` |
| `list-codecs` | 利用可能なコーデック一覧を表示する | `docs/command_list_codecs.md` |
| `generate-archive` | ダミー録画データを生成する | `docs/command_generate_archive.md` |
| `tune` | 映像エンコードパラメーターを最適化する | `docs/command_tune.md` |
| `vmaf` | VMAF スコアで品質評価を行う | `docs/command_vmaf.md` |
| `inspect` | 録画ファイルの詳細情報を取得する (開発者向け) | `docs/command_inspect.md` |

共通オプションは `-h/--help`、`--version`、`--verbose` である。
ログは標準エラー出力に出る。`NO_COLOR=1` で色付けを無効化できる。

### compose

```console
$ sora-archive-compositor compose /path/to/archive/RECORDING_ID/
$ sora-archive-compositor compose -l /path/to/layout.jsonc /path/to/archive/RECORDING_ID/
```

- `-l/--layout-file`: レイアウトファイル。省略時は `layout-examples/compose-default.jsonc` 相当のグリッド配置になる。
- `-o/--output-file`: 出力先。デフォルトは `ROOT_DIR/output.mp4` である。
- `-s/--stats-file`: 合成統計 JSON の保存先。
- `-T/--thread-count`: ワーカースレッド数。デフォルトは 1 である。1 プロセスで高速化したい場合は物理コア数を指定する。
- `-P/--no-progress-bar`: 進捗表示を無効化する。
- `--openh264`: OpenH264 共有ライブラリのパス。`SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH` でも指定できる。
- `--fdk-aac`: FDK-AAC 共有ライブラリのパス。`SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH` でも指定できる。システム探索は行わない。
- 標準出力には入出力件数、出力コーデック、所要時間、デコード・エンコード・ミキサー処理時間などが出る。

### list-codecs

```console
$ sora-archive-compositor list-codecs
```

- 環境依存の結果を JSON で返す。レイアウトを書く前に必ず確認する。
- `codecs` にはコーデック名、種別、対応デコーダーとエンコーダーが入る。
- `engines` には各エンジンの詳細が入る。
- OpenH264 と FDK-AAC は、共有ライブラリのパスを指定して読み込めた場合のみ表示される。
- Linux では `audio_toolbox` と `video_toolbox` は表示されない。

### generate-archive

```console
$ sora-archive-compositor generate-archive /path/to/output/ --connection-id alice --duration 10
$ sora-archive-compositor generate-archive /path/to/output/ --connection-id bob --codec VP9 --seed 42
```

- 出力は `archive-{ CONNECTION_ID }.json` と `archive-{ CONNECTION_ID }.mp4` である。
- `--connection-id` 省略時は自動生成される。
- `--resolution` (デフォルト `1280x720`)、`--frame-rate` (デフォルト `30`)、`--start-time` (デフォルト `0`)、`--duration` (デフォルト `10`)、`--seed`、`--codec` (`VP8` / `VP9` / `H264` / `H265` / `AV1`、デフォルト `VP9`) を指定できる。
- `--resolution-change "5:640x360"` で途中解像度変更を再現できる。複数回指定可能である。
- H.264 生成時は `--openh264` が必要になる場合がある。
- 音声は含まれない。`audio` は `false` になる。
- VP9 生成物は QuickTime Player で再生できない。ffplay や VLC を使う。
- 複数人分は `--connection-id` を変えて複数回実行する。生成ディレクトリをそのまま `compose` や `tune` に渡せる。

### tune

```console
$ sora-archive-compositor tune /path/to/archive/RECORDING_ID/
$ sora-archive-compositor tune -l /path/to/tune-layout.jsonc /path/to/archive/RECORDING_ID/
```

- NSGA-II で合成実行時間の最小化と VMAF スコア平均値の最大化を同時に探索する。
- 運用と同じ OS とマシン、運用に近い録画で実行する。
- レイアウト内で値が `null` の項目が探索対象になる。固定したい項目は具体値を書く。
- 探索範囲は `--search-space-file` (デフォルト `search-space-examples/full.jsonc`) で定義する。通常は `full.jsonc` のままでよい。
- `--trial-count` は既存履歴を含む合計試行回数である。デフォルトは 100 である。
- `--frame-count` (デフォルト `300`) を小さくすると 1 トライアルが速くなるが、品質評価の信頼度は下がる。
- `--trial-timeout` で長時間トライアルを打ち切れる。
- `--tune-working-dir` (デフォルト `ROOT_DIR/tune/`) 配下の `<name>.jsonl` に履歴が追記される。`--name` (デフォルト `tune`) ごとに履歴が分かれる。
- 同じ `ROOT_DIR` と `--name` で再実行すると自動で継続される。条件を変えた探索は `--name` を変える。
- `--trial-count 0` で新規試行なしに既存の最適解集合だけを表示できる。
- 結果は `BEST TRIALS` として複数表示される。単一の正解は出ないため、利用者が用途に応じて選ぶ。
- 採用前に候補レイアウトで実際に `compose` して目視確認する。`tune` は先頭部分のみの部分合成であり、VMAF は人間の感覚と完全には一致しない。
- `resolution` と `video_codec` は探索対象に含めない。参照映像自体が変わり、比較が成立しなくなるためである。
- ベース例は `layout-examples/tune-*.jsonc` にある。コーデックとエンジンごとに用意されている。

### vmaf

```console
$ sora-archive-compositor vmaf /path/to/archive/RECORDING_ID/
```

- 参照映像とエンコード・デコード後の歪み映像を生成し、VMAF スコアを計算する。
- デフォルトレイアウトは `layout-examples/vmaf-default.jsonc` である。`-l` で変更できる。
- `-f/--frame-count` (デフォルト `1000`)、`--timeout`、`--reference-yuv-file`、`--distorted-yuv-file` を指定できる。
- 出力には `vmaf_min`、`vmaf_max`、`vmaf_mean`、`vmaf_harmonic_mean` が含まれる。
- YUV 中間ファイルは巨大になる。評価後は削除してよい。
- 主に `tune` と組み合わせて使う。単独利用時はスコアの解釈に VMAF 公式情報を参照する。

## レイアウト機能の要点

詳細は `docs/layout.md`、`docs/layout_spec.md`、`docs/layout_region.md` を読む。
`.jsonc` では行コメント、ブロックコメント、末尾カンマを使える。

### 最小例

グリッド配置の基本形である。

```json
{
  "audio_sources": ["archive-*.json"],
  "video_layout": {
    "main": {
      "max_columns": 3,
      "max_rows": 2,
      "video_sources": ["archive-*.json"]
    }
  },
  "resolution": "960x480"
}
```

Picture-in-Picture など複数リージョンの例は `docs/layout.md` にある。

### よく使うトップレベル項目

- `audio_sources`、`audio_sources_excluded`: 音声合成対象と除外対象。デフォルトは `[]` (音声なし) である。
- `audio_codec`: `"OPUS"` (デフォルト) または `"AAC"` である。
- `audio_bitrate`: bps 単位。デフォルトは `65536` である。
- `video_codec`: `"VP8"` / `"VP9"` (デフォルト) / `"H264"` / `"H265"` / `"AV1"` である。
- `video_bitrate`: bps 単位。デフォルトは `映像ソース数 * 200 * 1024` である。旧 `bitrate` (kbps) より優先される。
- `video_encode_engines`、`video_decode_engines`: 候補を優先順に指定する。空配列や対応エンジンなしはエラーである。
- `resolution`: `"幅x高さ"` 形式。16 以上 3840 以下で偶数にする。奇数は偶数に丸められる。省略時はリージョン配置から自動計算される。
- `frame_rate`: 整数または `"分子/分母"` 文字列。デフォルトは `25` である。
- `trim`: デフォルトは `true` である。`true` では音声・映像ソースが存在しない期間を除去する。`false` でも冒頭の無音・無映像期間は除去される。

### リージョンの要点

- リージョン名は任意である。`video_sources` のみ必須である。
- ファイル名部分に `*` のワイルドカードを使える。例: `"archive-*.json"` である。
- `video_sources_excluded` で除外指定できる。
- 同じ `connection_id` の分割録画は自動で連結される。
- `max_columns`、`max_rows` でグリッド上限を決める。両方省略時は正方形に近い配置になる。
- `reuse` はセル不足時の動作である。`"none"`、`"show_oldest"` (デフォルト)、`"show_newest"` を指定できる。
- `x_pos`、`y_pos` は左上原点のピクセル座標である。`z_pos` (-99 から 99、デフォルト `0`) が大きいほど前面になる。
- `width` / `height` と `cell_width` / `cell_height` の同時指定はエラーになる。
- `cells_excluded` は左上から行順の 0 始まりセル番号で除外する。
- `border_pixels` はデフォルト `2` である。`0` で枠線なしになる。奇数はエラーになる。
- セル内映像はアスペクト比を維持して中央配置される。余白と未割当セルは黒塗りになる。

### エンコード設定の要点

詳細は `docs/layout_encode_params.md` と `docs/layout_decode_params.md` を読む。
利用可能な組み合わせは実行環境で変わるため、先に `list-codecs` を実行する。

- 音声はビットレートのみ調整できる。エンコーダー固有パラメーターはない。
- 映像は `video_codec` と `video_bitrate` に加えて、エンジン固有の `*_encode_params` を指定できる。
- 主なキー: `libvpx_vp8_encode_params`、`libvpx_vp9_encode_params`、`openh264_encode_params`、
  `svt_av1_encode_params`、`video_toolbox_h264_encode_params`、`video_toolbox_h265_encode_params`、
  `nvcodec_h264_encode_params`、`nvcodec_h265_encode_params`、`nvcodec_av1_encode_params` である。
- nvcodec デコード調整は `nvcodec_*_decode_params` で行う。
- `H264` には Video Toolbox、OpenH264 (`--openh264` 指定)、nvcodec のいずれかが必要である。
- `H265` には Video Toolbox または nvcodec が必要である。
- `AAC` には macOS の Audio Toolbox または FDK-AAC ビルド (`--fdk-aac` 指定) が必要である。
- nvcodec ビルドはデフォルト無効である。有効化は `docs/build.md` を読む。Ubuntu 向けビルド済みバイナリは有効だが、CUDA なし環境では実行時に無効になる。
- 最適値が不明な場合はデフォルトから始め、必要なら `tune` で探索する。手動微調整は最後に行う。
- サンプルは `layout-examples/` にある。`compose-default.jsonc` が合成既定、`tune-*.jsonc` が調整用、`vmaf-default.jsonc` が評価用である。探索空間例は `search-space-examples/` にある。

## 対応手順の目安

1. `list-codecs` で目的のコーデックが使えるか確認する。
2. 実録画がなければ `generate-archive` でダミーを作る。
3. まず既定レイアウトで `compose` する。
4. 配置やコーデックを変えたい場合のみレイアウトを作成する。`layout-examples/compose-default.jsonc` の複製から始める。
5. 速度か画質に不満がある場合は `vmaf` で現状を測り、`tune` で候補を作る。
6. `BEST TRIALS` から 2 から 3 候補を選び、フル尺の `compose` で目視比較する。
7. 採用レイアウトを固定し、`--stats-file` で統計を残す。

## 注意点とつまずきやすい点

- 録画 JSON に対応するメディアがない場合、ワイルドカード展開では除外、直接指定ではエラーになる。
- `archive-*.json` と `*archive*.json` は別パターンである。分割録画がある場合は後者のように先頭ワイルドカードが必要になることがある。
- `trim: true` では無人期間の分だけ出力尺が短くなる。通話開始時刻起点ではない。
- `--thread-count` は Sora Archive Compositor 本体の並列度のみ制御する。エンコーダー内部スレッドはレイアウト側パラメーターで制御する。
- エンコーダーが指定ビットレートを厳密に守る保証はない。正確な値は `compose` 実測で確認する。
- `tune` の処理時間は相対比較用である。絶対値はフル尺の `compose` で確認する。
- トライアル失敗が稀なら継続してよい。頻発する場合は探索空間かビルド・共有ライブラリ指定を疑う。
- `vmaf` と `tune` の中間 YUV は大容量になる。不要になれば削除する。
- Hisui からの移行者は `docs/migration_from_hisui_2025_3_2.md` を読む。バイナリ名、環境変数 (`HISUI_*` から `SORA_ARCHIVE_COMPOSITOR_*` へ)、`tune` 履歴形式の変更が要点である。新規利用者は移行文書を無視してよい。

## 参照ドキュメント

困ったときは次の順で読む。

- まず試す: `docs/usage.md`
- 合成: `docs/command_compose.md`
- コーデック確認: `docs/command_list_codecs.md`
- ダミー生成: `docs/command_generate_archive.md`
- 調整: `docs/command_tune.md`
- 品質評価: `docs/command_vmaf.md`
- 配置全体像: `docs/layout.md`
- 項目仕様: `docs/layout_spec.md`
- 配置計算: `docs/layout_region.md`
- エンコード: `docs/layout_encode_params.md`
- デコード: `docs/layout_decode_params.md`
- ビルド: `docs/build.md`
- Hisui 移行: `docs/migration_from_hisui_2025_3_2.md`

回答では推測でパラメーター名を作らず、該当ドキュメントと `list-codecs` 結果を根拠にする。
実行環境にないコーデックやエンジンを提案しない。
