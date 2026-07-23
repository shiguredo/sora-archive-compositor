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

- `compose` サブコマンドは `src/subcommand_compose.rs` → `src/composer.rs` → `src/encoder.rs` の経路で動作する。stdout JSON に `elapsed_seconds` と、encoder / decoder / mixer / muxer 別の `total_*_processing_seconds` (`total_audio_decoder_processing_seconds`, `total_video_decoder_processing_seconds`, `total_audio_encoder_processing_seconds`, `total_video_encoder_processing_seconds`, `total_audio_mixer_processing_seconds`, `total_video_mixer_processing_seconds` 等) を出力するため、内訳ベースでの比較が可能。
- 移植過程の変更のうち性能へ影響を与えうる主なもの:
  - log → tracing : ログのフォーマット・ANSI 色付け・stderr 経由の重さ
  - orfail 撤廃 : エラー型の変更に伴うホットパスの `Result` サイズ差
  - indicatif 撤廃 : `src/progress.rs` の内製プログレスバー
  - `shiguredo_*` crates.io 版へ更新 : 依存 codec / MP4 writer の版差
  - fdk-aac 実行時ロード : Linux のみ
  - audio_toolbox cfg 整理 : macOS のみ
  - NVENC EOS flush 修正 / async backpressure 導入 : NVENC 経路のみ
- 現状 `sora-archive-compositor` にも `hisui@2025.3.2` にも、両者を突き合わせる性能計測スクリプトは存在しない (`ls scripts/` は 0、`hisui@2025.3.2` の `git ls-tree` でも同様)。
- `testdata/` 直下の Sora 録画サンプル (`archive-blue-640x480-*.mp4`, `archive-red-320x320-*.mp4`, `archive-black-silent.webm`) は 1〜数秒規模の小サイズ (数 KB) で、`testdata/e2e/*/` 配下も同様。**実運用に近い 1〜数分規模の入力は既存 testdata に含まれない**。本 issue の実施範囲でサンプル収録もしくは同等サンプルの生成が必要になる (再現手段は本 issue 中で確立する)。

## 設計方針

### 対象範囲

- **対象**: `compose` サブコマンドの録画合成処理のみ。`tune` / `vmaf` / `inspect` / `list-codecs` はスコープ外。
- **計測項目 (最低)**:
  - 総処理時間 (compose の壁時計時間 = stdout JSON の `elapsed_seconds`、および外部からの `/usr/bin/time -p` 計測)
  - stdout JSON の `total_*_processing_seconds` (audio/video の encoder / decoder / mixer)
  - CPU 利用の傾向 (Linux で `/usr/bin/time -v` が使えれば `Elapsed (wall clock)` / `User time` / `System time` / `Percent of CPU this job got` / `Maximum resident set size (kbytes)` を採取)
  - macOS では `/usr/bin/time -l` で peak RSS 相当を採取 (`maximum resident set size`)
- **計測しない項目**: `tune` サブコマンドのパレートフロント収束速度、`vmaf` のスコア絶対値、encoder の bit-exact な出力差 (これらは書面の差分監査の担当領域、または libvmaf 版差の範囲で許容済み)。

### 比較対象のコーデック組み合わせ

以下を最低ケースとして選ぶ (`compose` の主要経路を代表)。

| # | 映像入力 | 音声入力 | 出力映像 encoder | 出力音声 encoder | プラットフォーム | 備考 |
|---|---|---|---|---|---|---|
| 1 | VP9 | Opus | VP9 (libvpx) | Opus | Linux / macOS | 定番。既定 codec。トランスコード有 |
| 2 | H.264 | Opus | H.264 (openh264) | Opus | Linux / macOS | openh264 経路の代表 |
| 3 | H.265 | Opus | H.265 (VideoToolbox) | Opus | macOS のみ | macOS 経路の代表。hisui 側でも同様 |
| 4 | VP9 | Opus | AV1 (svt-av1) | Opus | Linux / macOS | svt-av1 経路 |
| 5 | H.264 | Opus | H.264 (openh264) | AAC (fdk-aac) | Linux のみ | fdk-aac 実行時ロードの影響を測る |
| 6 (任意) | H.264 | Opus | H.264 (NVENC) | Opus | Linux (NVIDIA GPU 環境) | 環境がある場合のみ |

環境固有の case (macOS, NVENC, fdk-aac) は「計測できたら残す」扱い。1・2・4 は Linux / macOS どちらでも実施する。

### テストデータの選定

- **既存 `testdata/` の Sora 録画サンプルは短すぎる**ため本 issue では使わない。既存サンプルは integration テスト用途で、性能比較には向かない。
- 本 issue のための計測用サンプルを別途用意する。要件は「Sora の実運用に近い 1〜数分の録画」で、以下いずれかで用意する:
  - Sora の canary / dev 環境でダミー通話を録音した mp4 / webm を、`testdata/` とは別ディレクトリ (例: `testdata/perf/` 案) に配置する。**個人特定可能な音声・映像は絶対に含めない** (`shiguredo-no-secrets` 準拠)。
  - もしくは既存の合成可能な素材 (フリー素材の映像 + 無音) から Sora 録画互換フォーマット (`.mp4` / `.webm`) を作って配置する。
