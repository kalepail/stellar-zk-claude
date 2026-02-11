# Evolution Lessons Learned

## Baseline Knowledge (from prior hand-tuning)
- omega-marathon is the most balanced SearchBot: deep lookahead (20), high survival (2.05), moderate aggression (0.5)
- Higher survival_weight = more cautious = fewer deaths but lower score
- Higher aggression_weight + fire_reward = more kills but more risk
- Saucer bullets are the #1 killer — risk_weight_bullet is critical
- Edge deaths are common — center_weight and edge_penalty matter
- Lurk mechanic: game spawns saucers aggressively if you don't kill. lurk_trigger_frames controls when bot reacts
- fire_tolerance_bam controls aim precision required. Too tight (5-6) = misses good shots. Too loose (12+) = wastes ammo
- speed_soft_cap prevents excessive speed that makes dodging harder
- Action/turn/thrust penalties control energy efficiency. Too high = bot freezes. Too low = jittery movement

## Iteration History
(entries added by evolution loop)
