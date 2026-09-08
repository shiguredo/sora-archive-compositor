# hisui との録画合成性能を比較する

- Priority: Medium
- Created: 2026-07-31
- Completed: 2026-09-08
- Model: Opus 4.7
- Branch: feature/other-performance-parity-audit
- Polished: {YYYY-MM-DD}

## 目的

sora-archive-compositor は hisui (stable) の Sora 録画合成機能を切り出した派生プロジェクトで、移植の過程で複数の横断リファクタ (log → tracing、orfail 撤廃、indicatif 撤廃、`shiguredo_*` の crates.io 版への移行、NVENC EOS flush 修正、async backpressure 導入など) を通ってきた。これらは意図的な差分として書面の差分監査で分類済みだが、**処理性能への影響は監査範囲外** だった (差分監査は「ユーザーから観測可能な挙動」の突き合わせに留まり、処理時間・スループット・メモリ使用量には踏み込んでいない)。

本 issue では、hisui と sora-archive-compositor の間で `compose` サブコマンドの処理性能を計測ベースで突き合わせ、デグレの有無を確認する。公開前の validation として実施する。

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
- 計測スクリプト `scripts/perf_compose.py` と使い方 `scripts/README.md` を追加済み。
- **macOS での一式計測は実施済み** (詳細は「性能比較結果」)。比較対象バイナリは当初案の hisui `2025.3.2` タグではなく、手元で用意できた crates.io **hisui 2025.3.3** (`~/.cargo/bin/hisui`) とした。タグ固定ビルドとの差分は未確認。
- **openh264 経路は未計測**。計測ホストの `/usr/local/lib` は OpenH264 **2.5.0** で、ビルドが要求する **2.6.0** と不一致。手元の 2.6.0 では `generate-archive --codec H264` が `Annex B input has an empty NAL unit` で失敗した。そのため H.264 ケースは **VideoToolbox** 経路で代替計測した。
- **Linux / NVENC は未実施**。
- 改善理由の当たりは「性能比較結果」に **参考情報** として追記済み (根拠は弱く、深追いはしない)。

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
| 2 | H.264 | H.264 (openh264) | Linux / macOS | openh264 経路。環境不足時は VT で代替し、代替である旨を結果に明記する |
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
  - ソース数: **3 本** (グリッド合成が入る程度。2 本より実運用に近い)。`--connection-id` と出力先ディレクトリを分けて生成する
  - コーデック: ケースに合わせて VP9 / H.264 / (任意で H.265) を生成する
  - `--seed` を固定し、負荷プロファイルを安定させる
- 初回 macOS 計測はソース **2 本** で実施済み。方針としては 3 本を正とし、必要なら 3 本で確認計測を追加する。
- 生成物はコミットしない。ローカルまたは一時ディレクトリに置き、再現手順に生成コマンドを残す。
- 実 Sora 録画の利用は任意の追加手段とし、必須にしない (機密・配布の問題を避ける)。

### 計測スクリプト

再利用のため Python スクリプトを `scripts/perf_compose.py` に置く。使い方は `scripts/README.md`。

必須要件:

- **バイナリ差し替え**: `--bin PATH` (未指定時は `target/release/sora-archive-compositor`)。hisui 計測時は `--bin` で上書きする
- 引数: 入力ディレクトリ、layout、実行回数、出力ディレクトリ、ラベル (結果ファイル名用)
- 1 回の計測で compose の stdout JSON と `/usr/bin/time` の結果を保存する。可能なら `--stats-file` も保存する
- ウォームアップ (先頭 1 回破棄) と中央値集計をスクリプト側または付属の summarize で扱えるようにする
- 機密パスをハードコードしない (引数または環境変数で渡す)

### 計測環境と手順

- **同一ホスト**で hisui と sora-archive-compositor を計測する。
- hisui の版は本文の「性能比較結果」に明記する。当初案は `2025.3.2` タグ固定だったが、初回実施では **2025.3.3** (crates.io / cargo install) を使った。
- sora-archive-compositor は `cargo build --release`。
- **同一入力・意味論的に同等な layout** で両バイナリを実行する。env 名 (`HISUI_*` / `SORA_ARCHIVE_COMPOSITOR_*`) の差だけ吸収する。`--video-codec` による最小 layout (`audio_sources: []`) も可。
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

- hisui と sora-archive-compositor を、同一ホスト・同一 `generate-archive` 入力 (120 秒・映像、ソース数は本文記載)・同一実行回数 (最低 3 回、うち初回破棄) で計測した結果が「性能比較結果」に追記されている
- 最低ケース (VP9→VP9, H.264→H.264, VP9→AV1) が Linux / macOS のどちらか (または両方) で計測済みである (openh264 不可時は VT 代替と明記)
- 任意ケース (VideoToolbox H.265, NVENC) は「実施した」または「環境不足で未実施」が明記されている
- 各ケースについて差分率と判定 (許容 / 悪化 / 改善) がある
- 10% 超悪化があれば別 issue が起票され番号が書き戻されている
- 改善または 5% 前後の一貫悪化があれば、理由の当たり (または深追い不要の根拠) が本文にある
- `scripts/` の Python 計測スクリプトと再現手順 (入力生成コマンド含む) が残っている
- **本 issue の完了は上記の追記まで**。派生 issue の実装完了は含めない

