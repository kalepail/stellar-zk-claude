use crate::bots::{bot_fingerprint, create_bot, AutopilotBot};
use anyhow::{anyhow, Context, Result};
use asteroids_verifier_core::sim::LiveGame;
use asteroids_verifier_core::tape::{encode_input_byte, serialize_tape};
use asteroids_verifier_core::verify_tape;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
pub struct RunMetrics {
    pub bot_id: String,
    pub bot_fingerprint: String,
    pub seed: u32,
    pub max_frames: u32,
    pub frame_count: u32,
    pub final_score: u32,
    pub final_rng_state: u32,
    pub final_lives: i32,
    pub final_wave: i32,
    pub game_over: bool,
    pub rules_digest: u32,
    pub action_frames: u32,
    pub turn_frames: u32,
    pub thrust_frames: u32,
    pub fire_frames: u32,
}

#[derive(Clone, Debug)]
pub struct RunArtifact {
    pub metrics: RunMetrics,
    pub inputs: Vec<u8>,
    pub tape: Vec<u8>,
}

pub fn run_bot(bot_id: &str, seed: u32, max_frames: u32) -> Result<RunArtifact> {
    if max_frames == 0 {
        return Err(anyhow!("max_frames must be > 0"));
    }

    let mut bot = create_bot(bot_id).ok_or_else(|| anyhow!("unknown bot '{bot_id}'"))?;
    run_bot_instance(bot.as_mut(), seed, max_frames)
}

pub fn run_bot_instance(
    bot: &mut dyn AutopilotBot,
    seed: u32,
    max_frames: u32,
) -> Result<RunArtifact> {
    if max_frames == 0 {
        return Err(anyhow!("max_frames must be > 0"));
    }

    bot.reset(seed);

    let mut game = LiveGame::new(seed);
    game.validate()
        .map_err(|rule| anyhow!("initial invariant failure: {rule:?}"))?;

    let mut snapshot = game.snapshot();
    let mut inputs = Vec::with_capacity(max_frames as usize);
    let use_raw_inputs = bot.prefers_raw_inputs();

    while snapshot.frame_count < max_frames && (!snapshot.is_game_over || use_raw_inputs) {
        let next_raw = bot.next_raw_input(&snapshot);
        let primary = if let Some(raw) = next_raw {
            raw
        } else if use_raw_inputs {
            break;
        } else {
            let input = bot.next_input(&snapshot);
            encode_input_byte(input)
        };
        let chosen = if use_raw_inputs {
            primary
        } else {
            choose_strict_legal_input(&game, primary).ok_or_else(|| {
                anyhow!(
                    "no strict-legal input found at frame {}",
                    snapshot.frame_count
                )
            })?
        };
        inputs.push(chosen);
        game.step(chosen);
        snapshot = game.snapshot();
    }

    let result = game.result();
    let tape = serialize_tape(seed, &inputs, result.final_score, result.final_rng_state);
    let journal = verify_tape(&tape, max_frames.max(result.frame_count).max(1))
        .map_err(|err| anyhow!("generated tape failed verification: {err}"))?;

    let mut action_frames = 0u32;
    let mut turn_frames = 0u32;
    let mut thrust_frames = 0u32;
    let mut fire_frames = 0u32;
    for byte in &inputs {
        if *byte != 0 {
            action_frames += 1;
        }
        if (*byte & 0x01) != 0 || (*byte & 0x02) != 0 {
            turn_frames += 1;
        }
        if (*byte & 0x04) != 0 {
            thrust_frames += 1;
        }
        if (*byte & 0x08) != 0 {
            fire_frames += 1;
        }
    }

    Ok(RunArtifact {
        metrics: RunMetrics {
            bot_id: bot.id().to_string(),
            bot_fingerprint: bot_fingerprint(bot.id()).unwrap_or_else(|| "unknown".to_string()),
            seed,
            max_frames,
            frame_count: result.frame_count,
            final_score: result.final_score,
            final_rng_state: result.final_rng_state,
            final_lives: snapshot.lives,
            final_wave: snapshot.wave,
            game_over: snapshot.is_game_over,
            rules_digest: journal.rules_digest,
            action_frames,
            turn_frames,
            thrust_frames,
            fire_frames,
        },
        inputs,
        tape,
    })
}

pub fn write_tape(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating directory {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("failed writing {}", path.display()))
}

fn choose_strict_legal_input(game: &LiveGame, primary: u8) -> Option<u8> {
    if game.can_step_strict(primary).is_ok() {
        return Some(primary);
    }

    const FALLBACKS: [u8; 12] = [
        0x00, // idle
        0x04, // thrust
        0x01, // left
        0x02, // right
        0x08, // fire
        0x05, // left + thrust
        0x06, // right + thrust
        0x09, // left + fire
        0x0A, // right + fire
        0x0C, // thrust + fire
        0x03, // left + right
        0x0E, // right + thrust + fire
    ];

    for candidate in FALLBACKS {
        if candidate == primary {
            continue;
        }
        if game.can_step_strict(candidate).is_ok() {
            return Some(candidate);
        }
    }

    None
}
