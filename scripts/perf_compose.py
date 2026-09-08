#!/usr/bin/env python3
# 録画合成 (compose) の性能計測用スクリプト。
#
# hisui / sora-archive-compositor のどちらのバイナリでも、`--bin` で差し替えて測れる。
# 入力は `generate-archive` で作った映像のみのダミー録画を想定する。

from __future__ import annotations

import argparse
import json
import logging
import math
import os
import platform
import re
import statistics
import subprocess
import sys
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Final

__all__ = ["main"]

LOG: Final = logging.getLogger("perf_compose")

DEFAULT_DURATION_SECONDS: Final = 120
DEFAULT_RESOLUTION: Final = "1280x720"
DEFAULT_FRAME_RATE: Final = "30"
DEFAULT_SEED: Final = 1
DEFAULT_SOURCE_COUNT: Final = 3
DEFAULT_RUNS: Final = 5


@dataclass(frozen=True)
class TimeMetrics:
    """`/usr/bin/time` から取り出した指標。取れない項目は None。"""

    wall_seconds: float | None
    user_seconds: float | None
    system_seconds: float | None
    max_rss_kib: int | None
    raw_stderr: str


def main(argv: Sequence[str] | None = None) -> int:
    logging.basicConfig(
        level=logging.INFO,
        format="%(levelname)s %(message)s",
    )
    parser = _build_parser()
    args = parser.parse_args(argv)
    return int(args.func(args))


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Measure compose performance with a swappable binary path. "
            "Use prepare-input, run, and summarize subcommands."
        ),
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare = subparsers.add_parser(
        "prepare-input",
        help="Generate dummy video archives with generate-archive",
    )
    prepare.add_argument(
        "--generator-bin",
        type=Path,
        default=Path("target/release/sora-archive-compositor"),
        help="Binary that provides generate-archive (default: SAC release)",
    )
    prepare.add_argument(
        "--out-dir",
        type=Path,
        required=True,
        help="Directory to write archive-*.mp4 / archive-*.json",
    )
    prepare.add_argument(
        "--duration",
        type=int,
        default=DEFAULT_DURATION_SECONDS,
        help=f"Archive duration in seconds (default: {DEFAULT_DURATION_SECONDS})",
    )
    prepare.add_argument(
        "--resolution",
        default=DEFAULT_RESOLUTION,
        help=f"WxH (default: {DEFAULT_RESOLUTION})",
    )
    prepare.add_argument(
        "--frame-rate",
        default=DEFAULT_FRAME_RATE,
        help=f"FPS (default: {DEFAULT_FRAME_RATE})",
    )
    prepare.add_argument(
        "--seed",
        type=int,
        default=DEFAULT_SEED,
        help=f"generate-archive seed (default: {DEFAULT_SEED})",
    )
    prepare.add_argument(
        "--codec",
        default="VP9",
        help="Input video codec for generate-archive (default: VP9)",
    )
    prepare.add_argument(
        "--source-count",
        type=int,
        default=DEFAULT_SOURCE_COUNT,
        help=f"Number of archive sources (default: {DEFAULT_SOURCE_COUNT})",
    )
    prepare.add_argument(
        "--openh264",
        type=Path,
        default=None,
        help="OpenH264 shared library path (needed when --codec is H264)",
    )
    prepare.set_defaults(func=_cmd_prepare_input)

    run = subparsers.add_parser(
        "run",
        help="Run compose repeatedly and save per-run artifacts",
    )
    run.add_argument(
        "--bin",
        type=Path,
        default=Path("target/release/sora-archive-compositor"),
        help="compose binary path (SAC or hisui)",
    )
    run.add_argument(
        "--input-dir",
        type=Path,
        required=True,
        help="Root dir that contains archive-*.json (passed to compose as ROOT_DIR)",
    )
    run.add_argument(
        "--layout",
        type=Path,
        default=None,
        help="Layout JSON/JSONC path (optional; binary default layout if omitted)",
    )
    run.add_argument(
        "--video-codec",
        default=None,
        help=(
            "If set, write a minimal layout with this video_codec into out-dir "
            "and use it (overrides --layout)"
        ),
    )
    run.add_argument(
        "--runs",
        type=int,
        default=DEFAULT_RUNS,
        help=f"Total runs including one warmup (default: {DEFAULT_RUNS})",
    )
    run.add_argument(
        "--out-dir",
        type=Path,
        required=True,
        help="Directory to store run artifacts",
    )
    run.add_argument(
        "--label",
        required=True,
        help="Label for this binary (e.g. sac, hisui-2025.3.2)",
    )
    run.add_argument(
        "--openh264",
        type=Path,
        default=None,
        help="Forwarded to compose as --openh264 when set",
    )
    run.add_argument(
        "--thread-count",
        type=int,
        default=1,
        help="compose --thread-count (default: 1)",
    )
    run.set_defaults(func=_cmd_run)

    summarize = subparsers.add_parser(
        "summarize",
        help="Compute medians from run artifacts (skips run-00 warmup)",
    )
    summarize.add_argument(
        "--out-dir",
        type=Path,
        required=True,
        help="Directory that contains <label>/run-*/meta.json",
    )
    summarize.add_argument(
        "--label",
        default=None,
        help="If set, summarize only this label; otherwise all labels",
    )
    summarize.set_defaults(func=_cmd_summarize)

    compare = subparsers.add_parser(
        "compare",
        help="Summarize two labels and print relative delta of elapsed_seconds",
    )
    compare.add_argument("--out-dir", type=Path, required=True)
    compare.add_argument("--baseline-label", required=True, help="Usually hisui")
    compare.add_argument("--candidate-label", required=True, help="Usually sac")
    compare.set_defaults(func=_cmd_compare)

    return parser