- 選定したサンプルはコミットせずローカルで扱うか、公開に問題ない素材のみコミットするかを実装時に判断する。**現時点では issue 本文に「〇〇のサンプルを使った」記載はしない (機密回避)**。実施記録に「素材の入手経路 / 生成方法」だけ書き残す。

### 計測環境と手順

- **同一ホスト**で hisui 2025.3.2 と sora-archive-compositor の両方を計測する (ハードウェア差を排除)。macOS 上での hisui build には `../hisui` の worktree ないしは `git worktree add ../hisui-2025.3.2 2025.3.2` を用いる。
- **リリースビルド** (`cargo build --release`) で計測する。debug ビルドは使わない。
- **同一入力・同一 layout.json** で両バイナリを実行する。hisui 側 layout の `HISUI_*` 環境変数と、sora-archive-compositor 側の `SORA_ARCHIVE_COMPOSITOR_*` 環境変数の対応に注意する。layout.json は両者で同じキー構成 (encode_params 系) を使えるので同一ファイルを流用する (`docs/layout_encode_params.md` を参照)。
- **実行回数**: 各ケースにつき最低 3 回、可能なら 5 回。**最初の 1 回はウォームアップとして破棄** し、残りの中央値 (または平均) を比較値とする。
- **非対称な回し方をしない**: 「hisui は 5 回・sora-archive-compositor は 1 回」といった条件不揃いは禁止。同数回す。
- **他プロセス影響の低減**: 計測中は他の重いプロセスを停止する。macOS ではスリープ抑制 (`caffeinate`) を使う判断を実装時に行う。
- 計測項目の採取は shell script 相当で機械的に行う。実装時に `scripts/perf-compare.sh` 等を追加してよい (再現可能性を残す。ただし機密素材のパスは script 内に書かない・環境変数で渡す形にする)。

### 判定基準

- **10% 以内の悪化は許容**: 計測ノイズ + 依存 crate の版差 + ANSI 色付き tracing のオーバーヘッド等の範囲。ただし複数ケース (2 ケース以上) で **一貫して 5% 前後の悪化** が観測される場合は、原因の当たりだけ本文に書き残す (別 issue にするかは追って判断)。
- **10% を超える悪化**: 要調査扱い。原因分析の当たりを付け、修正 issue を別途 `create-issue` 経由で起票する。**修正の実装完了は本 issue の完了条件に含めない**。書面の差分監査の派生 issue の扱いに揃える。
- **改善方向 (sora-archive-compositor が速い)**: 記録に残し、原因の推定を書く。改善は許容だが「なぜ速くなったか」を書けないままだと後で degrade したときの参照点として使えないので、当たりだけでも記録する。

### 出力物

- 本 issue 本文の「性能比較結果」節に、ケースごとに以下を追記する:
  - ケース番号 / コーデック組み合わせ / プラットフォーム / 入力サンプルの概要 (機密回避のため詳細は書かない)
  - hisui 2025.3.2 の計測値 (elapsed / 内訳 / peak RSS)
  - sora-archive-compositor の計測値
  - 差分率と判定 (許容 / 悪化 / 改善)
  - 考察 (推定原因、または追跡不要な理由)
- 派生 issue を起票した場合は番号を書き戻す。
- 計測に使ったスクリプト (追加した場合) と手順は「再現手順」小節にコマンドラインベースで残す。

### 本 issue ではコード変更を行わない (原則)

- 主目的は監査であり、性能改善のためのコード修正は本 issue のスコープ外。悪化を検出しても本 issue で修正しない (別 issue で対応)。
- 例外は「計測のための script 追加」のみ。`scripts/` 配下への追加は許容 (`develop_direct` 方針でも develop 直接コミットで進めてよい)。
- Rust コード (`src/**/*.rs`) の変更は本 issue のブランチでは原則行わない。

## 完了条件

- hisui 2025.3.2 と sora-archive-compositor の両方を同一ホスト・同一入力・同一実行回数 (最低 3 回、うち初回破棄) で計測した結果が本 issue 本文の「性能比較結果」節に追記されている。
- 最低ケース (VP9→VP9, H.264→H.264, VP9→AV1) が Linux / macOS のどちらか (または両方) で計測済みで、環境固有ケース (VideoToolbox H.265, fdk-aac, NVENC) は「実施した」または「環境不足で未実施」のいずれかが明記されている。
- 各ケースについて差分率と判定 (許容 / 悪化 / 改善) が記録されている。
- 10% を超える悪化が観測された場合、修正のための別 issue が `create-issue` 経由で起票され、その番号が本文に書き戻されている。
- 5% 前後の一貫した悪化が観測された場合、原因の当たり (または「深追い不要」の判断根拠) が本文に記載されている。
- 計測に使ったスクリプト (追加した場合) と再現手順が「再現手順」節にコマンドラインで残っている。
- **本 issue の完了は上記の追記まで**。派生 issue の実装完了・polished・closed は本 issue の完了条件に含めない。

