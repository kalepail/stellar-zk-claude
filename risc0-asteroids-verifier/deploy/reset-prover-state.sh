#!/usr/bin/env bash
set -euo pipefail

# Reset all persisted prover state.
# This is destructive by design: jobs DB, result artifacts, and logs are removed.
#
# Defaults:
#   DATA_DIR=/var/lib/stellar-zk/prover
#   SUPERVISOR_PROGRAM=risc0-asteroids-api
#
# Usage:
#   sudo bash deploy/reset-prover-state.sh
#   sudo DATA_DIR=/custom/path SUPERVISOR_PROGRAM=risc0-asteroids-api bash deploy/reset-prover-state.sh
#   sudo bash deploy/reset-prover-state.sh --yes

DATA_DIR="${DATA_DIR:-/var/lib/stellar-zk/prover}"
SUPERVISOR_PROGRAM="${SUPERVISOR_PROGRAM:-risc0-asteroids-api}"
FORCE_YES=0
SUPERVISOR_WAS_RUNNING=0

usage() {
  cat <<'USAGE_EOF'
Usage: deploy/reset-prover-state.sh [--yes]

Environment overrides:
  DATA_DIR            Default: /var/lib/stellar-zk/prover
  SUPERVISOR_PROGRAM  Default: risc0-asteroids-api

What gets deleted:
  - jobs.db, jobs.db-wal, jobs.db-shm
  - results/ directory contents
  - api-server.log and api-server.err
USAGE_EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --yes)
      FORCE_YES=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
  echo "ERROR: run as root (use sudo)." >&2
  exit 1
fi

echo "================================================"
echo "RISC0 Prover State Reset"
echo "================================================"
echo "DATA_DIR:            $DATA_DIR"
echo "SUPERVISOR_PROGRAM:  $SUPERVISOR_PROGRAM"
echo ""
echo "This will permanently delete:"
echo "  - $DATA_DIR/jobs.db*"
echo "  - $DATA_DIR/results/*"
echo "  - $DATA_DIR/api-server.log"
echo "  - $DATA_DIR/api-server.err"
echo ""

if [[ "$FORCE_YES" -ne 1 ]]; then
  read -r -p "Type RESET to continue: " confirm
  if [[ "$confirm" != "RESET" ]]; then
    echo "Aborted."
    exit 1
  fi
fi

if command -v supervisorctl >/dev/null 2>&1; then
  status_line="$(supervisorctl status "$SUPERVISOR_PROGRAM" 2>/dev/null || true)"
  if [[ -n "$status_line" && "$status_line" != *"no such process"* ]]; then
    if [[ "$status_line" == *"RUNNING"* || "$status_line" == *"STARTING"* || "$status_line" == *"BACKOFF"* ]]; then
      SUPERVISOR_WAS_RUNNING=1
    fi
    echo "Stopping supervisor program: $SUPERVISOR_PROGRAM"
    supervisorctl stop "$SUPERVISOR_PROGRAM" >/dev/null || true
  else
    echo "Supervisor program not found: $SUPERVISOR_PROGRAM (continuing)"
  fi
else
  echo "supervisorctl not found; continuing without service stop"
fi

echo "Deleting persisted prover state..."
mkdir -p "$DATA_DIR"
rm -f "$DATA_DIR/jobs.db" "$DATA_DIR/jobs.db-shm" "$DATA_DIR/jobs.db-wal"
rm -rf "$DATA_DIR/results"
mkdir -p "$DATA_DIR/results"
rm -f "$DATA_DIR/api-server.log" "$DATA_DIR/api-server.err"

echo "State reset complete."

if command -v supervisorctl >/dev/null 2>&1; then
  if [[ "$SUPERVISOR_WAS_RUNNING" -eq 1 ]]; then
    echo "Starting supervisor program: $SUPERVISOR_PROGRAM"
    supervisorctl start "$SUPERVISOR_PROGRAM" >/dev/null
  else
    echo "Supervisor program was not running before reset; leaving stopped."
  fi
fi

echo "Done."
