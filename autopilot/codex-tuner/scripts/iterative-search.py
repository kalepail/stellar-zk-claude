#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import math
import random
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Tuple

SCALE_KEYS = [
    "risk_weight_scale",
    "survival_weight_scale",
    "aggression_weight_scale",
    "fire_reward_scale",
    "shot_penalty_scale",
    "miss_fire_penalty_scale",
    "action_penalty_scale",
    "turn_penalty_scale",
    "thrust_penalty_scale",
    "center_weight_scale",
    "edge_penalty_scale",
    "lookahead_frames_scale",
    "flow_weight_scale",
    "speed_soft_cap_scale",
    "fire_distance_scale",
    "lurk_trigger_scale",
    "lurk_boost_scale",
    "fire_tolerance_scale",
]

SCALE_BOUNDS: Dict[str, Tuple[float, float]] = {
    "risk_weight_scale": (0.35, 2.3),
    "survival_weight_scale": (0.35, 2.4),
    "aggression_weight_scale": (0.4, 2.6),
    "fire_reward_scale": (0.35, 2.6),
    "shot_penalty_scale": (0.25, 2.2),
    "miss_fire_penalty_scale": (0.25, 2.4),
    "action_penalty_scale": (0.25, 1.7),
    "turn_penalty_scale": (0.25, 1.7),
    "thrust_penalty_scale": (0.25, 1.7),
    "center_weight_scale": (0.25, 2.2),
    "edge_penalty_scale": (0.25, 2.4),
    "lookahead_frames_scale": (0.55, 1.8),
    "flow_weight_scale": (0.35, 2.4),
    "speed_soft_cap_scale": (0.6, 1.9),
    "fire_distance_scale": (0.55, 1.9),
    "lurk_trigger_scale": (0.5, 2.0),
    "lurk_boost_scale": (0.5, 2.5),
    "fire_tolerance_scale": (0.5, 2.6),
}

DELTA_KEY = "min_fire_quality_delta"
DELTA_BOUNDS = (-0.35, 0.3)
PROFILE_KEYS = SCALE_KEYS + [DELTA_KEY]


@dataclass
class CandidateResult:
    iteration: int
    candidate: int
    strategy: str
    objective_value: float
    avg_score: float
    max_score: int
    avg_frames: float
    out_dir: str
    profile: Dict[str, float]


def run_cmd(cmd: List[str], cwd: Path) -> None:
    display = " ".join(cmd)
    print(f"$ {display}", flush=True)
    subprocess.run(cmd, cwd=str(cwd), check=True)


def load_json(path: Path) -> Dict:
    return json.loads(path.read_text())


