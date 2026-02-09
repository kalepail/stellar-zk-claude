//! Configuration structures for Kimi bots

use serde::{Deserialize, Serialize};

/// Configuration for Kimi search-based bots
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct KimiSearchConfig {
    pub id: &'static str,
    pub description: &'static str,
    pub lookahead_frames: f64,
    pub risk_weight_asteroid: f64,
    pub risk_weight_saucer: f64,
    pub risk_weight_bullet: f64,
    pub survival_weight: f64,
    pub aggression_weight: f64,
    pub fire_reward: f64,
    pub shot_penalty: f64,
    pub miss_fire_penalty: f64,
    pub action_penalty: f64,
    pub turn_penalty: f64,
    pub thrust_penalty: f64,
    pub center_weight: f64,
    pub edge_penalty: f64,
    pub speed_soft_cap: f64,
    pub fire_tolerance_bam: i32,
    pub fire_distance_px: f64,
    pub lurk_trigger_frames: i32,
    pub lurk_aggression_boost: f64,
    pub learning_enabled: bool,
    pub learning_db_path: &'static str,
}

/// Configuration for Kimi precision/planning bots
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct KimiPrecisionConfig {
    pub id: &'static str,
    pub description: &'static str,
    pub depth: usize,
    pub beam_width: usize,
    pub discount: f64,
    pub risk_weight_asteroid: f64,
    pub risk_weight_saucer: f64,
    pub risk_weight_bullet: f64,
    pub survival_weight: f64,
    pub imminent_penalty: f64,
    pub shot_reward: f64,
    pub shot_penalty: f64,
    pub miss_fire_penalty: f64,
    pub action_penalty: f64,
    pub turn_penalty: f64,
    pub thrust_penalty: f64,
    pub center_weight: f64,
    pub edge_penalty: f64,
    pub speed_soft_cap: f64,
    pub target_weight_large: f64,
    pub target_weight_medium: f64,
    pub target_weight_small: f64,
    pub target_weight_saucer_large: f64,
    pub target_weight_saucer_small: f64,
    pub lurk_trigger_frames: i32,
    pub lurk_shot_boost: f64,
    pub learning_enabled: bool,
    pub learning_db_path: &'static str,
}

