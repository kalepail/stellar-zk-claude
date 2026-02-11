use anyhow::{anyhow, Result};
use asteroids_verifier_core::tape::parse_tape;
use asteroids_verifier_core::verify_tape;
use clap::{Parser, Subcommand, ValueEnum};
use rust_autopilot::benchmark::{resolve_bots, run_benchmark, BenchmarkConfig, Objective};
use rust_autopilot::bots::{bot_ids, bot_manifest_entries, create_bot, describe_bots};
use rust_autopilot::claude::lab::{run_multi_generation, EvolvedConfig};
use rust_autopilot::codex_lab::{collect_run_intel, default_codex_output_dir, run_learning_cycle};
use rust_autopilot::runner::{run_bot_with_claimant, write_tape};
use rust_autopilot::util::{parse_seed, parse_seed_csv, parse_seed_file, seed_to_hex};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug)]
#[command(name = "rust-autopilot")]
#[command(
    about = "Rust autopilot lab for deterministic Asteroids tape generation and benchmarking"
)]
struct Cli {
    /// Claimant address embedded in generated tapes (56-char Stellar strkey, G... or C...)
    #[arg(long, default_value = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGO6V")]
    claimant_address: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available bots
    ListBots,
    /// Export full bot manifest (including config fingerprints)
    RosterManifest {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Generate a single verifiable tape
    Generate {
        #[arg(long)]
        bot: String,
        #[arg(long)]
        seed: String,
        #[arg(long, default_value_t = 18_000)]
        max_frames: u32,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Verify an existing tape against the current verifier/game rules
    VerifyTape {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 108_000)]
        max_frames: u32,
    },
    /// Run multi-seed benchmark across one or more bots
    Benchmark {
        #[arg(long)]
        bots: Option<String>,
        #[arg(long)]
        seeds: Option<String>,
        #[arg(long)]
        seed_file: Option<PathBuf>,
        #[arg(long)]
        seed_start: Option<String>,
        #[arg(long, default_value_t = 12)]
        seed_count: u32,
        #[arg(long, default_value_t = 18_000)]
        max_frames: u32,
        #[arg(long, value_enum, default_value_t = CliObjective::Score)]
        objective: CliObjective,
        #[arg(long)]
        out_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 4)]
        save_top: usize,
        #[arg(long)]
        jobs: Option<usize>,
    },
    /// Collect rich per-frame run intel for one bot/seed (death causes + shot outcomes)
    CodexIntelRun {
        #[arg(long)]
        bot: String,
        #[arg(long)]
        seed: String,
        #[arg(long, default_value_t = 108_000)]
        max_frames: u32,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Learn an adaptive codex profile from an existing benchmark summary
    CodexLearn {
        #[arg(long)]
        summary: PathBuf,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long, default_value_t = true)]
        codex_only: bool,
        #[arg(long, default_value_t = false)]
        install_profile: bool,
    },
    /// Run iterative evolution on claude bots: benchmark, analyze deaths/misses, evolve params
    ClaudeEvolve {
        /// Starting bot (claude-chimera by default, or path to evolved config JSON)
        #[arg(long)]
        from: Option<String>,
        /// Number of evolution generations
        #[arg(long, default_value_t = 5)]
        generations: u32,
        /// Seeds per generation
        #[arg(long, default_value_t = 12)]
        seed_count: u32,
        /// Max frames per run (30 min at 60fps = 108000)
        #[arg(long, default_value_t = 108_000)]
        max_frames: u32,
        /// Output directory for evolution artifacts
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliObjective {
    Score,
    Survival,
    Hybrid,
}

impl From<CliObjective> for Objective {
    fn from(value: CliObjective) -> Self {
        match value {
            CliObjective::Score => Objective::Score,
            CliObjective::Survival => Objective::Survival,
            CliObjective::Hybrid => Objective::Hybrid,
        }
    }
}

fn main() -> Result<()> {
    let Cli {
        claimant_address,
        command,
    } = Cli::parse();
    let claimant_address = claimant_address.trim().to_string();

    match command {
        Commands::ListBots => {
            for (id, description) in describe_bots() {
                println!("{id:20} {description}");
            }
        }
        Commands::RosterManifest { output } => {
            let manifest = bot_manifest_entries();
            let encoded = serde_json::to_vec_pretty(&manifest)?;
            if let Some(path) = output {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, encoded)?;
                println!("wrote={}", path.display());
                println!("bots={}", manifest.len());
            } else {
                println!("{}", String::from_utf8_lossy(&encoded));
            }
        }
        Commands::Generate {
            bot,
            seed,
            max_frames,
            output,
        } => {
            if create_bot(&bot).is_none() {
                let available = bot_ids().join(", ");
                return Err(anyhow!("unknown bot '{bot}'. available: {available}"));
            }
            let seed = parse_seed(&seed)?;
            let artifact =
                run_bot_with_claimant(&bot, seed, max_frames, claimant_address.as_bytes())?;
            let output_path = output.unwrap_or_else(|| {
                PathBuf::from(format!(
                    "checkpoints/{}-{}-score{}-frames{}.tape",
                    bot,
                    seed_to_hex(seed).replace("0x", "seed"),
                    artifact.metrics.final_score,
                    artifact.metrics.frame_count
                ))
            });
            write_tape(&output_path, &artifact.tape)?;

            println!("bot={}", artifact.metrics.bot_id);
            println!("bot_fingerprint={}", artifact.metrics.bot_fingerprint);
            println!("seed={}", seed_to_hex(seed));
            println!("frames={}", artifact.metrics.frame_count);
            println!("score={}", artifact.metrics.final_score);
            println!("lives={}", artifact.metrics.final_lives);
            println!("wave={}", artifact.metrics.final_wave);
            println!("rng={:#010x}", artifact.metrics.final_rng_state);
            println!("rules_digest={:#010x}", artifact.metrics.rules_digest);
            println!("output={}", output_path.display());
        }
        Commands::VerifyTape { input, max_frames } => {
            let bytes = fs::read(&input)?;
            let tape = parse_tape(&bytes, max_frames)?;
            let journal = verify_tape(&bytes, max_frames)?;
            println!("input={}", input.display());
            println!("seed={}", seed_to_hex(tape.header.seed));
            println!("frame_count={}", tape.header.frame_count);
            println!("final_score={}", tape.footer.final_score);
            println!("final_rng_state={:#010x}", tape.footer.final_rng_state);
            println!("rules_digest={:#010x}", journal.rules_digest);
        }
        Commands::Benchmark {
            bots,
            seeds,
            seed_file,
            seed_start,
            seed_count,
            max_frames,
            objective,
            out_dir,
            save_top,
            jobs,
        } => {
            let bots = resolve_bots(bots.as_deref())?;
            let seeds = resolve_seeds(
                seeds.as_deref(),
                seed_file.as_deref(),
                seed_start.as_deref(),
                seed_count,
            )?;
            let objective: Objective = objective.into();

            let out_dir = out_dir.unwrap_or_else(|| {
                PathBuf::from(format!(
                    "benchmarks/{}-{}",
                    objective.as_str(),
                    timestamp_suffix()
                ))
            });

            let report = run_benchmark(BenchmarkConfig {
                bots,
                seeds,
                max_frames,
                objective,
                claimant_address: claimant_address.clone(),
                out_dir: out_dir.clone(),
                save_top,
                jobs,
            })?;

            println!("objective={}", objective.as_str());
            println!("runs={}", report.run_count);
            println!(
                "jobs={}",
                report
                    .jobs
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "auto".to_string())
            );
            println!("out_dir={}", out_dir.display());
            println!("top bots:");
            for (idx, bot) in report.bot_rankings.iter().take(5).enumerate() {
                println!(
                        "  {}. {}  objective={:.2} avg_score={:.1} avg_frames={:.1} avg_actions={:.1} avg_turn={:.1} avg_thrust={:.1} avg_fire={:.1} survival={:.0}%",
                        idx + 1,
                        bot.bot_id,
                        bot.objective_value,
                        bot.avg_score,
                        bot.avg_frames,
                        bot.avg_action_frames,
                        bot.avg_turn_frames,
                        bot.avg_thrust_frames,
                        bot.avg_fire_frames,
                        bot.survival_rate * 100.0,
                    );
            }

            println!("saved tapes:");
            for tape in report.saved_tapes.iter().take(10) {
                println!(
                    "  [{} #{:02}] {} {} score={} frames={} lives={}",
                    tape.metric,
                    tape.rank,
                    tape.bot_id,
                    tape.seed_hex,
                    tape.score,
                    tape.frames,
                    tape.lives,
                );
            }
        }
        Commands::CodexIntelRun {
            bot,
            seed,
            max_frames,
            output,
        } => {
            if create_bot(&bot).is_none() {
                let available = bot_ids().join(", ");
                return Err(anyhow!("unknown bot '{bot}'. available: {available}"));
            }

            let seed = parse_seed(&seed)?;
            let intel = collect_run_intel(&bot, seed, max_frames)?;
            let encoded = serde_json::to_vec_pretty(&intel)?;

            if let Some(path) = output {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, encoded)?;
                println!("bot={}", intel.bot_id);
                println!("seed={}", seed_to_hex(intel.seed));
                println!("frames={}", intel.frame_count);
                println!("score={}", intel.final_score);
                println!("deaths={}", intel.deaths.len());
                println!("shots_fired={}", intel.shot_summary.total_fired);
                println!("shots_hit={}", intel.shot_summary.total_hit);
                println!("shots_miss={}", intel.shot_summary.total_miss);
                println!("output={}", path.display());
            } else {
                println!("{}", String::from_utf8_lossy(&encoded));
            }
        }
        Commands::CodexLearn {
            summary,
            output_dir,
            codex_only,
            install_profile,
        } => {
            let output_dir = output_dir.unwrap_or_else(default_codex_output_dir);
            let cycle = run_learning_cycle(&summary, &output_dir, codex_only)?;

            println!("summary={}", summary.display());
            println!("codex_only={}", codex_only);
            println!("runs_considered={}", cycle.recommendation.runs_considered);
            println!("total_frames={}", cycle.recommendation.total_frames);
            println!("total_deaths={}", cycle.recommendation.total_deaths);
            println!(
                "death_rate_per_10k={:.3}",
                cycle.recommendation.deaths_per_10k_frames
            );
            println!(
                "shot_hit_rate={:.2}%",
                cycle.recommendation.hit_rate * 100.0
            );
            println!(
                "shot_miss_rate={:.2}%",
                cycle.recommendation.miss_rate * 100.0
            );
            println!("intel_report={}", cycle.intel_report_path);
            println!("learning_report={}", cycle.recommendation_path);
            println!("adaptive_profile={}", cycle.adaptive_profile_path);
            println!(
                "profile_scales=risk:{:.3},survival:{:.3},aggression:{:.3},fire_reward:{:.3}",
                cycle.recommendation.profile.risk_weight_scale,
                cycle.recommendation.profile.survival_weight_scale,
                cycle.recommendation.profile.aggression_weight_scale,
                cycle.recommendation.profile.fire_reward_scale
            );
            println!(
                "profile_penalties=shot:{:.3},miss_fire:{:.3},turn:{:.3},thrust:{:.3}",
                cycle.recommendation.profile.shot_penalty_scale,
                cycle.recommendation.profile.miss_fire_penalty_scale,
                cycle.recommendation.profile.turn_penalty_scale,
                cycle.recommendation.profile.thrust_penalty_scale
            );
            for note in &cycle.recommendation.notes {
                println!("note={note}");
            }

            if install_profile {
                let install_dir = default_codex_output_dir();
                fs::create_dir_all(&install_dir)?;
                let install_path = install_dir.join("adaptive-profile.json");
                fs::copy(&cycle.adaptive_profile_path, &install_path)?;
                println!("installed_profile={}", install_path.display());
            }
        }
        Commands::ClaudeEvolve {
            from,
            generations,
            seed_count,
            max_frames,
            out_dir,
        } => {
            let initial_config = if let Some(from_str) = from {
                let path = PathBuf::from(&from_str);
                if path.exists() && path.extension().is_some_and(|e| e == "json") {
                    let data = fs::read(&path)?;
                    serde_json::from_slice::<EvolvedConfig>(&data)?
                } else {
                    // Treat as bot name, create default config from it
                    let mut cfg = EvolvedConfig::default();
                    cfg.parent_id = from_str.clone();
                    cfg.id = from_str;
                    cfg
                }
            } else {
                EvolvedConfig::default()
            };

            let seeds = resolve_seeds(None, None, Some("0xEE010001"), seed_count)?;
            let out_dir = out_dir.unwrap_or_else(|| {
                PathBuf::from(format!("claude-evolution/{}", timestamp_suffix()))
            });

            println!("=== Claude Evolution ===");
            println!("parent={}", initial_config.id);
            println!("generations={generations}");
            println!("seeds={seed_count}");
            println!(
                "max_frames={max_frames} ({:.1} min @ 60fps)",
                max_frames as f64 / 3600.0
            );
            println!("out_dir={}", out_dir.display());
            println!();

            let reports =
                run_multi_generation(&initial_config, &seeds, max_frames, generations, &out_dir)?;

            println!("\n=== Evolution Summary ===");
            for report in &reports {
                println!(
                    "gen {} | avg_score={:.0} | max_score={} | deaths/10k={:.1} | hit_rate={:.0}% | adjustments={}",
                    report.generation,
                    report.avg_score,
                    report.max_score,
                    report.death_analysis.deaths_per_10k_frames,
                    report.shot_analysis.hit_rate * 100.0,
                    report.param_adjustments.len(),
                );
            }

            if let Some(last) = reports.last() {
                println!("\nBest evolved config: {}", last.evolved_config.id);
                println!("Config saved to: {}", out_dir.display());
                println!("\nDeath insights:");
                for insight in &last.death_analysis.insights {
                    println!("  - {insight}");
                }
                println!("Shot insights:");
                for insight in &last.shot_analysis.insights {
                    println!("  - {insight}");
                }
                println!("Parameter adjustments applied:");
                for adj in &last.param_adjustments {
                    println!("  - {adj}");
                }
            }
        }
    }

    Ok(())
}

fn resolve_seeds(
    seeds: Option<&str>,
    seed_file: Option<&Path>,
    seed_start: Option<&str>,
    seed_count: u32,
) -> Result<Vec<u32>> {
    if let Some(path) = seed_file {
        return parse_seed_file(path);
    }

    if let Some(csv) = seeds {
        return parse_seed_csv(csv);
    }

    let start = if let Some(start) = seed_start {
        parse_seed(start)?
    } else {
        0xA57E_0001
    };

    let mut out = Vec::with_capacity(seed_count as usize);
    let mut cur = start;
    for _ in 0..seed_count {
        out.push(cur);
        cur = cur.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    }
    Ok(out)
}

fn timestamp_suffix() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}
