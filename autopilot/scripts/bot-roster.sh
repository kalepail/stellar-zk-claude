#!/usr/bin/env bash
set -euo pipefail

# Canonical active bot roster. Update this file when rotating finalists.
ACTIVE_OMEGA_BOTS="omega-marathon,omega-lurk-breaker,omega-ace,omega-alltime-hunter,omega-supernova"
ACTIVE_OFFLINE_BOTS="offline-wrap-endurancex,offline-wrap-sniper30,offline-wrap-frugal-ace,offline-wrap-apex-score,offline-wrap-sureshot,offline-supernova-hunt"
ACTIVE_BOTS="$ACTIVE_OMEGA_BOTS,$ACTIVE_OFFLINE_BOTS"

# Tight finalist set for deep/high-cost runs.
ACTIVE_FINALISTS="offline-wrap-endurancex,offline-wrap-sniper30,offline-wrap-frugal-ace,offline-wrap-apex-score,omega-marathon,omega-ace"
ACTIVE_NON_OFFLINE_FINALISTS="omega-marathon,omega-lurk-breaker,omega-ace,omega-alltime-hunter,omega-supernova"