## 解決方法

macOS で hisui 2025.3.3 と SAC の `compose` 性能を計測し、結果・再現手順・参考仮説を本文に残したうえで closed にした。

- `scripts/perf_compose.py` と `scripts/README.md` を追加した
- 必須ケース相当 (VP9→VP9 / H.264→H.264 は VT 代替 / VP9→AV1) と任意の H.265 VT を計測し、いずれも改善 (デグレ起票なし)
- 出力 MP4 の解像度・尺・フレーム数は一致。ファイルサイズもほぼ同水準 (VT はバイト一致、AV1 は約 −1.5%)
- 改善理由は依存コーデック世代差の観察を **参考・根拠弱い** として追記したのみ (因果未検証、深追いしない)
- 未実施のまま残すもの (本 issue の完了条件外または任意): ソース 3 本の再計測、openh264 本線、Linux / NVENC、hisui `2025.3.2` タグ固定ビルド

### 実施時の手順メモ

1. **hisui の release バイナリを準備する** (版を本文に記録する)
2. **sora-archive-compositor の release バイナリを準備する** (`cargo build --release`)
3. **`generate-archive` で計測入力を生成する**
   - 120 秒・`1280x720`・30 fps・seed 固定・ソース **3 本** (初回計測は 2 本で実施済み)
   - ケースごとに必要な入力コーデック (VP9 / H.264 / 任意 H.265) を用意する
4. **layout を用意する**
   - `scripts/perf_compose.py run --video-codec ...` の最小 layout、または `layout-examples/compose-default.jsonc` ベース
   - hisui / SAC で意味論的に同等になるよう env 差だけ吸収する
5. **Python 計測スクリプトを `scripts/` に追加する** (済み: `perf_compose.py` / `README.md`)
6. **各ケースを計測する** (hisui と SAC を交互・同数を推奨)
7. **結果を集計し、判定を付ける**
8. **「性能比較結果」と「再現手順」を本文に追記する** (派生 issue があれば起票して番号を書く)
9. **改善理由の当たりを本文に追記する** (軽い調査まで。深追いはしない)

### リスク・留意点

- **hisui 版の取り違え**: 結果に版を必ず書く。タグ固定と crates.io 版で依存が違う可能性がある。
- **計測ノイズ**: 複数回 + 中央値で吸収する。判定の主境は 10%。
- **プラットフォーム偏り**: VT / NVENC は任意。必須 3 ケースの実施を完了の軸にする。
- **layout / env 差**: 意味論的同等性を優先する。最小 layout は各バイナリの既定 encode パラメータ差を残す。
- **openh264 環境**: 共有ライブラリ版不一致や Annex B 生成失敗で経路が取れないことがある。

## 参考

- hisui との書面ベース差分監査: 機能差の棚卸し。本 issue は性能差の棚卸しで対を成す。
- `generate-archive`: 計測入力の生成手段。
- `HISUI_*` → `SORA_ARCHIVE_COMPOSITOR_*`: layout 揃えの参考。
- `shiguredo_*` crates.io 版への更新: 依存版差の識別。
- NVENC EOS flush / async backpressure: NVENC 任意ケースの変更点。
- `scripts/README.md`: 計測スクリプトの使い方。

## 性能比較結果

### 環境 (2026-09-08 / macOS)

| 項目 | 値 |
|---|---|
| ホスト | macOS (darwin 25.6.0) |
| hisui | **2025.3.3** (`/Users/tohta/.cargo/bin/hisui`) |
| SAC | `target/release/sora-archive-compositor` (計測日時点の `develop`) |
| 入力 | `generate-archive`、120 秒、`1280x720`、30 fps、ソース **2 本**、seed 固定 |
| layout | `perf_compose.py --video-codec` の最小 layout (`audio_sources: []`) |
| 実行 | 各 5 回 (先頭 1 回ウォームアップ破棄)、中央値、`--thread-count 1` |
| 実行順 | ケース内で hisui ブロック → SAC ブロック (交互ではない) |
| 結果置き場 | `/tmp/perf-2025.3.3/` (ローカル一時。コミットしない) |

### ケース別 (中央値 `elapsed_seconds`)

| # | ケース | エンジン (decode / encode) | hisui | SAC | delta | 判定 |
|---|---|---|---|---|---|---|
| 1 | VP9 → VP9 | libvpx / libvpx | 62.94 s | 18.73 s | **−70%** | 改善 |
| 2' | H.264 → H.264 | video_toolbox / video_toolbox | 6.83 s | 2.92 s | **−57%** | 改善 (openh264 代替) |
| 3 | VP9 → AV1 | libvpx / svt_av1 | 49.40 s | 10.11 s | **−80%** | 改善 |
| A | H.265 → H.265 | video_toolbox / video_toolbox | 6.50 s | 2.55 s | **−61%** | 改善 |