/// Kimi Hunter - Aggressive score-focused bot configs
pub fn kimi_hunter_configs() -> &'static [KimiSearchConfig] {
    &[
        // v1: Base aggressive hunter
        KimiSearchConfig {
            id: "kimi-hunter-v1",
            description: "Base aggressive hunter with adaptive aggression",
            lookahead_frames: 18.0,
            risk_weight_asteroid: 1.35,
            risk_weight_saucer: 2.0,
            risk_weight_bullet: 2.75,
            survival_weight: 1.85,
            aggression_weight: 0.65,
            fire_reward: 0.78,
            shot_penalty: 0.85,
            miss_fire_penalty: 1.15,
            action_penalty: 0.01,
            turn_penalty: 0.012,
            thrust_penalty: 0.011,
            center_weight: 0.48,
            edge_penalty: 0.32,
            speed_soft_cap: 4.2,
            fire_tolerance_bam: 8,
            fire_distance_px: 280.0,
            lurk_trigger_frames: 300,
            lurk_aggression_boost: 1.5,
            learning_enabled: true,
            learning_db_path: "kimi-hunter-v1-learning.json",
        },
        // v2: Higher saucer priority
        KimiSearchConfig {
            id: "kimi-hunter-v2",
            description: "Aggressive hunter with extreme saucer priority",
            lookahead_frames: 17.0,
            risk_weight_asteroid: 1.25,
            risk_weight_saucer: 2.3,
            risk_weight_bullet: 2.65,
            survival_weight: 1.75,
            aggression_weight: 0.75,
            fire_reward: 0.88,
            shot_penalty: 0.78,
            miss_fire_penalty: 1.05,
            action_penalty: 0.0095,
            turn_penalty: 0.011,
            thrust_penalty: 0.0105,
            center_weight: 0.42,
            edge_penalty: 0.28,
            speed_soft_cap: 4.5,
            fire_tolerance_bam: 9,
            fire_distance_px: 300.0,
            lurk_trigger_frames: 280,
            lurk_aggression_boost: 1.7,
            learning_enabled: true,
            learning_db_path: "kimi-hunter-v2-learning.json",
        },
        // v3: Learning-enhanced with death tracking
        KimiSearchConfig {
            id: "kimi-hunter-v3",
            description: "Learning-enhanced hunter that adapts from death analysis",
            lookahead_frames: 19.0,
            risk_weight_asteroid: 1.42,
            risk_weight_saucer: 2.15,
            risk_weight_bullet: 2.9,
            survival_weight: 1.95,
            aggression_weight: 0.58,
            fire_reward: 0.72,
            shot_penalty: 0.92,
            miss_fire_penalty: 1.25,
            action_penalty: 0.0115,
            turn_penalty: 0.0135,
            thrust_penalty: 0.0125,
            center_weight: 0.52,
            edge_penalty: 0.38,
            speed_soft_cap: 4.0,
            fire_tolerance_bam: 7,
            fire_distance_px: 265.0,
            lurk_trigger_frames: 320,
            lurk_aggression_boost: 1.35,
            learning_enabled: true,
            learning_db_path: "kimi-hunter-v3-learning.json",
        },
        // v4: Max scoring attempt
        KimiSearchConfig {
            id: "kimi-hunter-v4-max",
            description: "Maximum score attempt with balanced risk",
            lookahead_frames: 20.0,
            risk_weight_asteroid: 1.15,
            risk_weight_saucer: 1.85,
            risk_weight_bullet: 2.45,
            survival_weight: 1.55,
            aggression_weight: 0.85,
            fire_reward: 0.95,
            shot_penalty: 0.72,
            miss_fire_penalty: 0.95,
            action_penalty: 0.0085,
            turn_penalty: 0.01,
            thrust_penalty: 0.0095,
            center_weight: 0.38,
            edge_penalty: 0.22,
            speed_soft_cap: 4.8,
            fire_tolerance_bam: 10,
            fire_distance_px: 340.0,
            lurk_trigger_frames: 270,
            lurk_aggression_boost: 1.85,
            learning_enabled: true,
            learning_db_path: "kimi-hunter-v4-learning.json",
        },
    ]
}

/// Kimi Survivor - Defensive survival-focused bot configs
pub fn kimi_survivor_configs() -> &'static [KimiSearchConfig] {
    &[
        KimiSearchConfig {
            id: "kimi-survivor-v1",
            description: "Ultra-defensive bot prioritizing survival above all",
            lookahead_frames: 24.0,
            risk_weight_asteroid: 1.85,
            risk_weight_saucer: 2.65,
            risk_weight_bullet: 3.45,
            survival_weight: 3.2,
            aggression_weight: 0.25,
            fire_reward: 0.45,
            shot_penalty: 1.35,
            miss_fire_penalty: 1.85,
            action_penalty: 0.018,
            turn_penalty: 0.022,
            thrust_penalty: 0.019,
            center_weight: 0.68,
            edge_penalty: 0.52,
            speed_soft_cap: 3.2,
            fire_tolerance_bam: 5,
            fire_distance_px: 200.0,
            lurk_trigger_frames: 450,
            lurk_aggression_boost: 0.85,
            learning_enabled: true,
            learning_db_path: "kimi-survivor-v1-learning.json",
        },
        KimiSearchConfig {
            id: "kimi-survivor-v2",
            description: "Balanced survivor with selective aggression",
            lookahead_frames: 22.0,
            risk_weight_asteroid: 1.65,
            risk_weight_saucer: 2.35,
            risk_weight_bullet: 3.15,
            survival_weight: 2.75,
            aggression_weight: 0.38,
            fire_reward: 0.58,
            shot_penalty: 1.15,
            miss_fire_penalty: 1.55,
            action_penalty: 0.015,
            turn_penalty: 0.018,
            thrust_penalty: 0.016,
            center_weight: 0.62,
            edge_penalty: 0.46,
            speed_soft_cap: 3.5,
            fire_tolerance_bam: 6,
            fire_distance_px: 220.0,
            lurk_trigger_frames: 400,
            lurk_aggression_boost: 1.1,
            learning_enabled: true,
            learning_db_path: "kimi-survivor-v2-learning.json",
        },
    ]
}

