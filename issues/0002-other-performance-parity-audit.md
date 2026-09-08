# hisui 2025.3.2 との録画合成性能を比較する

- Priority: Medium
- Created: 2026-07-31
- Completed: {YYYY-MM-DD}
- Model: Opus 4.7
- Branch: feature/other-performance-parity-audit
- Polished: {YYYY-MM-DD}

## 目的

sora-archive-compositor は hisui 2025.3.2 (stable) の Sora 録画合成機能を切り出した派生プロジェクトで、移植の過程で複数の横断リファクタ (log → tracing、orfail 撤廃、indicatif 撤廃、`shiguredo_*` の crates.io 版への移行、NVENC EOS flush 修正、async backpressure 導入など) を通ってきた。これらは意図的な差分として書面の差分監査で分類済みだが、**処理性能への影響は監査範囲外** だった (差分監査は「ユーザーから観測可能な挙動」の突き合わせに留まり、処理時間・スループット・メモリ使用量には踏み込んでいない)。

本 issue では、hisui 2025.3.2 と sora-archive-compositor の間で `compose` サブコマンドの処理性能を計測ベースで突き合わせ、デグレの有無を確認する。公開前の validation として実施する。

## 優先度根拠

- 公開後に「hisui でできていた処理を同時間で回せない」が判明すると信頼を損なう。緊急性は高くないが、公開までに必ずやる validation なので Medium。
- 監査の性質上、コード修正は原則発生せず、悪化が見つかった場合のみ別 issue を切り出す方針にできる (対応コストが監査結果に応じてスケールする)。
- 書面の差分監査と対を成す位置付け。片方 (差分監査) だけを公開前 validation として済ませるのは片手落ちになる。

## 現状

- `compose` サブコマンドは `src/subcommand_compose.rs` → `src/composer.rs` → `src/encoder.rs` の経路で動作する。stdout JSON に `elapsed_seconds` と processor 内訳を出力する。`--stats-file` で詳細統計も取れる。
- `generate-archive` (`src/subcommand_generate_archive.rs`) でダミー録画 (映像 MP4 + archive JSON) を生成できる。`--duration` / `--codec` / `--resolution` / `--seed` / `--connection-id` を指定可能。現状の生成物は映像のみ (`"audio": false`)。
- 移植過程の変更のうち性能へ影響を与えうる主なもの:
  - log → tracing : ログのフォーマット・ANSI 色付け・stderr 経由の重さ
  - orfail 撤廃 : エラー型の変更に伴うホットパスの `Result` サイズ差
  - indicatif 撤廃 : `src/progress.rs` の内製プログレスバー
  - `shiguredo_*` crates.io 版へ更新 : 依存 codec / MP4 writer の版差
  - audio_toolbox cfg 整理 : macOS のみ (本 issue では音声経路は対象外)
  - NVENC EOS flush 修正 / async backpressure 導入 : NVENC 経路のみ
- 現状、性能計測用の再利用スクリプトはリポジトリに無い (`scripts/` も未作成)。

## 設計方針

### 対象範囲

- **対象**: `compose` サブコマンドの録画合成のうち、**映像**の decode / mix / encode と壁時計時間。
- **対象外 (音声)**: 音声の encode / decode / mixer。映像に比べ負荷が低く計測誤差に埋もれるため、最初から対象外とする。入力も映像のみでよい。Opus / AAC / fdk-aac 経路の性能比較は本 issue の完了条件に含めない。
- **対象外 (その他)**: `tune` / `vmaf` / `inspect` / `list-codecs`。encoder の bit-exact な出力差や VMAF 絶対値は書面の差分監査側。

### 計測項目 (最低)

- 総処理時間: compose stdout JSON の `elapsed_seconds`、および `/usr/bin/time` の wall clock
- 可能なら `--stats-file` から映像系 processor の `total_*_processing_seconds` 内訳
- peak RSS: Linux は `/usr/bin/time -v`、macOS は `/usr/bin/time -l`

