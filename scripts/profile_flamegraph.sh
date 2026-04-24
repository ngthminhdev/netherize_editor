#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INPUT_DIR="${ROOT_DIR}/benchmarks/inputs"
MODE="${1:-perf-probe}"
GPU_SAMPLES="${GPU_SAMPLES:-40}"

if ! cargo flamegraph --help >/dev/null 2>&1; then
  echo "[Flamegraph] cargo-flamegraph is not installed."
  echo "[Flamegraph] install: cargo install flamegraph"
  exit 2
fi

if [[ ! -f "${INPUT_DIR}/rust_10k_lines.rs" || ! -f "${INPUT_DIR}/log_50mb.txt" ]]; then
  echo "[Flamegraph] benchmark samples missing, generating..."
  "${ROOT_DIR}/scripts/generate_bench_samples.sh"
fi

cd "${ROOT_DIR}"

if [[ "${MODE}" == "perf-probe" ]]; then
  echo "[Flamegraph] profiling phase10_perf_probe"
  cargo flamegraph --bin phase10_perf_probe -- \
    --inputs-dir "${INPUT_DIR}" \
    --gpu-samples "${GPU_SAMPLES}"
  echo "[Flamegraph] output: ${ROOT_DIR}/flamegraph.svg"
  exit 0
fi

if [[ "${MODE}" == "open-10k-only" ]]; then
  echo "[Flamegraph] profiling phase10_perf_probe with light gpu sampling"
  cargo flamegraph --bin phase10_perf_probe -- \
    --inputs-dir "${INPUT_DIR}" \
    --gpu-samples 1
  echo "[Flamegraph] output: ${ROOT_DIR}/flamegraph.svg"
  exit 0
fi

echo "[Flamegraph] unknown mode: ${MODE}"
echo "Usage:"
echo "  ./scripts/profile_flamegraph.sh perf-probe"
echo "  ./scripts/profile_flamegraph.sh open-10k-only"
exit 2