/// Kimi Sniper - Precision shot-focused bot configs
pub fn kimi_sniper_configs() -> &'static [KimiPrecisionConfig] {
    &[KimiPrecisionConfig {
        id: "kimi-sniper-v1",
        description: "High-precision bot with strict fire discipline",
        depth: 7,
        beam_width: 15,
        discount: 0.94,
        risk_weight_asteroid: 1.45,
        risk_weight_saucer: 2.15,
        risk_weight_bullet: 2.85,
        survival_weight: 2.25,
        imminent_penalty: 3.45,
        shot_reward: 1.65,
        shot_penalty: 1.25,
        miss_fire_penalty: 1.95,
        action_penalty: 0.019,
        turn_penalty: 0.026,
        thrust_penalty: 0.023,
        center_weight: 0.52,
        edge_penalty: 0.38,
        speed_soft_cap: 4.0,
        target_weight_large: 1.05,
        target_weight_medium: 1.35,
        target_weight_small: 1.68,
        target_weight_saucer_large: 2.45,
        target_weight_saucer_small: 3.95,
        lurk_trigger_frames: 290,
        lurk_shot_boost: 1.45,
        learning_enabled: true,
        learning_db_path: "kimi-sniper-v1-learning.json",
    }]
}

/// Kimi WrapMaster - Wrap-aware movement specialist
pub fn kimi_wrap_master_configs() -> &'static [KimiSearchConfig] {
    &[KimiSearchConfig {
        id: "kimi-wrap-master-v1",
        description: "Wrap-aware movement specialist using wrap boundaries",
        lookahead_frames: 21.0,
        risk_weight_asteroid: 1.38,
        risk_weight_saucer: 2.05,
        risk_weight_bullet: 2.75,
        survival_weight: 1.95,
        aggression_weight: 0.58,
        fire_reward: 0.75,
        shot_penalty: 0.88,
        miss_fire_penalty: 1.18,
        action_penalty: 0.011,
        turn_penalty: 0.013,
        thrust_penalty: 0.012,
        center_weight: 0.28, // Prefer edges/wrap
        edge_penalty: 0.15,  // Lower edge penalty
        speed_soft_cap: 4.35,
        fire_tolerance_bam: 8,
        fire_distance_px: 290.0,
        lurk_trigger_frames: 305,
        lurk_aggression_boost: 1.45,
        learning_enabled: true,
        learning_db_path: "kimi-wrap-master-v1-learning.json",
    }]
}