def _cmd_prepare_input(args: argparse.Namespace) -> int:
    generator_bin = args.generator_bin.resolve()
    out_dir = args.out_dir.resolve()
    if args.source_count < 1:
        raise SystemExit("source-count must be >= 1")
    if args.duration < 1:
        raise SystemExit("duration must be >= 1")
    _require_executable(generator_bin)
    out_dir.mkdir(parents=True, exist_ok=True)

    for index in range(args.source_count):
        connection_id = f"perf-{args.codec.lower()}-{index:02d}"
        command = [
            str(generator_bin),
            "generate-archive",
            str(out_dir),
            "--connection-id",
            connection_id,
            "--duration",
            str(args.duration),
            "--resolution",
            args.resolution,
            "--frame-rate",
            args.frame_rate,
            "--seed",
            str(args.seed + index),
            "--codec",
            args.codec,
        ]
        if args.openh264 is not None:
            command.extend(["--openh264", str(args.openh264.resolve())])
        LOG.info("Running: %s", " ".join(command))
        subprocess.run(command, check=True)

    manifest = {
        "generator_bin": str(generator_bin),
        "out_dir": str(out_dir),
        "duration_seconds": args.duration,
        "resolution": args.resolution,
        "frame_rate": args.frame_rate,
        "seed_base": args.seed,
        "codec": args.codec,
        "source_count": args.source_count,
    }
    manifest_path = out_dir / "perf_input_manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    LOG.info("Wrote %s", manifest_path)
    return 0


