use super::*;
use asteroids_verifier_core::tape::crc32;

// Curated roster: only high-performing bots retained for maintainability.
pub(super) fn search_bot_configs() -> &'static [SearchConfig] {
    &[
        SearchConfig {
            id: "omega-marathon",
            description: "Deep-lookahead action-search marathon profile.",
            lookahead_frames: 20.0,
            risk_weight_asteroid: 1.45,
            risk_weight_saucer: 2.05,
            risk_weight_bullet: 2.85,
            survival_weight: 2.05,
            aggression_weight: 0.5,
            fire_reward: 0.68,
            shot_penalty: 0.9,
            miss_fire_penalty: 1.2,
            action_penalty: 0.011,
            turn_penalty: 0.013,
            thrust_penalty: 0.012,
            center_weight: 0.52,
            edge_penalty: 0.36,
            speed_soft_cap: 3.95,
            fire_tolerance_bam: 7,
            fire_distance_px: 250.0,
            lurk_trigger_frames: 310,
            lurk_aggression_boost: 1.4,
        },
        SearchConfig {
            id: "omega-lurk-breaker",
            description: "Anti-lurk action-search bot that forces safe kills.",
            lookahead_frames: 16.0,
            risk_weight_asteroid: 1.2,
            risk_weight_saucer: 1.95,
            risk_weight_bullet: 2.6,
            survival_weight: 1.55,
            aggression_weight: 0.74,
            fire_reward: 1.12,
            shot_penalty: 0.7,
            miss_fire_penalty: 0.9,
            action_penalty: 0.0095,
            turn_penalty: 0.011,
            thrust_penalty: 0.01,
            center_weight: 0.46,
            edge_penalty: 0.29,
            speed_soft_cap: 4.4,
            fire_tolerance_bam: 8,
            fire_distance_px: 300.0,
            lurk_trigger_frames: 240,
            lurk_aggression_boost: 2.05,
        },
        SearchConfig {
            id: "omega-ace",
            description: "Ballistic shot-discipline striker with high-value saucer prioritization.",
            lookahead_frames: 17.0,
            risk_weight_asteroid: 1.25,
            risk_weight_saucer: 1.9,
            risk_weight_bullet: 2.55,
            survival_weight: 1.6,
            aggression_weight: 0.9,
            fire_reward: 1.34,
            shot_penalty: 0.78,
            miss_fire_penalty: 1.15,
            action_penalty: 0.0105,
            turn_penalty: 0.0115,
            thrust_penalty: 0.0108,
            center_weight: 0.44,
            edge_penalty: 0.24,
            speed_soft_cap: 4.45,
            fire_tolerance_bam: 8,
            fire_distance_px: 320.0,
            lurk_trigger_frames: 260,
            lurk_aggression_boost: 1.85,
        },
        SearchConfig {
            id: "omega-alltime-hunter",
            description: "Peak-score hunter for long 30-minute-cap spike runs.",
            lookahead_frames: 17.0,
            risk_weight_asteroid: 1.08,
            risk_weight_saucer: 1.62,
            risk_weight_bullet: 2.28,
            survival_weight: 1.32,
            aggression_weight: 1.18,
            fire_reward: 1.5,
            shot_penalty: 0.56,
            miss_fire_penalty: 0.72,
            action_penalty: 0.007,
            turn_penalty: 0.0085,
            thrust_penalty: 0.0085,
            center_weight: 0.34,
            edge_penalty: 0.16,
            speed_soft_cap: 4.95,
            fire_tolerance_bam: 9,
            fire_distance_px: 360.0,
            lurk_trigger_frames: 245,
            lurk_aggression_boost: 2.05,
        },
        SearchConfig {
            id: "omega-supernova",
            description: "High-pressure saucer/fragment farming profile for extreme score pace.",
            lookahead_frames: 16.0,
            risk_weight_asteroid: 1.0,
            risk_weight_saucer: 1.5,
            risk_weight_bullet: 2.15,
            survival_weight: 1.18,
            aggression_weight: 1.28,
            fire_reward: 1.65,
            shot_penalty: 0.48,
            miss_fire_penalty: 0.64,
            action_penalty: 0.0065,
            turn_penalty: 0.008,
            thrust_penalty: 0.008,
            center_weight: 0.26,
            edge_penalty: 0.14,
            speed_soft_cap: 5.2,
            fire_tolerance_bam: 10,
            fire_distance_px: 390.0,
            lurk_trigger_frames: 235,
            lurk_aggression_boost: 2.2,
        },
        // ── evolve-candidate: modified by automated evolution loop ──
        // v2: Seeded from cross-analysis of claude-autopilot (35K avg) + codex-autopilot (67K avg).
        // Key shifts: much higher survival+bullet risk, eager fire, strong center bias, fast lurk.
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
    ]
}