/// Kimi SaucerKiller - Saucer prioritization specialist
pub fn kimi_saucer_killer_configs() -> &'static [KimiSearchConfig] {
    &[
        KimiSearchConfig {
            id: "kimi-saucer-killer-v1",
            description: "Extreme saucer prioritization with hunting behavior",
            lookahead_frames: 17.0,
            risk_weight_asteroid: 1.05,
            risk_weight_saucer: 2.85,
            risk_weight_bullet: 2.55,
            survival_weight: 1.65,
            aggression_weight: 0.92,
            fire_reward: 1.15,
            shot_penalty: 0.65,
            miss_fire_penalty: 0.88,
            action_penalty: 0.008,
            turn_penalty: 0.01,
            thrust_penalty: 0.009,
            center_weight: 0.35,
            edge_penalty: 0.2,
            speed_soft_cap: 5.0,
            fire_tolerance_bam: 11,
            fire_distance_px: 380.0,
            lurk_trigger_frames: 250,
            lurk_aggression_boost: 2.1,
            learning_enabled: true,
            learning_db_path: "kimi-saucer-killer-v1-learning.json",
        },
        KimiSearchConfig {
            id: "kimi-saucer-killer-v2",
            description: "Balanced saucer killer with better survival",
            lookahead_frames: 18.0,
            risk_weight_asteroid: 1.15,
            risk_weight_saucer: 2.55,
            risk_weight_bullet: 2.75,
            survival_weight: 1.85,
            aggression_weight: 0.78,
            fire_reward: 0.98,
            shot_penalty: 0.72,
            miss_fire_penalty: 1.05,
            action_penalty: 0.009,
            turn_penalty: 0.011,
            thrust_penalty: 0.010,
            center_weight: 0.42,
            edge_penalty: 0.26,
            speed_soft_cap: 4.65,
            fire_tolerance_bam: 9,
            fire_distance_px: 350.0,
            lurk_trigger_frames: 265,
            lurk_aggression_boost: 1.85,
            learning_enabled: true,
            learning_db_path: "kimi-saucer-killer-v2-learning.json",
        },
    ]
}

/// Kimi SuperShip - Combined best features (final iteration)
pub fn kimi_super_ship_configs() -> &'static [KimiSearchConfig] {
    &[KimiSearchConfig {
        id: "kimi-super-ship-v1",
        description: "Super ship combining best features from all variants",
        lookahead_frames: 20.0,
        risk_weight_asteroid: 1.25,
        risk_weight_saucer: 2.25,
        risk_weight_bullet: 2.75,
        survival_weight: 1.95,
        aggression_weight: 0.72,
        fire_reward: 0.88,
        shot_penalty: 0.78,
        miss_fire_penalty: 1.12,
        action_penalty: 0.0095,
        turn_penalty: 0.0115,
        thrust_penalty: 0.0105,
        center_weight: 0.45,
        edge_penalty: 0.32,
        speed_soft_cap: 4.45,
        fire_tolerance_bam: 9,
        fire_distance_px: 320.0,
        lurk_trigger_frames: 285,
        lurk_aggression_boost: 1.65,
        learning_enabled: true,
        learning_db_path: "kimi-super-ship-v1-learning.json",
    }]
}

/// Get all Kimi bot IDs
pub fn all_kimi_bot_ids() -> Vec<&'static str> {
    let mut ids = Vec::new();

    for cfg in kimi_hunter_configs() {
        ids.push(cfg.id);
    }
    for cfg in kimi_survivor_configs() {
        ids.push(cfg.id);
    }
    for cfg in kimi_sniper_configs() {
        ids.push(cfg.id);
    }
    for cfg in kimi_wrap_master_configs() {
        ids.push(cfg.id);
    }
    for cfg in kimi_saucer_killer_configs() {
        ids.push(cfg.id);
    }
    for cfg in kimi_super_ship_configs() {
        ids.push(cfg.id);
    }

    ids
}

/// Find a config by ID
pub fn find_search_config(id: &str) -> Option<&'static KimiSearchConfig> {
    kimi_hunter_configs()
        .iter()
        .find(|c| c.id == id)
        .or_else(|| kimi_survivor_configs().iter().find(|c| c.id == id))
        .or_else(|| kimi_wrap_master_configs().iter().find(|c| c.id == id))
        .or_else(|| kimi_saucer_killer_configs().iter().find(|c| c.id == id))
        .or_else(|| kimi_super_ship_configs().iter().find(|c| c.id == id))
        .copied()
}

pub fn find_precision_config(id: &str) -> Option<&'static KimiPrecisionConfig> {
    kimi_sniper_configs().iter().find(|c| c.id == id).copied()
}