def write_json(path: Path, data: Dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n")


def clamp(value: float, lo: float, hi: float) -> float:
    return max(lo, min(hi, value))


def normalize_profile(profile: Dict[str, float]) -> Dict[str, float]:
    out = {}
    for key in SCALE_KEYS:
        lo, hi = SCALE_BOUNDS[key]
        out[key] = round(clamp(float(profile.get(key, 1.0)), lo, hi), 6)
    out[DELTA_KEY] = round(clamp(float(profile.get(DELTA_KEY, 0.0)), *DELTA_BOUNDS), 6)
    return out


def profile_signature(profile: Dict[str, float]) -> str:
    normalized = normalize_profile(profile)
    return json.dumps(normalized, sort_keys=True, separators=(",", ":"))


def metric_tuple(
    row: CandidateResult, selection_metric: str
) -> Tuple[float, float, float, float]:
    if selection_metric == "objective":
        return (
            row.objective_value,
            row.avg_score,
            float(row.max_score),
            row.avg_frames,
        )
    if selection_metric == "score":
        return (
            row.avg_score,
            float(row.max_score),
            row.objective_value,
            row.avg_frames,
        )
    if selection_metric == "insane":
        return (
            float(row.max_score),
            row.avg_score,
            row.objective_value,
            row.avg_frames,
        )
    raise ValueError(f"unknown selection metric: {selection_metric}")


def better_than(
    left: CandidateResult, right: CandidateResult, selection_metric: str
) -> bool:
    return metric_tuple(left, selection_metric) > metric_tuple(right, selection_metric)


def mutate_profile(
    base: Dict[str, float],
    rng: random.Random,
    step: float,
    min_fields: int = 3,
    max_fields: int = 7,
    delta_scale: float = 0.1,
) -> Dict[str, float]:
    out = dict(base)

    min_fields = max(1, min(min_fields, len(SCALE_KEYS)))
    max_fields = max(min_fields, min(max_fields, len(SCALE_KEYS)))
    field_count = rng.randint(min_fields, max_fields)

    for key in rng.sample(SCALE_KEYS, field_count):
        lo, hi = SCALE_BOUNDS[key]
        factor = 1.0 + rng.uniform(-step, step)
        out[key] = clamp(float(out[key]) * factor, lo, hi)

    if rng.random() < 0.95:
        delta_step = max(0.01, step * delta_scale)
        out[DELTA_KEY] = clamp(
            float(out[DELTA_KEY]) + rng.uniform(-delta_step, delta_step),
            *DELTA_BOUNDS,
        )

    return normalize_profile(out)


def mutate_profile_aggressive(
    base: Dict[str, float],
    rng: random.Random,
    step: float,
) -> Dict[str, float]:
    out = mutate_profile(
        base,
        rng,
        step=step * 1.75,
        min_fields=6,
        max_fields=len(SCALE_KEYS),
        delta_scale=0.2,
    )

    # Occasionally force one hard reset to escape local plateaus.
    if rng.random() < 0.45:
        key = rng.choice(SCALE_KEYS)
        lo, hi = SCALE_BOUNDS[key]
        out[key] = round(rng.uniform(lo, hi), 6)

    return normalize_profile(out)


def blend_profiles(a: Dict[str, float], b: Dict[str, float], alpha: float) -> Dict[str, float]:
    alpha = clamp(alpha, 0.0, 1.0)
    out = {}
    for key in SCALE_KEYS:
        lo, hi = SCALE_BOUNDS[key]
        value = float(a[key]) * alpha + float(b[key]) * (1.0 - alpha)
        out[key] = round(clamp(value, lo, hi), 6)
    delta_value = float(a[DELTA_KEY]) * alpha + float(b[DELTA_KEY]) * (1.0 - alpha)
    out[DELTA_KEY] = round(clamp(delta_value, *DELTA_BOUNDS), 6)
    return normalize_profile(out)


def profile_delta(newer: Dict[str, float], older: Dict[str, float]) -> Dict[str, float]:
    out = {}
    for key in PROFILE_KEYS:
        out[key] = round(float(newer[key]) - float(older[key]), 6)
    return out


def apply_momentum(
    base: Dict[str, float],
    momentum: Dict[str, float],
    scale: float,
) -> Dict[str, float]:
    out = dict(base)
    for key in SCALE_KEYS:
        lo, hi = SCALE_BOUNDS[key]
        out[key] = clamp(float(out[key]) + float(momentum.get(key, 0.0)) * scale, lo, hi)
    out[DELTA_KEY] = clamp(
        float(out[DELTA_KEY]) + float(momentum.get(DELTA_KEY, 0.0)) * scale,
        *DELTA_BOUNDS,
    )
    return normalize_profile(out)


def ensure_binary(autopilot_root: Path) -> Path:
    bin_path = autopilot_root / "target" / "release" / "rust-autopilot"
    if bin_path.exists():
        return bin_path

    run_cmd(
        [
            "cargo",
            "build",
            "--release",
            "--manifest-path",
            str(autopilot_root / "Cargo.toml"),
        ],
        cwd=autopilot_root,
    )

    if not bin_path.exists():
        raise RuntimeError(f"expected binary missing at {bin_path}")
    return bin_path


def benchmark_profile(
    binary: Path,
    autopilot_root: Path,
    bot: str,
    seeds_file: Path,
    max_frames: int,
    jobs: int,
    out_dir: Path,
) -> Tuple[float, float, int, float]:
    run_cmd(
        [
            str(binary),
            "benchmark",
            "--bots",
            bot,
            "--seed-file",
            str(seeds_file),
            "--max-frames",
            str(max_frames),
            "--objective",
            "score",
            "--save-top",
            "1",
            "--jobs",
            str(jobs),
            "--out-dir",
            str(out_dir),
        ],
        cwd=autopilot_root,
    )

    summary = load_json(out_dir / "summary.json")
    ranking = None
    for item in summary.get("bot_rankings", []):
        if item.get("bot_id") == bot:
            ranking = item
            break

    if ranking is None:
        raise RuntimeError(f"bot '{bot}' not found in {out_dir / 'summary.json'}")

    return (
        float(ranking["objective_value"]),
        float(ranking["avg_score"]),
        int(ranking["max_score"]),
        float(ranking["avg_frames"]),
    )


def write_leaderboard(path: Path, results: List[CandidateResult]) -> None:
    with path.open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(
            [
                "iteration",
                "candidate",
                "strategy",
                "objective_value",
                "avg_score",
                "max_score",
                "avg_frames",
                "out_dir",
            ]
        )
        for row in results:
            writer.writerow(
                [
                    row.iteration,
                    row.candidate,
                    row.strategy,
                    f"{row.objective_value:.6f}",
                    f"{row.avg_score:.6f}",
                    row.max_score,
                    f"{row.avg_frames:.6f}",
                    row.out_dir,
                ]
            )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Iteratively tune codex-potential-adaptive profile for score objective"
    )
    parser.add_argument("--iterations", type=int, default=6)
    parser.add_argument("--candidates", type=int, default=6)
    parser.add_argument("--max-frames", type=int, default=108_000)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--bot", default="codex-potential-adaptive")
    parser.add_argument(
        "--seeds-file",
        default="codex-tuner/seeds/screen-seeds.txt",
    )
    parser.add_argument("--random-seed", type=int, default=424242)
    parser.add_argument("--initial-step", type=float, default=0.18)
    parser.add_argument("--decay", type=float, default=0.86)
    parser.add_argument("--min-step", type=float, default=0.04)
    parser.add_argument(
        "--install-mode",
        choices=["champion", "restore"],
        default="champion",
        help="champion=install session best into active profile, restore=restore previous active profile",
    )
    parser.add_argument(
        "--start-profile",
        default="",
        help="Optional profile JSON path to use as the initial incumbent",
    )
    parser.add_argument(
        "--anchor-mode",
        choices=["all", "core"],
        default="core",
        help="core=base/champion anchors, all=core + auto champion-* archives",
    )
    parser.add_argument(
        "--selection-metric",
        choices=["objective", "score", "insane"],
        default="score",
        help="How to rank candidates: objective=balanced, score=avg score first, insane=max score first",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    if args.iterations < 1:
        raise ValueError("--iterations must be >= 1")
    if args.candidates < 2:
        raise ValueError("--candidates must be >= 2")
    if args.max_frames < 1:
        raise ValueError("--max-frames must be >= 1")

    autopilot_root = Path(__file__).resolve().parents[2]
    lab_root = autopilot_root / "codex-tuner"

    seeds_file = Path(args.seeds_file)
    if not seeds_file.is_absolute():
        seeds_file = (autopilot_root / seeds_file).resolve()

    base_profile_path = lab_root / "profiles" / "base.json"
    champion_profile_path = lab_root / "profiles" / "champion.json"
    active_profile_path = autopilot_root / "codex-" / "state" / "adaptive-profile.json"
    active_profile_path.parent.mkdir(parents=True, exist_ok=True)

    if not seeds_file.exists():
        raise FileNotFoundError(f"seed file not found: {seeds_file}")
    if not base_profile_path.exists():
        raise FileNotFoundError(f"base profile not found: {base_profile_path}")
    if not active_profile_path.exists():
        seed_source = champion_profile_path if champion_profile_path.exists() else base_profile_path
        write_json(active_profile_path, normalize_profile(load_json(seed_source)))

    session_name = time.strftime("session-%Y%m%d-%H%M%S", time.gmtime())
    session_dir = lab_root / "runs" / session_name
    session_dir.mkdir(parents=True, exist_ok=False)

    old_profile = normalize_profile(load_json(active_profile_path))
    write_json(session_dir / "backup-active-profile.json", old_profile)

    rng = random.Random(args.random_seed)
    if args.start_profile.strip():
        resolved_start = Path(args.start_profile)
        if not resolved_start.is_absolute():
            resolved_start = (autopilot_root / resolved_start).resolve()
        if not resolved_start.exists():
            raise FileNotFoundError(f"start profile not found: {resolved_start}")
        start_profile_path = resolved_start
    elif champion_profile_path.exists():
        start_profile_path = champion_profile_path
    else:
        start_profile_path = base_profile_path

    incumbent_profile = normalize_profile(load_json(start_profile_path))
    global_best_profile = dict(incumbent_profile)
    global_best_result: CandidateResult | None = None
    incumbent_sig = profile_signature(incumbent_profile)
    anchor_profiles: List[Tuple[str, Dict[str, float]]] = []
    anchor_source_pairs: List[Tuple[str, Path]] = [
        ("base", base_profile_path),
        ("champion", champion_profile_path),
    ]
    if args.anchor_mode == "all":
        seen_anchor_paths = {path.resolve() for _, path in anchor_source_pairs if path.exists()}
        for path in sorted((lab_root / "profiles").glob("champion-*.json")):
            resolved = path.resolve()
            if not path.exists() or resolved in seen_anchor_paths:
                continue
            if path.name in {"champion.json"}:
                continue
            label = f"auto_{path.stem.replace('-', '_')}"
            anchor_source_pairs.append((label, path))
            seen_anchor_paths.add(resolved)

    for label, path in anchor_source_pairs:
        if not path.exists():
            continue
        profile = normalize_profile(load_json(path))
        if profile_signature(profile) == incumbent_sig:
            continue
        anchor_profiles.append((label, profile))

    momentum: Dict[str, float] | None = None
    last_gain_anchor: Dict[str, float] | None = None
    stagnation_count = 0
    history: List[Dict] = []

    binary = ensure_binary(autopilot_root)
    print(f"Using binary: {binary}")
    print(f"Session dir: {session_dir}")
    print(f"Start profile: {start_profile_path}")

    success = False
    try:
        for iteration in range(1, args.iterations + 1):
            iter_dir = session_dir / f"iter-{iteration:03d}"
            iter_dir.mkdir(parents=True, exist_ok=True)

            base_step = max(args.min_step, args.initial_step * (args.decay ** (iteration - 1)))
            search_step = min(base_step * (1.0 + 0.3 * stagnation_count), base_step * 2.4)

            candidate_specs: List[Tuple[Dict[str, float], str]] = []
            seen: set[str] = set()

            def push_candidate(profile: Dict[str, float], strategy: str) -> bool:
                normalized = normalize_profile(profile)
                sig = profile_signature(normalized)
                if sig in seen:
                    return False
                seen.add(sig)
                candidate_specs.append((normalized, strategy))
                return True

            push_candidate(incumbent_profile, "incumbent")

            if momentum is not None and len(candidate_specs) < args.candidates:
                momentum_scale = 1.0 + rng.uniform(-0.25, 0.45)
                push_candidate(
                    apply_momentum(incumbent_profile, momentum, momentum_scale),
                    "momentum",
                )

            if last_gain_anchor is not None and len(candidate_specs) < args.candidates:
                alpha = 0.5 + rng.uniform(-0.18, 0.18)
                push_candidate(
                    blend_profiles(incumbent_profile, last_gain_anchor, alpha),
                    "blend_last_gain",
                )

            if anchor_profiles and len(candidate_specs) < args.candidates:
                anchor_label, anchor_profile = rng.choice(anchor_profiles)
                alpha = 0.58 + rng.uniform(-0.32, 0.2)
                push_candidate(
                    blend_profiles(incumbent_profile, anchor_profile, alpha),
                    f"blend_{anchor_label}",
                )

            if (
                anchor_profiles
                and len(candidate_specs) < args.candidates
                and (stagnation_count >= 1 or args.selection_metric == "insane")
            ):
                anchor_label, anchor_profile = rng.choice(anchor_profiles)
                anchor_step = search_step * (1.35 + rng.uniform(-0.1, 0.5))
                anchor_seed = mutate_profile(
                    anchor_profile,
                    rng,
                    step=anchor_step,
                    min_fields=5,
                    max_fields=len(SCALE_KEYS),
                    delta_scale=0.24,
                )
                alpha = 0.35 + rng.uniform(-0.12, 0.16)
                push_candidate(
                    blend_profiles(anchor_seed, incumbent_profile, alpha),
                    f"anchor_mutate_{anchor_label}",
                )

            if stagnation_count >= 2 and len(candidate_specs) < args.candidates:
                push_candidate(
                    mutate_profile_aggressive(incumbent_profile, rng, search_step),
                    "escape",
                )

            if args.selection_metric == "insane" and len(candidate_specs) < args.candidates:
                chaos = mutate_profile_aggressive(incumbent_profile, rng, search_step * 1.45)
                if rng.random() < 0.72:
                    for key in rng.sample(SCALE_KEYS, rng.randint(1, 3)):
                        lo, hi = SCALE_BOUNDS[key]
                        chaos[key] = round(rng.uniform(lo, hi), 6)
                    if rng.random() < 0.55:
                        chaos[DELTA_KEY] = round(rng.uniform(*DELTA_BOUNDS), 6)
                push_candidate(normalize_profile(chaos), "chaos")

            attempts = 0
            while len(candidate_specs) < args.candidates and attempts < 320:
                attempts += 1
                local_step = search_step * (1.0 + rng.uniform(-0.15, 0.35))
                if stagnation_count >= 2:
                    local_step *= 1.2
                if args.selection_metric == "insane":
                    local_step *= 1.2 + rng.uniform(-0.08, 0.32)
                    if rng.random() < 0.24:
                        profile = mutate_profile_aggressive(incumbent_profile, rng, local_step)
                        push_candidate(profile, "mutate_aggressive")
                    else:
                        profile = mutate_profile(
                            incumbent_profile,
                            rng,
                            local_step,
                            min_fields=4,
                            max_fields=len(SCALE_KEYS),
                            delta_scale=0.18,
                        )
                        push_candidate(profile, "mutate")
                else:
                    profile = mutate_profile(incumbent_profile, rng, local_step)
                    push_candidate(profile, "mutate")

            if len(candidate_specs) < args.candidates:
                raise RuntimeError(
                    f"could not generate {args.candidates} unique candidates (got {len(candidate_specs)})"
                )

            results: List[CandidateResult] = []
            for candidate_idx, (profile, strategy) in enumerate(candidate_specs):
                cand_dir = iter_dir / f"cand-{candidate_idx:02d}"
                cand_dir.mkdir(parents=True, exist_ok=True)

                write_json(cand_dir / "profile.json", profile)
                write_json(active_profile_path, profile)

                try:
                    objective, avg_score, max_score, avg_frames = benchmark_profile(
                        binary=binary,
                        autopilot_root=autopilot_root,
                        bot=args.bot,
                        seeds_file=seeds_file,
                        max_frames=args.max_frames,
                        jobs=args.jobs,
                        out_dir=cand_dir,
                    )
                except subprocess.CalledProcessError:
                    objective, avg_score, max_score, avg_frames = (
                        -math.inf,
                        -math.inf,
                        0,
                        0.0,
                    )

                result = CandidateResult(
                    iteration=iteration,
                    candidate=candidate_idx,
                    strategy=strategy,
                    objective_value=objective,
                    avg_score=avg_score,
                    max_score=max_score,
                    avg_frames=avg_frames,
                    out_dir=str(cand_dir),
                    profile=profile,
                )
                results.append(result)

                print(
                    f"iter={iteration:03d} cand={candidate_idx:02d} strategy={strategy} "
                    f"objective={objective:.3f} avg_score={avg_score:.3f} "
                    f"max_score={max_score} avg_frames={avg_frames:.2f}",
                    flush=True,
                )

            results.sort(
                key=lambda row: metric_tuple(row, args.selection_metric), reverse=True
            )
            write_leaderboard(iter_dir / "leaderboard.csv", results)

            winner = results[0]
            incumbent_result = next(r for r in results if r.strategy == "incumbent")
            improved = better_than(winner, incumbent_result, args.selection_metric)

            if improved:
                previous = dict(incumbent_profile)
                incumbent_profile = dict(winner.profile)
                momentum = profile_delta(incumbent_profile, previous)
                last_gain_anchor = previous
                stagnation_count = 0
            else:
                stagnation_count += 1

            if global_best_result is None or better_than(
                winner, global_best_result, args.selection_metric
            ):
                global_best_result = winner
                global_best_profile = dict(winner.profile)

            write_json(iter_dir / "winner-profile.json", incumbent_profile)

            history.append(
                {
                    "iteration": iteration,
                    "base_step": base_step,
                    "search_step": search_step,
                    "improved": improved,
                    "stagnation_count": stagnation_count,
                    "winner": {
                        "candidate": winner.candidate,
                        "strategy": winner.strategy,
                        "objective_value": winner.objective_value,
                        "avg_score": winner.avg_score,
                        "max_score": winner.max_score,
                        "avg_frames": winner.avg_frames,
                    },
                }
            )

            print(
                f"iter={iteration:03d} winner=cand-{winner.candidate:02d} ({winner.strategy}) "
                f"objective={winner.objective_value:.3f} avg_score={winner.avg_score:.3f} "
                f"improved={improved} stagnation={stagnation_count}",
                flush=True,
            )

        if global_best_result is None:
            raise RuntimeError("no candidates were evaluated")

        write_json(session_dir / "champion.json", global_best_profile)
        write_json(champion_profile_path, global_best_profile)

        summary = {
            "session": session_name,
            "bot": args.bot,
            "iterations": args.iterations,
            "candidates": args.candidates,
            "max_frames": args.max_frames,
            "jobs": args.jobs,
            "selection_metric": args.selection_metric,
            "anchor_mode": args.anchor_mode,
            "install_mode": args.install_mode,
            "seeds_file": str(seeds_file),
            "random_seed": args.random_seed,
            "start_profile": str(start_profile_path),
            "best": {
                "objective_value": global_best_result.objective_value,
                "avg_score": global_best_result.avg_score,
                "max_score": global_best_result.max_score,
                "avg_frames": global_best_result.avg_frames,
                "iteration": global_best_result.iteration,
                "candidate": global_best_result.candidate,
                "strategy": global_best_result.strategy,
            },
            "champion_profile": global_best_profile,
            "history": history,
        }
        write_json(session_dir / "summary.json", summary)

        if args.install_mode == "champion":
            write_json(active_profile_path, global_best_profile)
        else:
            write_json(active_profile_path, old_profile)

        (lab_root / "runs" / "latest-session.txt").write_text(f"{session_dir}\n")

        success = True
        print(f"SESSION_DIR={session_dir}")
        print(f"CHAMPION_PROFILE={champion_profile_path}")
        print(f"BEST_OBJECTIVE={global_best_result.objective_value:.6f}")
        print(f"BEST_AVG_SCORE={global_best_result.avg_score:.6f}")
        return 0
    finally:
        if not success:
            write_json(active_profile_path, old_profile)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise
