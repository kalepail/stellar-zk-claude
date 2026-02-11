//! claude-oracle: MCTS planner.
//!
//! Monte Carlo Tree Search with UCB1 selection policy.
//! Lightweight rollout using heuristic evaluation.
//! ~200 iterations per frame for real-time play.

use crate::bots::AutopilotBot;
use crate::claude::common::*;
use asteroids_verifier_core::constants::*;
use asteroids_verifier_core::fixed_point::{
    apply_drag, clamp_speed_q8_8, cos_bam, shortest_delta_q12_4, sin_bam, wrap_x_q12_4,
    wrap_y_q12_4,
};
use asteroids_verifier_core::sim::WorldSnapshot;
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};

const ITERATIONS: usize = 200;
const ROLLOUT_DEPTH: usize = 8;
const UCB_C: f64 = 1.4;
const CANDIDATE_ACTIONS: [u8; 10] = [0x00, 0x04, 0x01, 0x02, 0x05, 0x06, 0x08, 0x09, 0x0A, 0x0C];

struct MctsNode {
    action: u8,
    visits: u32,
    total_value: f64,
    children: Vec<MctsNode>,
}

impl MctsNode {
    fn new(action: u8) -> Self {
        Self {
            action,
            visits: 0,
            total_value: 0.0,
            children: Vec::new(),
        }
    }

    fn ucb1(&self, parent_visits: u32) -> f64 {
        if self.visits == 0 {
            return f64::MAX;
        }
        let exploit = self.total_value / self.visits as f64;
        let explore = UCB_C * ((parent_visits as f64).ln() / self.visits as f64).sqrt();
        exploit + explore
    }

    fn best_child_idx(&self) -> usize {
        let mut best_idx = 0;
        let mut best_ucb = f64::NEG_INFINITY;
        for (i, child) in self.children.iter().enumerate() {
            let ucb = child.ucb1(self.visits);
            if ucb > best_ucb {
                best_ucb = ucb;
                best_idx = i;
            }
        }
        best_idx
    }
}

pub struct OracleBot;

impl OracleBot {
    pub fn new() -> Self {
        Self
    }

    fn heuristic_eval(&self, world: &WorldSnapshot, pred: PredictedShip) -> f64 {
        let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
        let survival = (nearest_threat / 120.0).min(2.0);

        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_bonus = 1.0 - ((cx * cx + cy * cy).sqrt() / 600.0).min(1.0);

        let target_bonus = if let Some(target) = best_target(world, pred) {
            let angle_error = signed_angle_delta(pred.angle, target.aim_angle).abs() as f64;
            let align = (1.0 - angle_error / 128.0).clamp(0.0, 1.0);
            target.value * align * 0.5
        } else {
            0.0
        };

        let speed_px = pred.speed_px();
        let speed_penalty = if speed_px > 4.5 {
            -((speed_px - 4.5) / 4.5) * 0.3
        } else {
            0.0
        };

        survival + center_bonus * 0.3 + target_bonus + speed_penalty
    }

    fn simulate_step(pred: PredictedShip, action: u8) -> PredictedShip {
        let input = decode_input_byte(action);
        let mut angle = pred.angle;
        if input.left {
            angle = (angle - SHIP_TURN_SPEED_BAM) & 0xff;
        }
        if input.right {
            angle = (angle + SHIP_TURN_SPEED_BAM) & 0xff;
        }

        let mut vx = pred.vx;
        let mut vy = pred.vy;
        if input.thrust {
            vx += (cos_bam(angle) * SHIP_THRUST_Q8_8) >> 14;
            vy += (sin_bam(angle) * SHIP_THRUST_Q8_8) >> 14;
        }
        vx = apply_drag(vx);
        vy = apply_drag(vy);
        (vx, vy) = clamp_speed_q8_8(vx, vy, SHIP_MAX_SPEED_SQ_Q16_16);

        PredictedShip {
            x: wrap_x_q12_4(pred.x + (vx >> 4)),
            y: wrap_y_q12_4(pred.y + (vy >> 4)),
            vx,
            vy,
            angle,
            radius: pred.radius,
            fire_cooldown: if pred.fire_cooldown > 0 {
                pred.fire_cooldown - 1
            } else {
                0
            },
        }
    }

