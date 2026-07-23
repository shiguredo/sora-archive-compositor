# VideoToolbox / NVCODEC のエンコード・デコードエラーが握り潰され、壊れた出力が success 扱いになる

- Created: 2026-08-04
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-video-engine-error-swallowing
- Polished: 2026-08-20

## 目的

VideoToolbox / NVCODEC のエンコード・デコードエラーをログ出力のみで破棄している実装をやめ、エラー発生時に合成結果を失敗扱い (`ComposeResult.success == false`) にする。

局所的に `next_*` の戻り値だけを `Result` 化するのではなく、コールバック結果キューの契約ごと fail-fast に直す。

## 現状

VideoToolbox / NVCODEC はコールバックで結果を積む。4 エンジンそれぞれがほぼ同じ `ok_frames: VecDeque` + `errors: VecDeque` を持ち、`next_encoded_frame` / `next_decoded_frame` はエラーを `tracing::error!` で捨てて `Option` を返す。

- `src/encoder_video_toolbox.rs` の `VideoToolboxEncoder::next_encoded_frame`
- `src/decoder_video_toolbox.rs` の `VideoToolboxDecoder::next_decoded_frame`
- `src/encoder_nvcodec.rs` の `NvcodecEncoder::next_encoded_frame`
- `src/decoder_nvcodec.rs` の `NvcodecDecoder::next_decoded_frame`（libyuv / `new_i420` 失敗も `None`）

呼び出し側の `src/encoder.rs` / `src/decoder.rs` は `process_input` 内で `while let Some(...) = self.inner.next_*()` のあと常に `Ok(())` を返す。`src/scheduler.rs` の `TaskRunner` は `Err` を受けたときだけ集約 `Stats.error` を立てる。`src/composer.rs` の `ComposeResult.success` は集約 `Stats.error` のみを見る。

そのためエンジンエラーが起きても合成は `success: true` のまま完走する。VideoFrame スライス操作まわりの握り潰し解消も本 issue の管轄とする。

この 4 重複キューは、エラーをリストに積む前提そのものが合っていない。一度失敗したらそのエンジンは使わず処理を終えるべきで、複数エラーを溜めても呼び出し側は最初の 1 件で止まる。nvcodec-rs のデコーダ終端（最初のエラーを 1 件だけ保持し、以降はデコードしない）と同じ契約が必要。

`next_*` を `Result` にするだけの局所修正は、このキュー設計を残したまま握り潰しだけを塞ぐ形になり、変更対象と契約が中途半端になる。一度その方針で着手したが、差分を見て取り下げた。

## 設計方針

- コールバック結果キューを 1 つの型にまとめる（仮称 `OutputQueue<T>`。`src/output_queue.rs` などエンジン共通の置き場所）。`error.rs` には置かない
- エラーは `VecDeque` にしない。最初の 1 件で終端する (`error: Option` + 終端フラグ)
- 終端後の成功フレームは捨てる。後続エラーは無視する。`pop` は原因エラーを 1 回返し、以降は終端済みを返す
- `VideoEncoder` / `VideoDecoder` の `process_input` がその `Err` を返す。既存の `TaskRunner` 経路で集約 `Stats.error` を立て、パイプラインを止める
- プロセッサ個別の `VideoEncoderStats.error` / `VideoDecoderStats.error` だけを立てて完了とみなさない
- 全エンジンへ `next_*_result()` のような別名 API を足して揃える必要はない。届け先は `process_input` の `Err` でよい
- 出力ファイルの削除・一時名リネームは対象外。合成エラー時の残骸 MP4 残置は許容済み
- VideoToolbox デコーダ再初期化時のキュー引き継ぎも、同じ終端契約で移す

本 issue はパイプライン横断の契約変更であり、他の局所 issue より先に急がない。局所で閉じる issue を先に片付ける。

## 完了条件

- 対象 4 エンジン経路でエンコード・デコードエラーが発生したとき、`ComposeResult.success == false` になる（CLI 利用時は非 0 終了）
- エラーは最初の 1 件で終端し、リストに溜めない
- 正常系の動作が従来と変わらない
- 出力ファイルの削除は求めない（残骸 MP4 残置は許容済み）