fn offline_bot_configs() -> &'static [OfflineConfig] {
    &[
        OfflineConfig {
            id: "offline-supernova-hunt",
            description:
                "Offline aggressive wrap-native scorer for 30-minute-cap record hunting.",
            planner: PrecisionConfig {
                id: "offline-supernova-hunt-inner",
                description: "internal",
                depth: 6,
                beam_width: 14,
                discount: 0.89,
                risk_weight_asteroid: 1.04,
                risk_weight_saucer: 1.58,
                risk_weight_bullet: 2.26,
                survival_weight: 1.48,
                imminent_penalty: 2.48,
                shot_reward: 2.08,
                shot_penalty: 0.66,
                miss_fire_penalty: 0.82,
                action_penalty: 0.011,
                turn_penalty: 0.016,
                thrust_penalty: 0.014,
                center_weight: 0.12,
                edge_penalty: 0.03,
                speed_soft_cap: 5.05,
                target_weight_large: 1.14,
                target_weight_medium: 1.68,
                target_weight_small: 2.56,
                target_weight_saucer_large: 2.52,
                target_weight_saucer_small: 4.05,
                lurk_trigger_frames: 225,
                lurk_shot_boost: 2.38,
            },
            depth: 7,
            max_actions_per_node: 9,
            upper_step_bound: 37.0,
            bound_slack: 0.52,
            guardian_mode: true,
            action_change_penalty: 0.0055,
        },
        OfflineConfig {
            id: "offline-wrap-apex-score",
            description: "Long-horizon scorer optimized for high-value wrap intercept chains.",
            planner: PrecisionConfig {
                id: "offline-wrap-apex-score-inner",
                description: "internal",
                depth: 6,
                beam_width: 14,
                discount: 0.91,
                risk_weight_asteroid: 1.22,
                risk_weight_saucer: 1.9,
                risk_weight_bullet: 2.6,
                survival_weight: 1.75,
                imminent_penalty: 2.8,
                shot_reward: 1.82,
                shot_penalty: 0.82,
                miss_fire_penalty: 1.0,
                action_penalty: 0.016,
                turn_penalty: 0.023,
                thrust_penalty: 0.019,
                center_weight: 0.12,
                edge_penalty: 0.03,
                speed_soft_cap: 4.75,
                target_weight_large: 1.08,
                target_weight_medium: 1.55,
                target_weight_small: 2.36,
                target_weight_saucer_large: 2.24,
                target_weight_saucer_small: 3.62,
                lurk_trigger_frames: 260,
                lurk_shot_boost: 1.95,
            },
            depth: 7,
            max_actions_per_node: 8,
            upper_step_bound: 33.0,
            bound_slack: 0.46,
            guardian_mode: true,
            action_change_penalty: 0.0075,
        },
        OfflineConfig {
            id: "offline-wrap-sniper30",
            description:
                "Offline 30-minute planner prioritizing high-certainty intercepts with low miss fire.",
            planner: PrecisionConfig {
                id: "offline-wrap-sniper30-inner",
                description: "internal",
                depth: 7,
                beam_width: 16,
                discount: 0.92,
                risk_weight_asteroid: 1.34,
                risk_weight_saucer: 2.08,
                risk_weight_bullet: 2.86,
                survival_weight: 2.06,
                imminent_penalty: 3.22,
                shot_reward: 1.48,
                shot_penalty: 1.04,
                miss_fire_penalty: 1.52,
                action_penalty: 0.017,
                turn_penalty: 0.024,
                thrust_penalty: 0.021,
                center_weight: 0.12,
                edge_penalty: 0.04,
                speed_soft_cap: 4.45,
                target_weight_large: 0.98,
                target_weight_medium: 1.42,
                target_weight_small: 1.95,
                target_weight_saucer_large: 2.0,
                target_weight_saucer_small: 3.25,
                lurk_trigger_frames: 280,
                lurk_shot_boost: 1.86,
            },
            depth: 8,
            max_actions_per_node: 8,
            upper_step_bound: 31.0,
            bound_slack: 0.44,
            guardian_mode: true,
            action_change_penalty: 0.0075,
        },
        OfflineConfig {
            id: "offline-wrap-endurancex",
            description:
                "Offline ultra-endurance planner tuned for deep survival with efficient controls.",
            planner: PrecisionConfig {
                id: "offline-wrap-endurancex-inner",
                description: "internal",
                depth: 7,
                beam_width: 14,
                discount: 0.94,
                risk_weight_asteroid: 1.72,
                risk_weight_saucer: 2.58,
                risk_weight_bullet: 3.52,
                survival_weight: 2.78,
                imminent_penalty: 4.12,
                shot_reward: 0.82,
                shot_penalty: 1.42,
                miss_fire_penalty: 1.88,
                action_penalty: 0.028,
                turn_penalty: 0.039,
                thrust_penalty: 0.033,
                center_weight: 0.18,
                edge_penalty: 0.05,
                speed_soft_cap: 3.7,
                target_weight_large: 0.66,
                target_weight_medium: 0.88,
                target_weight_small: 1.06,
                target_weight_saucer_large: 1.08,
                target_weight_saucer_small: 1.62,
                lurk_trigger_frames: 390,
                lurk_shot_boost: 1.18,
            },
            depth: 8,
            max_actions_per_node: 6,
            upper_step_bound: 14.5,
            bound_slack: 0.26,
            guardian_mode: true,
            action_change_penalty: 0.0115,
        },
        OfflineConfig {
            id: "offline-wrap-frugal-ace",
            description:
                "Offline ultra-frugal control planner enforcing sparse shots and minimal movement.",
            planner: PrecisionConfig {
                id: "offline-wrap-frugal-ace-inner",
                description: "internal",
                depth: 7,
                beam_width: 13,
                discount: 0.94,
                risk_weight_asteroid: 1.78,
                risk_weight_saucer: 2.66,
                risk_weight_bullet: 3.6,
                survival_weight: 2.9,
                imminent_penalty: 4.25,
                shot_reward: 0.96,
                shot_penalty: 1.62,
                miss_fire_penalty: 2.15,
                action_penalty: 0.034,
                turn_penalty: 0.046,
                thrust_penalty: 0.039,
                center_weight: 0.2,
                edge_penalty: 0.05,
                speed_soft_cap: 3.65,
                target_weight_large: 0.7,
                target_weight_medium: 0.92,
                target_weight_small: 1.1,
                target_weight_saucer_large: 1.18,
                target_weight_saucer_small: 1.8,
                lurk_trigger_frames: 400,
                lurk_shot_boost: 1.16,
            },
            depth: 8,
            max_actions_per_node: 5,
            upper_step_bound: 14.0,
            bound_slack: 0.24,
            guardian_mode: true,
            action_change_penalty: 0.013,
        },
        OfflineConfig {
            id: "offline-wrap-sureshot",
            description:
                "Offline intercept planner favoring high-certainty hits with restrained control usage.",
            planner: PrecisionConfig {
                id: "offline-wrap-sureshot-inner",
                description: "internal",
                depth: 7,
                beam_width: 15,
                discount: 0.93,
                risk_weight_asteroid: 1.44,
                risk_weight_saucer: 2.12,
                risk_weight_bullet: 2.95,
                survival_weight: 2.22,
                imminent_penalty: 3.36,
                shot_reward: 1.42,
                shot_penalty: 1.2,
                miss_fire_penalty: 1.82,
                action_penalty: 0.021,
                turn_penalty: 0.029,
                thrust_penalty: 0.025,
                center_weight: 0.14,
                edge_penalty: 0.04,
                speed_soft_cap: 4.25,
                target_weight_large: 0.95,
                target_weight_medium: 1.34,
                target_weight_small: 1.82,
                target_weight_saucer_large: 1.92,
                target_weight_saucer_small: 3.05,
                lurk_trigger_frames: 305,
                lurk_shot_boost: 1.74,
            },
            depth: 8,
            max_actions_per_node: 7,
            upper_step_bound: 27.0,
            bound_slack: 0.4,
            guardian_mode: true,
            action_change_penalty: 0.009,
        },
    ]
}

