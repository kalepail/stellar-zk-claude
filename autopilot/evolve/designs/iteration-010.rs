// ── evolve-candidate: iteration-010 ──
// Change: speed_soft_cap 3.8 → 3.5 (lower max speed for better edge control)
// Result: avg_score 49,629 → 55,713 (+12.2%), max_score 154,540 → 175,710 (+14%)
SearchConfig {
    id: "evolve-candidate",
    description:
        "Progressive evolution candidate — iteratively improved by automated loop.",
    lookahead_frames: 22.0,
    risk_weight_asteroid: 2.2,
    risk_weight_saucer: 2.8,
    risk_weight_bullet: 4.5,
    survival_weight: 3.2,
    aggression_weight: 0.65,
    fire_reward: 1.5,
    shot_penalty: 0.75,
    miss_fire_penalty: 1.0,
    action_penalty: 0.009,
    turn_penalty: 0.008,
    thrust_penalty: 0.005,
    center_weight: 0.85,
    edge_penalty: 0.70,
    speed_soft_cap: 3.5,
    fire_tolerance_bam: 8,
    fire_distance_px: 300.0,
    lurk_trigger_frames: 250,
    lurk_aggression_boost: 1.8,
},
