#!/usr/bin/env bash
set -euo pipefail

# Legacy entrypoint retained for compatibility. Precision bots were retired.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$ROOT_DIR/scripts/run-efficiency-elite-suite.sh" "$@"
