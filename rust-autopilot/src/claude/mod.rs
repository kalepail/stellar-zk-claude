pub mod common;
pub mod phoenix;
pub mod navigator;
pub mod vulture;
pub mod tortoise;
pub mod oracle;
pub mod predator;
pub mod chimera;
pub mod lab;

use crate::bots::AutopilotBot;
use std::path::PathBuf;

pub fn bot_ids() -> Vec<&'static str> {
    vec![
        "claude-phoenix",
        "claude-navigator",
        "claude-vulture",
        "claude-tortoise",
        "claude-oracle",
        "claude-predator",
        "claude-chimera",
    ]
}

pub fn describe_bots() -> Vec<(&'static str, &'static str)> {
    vec![
        ("claude-phoenix", "Adaptive phase bot: aggressive early, balanced mid, survival late."),
        ("claude-navigator", "Danger-field navigator using spatial gradient descent for safe positioning."),
        ("claude-vulture", "Saucer farmer exploiting anti-lurk mechanic for high-value kills."),
        ("claude-tortoise", "Ultra-conservative deep survival bot prioritizing dodging over scoring."),
        ("claude-oracle", "MCTS planner with UCB1 selection and rollout-based evaluation."),
        ("claude-predator", "Intercept chain optimizer planning multi-target kill sequences."),
        ("claude-chimera", "Ensemble hybrid weighting sub-strategies by threat level."),
    ]
}

pub fn create_bot(id: &str) -> Option<Box<dyn AutopilotBot>> {
    match id {
        "claude-phoenix" => Some(Box::new(phoenix::PhoenixBot::new())),
        "claude-navigator" => Some(Box::new(navigator::NavigatorBot::new())),
        "claude-vulture" => Some(Box::new(vulture::VultureBot::new())),
        "claude-tortoise" => Some(Box::new(tortoise::TortoiseBot::new())),
        "claude-oracle" => Some(Box::new(oracle::OracleBot::new())),
        "claude-predator" => Some(Box::new(predator::PredatorBot::new())),
        "claude-chimera" => Some(Box::new(chimera::ChimeraBot::new())),
        _ => try_load_evolved_bot(id),
    }
}

/// Try loading an evolved bot from a JSON config file.
/// Supports: "evolved:<path>" to load from an explicit path.
fn try_load_evolved_bot(id: &str) -> Option<Box<dyn AutopilotBot>> {
    let path = if let Some(path_str) = id.strip_prefix("evolved:") {
        PathBuf::from(path_str)
    } else {
        return None;
    };
    lab::EvolvedBot::from_file(&path).ok().map(|bot| Box::new(bot) as Box<dyn AutopilotBot>)
}
