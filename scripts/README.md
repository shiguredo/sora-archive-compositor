# scripts/

録画合成 (`compose`) の性能計測用スクリプトを置く。

## `perf_compose.py`

hisui / sora-archive-compositor のどちらのバイナリでも `--bin` で差し替えて測れる。
入力は `generate-archive` で作った映像のみのダミー録画を想定する。

依存は Python 標準ライブラリのみ。

### 前提

```bash
cargo build --release
SAC=target/release/sora-archive-compositor
# 例: cargo install した hisui
HISUI="$HOME/.cargo/bin/hisui"
```

### 入力の準備

```bash
BASE=/tmp/perf-run

python3 scripts/perf_compose.py prepare-input \
  --generator-bin "$SAC" \
  --out-dir "$BASE/input-vp9" \
  --duration 120 \
  --source-count 3 \
  --codec VP9
```

- 既定: 120 秒 / `1280x720` / 30 fps / ソース 3 本 / seed 1 起算
- H.264 で openh264 を使う場合は `--openh264 /path/to/libopenh264.dylib` を付ける
- macOS で H.264 / H.265 を VideoToolbox 経路で作る場合は `--openh264` を付けない

### 計測

同じ `--out-dir` にラベルごとのディレクトリが作られる。先頭 1 回はウォームアップ (集計から除外)。

```bash
RUNS=5

python3 scripts/perf_compose.py run \
  --bin "$HISUI" \
  --input-dir "$BASE/input-vp9" \
  --video-codec VP9 \
  --runs "$RUNS" \
  --out-dir "$BASE/results-vp9" \
  --label hisui-2025.3.3 \
  --thread-count 1

python3 scripts/perf_compose.py run \
  --bin "$SAC" \
  --input-dir "$BASE/input-vp9" \
  --video-codec VP9 \
  --runs "$RUNS" \
  --out-dir "$BASE/results-vp9" \
  --label sac \
  --thread-count 1
```

- `--video-codec` を指定すると、最小 layout (`audio_sources: []` + 指定 codec) を out-dir 内に書いて使う
- 既存 layout を使う場合は `--layout PATH` ( `--video-codec` と併用しない)

### 集計・比較

```bash
python3 scripts/perf_compose.py summarize --out-dir "$BASE/results-vp9"

python3 scripts/perf_compose.py compare \
  --out-dir "$BASE/results-vp9" \
  --baseline-label hisui-2025.3.3 \
  --candidate-label sac
```

`elapsed_seconds` の中央値差分率と判定 (`improvement` / `regression` / `within_tolerance`) を JSON に書く。

### 他コーデックの例

```bash
# VP9 入力 → AV1 出力
python3 scripts/perf_compose.py run \
  --bin "$SAC" --input-dir "$BASE/input-vp9" \
  --video-codec AV1 --runs "$RUNS" \
  --out-dir "$BASE/results-vp9-av1" --label sac

# H.265 入力 (VT 生成) → H.265 出力
python3 scripts/perf_compose.py prepare-input \
  --generator-bin "$SAC" --out-dir "$BASE/input-h265" \
  --duration 120 --source-count 3 --codec H265

python3 scripts/perf_compose.py run \
  --bin "$SAC" --input-dir "$BASE/input-h265" \
  --video-codec H265 --runs "$RUNS" \
  --out-dir "$BASE/results-h265" --label sac
```

### 成果物

各 run ディレクトリにだいたい次が入る。

- `meta.json` … 壁時計・RSS・compose の要約
- `stats.json` … `--stats-file` の詳細
- `stdout.json` … compose の stdout
- `output.mp4` … 合成結果 (比較用。コミットしない)
