# 時間指定した範囲だけの合成方法を確認する

- Created: 2026-09-10
- Completed: 2026-09-10
- Branch: feature/other-time-range-composition
- Polished: {YYYY-MM-DD}

## 目的

Hisui の Sora 録画合成機能に対する要望として、時間指定した範囲だけを合成したいというものがあった。本リポジトリは同機能を引き継いでいるため、現行実装での実現方法を確認する。

## 現状

- ソースの表示区間は、archive JSON の `start_time_offset` と `stop_time_offset` から決まる (`src/metadata.rs` の `ArchiveMetadata` / `src/layout.rs` の `AggregatedSourceInfo`)
- 同じ `connection_id` の分割録画は 1 つのソースとして扱われ、表示区間は分割ファイル全体の最小 `start_time_offset` と最大 `stop_time_offset` になる (`src/layout.rs` の `AggregatedSourceInfo::update`)
- 分割録画はファイルごとに `start_time_offset` と `stop_time_offset` の範囲を持つため、指定するファイルによって合成対象の時間範囲を選べる
- `trim` (既定有効) により、ソースが存在しない区間は合成結果から除去される (`src/layout.rs` の `decide_trim_spans`)
- 冒頭でソースが存在しない区間は `trim` の値に関わらず常に除去され、出力は最初のソースの `start_time_offset` の地点から始まる (`docs/layout_region.md` の `trim` の扱いを参照)
- 1 つのメディアファイルの途中から任意の時間範囲だけを切り出す専用オプションはない

## 解決方法

指定したい時間範囲に存在するソースだけを `audio_sources` と `video_layout.$REGION_NAME.video_sources` に指定すれば、その時間範囲だけが合成結果に残る。`trim` が既定で有効なため、ソースが存在しない区間は自動的に除去される。分割録画の場合には、範囲に対応する `split-archive-*.json` だけを指定すればよい。

出力尺の確認結果は次のとおりである。

- 0-10 秒、10-20 秒、20-30 秒の 3 分割ファイルのうち、10-20 秒のファイルだけを指定した場合の出力尺は 10 秒
- 0-10 秒と 20-30 秒の 2 ソースを指定した場合の出力尺は 20 秒 (間の 10 秒は `trim` で除去される)

コード変更は不要であり、既存機能で実現できることを確認して closed にする。
