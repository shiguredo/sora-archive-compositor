# 特定のコネクション ID が参加している間の映像を切り出す方法を確認する

- Created: 2026-09-10
- Completed: {YYYY-MM-DD}
- Branch: feature/other-connection-segment-extraction
- Polished: {YYYY-MM-DD}

## 目的

Hisui の Sora 録画合成機能に対する要望として、特定のコネクション ID が参加している間の映像だけを切り出したいというものがあった。本リポジトリは同機能を引き継いでいるため、現行実装での実現方法を確認する。

## 現状

- `audio_sources` と `video_layout.$REGION_NAME.video_sources` には、特定コネクションの archive JSON だけを指定できる (`docs/layout_region.md` / `docs/layout_spec.md`)
- 同じ `connection_id` の分割録画は 1 つのソースとして扱われ、表示区間は `start_time_offset` と `stop_time_offset` から決まる (`src/layout.rs` の `AggregatedSourceInfo`)
- `trim` (既定有効) により、ソースが存在しない区間は合成結果から除去される (`src/layout.rs` の `decide_trim_spans`)
- 冒頭でソースが存在しない区間は `trim` の値に関わらず常に除去される
