//! Kimi Bot Roster Integration
//!
//! Integrates Kimi bots into the main bot roster system.

use super::configs;
use crate::bots::{AutopilotBot, BotManifestEntry, KimiLearningBot, SearchConfig};
use serde_json;

/// Returns all Kimi bot configurations as SearchConfigs for roster compatibility
pub fn kimi_search_configs() -> Vec<SearchConfig> {
    let mut configs = Vec::new();

    // Add all hunter configs
    for cfg in configs::kimi_hunter_configs() {
        configs.push(convert_kimi_to_search_config(cfg));
    }

    // Add all survivor configs
    for cfg in configs::kimi_survivor_configs() {
        configs.push(convert_kimi_to_search_config(cfg));
    }

    // Add all sniper configs (map precision to search)
    for cfg in configs::kimi_sniper_configs() {
        configs.push(convert_precision_to_search_config(cfg));
    }

    // Add all wrap master configs
    for cfg in configs::kimi_wrap_master_configs() {
        configs.push(convert_kimi_to_search_config(cfg));
    }

    // Add all saucer killer configs
    for cfg in configs::kimi_saucer_killer_configs() {
        configs.push(convert_kimi_to_search_config(cfg));
    }

    // Add all super ship configs
    for cfg in configs::kimi_super_ship_configs() {
        configs.push(convert_kimi_to_search_config(cfg));
    }

    configs
}

fn convert_kimi_to_search_config(cfg: &configs::KimiSearchConfig) -> SearchConfig {
    SearchConfig {
        id: cfg.id,
        description: cfg.description,
        lookahead_frames: cfg.lookahead_frames,
        risk_weight_asteroid: cfg.risk_weight_asteroid,
        risk_weight_saucer: cfg.risk_weight_saucer,
        risk_weight_bullet: cfg.risk_weight_bullet,
        survival_weight: cfg.survival_weight,
        aggression_weight: cfg.aggression_weight,
        fire_reward: cfg.fire_reward,
        shot_penalty: cfg.shot_penalty,
        miss_fire_penalty: cfg.miss_fire_penalty,
        action_penalty: cfg.action_penalty,
        turn_penalty: cfg.turn_penalty,
        thrust_penalty: cfg.thrust_penalty,
        center_weight: cfg.center_weight,
        edge_penalty: cfg.edge_penalty,
        speed_soft_cap: cfg.speed_soft_cap,
        fire_tolerance_bam: cfg.fire_tolerance_bam,
        fire_distance_px: cfg.fire_distance_px,
        lurk_trigger_frames: cfg.lurk_trigger_frames,
        lurk_aggression_boost: cfg.lurk_aggression_boost,
    }
}

fn convert_precision_to_search_config(cfg: &configs::KimiPrecisionConfig) -> SearchConfig {
    SearchConfig {
        id: cfg.id,
        description: cfg.description,
        lookahead_frames: (cfg.depth as f64) * 3.0 + 3.0,
        risk_weight_asteroid: cfg.risk_weight_asteroid,
        risk_weight_saucer: cfg.risk_weight_saucer,
        risk_weight_bullet: cfg.risk_weight_bullet,
        survival_weight: cfg.survival_weight,
        aggression_weight: cfg.survival_weight * 0.35, // Estimate
        fire_reward: cfg.shot_reward * 0.5,
        shot_penalty: cfg.shot_penalty,
        miss_fire_penalty: cfg.miss_fire_penalty,
        action_penalty: cfg.action_penalty,
        turn_penalty: cfg.turn_penalty,
        thrust_penalty: cfg.thrust_penalty,
        center_weight: cfg.center_weight,
        edge_penalty: cfg.edge_penalty,
        speed_soft_cap: cfg.speed_soft_cap,
        fire_tolerance_bam: 7,
        fire_distance_px: 280.0,
        lurk_trigger_frames: cfg.lurk_trigger_frames,
        lurk_aggression_boost: cfg.lurk_shot_boost,
    }
}

/// Get Kimi bot IDs
pub fn kimi_bot_ids() -> Vec<&'static str> {
    configs::all_kimi_bot_ids()
}

/// Create a Kimi bot by ID
pub fn create_kimi_bot(id: &str) -> Option<Box<dyn AutopilotBot>> {
    super::create_kimi_bot(id)
}

/// Get Kimi bot manifest entries
pub fn kimi_bot_manifest_entries() -> Vec<BotManifestEntry> {
    let mut entries = Vec::new();

    for cfg in configs::kimi_hunter_configs() {
        let search_cfg = convert_kimi_to_search_config(cfg);
        let config = serde_json::to_value(&search_cfg).expect("config should serialize");
        entries.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "kimi_hunter".to_string(),
            description: cfg.description.to_string(),
            config_hash: format!("kimi:{}", cfg.id),
            config,
        });
    }

    for cfg in configs::kimi_survivor_configs() {
        let search_cfg = convert_kimi_to_search_config(cfg);
        let config = serde_json::to_value(&search_cfg).expect("config should serialize");
        entries.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "kimi_survivor".to_string(),
            description: cfg.description.to_string(),
            config_hash: format!("kimi:{}", cfg.id),
            config,
        });
    }

    for cfg in configs::kimi_sniper_configs() {
        let search_cfg = convert_precision_to_search_config(cfg);
        let config = serde_json::to_value(&search_cfg).expect("config should serialize");
        entries.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "kimi_sniper".to_string(),
            description: cfg.description.to_string(),
            config_hash: format!("kimi:{}", cfg.id),
            config,
        });
    }

    for cfg in configs::kimi_wrap_master_configs() {
        let search_cfg = convert_kimi_to_search_config(cfg);
        let config = serde_json::to_value(&search_cfg).expect("config should serialize");
        entries.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "kimi_wrap_master".to_string(),
            description: cfg.description.to_string(),
            config_hash: format!("kimi:{}", cfg.id),
            config,
        });
    }

    for cfg in configs::kimi_saucer_killer_configs() {
        let search_cfg = convert_kimi_to_search_config(cfg);
        let config = serde_json::to_value(&search_cfg).expect("config should serialize");
        entries.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "kimi_saucer_killer".to_string(),
            description: cfg.description.to_string(),
            config_hash: format!("kimi:{}", cfg.id),
            config,
        });
    }

    for cfg in configs::kimi_super_ship_configs() {
        let search_cfg = convert_kimi_to_search_config(cfg);
        let config = serde_json::to_value(&search_cfg).expect("config should serialize");
        entries.push(BotManifestEntry {
            id: cfg.id.to_string(),
            family: "kimi_super_ship".to_string(),
            description: cfg.description.to_string(),
            config_hash: format!("kimi:{}", cfg.id),
            config,
        });
    }

    entries
}

/// Get Kimi bot descriptions
pub fn kimi_describe_bots() -> Vec<(&'static str, &'static str)> {
    let mut descriptions = Vec::new();

    for cfg in configs::kimi_hunter_configs() {
        descriptions.push((cfg.id, cfg.description));
    }
    for cfg in configs::kimi_survivor_configs() {
        descriptions.push((cfg.id, cfg.description));
    }
    for cfg in configs::kimi_sniper_configs() {
        descriptions.push((cfg.id, cfg.description));
    }
    for cfg in configs::kimi_wrap_master_configs() {
        descriptions.push((cfg.id, cfg.description));
    }
    for cfg in configs::kimi_saucer_killer_configs() {
        descriptions.push((cfg.id, cfg.description));
    }
    for cfg in configs::kimi_super_ship_configs() {
        descriptions.push((cfg.id, cfg.description));
    }

    descriptions
}