## 解決方法

### 実施ステップ

1. **`hisui@2025.3.2` の release バイナリを準備する**:
   - `git -C ../hisui worktree add ../hisui-2025.3.2 2025.3.2` で 2025.3.2 の worktree を切る (もしくは既存 worktree があればそれを使う)。
   - `cargo build --release` で `../hisui-2025.3.2/target/release/hisui` を得る。ビルドが通らない (依存の変化などで) 場合は原因を記録し、実施時点で最も近い hisui のバイナリを使う判断を行う (この場合は「厳密には 2025.3.2 と等価ではない」旨を実施記録に残す)。
2. **sora-archive-compositor の release バイナリを準備する**:
   - `cargo build --release` で `target/release/sora-archive-compositor` を得る。
3. **計測用サンプルを用意する**:
   - 「設計方針: テストデータの選定」に従い、実運用に近い 1〜数分規模の Sora 録画互換サンプルを用意する。機密回避のためコミットするか否かは実装時に判断。
4. **layout.json を用意する**:
   - `layout-examples/compose-default.jsonc` (sora-archive-compositor) と `../hisui-2025.3.2/layout-examples/compose-default.jsonc` (hisui) を比較し、両者で同一に動く layout.json を用意する。差分がある場合は、性能比較目的では意味論的に同等な設定に揃える (製品名 URL 等の cosmetic 差は無視)。
5. **計測スクリプトを用意する** (任意、`scripts/perf-compare.sh` 案):
   - 引数として (a) バイナリパス、(b) 入力ディレクトリ、(c) layout.json、(d) 実行回数を取り、`/usr/bin/time -p` / `/usr/bin/time -v` (Linux) / `/usr/bin/time -l` (macOS) 経由で計測して stdout JSON もあわせて保存する形。
   - スクリプト内には機密素材のパスを書かない (環境変数か引数で渡す)。
6. **各ケースを計測する**:
   - 「設計方針: 比較対象のコーデック組み合わせ」の表に沿って、hisui / sora-archive-compositor をそれぞれ最低 3 回 (初回破棄) 実行する。
   - 同ケース内では実行順序を hisui → sora-archive-compositor で交互に回す (熱状態の偏りを減らす)。
7. **結果を集計する**:
   - ケースごとに elapsed_seconds、`total_*_processing_seconds` 内訳、peak RSS の中央値を計算する。
   - hisui 2025.3.2 に対する sora-archive-compositor の差分率を計算する: `(sora - hisui) / hisui * 100 [%]`。
   - 判定を付ける (許容 / 悪化 / 改善)。
8. **本 issue 本文に「性能比較結果」節を追記する**:
   - ケースごとの表と考察を書き残す。
   - 派生 issue を起票する必要があれば `create-issue` スキル経由で起票し、番号を本文に書き戻す。
9. **再現手順を残す**:
   - コマンドラインベースで、`scripts/perf-compare.sh` (追加した場合) の使い方と、layout.json / サンプルの用意方法を残す。

### リスク・留意点

- **hisui 2025.3.2 のビルド失敗リスク**: 依存 crate や toolchain の変化でビルドが通らない可能性がある。その場合は最近の hisui release バイナリ (公式配布 tar など) を使うか、hisui 側の Cargo.lock を厳密に維持した状態でビルドを試みる。ビルド不能な場合は本 issue の実施を保留し、対処方針を issue 内に記録する。
- **計測ノイズ**: 単一ホストでも他プロセスの影響で数 % はブレる。この issue の判定基準は 10% を境にしているので、複数回計測と中央値の採用でノイズを吸収する。
- **プラットフォーム偏り**: VideoToolbox / NVENC / fdk-aac のように環境固有の経路は片方の OS でしか計測できない。網羅は完了条件から外し、「実施した」または「環境不足で未実施」の明記のみを完了条件にする。
- **layout.json の差**: `HISUI_*` → `SORA_ARCHIVE_COMPOSITOR_*` の env 変数リネームなど、layout.json 側で書き分けが必要な場合は意味論的に同等な設定に揃える。
- **機密素材の扱い**: Sora 録画を実運用データで作る場合、`shiguredo-no-secrets` に従って個人特定可能な音声・映像を含めない。issue 本文にサンプル内容を書かない。

## 参考

- hisui 2025.3.2 との書面ベース差分監査: 機能差の棚卸し。本 issue は「性能差の棚卸し」で対を成す。
- 既存 testdata: integration テスト用途。性能比較用には短すぎる。
- `HISUI_*` → `SORA_ARCHIVE_COMPOSITOR_*` リネーム: layout.json 揃えの参考。
- `shiguredo_*` crates.io 版への更新: 依存 crate 版差の識別に使う。
- NVENC EOS flush / async backpressure: NVENC 経路の変更点。

## 性能比較結果

(実施時に追記)

### 再現手順

(実施時に追記)
