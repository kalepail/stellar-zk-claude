//! Kimi Learning Framework
//!
//! Tracks deaths, near-misses, and missed shots to iteratively improve the autopilot.
//! Analyzes game history to identify patterns and adjust strategies.

use asteroids_verifier_core::sim::WorldSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Records a single death event with full context
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeathRecord {
    pub frame: u32,
    pub score: u32,
    pub cause: DeathCause,
    pub ship_x: i32,
    pub ship_y: i32,
    pub ship_vx: i32,
    pub ship_vy: i32,
    pub ship_angle: i32,
    pub nearby_asteroids: Vec<EntitySnapshot>,
    pub nearby_saucers: Vec<EntitySnapshot>,
    pub nearby_bullets: Vec<EntitySnapshot>,
    pub recent_actions: Vec<FrameAction>, // Last 60 frames of actions
    pub threat_count_30_frames_ago: usize,
    pub was_cornered: bool,
    pub speed_at_death: f64,
    pub frames_since_last_kill: i32,
    pub bullets_in_flight: usize,
}

/// Records a missed shot event
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissedShotRecord {
    pub frame: u32,
    pub intended_target: TargetType,
    pub bullet_start_x: i32,
    pub bullet_start_y: i32,
    pub bullet_angle: i32,
    pub ship_vx: i32,
    pub ship_vy: i32,
    pub target_count_at_fire: usize,
    pub nearest_threat_distance: f64,
    pub fire_quality_estimate: f64,
    pub was_under_pressure: bool,
}

/// Frame-by-frame action recording
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameAction {
    pub frame: u32,
    pub action_byte: u8,
    pub score: u32,
    pub threat_distance: f64,
    pub target_count: usize,
    pub saucer_count: usize,
    pub bullet_count: usize,
}

