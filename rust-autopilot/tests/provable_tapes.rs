use anyhow::Result;
use rust_autopilot::bots::bot_ids;
use rust_autopilot::runner::run_bot;

#[test]
fn all_bots_generate_provable_tapes_on_smoke_seed() -> Result<()> {
    let seed = 0xDEAD_BEEF;
    for bot in bot_ids() {
        let artifact = run_bot(bot, seed, 900)?;
        assert!(artifact.metrics.frame_count > 0, "bot={bot}");
        assert_eq!(artifact.metrics.bot_id, bot, "bot id mismatch for {bot}");
        assert!(artifact.tape.len() > 72 + 12, "tape too small for {bot}");
    }
    Ok(())
}

#[test]
fn all_bots_generate_provable_tapes_on_multiple_seeds() -> Result<()> {
    let seeds = [0xDEAD_BEEF, 0xC0FF_EE11, 0x1234_5678];
    for seed in seeds {
        for bot in bot_ids() {
            let artifact = run_bot(bot, seed, 1200)?;
            assert!(artifact.metrics.frame_count > 0, "bot={bot} seed={seed:#x}");
            assert_eq!(
                artifact.metrics.bot_id, bot,
                "bot id mismatch for {bot} seed={seed:#x}"
            );
            assert!(
                artifact.tape.len() > 72 + 12,
                "tape too small for {bot} seed={seed:#x}"
            );
        }
    }
    Ok(())
}

#[test]
fn benchmark_smoke_outputs_expected_metadata() -> Result<()> {
    use rust_autopilot::benchmark::{run_benchmark, BenchmarkConfig, Objective};

    let tmp = tempfile::tempdir()?;
    let report = run_benchmark(BenchmarkConfig {
        bots: vec![
            "omega-marathon".to_string(),
            "offline-wrap-sniper30".to_string(),
        ],
        seeds: vec![0xDEAD_BEEF, 0xC0FF_EE11],
        max_frames: 900,
        objective: Objective::Hybrid,
        out_dir: tmp.path().to_path_buf(),
        save_top: 1,
        jobs: None,
    })?;

    assert_eq!(report.run_count, 4);
    assert_eq!(report.bot_rankings.len(), 2);
    assert!(!report.saved_tapes.is_empty());
    assert!(tmp.path().join("summary.json").exists());
    assert!(tmp.path().join("runs.csv").exists());
    assert!(tmp.path().join("rankings.csv").exists());

    Ok(())
}