- ケース 2 の openh264 本線は未計測 (上記「現状」参照)。2' は同一入力・同一 VT エンジンでの比較。
- 任意ケース B (NVENC) は環境不足で未実施。
- Linux は未実施。
- ソース 3 本での再計測は未実施 (方針上は 3 本を正とする)。

### run-01 の processor 内訳メモ (参考・単発)

`stats.json` の `processors[].total_processing_seconds` 合算。壁時計中央値とは一致しないが、差の所在の当たりになる。

| ケース | 側 | decoder | mixer | encoder |
|---|---|---|---|---|
| VP9→VP9 | hisui | 33.2 s | 0.41 s | 28.5 s |
| VP9→VP9 | SAC | 9.2 s | 0.41 s | 9.3 s |
| VP9→AV1 | hisui | 35.0 s | 0.44 s | 14.5 s |
| VP9→AV1 | SAC | 9.4 s | 0.46 s | 0.22 s |
| H.264 VT | hisui | 5.95 s | 0.42 s | 0.33 s |
| H.264 VT | SAC | 1.62 s | 0.43 s | 0.82 s |
| H.265 VT | hisui | 5.85 s | 0.44 s | 0.36 s |
| H.265 VT | SAC | 1.36 s | 0.43 s | 0.70 s |

- **mixer はほぼ同等**。差の主因は decoder / encoder 側。
- ソフトコーデック (VP9 / AV1) では SAC の decode・encode 双方が大幅に短い。
- VT 系では SAC の decode が短く、encode 処理秒は hisui より長いが、壁時計全体では SAC が勝つ。

### 判定まとめ

- 計測した全ケースで **10% 超の悪化は無し**。いずれも改善。
- デグレ起票は不要。
- **残作業**: ソース 3 本の確認計測 (任意)、openh264 / Linux / NVENC (環境次第)。改善理由の深追いは必須としない。

### 改善理由の当たり (参考・根拠は弱い)

**断定しない。** 依存版の突き合わせと processor 内訳からの仮説に過ぎず、同一 crate 版での再計測や品質突合はしていない。改善の公式説明ではなく、後で読む人向けの **参考情報**。

アプリ側の encode 既定 (VP9 `cpu_used: 9` / `deadline: realtime` / `threads: 1`、AV1 `enc_mode: 13`) や `thread-count 1` の scheduler 設計は hisui 2025.3.3 と大きくは違わなさそうに見える。差の **候補** のひとつはネイティブコーデック / ラッパーの世代差。

| 依存 | hisui 2025.3.3 | SAC (計測時点) | 内蔵 upstream の差 (ラッパー metadata) |
|---|---|---|---|
| `shiguredo_libvpx` | 2025.1.0 | 2026.2.0-canary.1 | libvpx v1.15.2 → v1.16.0 |
| `shiguredo_svt_av1` | 2025.1.0 | 2026.2.0 | SVT-AV1 v3.1.2 → v4.2.0 |
| `shiguredo_dav1d` | 2025.1.0 | 2026.2.0 | (付帯) |
| `shiguredo_mp4` / `libyuv` / `video_toolbox` | 2025.x | 2026.x | 版差あり |

- VP9 の decode/encode 短縮や AV1 encode の大幅短縮は、上記世代差と **方向が一致する** 程度の観察。因果は未検証。
- mixer がほぼ同等な点も、「アプリ再設計より codec 実体」仮説と矛盾しない、という以上ではない。
- VT 経路はラッパー変更で `total_processing_seconds` の帰属がずれうる (encode 積算が長くても壁時計が短い、など)。指標差と実コストは分けて見る。
- 未確認のまま残すもの: 同一 crate 版に揃えた再計測、出力品質・フレーム数の突合、Linux。

### 再現手順

詳細は `scripts/README.md`。要点のみ:

```bash
cargo build --release
SAC=target/release/sora-archive-compositor
HISUI="$HOME/.cargo/bin/hisui"   # 例: hisui 2025.3.3
BASE=/tmp/perf-run
RUNS=5

# 入力 (方針どおりなら --source-count 3)
python3 scripts/perf_compose.py prepare-input \
  --generator-bin "$SAC" --out-dir "$BASE/input-vp9" \
  --duration 120 --source-count 3 --codec VP9

# hisui / SAC を同じ out-dir にラベル分けして計測
python3 scripts/perf_compose.py run \
  --bin "$HISUI" --input-dir "$BASE/input-vp9" \
  --video-codec VP9 --runs "$RUNS" --out-dir "$BASE/results-vp9" \
  --label hisui-2025.3.3 --thread-count 1

python3 scripts/perf_compose.py run \
  --bin "$SAC" --input-dir "$BASE/input-vp9" \
  --video-codec VP9 --runs "$RUNS" --out-dir "$BASE/results-vp9" \
  --label sac --thread-count 1

python3 scripts/perf_compose.py compare \
  --out-dir "$BASE/results-vp9" \
  --baseline-label hisui-2025.3.3 \
  --candidate-label sac
```

H.264 / H.265 / AV1 も `--codec` / `--video-codec` を差し替える。H.264 を VT で測る場合は `--openh264` を付けない。
