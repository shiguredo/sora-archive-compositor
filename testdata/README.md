# `archive-h264-resolution-change.mp4` / `archive-h265-resolution-change.mp4`

多エントリ `stsd` (sample_entry が 1 トラック内で切り替わる) の解像度変更の回帰テスト用データ。
nvcodec デコーダーが sample_entry 変化に伴う SPS / PPS (H.264) / VPS / SPS / PPS (H.265) 更新を
追従できることを検証する。

- **構成**: 多エントリ stsd (entry_count=3)。15 fps × 3 秒 = 45 フレームで、キーフレームが frame 0 / 15 / 30 にある
  - frame 0..15 → 320x240
  - frame 15..30 → 224x160
  - frame 30..45 → 320x240
- **前提**: キーフレーム先頭に in-band のパラメータセット (SPS / PPS / VPS) は含まれない。
  再生成する場合は、利用するエンコーダーによっては IDR 先頭にパラメータセットが付くことがある。
  その前提が崩れると回帰テストがデータ原因で失敗するため、再生成後は必ずキーフレーム先頭の
  NAL 種別を確認すること。
- **生成方法** (シードやエンコーダー差でビット単位の一致は保証しない):
  ```
  sora-archive-compositor generate-archive /path/to/output/ \
    --connection-id resolution-change \
    --duration 3 \
    --frame-rate 15 \
    --resolution 320x240 \
    --resolution-change 1:224x160 \
    --resolution-change 2:320x240 \
    --codec h264   # または h265
  ```