### 比較対象のコーデック組み合わせ

音声は持たない。最低ケースは次の 3 つ。

| # | 映像入力 | 出力映像 encoder | プラットフォーム | 備考 |
|---|---|---|---|---|
| 1 | VP9 | VP9 (libvpx) | Linux / macOS | 定番。既定 codec |
| 2 | H.264 | H.264 (openh264) | Linux / macOS | openh264 経路 |
| 3 | VP9 | AV1 (svt-av1) | Linux / macOS | svt-av1 経路 |

任意 (環境があるときだけ):

| # | 映像入力 | 出力映像 encoder | プラットフォーム | 備考 |
|---|---|---|---|---|
| A | H.265 | H.265 (VideoToolbox) | macOS のみ | |
| B | H.264 | H.264 (NVENC) | Linux (NVIDIA GPU) | |

### テストデータの選定

- **既存 `testdata/` の短尺サンプルは使わない** (integration テスト用途で性能比較には短すぎる)。
- **入力は `generate-archive` で生成する** (既定方針)。
  - 長さ: **120 秒**
  - 解像度: `1280x720`、フレームレート: 30 fps を目安とする
  - ソース数: **2 本** (グリッド合成が入る程度)。`--connection-id` と出力先ディレクトリを分けて 2 回生成する
  - コーデック: ケースに合わせて VP9 / H.264 / (任意で H.265) を生成する
  - `--seed` を固定し、負荷プロファイルを安定させる
- 生成物はコミットしない。ローカルまたは一時ディレクトリに置き、再現手順に生成コマンドを残す。
- 実 Sora 録画の利用は任意の追加手段とし、必須にしない (機密・配布の問題を避ける)。

### 計測スクリプト

今後の再利用も見据え、本 issue の一環で Python スクリプトを `scripts/` に追加する。

必須要件:

- **バイナリ差し替え**: `--bin PATH` (未指定時は `target/release/sora-archive-compositor`)。hisui 計測時は `--bin ../hisui-2025.3.2/target/release/hisui` のように上書きする
- 引数: 入力ディレクトリ、layout、実行回数、出力ディレクトリ、ラベル (結果ファイル名用)
- 1 回の計測で compose の stdout JSON と `/usr/bin/time` の結果を保存する。可能なら `--stats-file` も保存する
- ウォームアップ (先頭 1 回破棄) と中央値集計をスクリプト側または付属の summarize で扱えるようにする
- 機密パスをハードコードしない (引数または環境変数で渡す)

案: `scripts/perf_compose.py` (単体実行 + summarize)。A/B 交互実行があると熱バイアスを減らせる。

### 計測環境と手順

- **同一ホスト**で hisui 2025.3.2 と sora-archive-compositor を計測する。
- hisui は **`2025.3.2` タグの release バイナリに限定する**。`git worktree add ../hisui-2025.3.2 2025.3.2` 等で用意し、`cargo build --release` する。タグ以外のバイナリや「近い release」へのフォールバックはしない。ビルド不能なら本 issue を保留し、原因を本文に記録する。
- sora-archive-compositor も `cargo build --release`。
- **同一入力・意味論的に同等な layout** で両バイナリを実行する。env 名 (`HISUI_*` / `SORA_ARCHIVE_COMPOSITOR_*`) の差だけ吸収する。
- **実行回数**: 各ケース最低 3 回、可能なら 5 回。**先頭 1 回はウォームアップ破棄**、残りは中央値で比較。両バイナリ同数。交互実行を推奨。
- macOS では計測中のスリープ抑制 (`caffeinate`) を検討する。

### 判定基準

- **10% 以内の悪化は許容** (ノイズ + 依存版差 + tracing 等)。複数ケースで **一貫して 5% 前後の悪化** なら原因の当たりを本文に残す。
- **10% 超の悪化**: 別 issue を `create-issue` で起票。修正完了は本 issue の完了条件に含めない。
- **改善**: 記録し、理由の当たりを書く。

