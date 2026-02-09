#!/bin/bash
# Kimi Bot Test Harness
# Runs multiple Kimi bots against each other and generates comparison reports

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Configuration
SEED_START=${1:-0x00000001}
SEED_COUNT=${2:-12}
MAX_FRAMES=${3:-108000}  # 30 minutes at 60fps
JOBS=${4:-4}
OUTPUT_DIR="${PROJECT_DIR}/kimi-results"

# Bot lists
KIMI_BOTS=(
    "kimi-hunter-v1"
    "kimi-hunter-v2"
    "kimi-hunter-v3"
    "kimi-hunter-v4-max"
    "kimi-survivor-v1"
    "kimi-survivor-v2"
    "kimi-wrap-master-v1"
    "kimi-saucer-killer-v1"
    "kimi-saucer-killer-v2"
    "kimi-super-ship-v1"
)

COMPETITOR_BOTS=(
    "omega-marathon"
    "omega-ace"
    "omega-alltime-hunter"
    "omega-supernova"
    "offline-wrap-endurancex"
    "offline-wrap-sniper30"
)

echo "=========================================="
echo "Kimi Bot Test Harness"
echo "=========================================="
echo "Seeds: $SEED_COUNT (starting from $SEED_START)"
echo "Max Frames: $MAX_FRAMES"
echo "Jobs: $JOBS"
echo "Output: $OUTPUT_DIR"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Build the project
echo "Building project..."
cd "$PROJECT_DIR"
cargo build --release --quiet

# Function to run a single bot benchmark
run_bot_benchmark() {
    local bot=$1
    local output_file="$OUTPUT_DIR/${bot}.json"
    
    echo "Running benchmark for $bot..."
    
    cargo run --release --quiet -- benchmark \
        --bots "$bot" \
        --seed-start "$SEED_START" \
        --seed-count "$SEED_COUNT" \
        --max-frames "$MAX_FRAMES" \
        --objective survival \
        --jobs 1 \
        --output "$output_file" 2>&1 | grep -E "(Score|Frames|Status)" || true
}

# Function to run comparison benchmark
run_comparison() {
    local bots=$1
    local name=$2
    local output_file="$OUTPUT_DIR/comparison-${name}.json"
    
    echo ""
    echo "Running $name comparison..."
    echo "Bots: $bots"
    
    cargo run --release --quiet -- benchmark \
        --bots "$bots" \
        --seed-start "$SEED_START" \
        --seed-count "$SEED_COUNT" \
        --max-frames "$MAX_FRAMES" \
        --objective survival \
        --jobs "$JOBS" \
        --output "$output_file"
}

# Export functions for parallel execution
export -f run_bot_benchmark
export -f run_comparison
export PROJECT_DIR OUTPUT_DIR SEED_START SEED_COUNT MAX_FRAMES

# Run all Kimi bot benchmarks in parallel
echo ""
echo "Running all Kimi bot benchmarks..."
printf '%s\n' "${KIMI_BOTS[@]}" | xargs -P "$JOBS" -I {} bash -c 'run_bot_benchmark "$@"' _ {}

# Run comparisons
run_comparison "$(IFS=,; echo "${KIMI_BOTS[*]}")" "kimi-all"
run_comparison "$(IFS=,; echo "${COMPETITOR_BOTS[*]}")" "competitors"

# Run cross-comparison with top performers
TOP_KIMI="kimi-hunter-v4-max,kimi-super-ship-v1"
run_comparison "${TOP_KIMI},omega-supernova,offline-wrap-endurancex" "champions"

# Generate report
echo ""
echo "Generating analysis report..."
cat > "$OUTPUT_DIR/analysis.sh" << 'ANALYSIS_SCRIPT'
#!/bin/bash
OUTPUT_DIR="$1"

echo "=========================================="
echo "Kimi Bot Analysis Report"
echo "=========================================="
echo ""