    fn rollout(&self, world: &WorldSnapshot, mut pred: PredictedShip) -> f64 {
        let mut total = 0.0;
        let discount = 0.9;
        let mut factor = 1.0;

        for _ in 0..ROLLOUT_DEPTH {
            // Greedy rollout: pick best immediate heuristic action
            let mut best_action = 0x00u8;
            let mut best_val = f64::NEG_INFINITY;
            for &action in &[0x00u8, 0x01, 0x02, 0x04, 0x05, 0x06] {
                let next = Self::simulate_step(pred, action);
                let val = self.heuristic_eval(world, next);
                if val > best_val {
                    best_val = val;
                    best_action = action;
                }
            }
            pred = Self::simulate_step(pred, best_action);
            total += factor * self.heuristic_eval(world, pred);
            factor *= discount;
        }

        total
    }

    fn select_action(&self, world: &WorldSnapshot) -> u8 {
        // Create root with children for each action
        let mut root = MctsNode::new(0x00);
        for &action in &CANDIDATE_ACTIONS {
            root.children.push(MctsNode::new(action));
        }

        for _ in 0..ITERATIONS {
            // Selection: pick best child by UCB1
            let child_idx = root.best_child_idx();
            let action = root.children[child_idx].action;

            // Expansion + simulation
            let pred = predict_ship(world, action);
            let immediate = self.heuristic_eval(world, pred);

            // Rollout from this state
            let rollout_value = self.rollout(world, pred);
            let value = immediate + rollout_value * 0.5;

            // Fire quality bonus
            let fire_bonus = if (action & 0x08) != 0
                && world.bullets.len() < SHIP_BULLET_LIMIT
                && world.ship.fire_cooldown <= 0
            {
                let fire_quality = estimate_fire_quality(pred, world);
                if fire_quality >= 0.2 {
                    fire_quality * 0.8
                } else {
                    -0.5
                }
            } else if (action & 0x08) != 0 {
                -0.3 // can't fire but trying to
            } else {
                0.0
            };

            let total_value = value + fire_bonus;

            // Backpropagate
            root.visits += 1;
            root.children[child_idx].visits += 1;
            root.children[child_idx].total_value += total_value;
        }

        // Select most-visited child
        let mut best_action = 0x00;
        let mut best_visits = 0;
        for child in &root.children {
            if child.visits > best_visits {
                best_visits = child.visits;
                best_action = child.action;
            }
        }

        // Fire discipline post-check
        if (best_action & 0x08) != 0 {
            let pred = predict_ship(world, best_action);
            let fire_quality = estimate_fire_quality(pred, world);
            let nearest_saucer = nearest_saucer_distance(world, pred);
            let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
            let (active, shortest) = own_bullet_stats(&world.bullets);

            let target = best_target(world, pred);
            let is_duplicate = target
                .map(|t| {
                    target_already_covered(
                        &world.bullets,
                        t.target_x,
                        t.target_y,
                        t.target_vx,
                        t.target_vy,
                        t.target_radius,
                    )
                })
                .unwrap_or(false);

            if is_duplicate
                || !disciplined_fire_ok(
                    active,
                    shortest,
                    fire_quality,
                    0.18,
                    nearest_saucer,
                    nearest_threat,
                    is_duplicate,
                )
            {
                // Strip fire bit
                best_action &= 0x07;
            }
        }

        best_action
    }
}

impl AutopilotBot for OracleBot {
    fn id(&self) -> &'static str {
        "claude-oracle"
    }
    fn description(&self) -> &'static str {
        "MCTS planner with UCB1 selection and rollout-based evaluation."
    }
    fn reset(&mut self, _seed: u32) {}
    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        if world.is_game_over || !world.ship.can_control {
            return FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: false,
            };
        }
        decode_input_byte(self.select_action(world))
    }
}