const RECORD_ENDURANCEX_TAPE_PATH: &str = concat!(
    "checkpoints/",
    "rank01-offline-wrap-endurancex-seed6046c93d-score289810-frames67109.tape"
);

pub(super) fn record_locked_bot_configs() -> &'static [ReplayConfig] {
    &[ReplayConfig {
        id: "record-lock-endurancex-6046c93d",
        description: "Locked replay bot for canonical all-time run preservation (seed 0x6046c93d).",
        expected_seed: 0x6046_C93D,
        tape_rel_path: RECORD_ENDURANCEX_TAPE_PATH,
        max_frames_hint: 108_000,
    }]
}

pub fn bot_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = search_bot_configs().iter().map(|cfg| cfg.id).collect();
    ids.extend(offline_bot_configs().iter().map(|cfg| cfg.id));
    ids.extend(record_locked_bot_configs().iter().map(|cfg| cfg.id));
    ids.extend(codex::codex_bot_ids());
    ids.extend(crate::claude::bot_ids());
    ids
}

pub fn describe_bots() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = search_bot_configs()
        .iter()
        .map(|cfg| (cfg.id, cfg.description))
        .collect();
    out.extend(
        offline_bot_configs()
            .iter()
            .map(|cfg| (cfg.id, cfg.description)),
    );
    out.extend(
        record_locked_bot_configs()
            .iter()
            .map(|cfg| (cfg.id, cfg.description)),
    );
    out.extend(codex::describe_codex_bots());
    out.extend(crate::claude::describe_bots());
    out
}

