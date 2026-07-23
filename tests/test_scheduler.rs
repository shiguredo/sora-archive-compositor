use std::{thread, time::Duration};

use sora_archive_compositor::{
    audio::{AudioData, AudioFormat},
    media::MediaStreamId,
    processor::{
        MediaProcessor, MediaProcessorInput, MediaProcessorOutput, MediaProcessorSpec,
        MediaProcessorWorkloadHint,
    },
    scheduler::Scheduler,
    stats::ProcessorStats,
};

fn dummy_audio() -> AudioData {
    AudioData {
        source_id: None,
        data: Vec::new(),
        format: AudioFormat::I16Be,
        stereo: true,
        sample_rate: 48000,
        timestamp: Duration::ZERO,
        duration: Duration::from_millis(20),
        sample_entry: None,
    }
}

/// 出力するたびに指定時間スリープするソース
struct DelayedSource {
    output_stream_id: MediaStreamId,
    stats: ProcessorStats,
    remaining: usize,
    delay: Duration,
}

impl MediaProcessor for DelayedSource {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: Vec::new(),
            output_stream_ids: vec![self.output_stream_id],
            workload_hint: MediaProcessorWorkloadHint::CPU_MISC,
            stats: self.stats.clone(),
        }
    }

    fn process_input(
        &mut self,
        _input: MediaProcessorInput,
    ) -> sora_archive_compositor::Result<()> {
        Ok(())
    }

    fn process_output(&mut self) -> sora_archive_compositor::Result<MediaProcessorOutput> {
        if self.remaining == 0 {
            return Ok(MediaProcessorOutput::Finished);
        }
        thread::sleep(self.delay);
        self.remaining -= 1;
        Ok(MediaProcessorOutput::audio_data(
            self.output_stream_id,
            dummy_audio(),
        ))
    }
}

/// 入力を受け取るだけで中身は見ないシンク
struct Sink {
    input_stream_id: MediaStreamId,
    stats: ProcessorStats,
    eos: bool,
}

impl MediaProcessor for Sink {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: vec![self.input_stream_id],
            output_stream_ids: Vec::new(),
            workload_hint: MediaProcessorWorkloadHint::CPU_MISC,
            stats: self.stats.clone(),
        }
    }

    fn process_input(&mut self, input: MediaProcessorInput) -> sora_archive_compositor::Result<()> {
        if input.sample.is_none() {
            self.eos = true;
        }
        Ok(())
    }

    fn process_output(&mut self) -> sora_archive_compositor::Result<MediaProcessorOutput> {
        if self.eos {
            Ok(MediaProcessorOutput::Finished)
        } else {
            Ok(MediaProcessorOutput::pending(self.input_stream_id))
        }
    }
}

/// process_output で実処理した周の時間が processing に加算される
#[test]
fn processing_duration_includes_actual_work() {
    let stream_id = MediaStreamId::new(0);
    let delay = Duration::from_millis(40);
    let source_stats = ProcessorStats::other("delayed_source");
    let sink_stats = ProcessorStats::other("sink");

    let mut scheduler = Scheduler::new();
    scheduler
        .register(DelayedSource {
            output_stream_id: stream_id,
            stats: source_stats.clone(),
            remaining: 2,
            delay,
        })
        .expect("ソースの登録に失敗した");
    scheduler
        .register(Sink {
            input_stream_id: stream_id,
            stats: sink_stats,
            eos: false,
        })
        .expect("シンクの登録に失敗した");

    let stats = scheduler.run().expect("スケジューラの実行に失敗した");
    let processing = stats.processors[0].total_processing_duration().get();
    assert!(
        processing >= delay,
        "実処理のスリープが processing に入っていない: {processing:?}"
    );
}

/// 入力待ちの空 poll は processing に入らず、バックオフ sleep は waiting に入る
#[test]
fn idle_poll_is_not_counted_as_processing() {
    let stream_id = MediaStreamId::new(0);
    let waiter_stats = ProcessorStats::other("idle_waiter");

    let mut scheduler = Scheduler::new();
    scheduler
        .register(IdleWaiter {
            stream_id,
            stats: waiter_stats,
        })
        .expect("待機プロセッサの登録に失敗した");

    let timeout = Duration::from_millis(250);
    let (expired, stats) = scheduler
        .run_timeout(timeout)
        .expect("タイムアウト付き実行に失敗した");
    assert!(expired, "入力が来ない待機なのにタイムアウトしなかった");

    let processing = stats.processors[0].total_processing_duration().get();
    let waiting = stats.worker_threads[0].total_waiting_duration.get();

    assert!(
        waiting > Duration::from_millis(50),
        "バックオフ sleep が waiting に入っていない: {waiting:?}"
    );
    assert!(
        processing < waiting,
        "空 poll 待ちが processing に混ざっている: processing={processing:?} waiting={waiting:?}"
    );
}

/// 入力と出力を同じストリームにして送信側を自分で保持し、Disconnected にしない
struct IdleWaiter {
    stream_id: MediaStreamId,
    stats: ProcessorStats,
}

impl MediaProcessor for IdleWaiter {
    fn spec(&self) -> MediaProcessorSpec {
        MediaProcessorSpec {
            input_stream_ids: vec![self.stream_id],
            output_stream_ids: vec![self.stream_id],
            workload_hint: MediaProcessorWorkloadHint::CPU_MISC,
            stats: self.stats.clone(),
        }
    }

    fn process_input(
        &mut self,
        _input: MediaProcessorInput,
    ) -> sora_archive_compositor::Result<()> {
        Ok(())
    }

    fn process_output(&mut self) -> sora_archive_compositor::Result<MediaProcessorOutput> {
        Ok(MediaProcessorOutput::pending(self.stream_id))
    }
}
