use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use claude_autopilot::benchmark::{self, BenchmarkConfig};
use claude_autopilot::bot::Bot;
use claude_autopilot::config::BotConfig;
use claude_autopilot::evolution;
use claude_autopilot::runner;

#[derive(Parser)]
#[command(
    name = "claude-autopilot",
    about = "Action-search autopilot with built-in evolution"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a single game with the bot
    Run {
        /// Game seed (hex, e.g. 0xDEADBEEF)
        #[arg(long)]
        seed: String,

        /// Maximum frames to run
        #[arg(long, default_value = "108000")]
        max_frames: u32,

        /// Config file path (JSON)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Use a named preset instead of config file
        #[arg(long)]
        preset: Option<String>,

        /// Output tape path
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Benchmark across multiple seeds
    Bench {
        /// Number of random seeds
        #[arg(long, default_value = "8")]
        seed_count: usize,

        /// Starting seed (incremented for each run)
        #[arg(long, default_value = "0xDEADBEEF")]
        base_seed: String,

        /// Maximum frames per run
        #[arg(long, default_value = "108000")]
        max_frames: u32,

        /// Config file path (JSON)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Use a named preset
        #[arg(long)]
        preset: Option<String>,

        /// Output directory
        #[arg(long, default_value = "bench-output")]
        out_dir: PathBuf,

        /// Save top N tapes
        #[arg(long, default_value = "3")]
        save_top: usize,

        /// Parallel jobs (default: all cores)
        #[arg(long)]
        jobs: Option<usize>,
    },

    /// Run multi-generation evolution
    Evolve {
        /// Number of generations
        #[arg(long, default_value = "5")]
        generations: u32,

        /// Number of seeds per generation
        #[arg(long, default_value = "8")]
        seed_count: usize,

        /// Starting seed
        #[arg(long, default_value = "0xDEADBEEF")]
        base_seed: String,

        /// Maximum frames per run
        #[arg(long, default_value = "108000")]
        max_frames: u32,

        /// Initial config file (JSON)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Use a named preset as starting point
        #[arg(long)]
        preset: Option<String>,

        /// Output directory
        #[arg(long, default_value = "evolve-output")]
        out_dir: PathBuf,
    },

    /// Print a config as JSON
    ShowConfig {
        /// Config file path
        #[arg(long)]
        config: Option<PathBuf>,

        /// Use a named preset
        #[arg(long)]
        preset: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Run {
            seed,
            max_frames,
            config,
            preset,
            output,
        } => {
            let cfg = load_config(config, preset)?;
            let seed_val = parse_seed(&seed)?;
            let mut bot = Bot::new(cfg);
            let artifact = runner::run(&mut bot, seed_val, max_frames)?;

            eprintln!(
                "seed={:#010x} score={} frames={} lives={} wave={} game_over={} rules_digest={:#010x}",
                seed_val,
                artifact.metrics.final_score,
                artifact.metrics.frame_count,
                artifact.metrics.final_lives,
                artifact.metrics.final_wave,
                artifact.metrics.game_over,
                artifact.metrics.rules_digest,
            );

            if let Some(path) = output {
                runner::write_tape(&path, &artifact.tape)?;
                eprintln!("tape written to {}", path.display());
            }
        }

        Command::Bench {
            seed_count,
            base_seed,
            max_frames,
            config,
            preset,
            out_dir,
            save_top,
            jobs,
        } => {
            let cfg = load_config(config, preset)?;
            let base = parse_seed(&base_seed)?;
            let seeds = generate_seeds(base, seed_count);

            eprintln!(
                "Benchmarking {} with {} seeds, max_frames={}",
                cfg.id, seed_count, max_frames
            );

            let report = benchmark::run_benchmark(BenchmarkConfig {
                bot_config: cfg,
                seeds,
                max_frames,
                out_dir: out_dir.clone(),
                save_top,
                jobs,
            })?;

            eprintln!(
                "avg_score={:.0} max_score={} avg_frames={:.0} survival_rate={:.1}%",
                report.avg_score,
                report.max_score,
                report.avg_frames,
                report.survival_rate * 100.0,
            );
            eprintln!("report saved to {}/summary.json", out_dir.display());
        }

        Command::Evolve {
            generations,
            seed_count,
            base_seed,
            max_frames,
            config,
            preset,
            out_dir,
        } => {
            let cfg = load_config(config, preset)?;
            let base = parse_seed(&base_seed)?;
            let seeds = generate_seeds(base, seed_count);

            eprintln!(
                "Evolving from {} for {} generations, {} seeds, max_frames={}",
                cfg.id, generations, seed_count, max_frames
            );

            let reports =
                evolution::run_evolution(&cfg, &seeds, max_frames, generations, &out_dir)?;

            eprintln!("\n=== Evolution Summary ===");
            for report in &reports {
                eprintln!(
                    "gen{}: avg={:.0} max={} deaths/10k={:.1} hit={:.0}%",
                    report.generation,
                    report.avg_score,
                    report.max_score,
                    report.death_analysis.deaths_per_10k_frames,
                    report.shot_analysis.hit_rate * 100.0,
                );
            }

            if let Some(last) = reports.last() {
                let final_config_path = out_dir.join("final-config.json");
                std::fs::write(
                    &final_config_path,
                    serde_json::to_vec_pretty(&last.evolved_config)?,
                )?;
                eprintln!("\nFinal config saved to {}", final_config_path.display());
            }
        }

        Command::ShowConfig { config, preset } => {
            let cfg = load_config(config, preset)?;
            println!("{}", serde_json::to_string_pretty(&cfg)?);
        }
    }

    Ok(())
}

fn load_config(path: Option<PathBuf>, preset: Option<String>) -> Result<BotConfig> {
    if let Some(path) = path {
        let data = std::fs::read(&path)?;
        let cfg: BotConfig = serde_json::from_slice(&data)?;
        Ok(cfg)
    } else if let Some(name) = preset {
        BotConfig::preset(&name).ok_or_else(|| {
            anyhow!(
                "unknown preset '{}' (try: marathon, hunter, supernova)",
                name
            )
        })
    } else {
        Ok(BotConfig::default())
    }
}

fn parse_seed(s: &str) -> Result<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|e| anyhow!("invalid hex seed '{}': {}", s, e))
    } else {
        s.parse::<u32>()
            .map_err(|e| anyhow!("invalid seed '{}': {}", s, e))
    }
}

fn generate_seeds(base: u32, count: usize) -> Vec<u32> {
    (0..count as u32)
        .map(|i| base.wrapping_add(i.wrapping_mul(0x9E3779B9)))
        .collect()
}