# Function to extract stats from JSON
extract_stats() {
    local file=$1
    if [ -f "$file" ]; then
        python3 -c "
import json
import sys
try:
    with open('$file') as f:
        data = json.load(f)
    if 'results' in data and len(data['results']) > 0:
        result = data['results'][0]
        print(f\"Score: {result.get('score', 'N/A')}\")
        print(f\"Frames: {result.get('frames', 'N/A')}\")
        print(f\"Status: {result.get('status', 'N/A')}\")
except Exception as e:
    print(f\"Error: {e}\")
" 2>/dev/null || echo "  Could not parse results"
    fi
}

# Analyze each bot
echo "Individual Bot Results:"
echo "--------------------"
for bot_file in "$OUTPUT_DIR"/*.json; do
    if [ -f "$bot_file" ]; then
        bot_name=$(basename "$bot_file" .json)
        if [[ "$bot_name" != comparison* ]]; then
            echo ""
            echo "$bot_name:"
            extract_stats "$bot_file"
        fi
    fi
done

echo ""
echo "Comparison Results:"
echo "-------------------"
for comp_file in "$OUTPUT_DIR"/comparison-*.json; do
    if [ -f "$comp_file" ]; then
        comp_name=$(basename "$comp_file" .json | sed 's/comparison-//')
        echo ""
        echo "$comp_name:"
        python3 -c "
import json
try:
    with open('$comp_file') as f:
        data = json.load(f)
    if 'results' in data:
        print(f'  Total runs: {len(data[\"results\"])}')
        # Group by bot
        bots = {}
        for r in data['results']:
            bot = r.get('bot', 'unknown')
            if bot not in bots:
                bots[bot] = {'scores': [], 'frames': [], 'survived': 0}
            bots[bot]['scores'].append(r.get('score', 0))
            bots[bot]['frames'].append(r.get('frames', 0))
            if r.get('status') == 'completed':
                bots[bot]['survived'] += 1
        
        print('  Results by bot:')
        for bot, stats in sorted(bots.items()):
            avg_score = sum(stats['scores']) / len(stats['scores'])
            avg_frames = sum(stats['frames']) / len(stats['frames'])
            print(f'    {bot}: avg_score={avg_score:.0f}, avg_frames={avg_frames:.0f}, survived={stats[\"survived\"]}/{len(stats[\"scores\"])}')
except Exception as e:
    print(f'  Error: {e}')
" 2>/dev/null || echo "  Could not parse"
    fi
done

echo ""
echo "Learning Database Summary:"
echo "--------------------------"
for db_file in kimi-*.json; do
    if [ -f "$db_file" ]; then
        echo "Found: $db_file"
        python3 -c "
import json
try:
    with open('$db_file') as f:
        data = json.load(f)
    print(f'  Games: {data.get(\"game_count\", 0)}')
    print(f'  Deaths: {len(data.get(\"deaths\", []))}')
    print(f'  Missed shots: {len(data.get(\"missed_shots\", []))}')
except Exception as e:
    print(f'  Error: {e}')
" 2>/dev/null
    fi
done 2>/dev/null || true

echo ""
echo "Report complete. Results saved to: $OUTPUT_DIR"
ANALYSIS_SCRIPT
chmod +x "$OUTPUT_DIR/analysis.sh"
bash "$OUTPUT_DIR/analysis.sh" "$OUTPUT_DIR"

# Save summary
cat > "$OUTPUT_DIR/README.md" << EOF
# Kimi Bot Test Results

Generated: $(date)

## Configuration
- Seeds: $SEED_COUNT (starting from $SEED_START)
- Max Frames: $MAX_FRAMES (30 minutes @ 60fps)
- Parallel Jobs: $JOBS

## Results

See individual .json files for detailed results.

## Analysis

Run the analysis script:
\`\`\`bash
bash $OUTPUT_DIR/analysis.sh $OUTPUT_DIR
\`\`\`

## Learning Databases

Bot learning data is stored in JSON files in this directory.

## Next Steps

1. Review comparison results
2. Analyze learning databases for patterns
3. Adjust bot parameters based on death/miss patterns
4. Re-run benchmarks to test improvements
5. Iterate until max score achieved

EOF

echo ""
echo "=========================================="
echo "Test harness complete!"
echo "Results saved to: $OUTPUT_DIR"
echo ""
echo "To view detailed analysis:"
echo "  bash $OUTPUT_DIR/analysis.sh $OUTPUT_DIR"
echo ""
echo "To view a bot's learning report:"
echo "  cargo run --release -- kimi-report <bot-id>"
echo "=========================================="
