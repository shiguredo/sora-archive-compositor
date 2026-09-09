# tune の結果に基づくデフォルトパラメータの決定

- Created: 2026-08-04
- Completed: 2026-09-09
- Branch: feature/tune-default-params
- Polished: {YYYY-MM-DD}

## 目的

コーデック crate 更新後に、`layout-examples/compose-default.jsonc` の既定値を `tune` で作り直す必要があるかを判断する。必要ならパレートフロントから既定を決める。不要なら既存既定を維持し、利用可能パラメーターの変化だけを記録する。

## 現状

- `issues/closed/0002-other-performance-parity-audit.md` の macOS 計測では、hisui 2025.3.3 に対して VP9 / AV1 / Video Toolbox いずれも改善だった (openh264 本線と Linux / NVENC は未計測)
- 比較対象のエンコード既定は hisui と大きくは違わず、差の主因候補はコーデック世代差である
- `layout-examples/compose-default.jsonc` の数値・フラグは、廃止キーを除いて hisui 2025.3.2 と一致している
- 新規公開パラメーターは `compose-default.jsonc` に書いておらず、省略時はライブラリ既定になる
- `search-space-examples/full.jsonc` と `docs/layout_encode_params.md` は、`shiguredo_svt_av1` 2026.2.0 の検証範囲に合わせて直した
- tune の入力は `generate-archive` で生成できる。`tune.yml` で手動実行できる

## 設計方針

- **既定値は `tune` で作り直さない**。0002 の結果と crate 差分の突き合わせから、既存既定を変える根拠がない
- 新規パラメーター (HDR、スーパーレゾリューション、量子化マトリクスなど) はコンテンツ依存なので、既定では省略してライブラリ既定に任せる
- 明らかに変えるべき既定が見つかった場合のみ `compose-default.jsonc` を触る。今回の調査では該当なし
- 比較対象は hisui 2025.3.2 とする。2025.3.3 は nvcodec 更新の追従が途中のため、今回の差分対象から外す
- 利用可能パラメーターの棚卸しは `issues/0004-doc-organize-migration-doc.md` の移行ドキュメントで使う
- 探索空間は crate が拒否する値を含まないこと。tune 用の探索範囲は crate 範囲の部分集合であってよい

## 完了条件

- hisui との性能比較 (0002) により、既存既定での劣化がないことを本文に残している
- コーデック crate 更新による利用可能パラメーターの差分を本文に記録している
- 明らかに変えるべき既定値がないと判断し、`compose-default.jsonc` の数値は維持している
- `search-space-examples/full.jsonc` と `docs/layout_encode_params.md` の範囲が `shiguredo_svt_av1::EncoderConfig` の検証と rustdoc に一致している

## 依存

- `issues/closed/0002-other-performance-parity-audit.md` (性能比較。closed)
- `issues/0004-doc-organize-migration-doc.md` (移行ドキュメント側で本 issue の棚卸しを使う)

## 調査結果 (2026-09-09)

比較基準は hisui 2025.3.2。SAC 側の crate は調査時点の `Cargo.toml` どおり。

| crate | hisui 2025.3.2 | SAC | upstream |
|---|---|---|---|
| `shiguredo_libvpx` | 2025.1.0 | 2026.2.0-canary.1 | libvpx v1.15.2 → v1.16.0 |
| `shiguredo_svt_av1` | 2025.1.0 | 2026.2.0 | SVT-AV1 v3.1.2 → v4.2.0 |
| `shiguredo_openh264` | 2025.1.0 | 2026.2.0 | OpenH264 v2.6.0 のまま |
| `shiguredo_video_toolbox` | 2025.1.0 | 2026.2.0-canary.2 | API 再構成 |
| `shiguredo_nvcodec` | 2025.2.1 | 2026.3.0-canary.0 | Video Codec SDK 13.0.19 のまま。ラッパ再構成 |

### `compose-default.jsonc` の実差分

速度・品質を決める数値は動かしていない。

- SVT-AV1: `enable_tpl_la` / `force_key_frames` / `pin_threads` / `tier` を削除 (指定しても無視)
- Video Toolbox: `use_parallelization` を削除 (指定しても無視)
- nvcodec decode: `reconfigure_enabled: false` を追加 (`shiguredo_nvcodec::DecoderConfig` の推奨どおり)

### レイアウト JSON で変わったキー

- **libvpx**: 公開キーは実質そのまま。crate の `Vp9Config.profile` と `image_format` は JSON 未公開
- **OpenH264**: `entropy_coding_mode` (`"cavlc"` / `"cabac"`) を追加。旧 `entropy_coding` (bool) は互換のため残す。crate の `level` は JSON 未公開
- **SVT-AV1**: 追加が多い (品質、レート制御、GOP、スーパーレゾリューション、HDR など)。廃止キーは `pred_structure` / `pin_threads` / `target_socket` / `enable_tpl_la` / `force_key_frames` / `recon_enabled` / `encoder_bit_depth` / `encoder_color_format` / `profile` / `level` / `tier`。`intra_period_length: -1` は `NonZeroUsize` のためパースできない
- **Video Toolbox**: `data_rate_limits` を追加。`use_parallelization` は無視。JSON キー `prioritize_speed_over_quality` は crate の `prioritize_encoding_speed_over_quality` に対応
- **nvcodec encode**: hisui 2025.3.2 から公開キーは増えていない
- **nvcodec decode**: `reconfigure_enabled` のみ追加

### 既定を変えない判断

次は根拠不足のため変更しない。

- VP9 `deadline` を `"good"` にする
- AV1 `enc_mode` 13 を crate 既定の 8 にする
- NVENC `tuning_info` を `"high_quality"` にする (0002 で未計測)
- OpenH264 の `deblocking_filter` を true にする
- Video Toolbox H.264 を Main + CABAC にする
- SVT-AV1 の新規キーを既定で埋める

### 探索空間・ドキュメントで直したずれ

`shiguredo_svt_av1::Encoder::validate_config` および `EncoderConfig` の rustdoc に合わせた。

- 廃止キー `pred_structure` を探索空間から削除した
- `aq_mode` を 0〜2 にした (3 以上は `Encoder::new` が拒否する)
- `intra_period_length` の下限を 1 にした (`-1` はパースできない)
- `sharpness` を -7〜7、`screen_content_mode` を 0〜3、`tune` に `"iq"` / `"ms_ssim"` を足した
- `fast_decode` / `enable_dlf_flag` / `enable_tf` を 0〜2 の整数にした (真偽値はパーサーが互換で受け付ける)
- `enable_restoration_filtering` を -1〜1 にした
- `tile_columns` を 1〜4 (log2) にした
- `docs/layout_encode_params.md` の「指定可能な範囲」を探索空間の部分集合ではなく crate の受理範囲に直した

## 解決方法

既存既定を `tune` で作り直す必要はないと判断し、`compose-default.jsonc` の数値は維持したうえで closed にした。

- 0002 の macOS 計測で、既存既定での劣化は確認されなかった
- crate 差分を突き合わせ、明らかに変えるべき既定はなかった
- 新規パラメーターは省略し、ライブラリ既定に任せた
- `search-space-examples/full.jsonc` と `docs/layout_encode_params.md` の範囲を crate 検証に合わせた
- 利用可能パラメーターの棚卸しは `issues/0004-doc-organize-migration-doc.md` へ渡した
