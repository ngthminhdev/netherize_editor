# Netherize Editor Perf Kit (Module 10 / Phase 3)

This document defines a reproducible benchmark and profiling workflow so tuning decisions are driven by data.

## What is measured

`src/bin/phase10_perf_probe.rs` reports:

- `startup_ms`
- `open_10k_ms`
- `open_50mb_ms`
- `edit_loop_avg_us`
- `edit_loop_p95_us`
- `gpu_frame_avg_ms`
- `gpu_frame_p95_ms`
- `memory_rss_current_mb`
- `memory_rss_peak_mb`

`benches/editor_bench.rs` (Criterion) measures:

- open-file benchmark (`10k_lines`, `50mb_log`)
- edit-loop benchmark (`insert_move_backspace_20k`)

## Reproducible inputs

Generate standard inputs once:

```bash
./scripts/generate_bench_samples.sh
```

Generated files:

- `benchmarks/inputs/rust_10k_lines.rs`
- `benchmarks/inputs/log_50mb.txt`

## Run benchmarks

Run Criterion benchmarks:

```bash
cargo bench --bench editor_bench -- --noplot
```

Run perf probe directly:

```bash
cargo run --bin phase10_perf_probe -- \
  --inputs-dir benchmarks/inputs \
  --json-out benchmarks/baselines/manual_snapshot.json
```

## Baseline workflow (Before/After)

Capture a baseline snapshot:

```bash
./scripts/run_perf_baseline.sh capture
```

Compare current run with a previous baseline:

```bash
./scripts/run_perf_baseline.sh compare benchmarks/baselines/baseline_YYYYMMDD_HHMMSS.json
```

Quick local iteration (short Criterion window + fewer GPU samples):

```bash
BENCH_ARGS="--noplot --sample-size 10 --measurement-time 0.05 --warm-up-time 0.05" \
PERF_GPU_SAMPLES=5 \
./scripts/run_perf_baseline.sh capture
```

The script stores:

- benchmark log: `benchmarks/baselines/bench_<timestamp>.log`
- metrics snapshot: `benchmarks/baselines/baseline_<timestamp>.json`

## CPU Flamegraph

Install once:

```bash
cargo install flamegraph
```

Generate a flamegraph:

```bash
./scripts/profile_flamegraph.sh perf-probe
```

Output:

- `flamegraph.svg` in workspace root

Notes:

- On macOS, `cargo flamegraph` may require elevated privileges depending on sampling backend permissions.
- On Linux, ensure `perf` permissions are configured.

## Memory profiling

### Quick built-in signal

`phase10_perf_probe` already tracks process RSS (`current` and `peak`) for coarse memory regression checks.

### Deep allocation / leak tracing

Linux example (`heaptrack`):

```bash
heaptrack target/debug/phase10_perf_probe --inputs-dir benchmarks/inputs
```

macOS example (`leaks`):

```bash
MallocStackLogging=1 target/debug/phase10_perf_probe --inputs-dir benchmarks/inputs
leaks --atExit -- target/debug/phase10_perf_probe --inputs-dir benchmarks/inputs
```

## GPU timing

`phase10_perf_probe` includes a headless `wgpu` frame timing sample (`gpu_frame_avg_ms`, `gpu_frame_p95_ms`).

For deeper frame-level investigation in live UI binaries:

- run the UI probe with logs enabled
- capture a CPU flamegraph in parallel
- compare with `gpu_frame_*` metrics from `phase10_perf_probe` to separate CPU-side and GPU-submit bottlenecks

## Suggested practice

1. Capture baseline before optimization.
2. Land a focused change.
3. Run compare mode and inspect deltas.
4. Keep optimization only when key metrics improve without obvious regressions.
