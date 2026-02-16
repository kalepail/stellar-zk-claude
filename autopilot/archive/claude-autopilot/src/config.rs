use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BotConfig {
    pub id: String,
    pub generation: u32,
    pub parent_id: String,
    pub description: String,

    // Risk weights
    pub risk_weight_asteroid: f64,
    pub risk_weight_saucer: f64,
    pub risk_weight_bullet: f64,
    pub survival_weight: f64,

    // Aggression
    pub aggression: f64,
    pub fire_reward: f64,
    pub shot_penalty: f64,
    pub miss_fire_penalty: f64,
    pub min_fire_quality: f64,

    // Control penalties
    pub action_penalty: f64,
    pub turn_penalty: f64,
    pub thrust_penalty: f64,

    // Position
    pub center_weight: f64,
    pub edge_penalty: f64,
    pub speed_soft_cap: f64,

    // Timing
    pub lookahead: f64,
    pub fire_tolerance_bam: i32,
    pub fire_distance_px: f64,
    pub lurk_trigger: i32,
    pub lurk_boost: f64,

    // Saucer-kill urgency: bonus for firing at saucers (they are bullet factories)
    #[serde(default = "default_saucer_kill_urgency")]
    pub saucer_kill_urgency: f64,

    // Bullet risk discount: diminishing returns exponent for multiple bullets
    // 1.0 = linear (no discount), 0.5 = sqrt scaling
    #[serde(default = "default_bullet_risk_discount")]
    pub bullet_risk_discount: f64,
}

fn default_saucer_kill_urgency() -> f64 {
    0.0
}

fn default_bullet_risk_discount() -> f64 {
    1.0
}

impl Default for BotConfig {
    fn default() -> Self {
        // Evolved high-scoring profile: high defense + high offense.
        // Key insight: both survival_weight AND fire_reward must be high.
        // Multi-frame prediction (5 frames) + tuned closing factors.
        // Avg ~632K, max ~785K over 256 seeds x 108K frames (96.5% survival).
        Self {
            id: "claude-evolved-gen0".to_string(),
            generation: 0,
            parent_id: "evolved-gen7".to_string(),
            description: "Evolved high-scoring marathon config.".to_string(),
            risk_weight_asteroid: 2.8,
            risk_weight_saucer: 3.5,
            risk_weight_bullet: 8.0,
            survival_weight: 5.0,
            aggression: 0.65,
            fire_reward: 2.5,
            shot_penalty: 1.5,
            miss_fire_penalty: 1.0,
            min_fire_quality: 0.05,
            action_penalty: 0.011,
            turn_penalty: 0.013,
            thrust_penalty: 0.012,
            center_weight: 1.0,
            edge_penalty: 0.85,
            speed_soft_cap: 3.3,
            lookahead: 30.0,
            fire_tolerance_bam: 8,
            fire_distance_px: 280.0,
            lurk_trigger: 280,
            lurk_boost: 1.6,
            saucer_kill_urgency: 0.5,
            bullet_risk_discount: 1.0,
        }
    }
}

impl BotConfig {
    pub fn preset(name: &str) -> Option<Self> {
        match name {
            "marathon" => Some(Self::default()),
            "hunter" => Some(Self {
                id: "hunter-preset".to_string(),
                parent_id: "omega-alltime-hunter-v2".to_string(),
                description: "Peak-score hunter for long spike runs.".to_string(),
                risk_weight_asteroid: 1.05,
                risk_weight_saucer: 1.55,
                risk_weight_bullet: 2.2,
                survival_weight: 1.25,
                aggression: 1.2,
                fire_reward: 1.6,
                shot_penalty: 0.52,
                miss_fire_penalty: 0.68,
                min_fire_quality: 0.11,
                action_penalty: 0.007,
                turn_penalty: 0.0085,
                thrust_penalty: 0.008,
                center_weight: 0.30,
                edge_penalty: 0.16,
                speed_soft_cap: 5.0,
                lookahead: 16.0,
                fire_tolerance_bam: 9,
                fire_distance_px: 360.0,
                lurk_trigger: 230,
                lurk_boost: 2.1,
                saucer_kill_urgency: 0.55,
                bullet_risk_discount: 0.55,
                ..Self::default()
            }),
            "supernova" => Some(Self {
                id: "supernova-preset".to_string(),
                parent_id: "omega-supernova-v2".to_string(),
                description: "High-pressure saucer/fragment farming profile.".to_string(),
                risk_weight_asteroid: 0.95,
                risk_weight_saucer: 1.45,
                risk_weight_bullet: 2.1,
                survival_weight: 1.15,
                aggression: 1.35,
                fire_reward: 1.75,
                shot_penalty: 0.45,
                miss_fire_penalty: 0.6,
                min_fire_quality: 0.10,
                action_penalty: 0.006,
                turn_penalty: 0.0075,
                thrust_penalty: 0.0075,
                center_weight: 0.24,
                edge_penalty: 0.14,
                speed_soft_cap: 5.2,
                lookahead: 15.0,
                fire_tolerance_bam: 10,
                fire_distance_px: 390.0,
                lurk_trigger: 220,
                lurk_boost: 2.3,
                saucer_kill_urgency: 0.65,
                bullet_risk_discount: 0.5,
                ..Self::default()
            }),
            _ => None,
        }
    }

    pub fn clamp(&mut self) {
        self.risk_weight_asteroid = self.risk_weight_asteroid.clamp(0.6, 2.8);
        self.risk_weight_saucer = self.risk_weight_saucer.clamp(0.8, 3.5);
        self.risk_weight_bullet = self.risk_weight_bullet.clamp(1.2, 8.0);
        self.survival_weight = self.survival_weight.clamp(0.8, 5.0);
        self.aggression = self.aggression.clamp(0.3, 2.0);
        self.fire_reward = self.fire_reward.clamp(0.5, 3.0);
        self.shot_penalty = self.shot_penalty.clamp(0.2, 2.0);
        self.miss_fire_penalty = self.miss_fire_penalty.clamp(0.3, 2.5);
        self.min_fire_quality = self.min_fire_quality.clamp(0.05, 0.4);
        self.speed_soft_cap = self.speed_soft_cap.clamp(3.0, 6.0);
        self.center_weight = self.center_weight.clamp(0.1, 1.0);
        self.edge_penalty = self.edge_penalty.clamp(0.05, 1.5);
        self.lookahead = self.lookahead.clamp(12.0, 30.0);
        self.lurk_trigger = self.lurk_trigger.clamp(150, 400);
        self.lurk_boost = self.lurk_boost.clamp(1.0, 3.0);
        self.action_penalty = self.action_penalty.clamp(0.003, 0.04);
        self.turn_penalty = self.turn_penalty.clamp(0.003, 0.05);
        self.thrust_penalty = self.thrust_penalty.clamp(0.003, 0.045);
        self.fire_tolerance_bam = self.fire_tolerance_bam.clamp(4, 16);
        self.fire_distance_px = self.fire_distance_px.clamp(150.0, 500.0);
        self.saucer_kill_urgency = self.saucer_kill_urgency.clamp(0.0, 1.5);
        self.bullet_risk_discount = self.bullet_risk_discount.clamp(0.35, 1.0);
    }
}