def _cmd_run(args: argparse.Namespace) -> int:
    compose_bin = args.bin.resolve()
    input_dir = args.input_dir.resolve()
    out_dir = args.out_dir.resolve()
    if args.runs < 2:
        raise SystemExit("runs must be >= 2 (one warmup + at least one measured run)")
    if not input_dir.is_dir():
        raise SystemExit(f"input-dir does not exist: {input_dir}")
    _require_executable(compose_bin)

    label_dir = out_dir / args.label
    label_dir.mkdir(parents=True, exist_ok=True)

    layout_path: Path | None
    if args.video_codec is not None:
        layout_path = label_dir / f"layout-{args.video_codec.lower()}.json"
        layout_path.write_text(
            _minimal_layout_json(args.video_codec) + "\n",
            encoding="utf-8",
        )
        LOG.info("Wrote minimal layout: %s", layout_path)
    elif args.layout is not None:
        layout_path = args.layout.resolve()
        if not layout_path.is_file():
            raise SystemExit(f"layout file does not exist: {layout_path}")
    else:
        layout_path = None

    for run_index in range(args.runs):
        run_dir = label_dir / f"run-{run_index:02d}"
        run_dir.mkdir(parents=True, exist_ok=True)
        is_warmup = run_index == 0
        LOG.info(
            "Run %s/%s label=%s warmup=%s",
            run_index + 1,
            args.runs,
            args.label,
            is_warmup,
        )
        meta = _run_one_compose(
            compose_bin=compose_bin,
            input_dir=input_dir,
            layout_path=layout_path,
            run_dir=run_dir,
            openh264=args.openh264.resolve() if args.openh264 is not None else None,
            thread_count=args.thread_count,
            label=args.label,
            run_index=run_index,
            is_warmup=is_warmup,
        )
        (run_dir / "meta.json").write_text(
            json.dumps(meta, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
    return 0


def _cmd_summarize(args: argparse.Namespace) -> int:
    out_dir = args.out_dir.resolve()
    if not out_dir.is_dir():
        raise SystemExit(f"out-dir does not exist: {out_dir}")

    labels = [args.label] if args.label is not None else _discover_labels(out_dir)
    if not labels:
        raise SystemExit(f"no labels found under {out_dir}")

    summary: dict[str, object] = {}
    for label in labels:
        summary[label] = _summarize_label(out_dir / label)

    summary_path = out_dir / "summary.json"
    summary_path.write_text(
        json.dumps(summary, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    LOG.info("Wrote %s", summary_path)
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0


def _cmd_compare(args: argparse.Namespace) -> int:
    out_dir = args.out_dir.resolve()
    baseline = _summarize_label(out_dir / args.baseline_label)
    candidate = _summarize_label(out_dir / args.candidate_label)
    baseline_elapsed = baseline["median_elapsed_seconds"]
    candidate_elapsed = candidate["median_elapsed_seconds"]
    if not isinstance(baseline_elapsed, (int, float)) or not isinstance(
        candidate_elapsed, (int, float)
    ):
        raise SystemExit("median_elapsed_seconds missing; run measured runs first")
    if baseline_elapsed == 0:
        raise SystemExit("baseline median_elapsed_seconds is 0")

    delta_pct = (float(candidate_elapsed) - float(baseline_elapsed)) / float(
        baseline_elapsed
    ) * 100.0
    report = {
        "baseline_label": args.baseline_label,
        "candidate_label": args.candidate_label,
        "baseline": baseline,
        "candidate": candidate,
        "elapsed_seconds_delta_pct": delta_pct,
        "verdict": _verdict(delta_pct),
    }
    report_path = (
        out_dir / f"compare-{args.baseline_label}-vs-{args.candidate_label}.json"
    )
    report_path.write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    LOG.info("Wrote %s", report_path)
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0


def _run_one_compose(
    *,
    compose_bin: Path,
    input_dir: Path,
    layout_path: Path | None,
    run_dir: Path,
    openh264: Path | None,
    thread_count: int,
    label: str,
    run_index: int,
    is_warmup: bool,
) -> dict[str, object]:
    output_mp4 = run_dir / "output.mp4"
    stats_path = run_dir / "stats.json"
    stdout_path = run_dir / "compose.stdout.json"
    time_stderr_path = run_dir / "time.stderr.txt"

    compose_command = [
        str(compose_bin),
        "compose",
        str(input_dir),
        "--output-file",
        str(output_mp4),
        "--stats-file",
        str(stats_path),
        "--no-progress-bar",
        "--thread-count",
        str(thread_count),
    ]
    if layout_path is not None:
        compose_command.extend(["--layout-file", str(layout_path)])
    if openh264 is not None:
        compose_command.extend(["--openh264", str(openh264)])

    time_command = _time_prefix() + compose_command
    LOG.info("Running: %s", " ".join(time_command))
    completed = subprocess.run(
        time_command,
        check=False,
        capture_output=True,
        text=True,
    )
    stdout_path.write_text(completed.stdout, encoding="utf-8")
    time_stderr_path.write_text(completed.stderr, encoding="utf-8")
    if completed.returncode != 0:
        raise SystemExit(
            "compose failed with exit "
            f"{completed.returncode}; see {stdout_path} and {time_stderr_path}"
        )

    compose_json = _parse_json_object(completed.stdout, source=str(stdout_path))
    time_metrics = _parse_time_stderr(completed.stderr)
    elapsed = compose_json.get("elapsed_seconds")
    if not isinstance(elapsed, (int, float)):
        raise SystemExit(f"elapsed_seconds missing in compose stdout: {stdout_path}")

    return {
        "label": label,
        "run_index": run_index,
        "is_warmup": is_warmup,
        "compose_bin": str(compose_bin),
        "input_dir": str(input_dir),
        "layout_path": str(layout_path) if layout_path is not None else None,
        "platform": platform.platform(),
        "elapsed_seconds": float(elapsed),
        "time_wall_seconds": time_metrics.wall_seconds,
        "time_user_seconds": time_metrics.user_seconds,
        "time_system_seconds": time_metrics.system_seconds,
        "max_rss_kib": time_metrics.max_rss_kib,
        "compose_command": compose_command,
    }


def _summarize_label(label_dir: Path) -> dict[str, object]:
    if not label_dir.is_dir():
        raise SystemExit(f"label dir does not exist: {label_dir}")

    metas: list[Mapping[str, object]] = []
    for meta_path in sorted(label_dir.glob("run-*/meta.json")):
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        if not isinstance(meta, dict):
            raise SystemExit(f"invalid meta.json: {meta_path}")
        if meta.get("is_warmup") is True:
            continue
        metas.append(meta)

    if not metas:
        raise SystemExit(f"no measured runs (non-warmup) under {label_dir}")

    elapsed_values = [_require_float(meta, "elapsed_seconds") for meta in metas]
    wall_values = [
        value
        for value in (_optional_float(meta, "time_wall_seconds") for meta in metas)
        if value is not None
    ]
    rss_values = [
        value
        for value in (_optional_int(meta, "max_rss_kib") for meta in metas)
        if value is not None
    ]

    return {
        "label_dir": str(label_dir),
        "measured_runs": len(metas),
        "median_elapsed_seconds": statistics.median(elapsed_values),
        "mean_elapsed_seconds": statistics.fmean(elapsed_values),
        "median_time_wall_seconds": (
            statistics.median(wall_values) if wall_values else None
        ),
        "median_max_rss_kib": statistics.median(rss_values) if rss_values else None,
        "elapsed_seconds_samples": elapsed_values,
    }


def _discover_labels(out_dir: Path) -> list[str]:
    labels: list[str] = []
    for child in sorted(out_dir.iterdir()):
        if child.is_dir() and any(child.glob("run-*/meta.json")):
            labels.append(child.name)
    return labels


def _minimal_layout_json(video_codec: str) -> str:
    # 音声は計測対象外。audio_sources は空にして映像のみ合成する。
    layout = {
        "audio_sources": [],
        "video_layout": {
            "main": {
                "cell_width": 640,
                "cell_height": 360,
                "max_columns": 2,
                "video_sources": ["*archive*.json"],
            }
        },
        "video_codec": video_codec,
    }
    return json.dumps(layout, indent=2, ensure_ascii=False)


def _time_prefix() -> list[str]:
    system = platform.system()
    if system == "Darwin":
        return ["/usr/bin/time", "-l"]
    if system == "Linux":
        return ["/usr/bin/time", "-v"]
    # その他は POSIX な -p に落とす
    return ["/usr/bin/time", "-p"]


def _parse_time_stderr(stderr_text: str) -> TimeMetrics:
    # macOS `/usr/bin/time -l` 先頭行: `0.35 real 0.33 user 0.01 sys`
    macos_triplet = re.search(
        r"([0-9.]+)\s+real\s+([0-9.]+)\s+user\s+([0-9.]+)\s+sys",
        stderr_text,
    )
    wall_seconds: float | None = None
    user_seconds: float | None = None
    system_seconds: float | None = None
    if macos_triplet is not None:
        wall_seconds = float(macos_triplet.group(1))
        user_seconds = float(macos_triplet.group(2))
        system_seconds = float(macos_triplet.group(3))
    else:
        wall_seconds = _match_float(
            stderr_text,
            [
                r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): (.+)",
                r"^real\s+([0-9.]+)",
            ],
        )
        user_seconds = _match_float(
            stderr_text,
            [
                r"User time \(seconds\): ([0-9.]+)",
                r"^user\s+([0-9.]+)",
            ],
        )
        system_seconds = _match_float(
            stderr_text,
            [
                r"System time \(seconds\): ([0-9.]+)",
                r"^sys\s+([0-9.]+)",
            ],
        )

    max_rss_kib: int | None = None
    linux_rss = _match_group(
        stderr_text,
        r"Maximum resident set size \(kbytes\): ([0-9]+)",
    )
    if linux_rss is not None:
        max_rss_kib = int(linux_rss)
    else:
        # macOS -l: maximum resident set size はバイト単位
        macos_rss_bytes = _match_group(
            stderr_text,
            r"\s*([0-9]+)\s+maximum resident set size",
        )
        if macos_rss_bytes is not None:
            max_rss_kib = int(math.ceil(int(macos_rss_bytes) / 1024.0))

    return TimeMetrics(
        wall_seconds=wall_seconds,
        user_seconds=user_seconds,
        system_seconds=system_seconds,
        max_rss_kib=max_rss_kib,
        raw_stderr=stderr_text,
    )


def _parse_hms(value: str) -> float:
    parts = value.strip().split(":")
    if len(parts) == 2:
        minutes = float(parts[0])
        seconds = float(parts[1])
        return minutes * 60.0 + seconds
    if len(parts) == 3:
        hours = float(parts[0])
        minutes = float(parts[1])
        seconds = float(parts[2])
        return hours * 3600.0 + minutes * 60.0 + seconds
    return float(value)


def _match_float(text: str, patterns: Sequence[str]) -> float | None:
    group = None
    for pattern in patterns:
        group = _match_group(text, pattern)
        if group is not None:
            break
    if group is None:
        return None
    if ":" in group:
        return _parse_hms(group)
    return float(group)


def _match_group(text: str, pattern: str) -> str | None:
    matched = re.search(pattern, text, flags=re.MULTILINE)
    if matched is None:
        return None
    return matched.group(1)


def _parse_json_object(text: str, *, source: str) -> dict[str, object]:
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise SystemExit(f"failed to parse JSON from {source}: {error}") from error
    if not isinstance(value, dict):
        raise SystemExit(f"expected JSON object in {source}")
    return value


def _require_executable(path: Path) -> None:
    if not path.is_file():
        raise SystemExit(f"binary does not exist: {path}")
    if not os.access(path, os.X_OK):
        raise SystemExit(f"binary is not executable: {path}")


def _require_float(meta: Mapping[str, object], key: str) -> float:
    value = meta.get(key)
    if not isinstance(value, (int, float)):
        raise SystemExit(f"meta missing numeric {key}: {meta!r}")
    return float(value)


def _optional_float(meta: Mapping[str, object], key: str) -> float | None:
    value = meta.get(key)
    if value is None:
        return None
    if not isinstance(value, (int, float)):
        raise SystemExit(f"meta has non-numeric {key}: {meta!r}")
    return float(value)


def _optional_int(meta: Mapping[str, object], key: str) -> int | None:
    value = meta.get(key)
    if value is None:
        return None
    if not isinstance(value, int):
        raise SystemExit(f"meta has non-int {key}: {meta!r}")
    return value


def _verdict(delta_pct: float) -> str:
    if delta_pct > 10.0:
        return "regression"
    if delta_pct < -10.0:
        return "improvement"
    return "within_tolerance"


if __name__ == "__main__":
    sys.exit(main())
