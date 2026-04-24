#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINES_DIR="${ROOT_DIR}/benchmarks/baselines"
INPUT_DIR="${ROOT_DIR}/benchmarks/inputs"
BENCH_ARGS="${BENCH_ARGS:---noplot}"
PERF_GPU_SAMPLES="${PERF_GPU_SAMPLES:-60}"

MODE="${1:-capture}"
REFERENCE_BASELINE="${2:-}"
TIMESTAMP="$(date '+%Y%m%d_%H%M%S')"
CURRENT_BASELINE_JSON="${BASELINES_DIR}/baseline_${TIMESTAMP}.json"
BENCH_LOG_FILE="${BASELINES_DIR}/bench_${TIMESTAMP}.log"

mkdir -p "${BASELINES_DIR}"

echo "[Perf Baseline] root=${ROOT_DIR}"
echo "[Perf Baseline] mode=${MODE}"

"${ROOT_DIR}/scripts/generate_bench_samples.sh"

echo "[Perf Baseline] running criterion benchmark..."
(
  cd "${ROOT_DIR}"
  cargo bench --bench editor_bench -- ${BENCH_ARGS} | tee "${BENCH_LOG_FILE}"
)

if [[ "${MODE}" == "capture" ]]; then
  echo "[Perf Baseline] capturing current snapshot..."
  (
    cd "${ROOT_DIR}"
    cargo run --bin phase10_perf_probe -- \
      --inputs-dir "${INPUT_DIR}" \
      --gpu-samples "${PERF_GPU_SAMPLES}" \
      --json-out "${CURRENT_BASELINE_JSON}"
  )
  echo "[Perf Baseline] baseline saved: ${CURRENT_BASELINE_JSON}"
  exit 0
fi

if [[ "${MODE}" == "compare" ]]; then
  if [[ -z "${REFERENCE_BASELINE}" ]]; then
    echo "[Perf Baseline] compare mode requires baseline path argument"
    echo "Usage: ./scripts/run_perf_baseline.sh compare benchmarks/baselines/baseline_YYYYMMDD_HHMMSS.json"
    exit 2
  fi

  if [[ ! -f "${REFERENCE_BASELINE}" ]]; then
    echo "[Perf Baseline] baseline file not found: ${REFERENCE_BASELINE}"
    exit 2
  fi

  echo "[Perf Baseline] comparing against ${REFERENCE_BASELINE}"
  (
    cd "${ROOT_DIR}"
    cargo run --bin phase10_perf_probe -- \
      --inputs-dir "${INPUT_DIR}" \
      --gpu-samples "${PERF_GPU_SAMPLES}" \
      --baseline "${REFERENCE_BASELINE}" \
      --json-out "${CURRENT_BASELINE_JSON}"
  )
  echo "[Perf Baseline] comparison snapshot saved: ${CURRENT_BASELINE_JSON}"
  exit 0
fi

echo "[Perf Baseline] unknown mode: ${MODE}"
echo "Usage:"
echo "  ./scripts/run_perf_baseline.sh capture"
echo "  ./scripts/run_perf_baseline.sh compare <baseline_json>"
exit 2