/// Entity snapshot for death context
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub distance: f64,
    pub relative_velocity_toward_ship: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum DeathCause {
    AsteroidCollision { size: String },
    SaucerCollision { small: bool },
    SaucerBullet,
    MultipleThreats, // Died surrounded by multiple threats
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TargetType {
    Asteroid { size: String },
    Saucer { small: bool },
    None,
}

/// Learning database that accumulates knowledge across games
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LearningDatabase {
    pub game_count: u32,
    pub deaths: Vec<DeathRecord>,
    pub missed_shots: Vec<MissedShotRecord>,
    pub death_patterns: DeathPatternAnalysis,
    pub shot_patterns: ShotPatternAnalysis,
    pub strategy_adjustments: StrategyAdjustments,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DeathPatternAnalysis {
    pub corner_deaths: u32,         // Died in corners
    pub high_speed_deaths: u32,     // Died while moving fast
    pub low_speed_deaths: u32,      // Died while stationary/slow
    pub saucer_focus_deaths: u32,   // Died while focused on saucer
    pub multitarget_deaths: u32,    // Died with many targets
    pub lurk_timeout_deaths: u32,   // Died after long time without killing
    pub spawn_deaths: u32,          // Died shortly after respawn
    pub bullet_dodge_failures: u32, // Failed to dodge saucer bullets
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ShotPatternAnalysis {
    pub shots_fired: u32,
    pub shots_hit: u32,
    pub shots_missed: u32,
    pub shots_under_pressure: u32, // Fired when threats nearby
    pub shots_at_edge: u32,        // Fired near screen edge
    pub shots_at_fast_target: u32, // Fired at fast-moving targets
    pub shots_at_wrap_target: u32, // Fired at wrap-around targets
}

/// Dynamic strategy adjustments based on learning
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct StrategyAdjustments {
    // Risk weights adjusted based on death analysis
    pub asteroid_risk_multiplier: f64,
    pub saucer_risk_multiplier: f64,
    pub bullet_risk_multiplier: f64,

    // Movement adjustments
    pub corner_avoidance_boost: f64,
    pub edge_buffer_px: f64,
    pub max_safe_speed: f64,

    // Fire discipline adjustments
    pub fire_quality_threshold: f64,
    pub pressure_fire_threshold: f64,
    pub min_threat_distance_for_fire: f64,

    // Aggression adjustments
    pub base_aggression: f64,
    pub lurk_aggression: f64,
    pub survival_priority: f64,

    // Spawn protection
    pub spawn_caution_frames: i32,
    pub spawn_min_threat_distance: f64,
}

impl LearningDatabase {
    pub fn new() -> Self {
        Self {
            game_count: 0,
            deaths: Vec::new(),
            missed_shots: Vec::new(),
            death_patterns: DeathPatternAnalysis::default(),
            shot_patterns: ShotPatternAnalysis::default(),
            strategy_adjustments: StrategyAdjustments::default(),
        }
    }

    pub fn load_or_create(path: &str) -> Self {
        if Path::new(path).exists() {
            if let Ok(data) = fs::read_to_string(path) {
                if let Ok(db) = serde_json::from_str(&data) {
                    return db;
                }
            }
        }
        Self::new()
    }

    pub fn save(&self, path: &str) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn start_new_game(&mut self) {
        self.game_count += 1;
    }

    /// Record a death with full context analysis
    pub fn record_death(&mut self, record: DeathRecord) {
        // Analyze death patterns
        self.analyze_death_patterns(&record);
        self.deaths.push(record);

        // Keep only last 100 deaths to prevent memory bloat
        if self.deaths.len() > 100 {
            self.deaths.remove(0);
        }

        self.update_strategy_from_deaths();
    }

    /// Record a missed shot
    pub fn record_missed_shot(&mut self, record: MissedShotRecord) {
        self.shot_patterns.shots_fired += 1;
        self.shot_patterns.shots_missed += 1;

        if record.was_under_pressure {
            self.shot_patterns.shots_under_pressure += 1;
        }

        self.missed_shots.push(record);

        // Keep only last 500 missed shots
        if self.missed_shots.len() > 500 {
            self.missed_shots.remove(0);
        }

        self.update_strategy_from_missed_shots();
    }

    /// Record a successful hit
    pub fn record_hit(&mut self) {
        self.shot_patterns.shots_fired += 1;
        self.shot_patterns.shots_hit += 1;
    }

    fn analyze_death_patterns(&mut self, record: &DeathRecord) {
        // Check if in corner
        let x_px = record.ship_x as f64 / 16.0;
        let y_px = record.ship_y as f64 / 16.0;
        let in_corner = (x_px < 120.0 || x_px > 480.0) && (y_px < 90.0 || y_px > 360.0);
        if in_corner {
            self.death_patterns.corner_deaths += 1;
        }

        // Check speed
        if record.speed_at_death > 4.0 {
            self.death_patterns.high_speed_deaths += 1;
        } else if record.speed_at_death < 1.5 {
            self.death_patterns.low_speed_deaths += 1;
        }

        // Check saucer focus
        if record.nearby_saucers.len() > 0 && record.nearby_asteroids.len() == 0 {
            self.death_patterns.saucer_focus_deaths += 1;
        }

        // Check multiple threats
        if record.nearby_asteroids.len() + record.nearby_saucers.len() > 3 {
            self.death_patterns.multitarget_deaths += 1;
        }

        // Check lurk timeout
        if record.frames_since_last_kill > 250 {
            self.death_patterns.lurk_timeout_deaths += 1;
        }

        // Check spawn death
        if record.frame < 500 {
            self.death_patterns.spawn_deaths += 1;
        }

        // Check bullet death
        if record.cause == DeathCause::SaucerBullet {
            self.death_patterns.bullet_dodge_failures += 1;
        }
    }

    fn update_strategy_from_deaths(&mut self) {
        let total_deaths = self.deaths.len().max(1) as f64;
        let adj = &mut self.strategy_adjustments;

        // Increase corner avoidance if dying in corners
        let corner_rate = self.death_patterns.corner_deaths as f64 / total_deaths;
        adj.corner_avoidance_boost = 0.36 + corner_rate * 0.5;
        adj.edge_buffer_px = 140.0 + corner_rate * 80.0;

        // Adjust max speed based on high/low speed death rates
        let high_speed_rate = self.death_patterns.high_speed_deaths as f64 / total_deaths;
        let low_speed_rate = self.death_patterns.low_speed_deaths as f64 / total_deaths;

        if high_speed_rate > 0.3 {
            adj.max_safe_speed = (adj.max_safe_speed * 0.9).max(3.0);
        } else if low_speed_rate > 0.3 {
            adj.max_safe_speed = (adj.max_safe_speed * 1.1).min(5.0);
        }

        // Increase caution after spawn if dying early
        let spawn_death_rate = self.death_patterns.spawn_deaths as f64 / total_deaths;
        adj.spawn_caution_frames = 120 + (spawn_death_rate * 120.0) as i32;
        adj.spawn_min_threat_distance = 150.0 + spawn_death_rate * 100.0;

        // Reduce aggression if dying while lurk-timed out
        let lurk_death_rate = self.death_patterns.lurk_timeout_deaths as f64 / total_deaths;
        if lurk_death_rate > 0.25 {
            adj.lurk_aggression = (adj.lurk_aggression * 0.9).max(1.0);
        }

        // Increase bullet dodge priority if dying to bullets
        let bullet_death_rate = self.death_patterns.bullet_dodge_failures as f64 / total_deaths;
        adj.bullet_risk_multiplier = 2.85 + bullet_death_rate * 1.5;

        // Adjust fire discipline if dying with bullets in flight
        let recent_deaths = self.deaths.iter().rev().take(20);
        let avg_bullets_at_death: f64 = recent_deaths
            .map(|d| d.bullets_in_flight as f64)
            .sum::<f64>()
            / 20.0;
        if avg_bullets_at_death > 2.5 {
            adj.fire_quality_threshold = (adj.fire_quality_threshold + 0.05).min(0.4);
            adj.min_threat_distance_for_fire = (adj.min_threat_distance_for_fire + 20.0).min(150.0);
        }
    }

    fn update_strategy_from_missed_shots(&mut self) {
        if self.missed_shots.is_empty() {
            return;
        }

        let recent_misses = self.missed_shots.iter().rev().take(100);
        let total = recent_misses.clone().count().max(1) as f64;

        let under_pressure_rate = recent_misses
            .clone()
            .filter(|m| m.was_under_pressure)
            .count() as f64
            / total;

        let adj = &mut self.strategy_adjustments;

        // If missing under pressure, increase fire threshold
        if under_pressure_rate > 0.4 {
            adj.pressure_fire_threshold = (adj.pressure_fire_threshold + 0.08).min(0.5);
            adj.min_threat_distance_for_fire = (adj.min_threat_distance_for_fire + 10.0).min(150.0);
        }

        // Analyze miss reasons from last 50 shots
        let last_50: Vec<_> = self.missed_shots.iter().rev().take(50).collect();
        let fast_target_misses = last_50
            .iter()
            .filter(|m| {
                let vx = m.ship_vx as f64 / 256.0;
                let vy = m.ship_vy as f64 / 256.0;
                (vx * vx + vy * vy).sqrt() > 5.0
            })
            .count() as f64
            / 50.0;

        // If missing while moving fast, consider slowing down more
        if fast_target_misses > 0.35 {
            adj.max_safe_speed = (adj.max_safe_speed * 0.95).max(3.0);
        }
    }

    /// Generate a report of current strategy adjustments
    pub fn generate_learning_report(&self) -> String {
        let mut report = format!("Kimi Learning Report - Game {}\n", self.game_count);
        report.push_str("========================================\n\n");

        report.push_str("Death Pattern Analysis:\n");
        report.push_str(&format!(
            "  Corner deaths: {}\n",
            self.death_patterns.corner_deaths
        ));
        report.push_str(&format!(
            "  High speed deaths: {}\n",
            self.death_patterns.high_speed_deaths
        ));
        report.push_str(&format!(
            "  Low speed deaths: {}\n",
            self.death_patterns.low_speed_deaths
        ));
        report.push_str(&format!(
            "  Saucer focus deaths: {}\n",
            self.death_patterns.saucer_focus_deaths
        ));
        report.push_str(&format!(
            "  Multitarget deaths: {}\n",
            self.death_patterns.multitarget_deaths
        ));
        report.push_str(&format!(
            "  Lurk timeout deaths: {}\n",
            self.death_patterns.lurk_timeout_deaths
        ));
        report.push_str(&format!(
            "  Spawn deaths: {}\n",
            self.death_patterns.spawn_deaths
        ));
        report.push_str(&format!(
            "  Bullet dodge failures: {}\n\n",
            self.death_patterns.bullet_dodge_failures
        ));

        report.push_str("Shot Pattern Analysis:\n");
        report.push_str(&format!(
            "  Total shots: {}\n",
            self.shot_patterns.shots_fired
        ));
        report.push_str(&format!(
            "  Hits: {} ({:.1}%)\n",
            self.shot_patterns.shots_hit,
            if self.shot_patterns.shots_fired > 0 {
                100.0 * self.shot_patterns.shots_hit as f64 / self.shot_patterns.shots_fired as f64
            } else {
                0.0
            }
        ));
        report.push_str(&format!(
            "  Under pressure: {}\n\n",
            self.shot_patterns.shots_under_pressure
        ));

        report.push_str("Current Strategy Adjustments:\n");
        report.push_str(&format!(
            "  Corner avoidance boost: {:.2}\n",
            self.strategy_adjustments.corner_avoidance_boost
        ));
        report.push_str(&format!(
            "  Edge buffer: {:.1}px\n",
            self.strategy_adjustments.edge_buffer_px
        ));
        report.push_str(&format!(
            "  Max safe speed: {:.2}\n",
            self.strategy_adjustments.max_safe_speed
        ));
        report.push_str(&format!(
            "  Fire quality threshold: {:.2}\n",
            self.strategy_adjustments.fire_quality_threshold
        ));
        report.push_str(&format!(
            "  Spawn caution frames: {}\n",
            self.strategy_adjustments.spawn_caution_frames
        ));
        report.push_str(&format!(
            "  Bullet risk multiplier: {:.2}\n",
            self.strategy_adjustments.bullet_risk_multiplier
        ));

        report
    }
}

/// Learning-aware bot trait
pub trait LearningBot: AutopilotBot {
    fn update_from_world(&mut self, world: &WorldSnapshot);
    fn on_death(&mut self, world: &WorldSnapshot);
    fn on_fire(&mut self, world: &WorldSnapshot, quality_estimate: f64);
    fn on_hit(&mut self, world: &WorldSnapshot);
    fn get_learning_db(&self) -> &LearningDatabase;
    fn get_learning_db_mut(&mut self) -> &mut LearningDatabase;
    fn apply_learned_adjustments(&mut self);
}