### 出力物

- 本 issue の「性能比較結果」節にケースごとの計測値・差分率・判定・考察を追記する
- 派生 issue があれば番号を書き戻す
- スクリプトの使い方と入力生成コマンドを「再現手順」に残す

### 本 issue で触ってよいコード

- 主目的は監査。性能改善の Rust 修正はスコープ外 (悪化時は別 issue)。
- **例外**: 計測用 Python スクリプトの `scripts/` 追加は本 issue で行う。
- `src/**/*.rs` の変更は原則行わない。

## 完了条件

- hisui **2025.3.2 タグ**と sora-archive-compositor を、同一ホスト・同一 `generate-archive` 入力 (120 秒・映像 2 ソース)・同一実行回数 (最低 3 回、うち初回破棄) で計測した結果が「性能比較結果」に追記されている
- 最低ケース (VP9→VP9, H.264→H.264, VP9→AV1) が Linux / macOS のどちらか (または両方) で計測済みである
- 任意ケース (VideoToolbox H.265, NVENC) は「実施した」または「環境不足で未実施」が明記されている
- 各ケースについて差分率と判定 (許容 / 悪化 / 改善) がある
- 10% 超悪化があれば別 issue が起票され番号が書き戻されている
- 5% 前後の一貫悪化があれば当たり (または深追い不要の根拠) が本文にある
- `scripts/` の Python 計測スクリプトと再現手順 (入力生成コマンド含む) が残っている
- **本 issue の完了は上記の追記まで**。派生 issue の実装完了は含めない

## 解決方法

### 実施ステップ

1. **hisui `2025.3.2` の release バイナリを準備する**
   - `git worktree add ../hisui-2025.3.2 2025.3.2` (または同等) のあと `cargo build --release`
   - タグ以外へのフォールバックはしない。ビルド不能なら保留して本文に記録する
2. **sora-archive-compositor の release バイナリを準備する** (`cargo build --release`)
3. **`generate-archive` で計測入力を生成する**
   - 120 秒・`1280x720`・30 fps・seed 固定・ソース 2 本
   - ケースごとに必要な入力コーデック (VP9 / H.264 / 任意 H.265) を用意する
4. **layout を用意する**
   - `layout-examples/compose-default.jsonc` をベースに、ケースごとの `video_codec` を揃えた layout を用意する
   - hisui / SAC で意味論的に同等になるよう env 差だけ吸収する
5. **Python 計測スクリプトを `scripts/` に追加する**
   - `--bin` でバイナリパスを上書き可能にする
   - compose + `/usr/bin/time` (+ 任意で `--stats-file`) の結果を保存し、中央値集計できるようにする
6. **各ケースを計測する** (hisui と SAC を交互・同数)
7. **結果を集計し、判定を付ける**
8. **「性能比較結果」と「再現手順」を本文に追記する** (派生 issue があれば起票して番号を書く)

### リスク・留意点

- **hisui 2025.3.2 のビルド失敗**: タグ固定のためフォールバックしない。不能なら実施保留。
- **計測ノイズ**: 複数回 + 中央値で吸収する。判定の主境は 10%。
- **プラットフォーム偏り**: VT / NVENC は任意。必須 3 ケースの実施を完了の軸にする。
- **layout / env 差**: 意味論的同等性を優先する。

## 参考

- hisui 2025.3.2 との書面ベース差分監査: 機能差の棚卸し。本 issue は性能差の棚卸しで対を成す。
- `generate-archive`: 計測入力の生成手段。
- `HISUI_*` → `SORA_ARCHIVE_COMPOSITOR_*`: layout 揃えの参考。
- `shiguredo_*` crates.io 版への更新: 依存版差の識別。
- NVENC EOS flush / async backpressure: NVENC 任意ケースの変更点。

## 性能比較結果

(実施時に追記)

### 再現手順

(実施時に追記)