pub fn create_bot(id: &str) -> Option<Box<dyn AutopilotBot>> {
    if let Some(cfg) = search_bot_configs().iter().find(|cfg| cfg.id == id) {
        return Some(Box::new(SearchBot::new(*cfg)));
    }
    if let Some(cfg) = offline_bot_configs().iter().find(|cfg| cfg.id == id) {
        return Some(Box::new(OfflineControlBot::new(*cfg)));
    }
    if let Some(cfg) = record_locked_bot_configs().iter().find(|cfg| cfg.id == id) {
        return Some(Box::new(ReplayBot::new(*cfg)));
    }
    if let Some(bot) = codex::create_codex_bot(id) {
        return Some(bot);
    }
    if let Some(bot) = crate::claude::create_bot(id) {
        return Some(bot);
    }
    None
}

fn hash_json(value: &serde_json::Value) -> String {
    let encoded =
        serde_json::to_vec(value).expect("serializing bot config for fingerprint should not fail");
    let digest = crc32(&encoded);
    format!("crc32:{digest:08x}:len:{}", encoded.len())
}

pub fn bot_manifest_entries() -> Vec<BotManifestEntry> {
    let mut out = Vec::new();

    for cfg in search_bot_configs() {
        let config = serde_json::to_value(cfg).expect("search bot config should serialize");
        out.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "search".to_string(),
            description: cfg.description.to_string(),
            config_hash: hash_json(&config),
            config,
        });
    }

    for cfg in offline_bot_configs() {
        let config = serde_json::to_value(cfg).expect("offline bot config should serialize");
        out.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "offline_control".to_string(),
            description: cfg.description.to_string(),
            config_hash: hash_json(&config),
            config,
        });
    }

    for cfg in record_locked_bot_configs() {
        let config = serde_json::to_value(cfg).expect("record-lock bot config should serialize");
        out.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "record_lock".to_string(),
            description: cfg.description.to_string(),
            config_hash: hash_json(&config),
            config,
        });
    }

    for (id, family, config) in codex::codex_manifest_entries() {
        out.push(BotManifestEntry {
            id: id.to_string(),
            family: family.to_string(),
            description: describe_bots()
                .into_iter()
                .find_map(|(bot_id, desc)| (bot_id == id).then_some(desc))
                .unwrap_or("codex bot")
                .to_string(),
            config_hash: hash_json(&config),
            config,
        });
    }

    out
}

pub fn bot_fingerprint(id: &str) -> Option<String> {
    bot_manifest_entries()
        .into_iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.config_hash)
}
