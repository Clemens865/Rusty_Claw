#!/usr/bin/env bash
# Measure performance metrics for Rusty Claw.
# Usage: ./scripts/measure.sh
#
# Reports:
#   - Binary size (goal: <20 MB)
#   - Startup time to first /health response (goal: <2s)
#   - Memory (RSS) of the running gateway
set -euo pipefail

BINARY="target/release/rusty-claw"
PORT=18799  # Use a non-default port for testing
TIMEOUT=10  # seconds

echo "=== Rusty Claw Performance Measurement ==="
echo

# 1. Build release binary
echo "[1/4] Building release binary..."
cargo build --release -p rusty-claw-cli --quiet 2>/dev/null || cargo build --release -p rusty-claw-cli

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

# 2. Report binary size
SIZE_BYTES=$(stat -f%z "$BINARY" 2>/dev/null || stat --format=%s "$BINARY" 2>/dev/null)
SIZE_MB=$(echo "scale=2; $SIZE_BYTES / 1048576" | bc)
echo
echo "[2/4] Binary size: ${SIZE_MB} MB (${SIZE_BYTES} bytes)"
if (( $(echo "$SIZE_MB < 20" | bc -l) )); then
    echo "       PASS: Under 20 MB target"
else
    echo "       WARN: Over 20 MB target"
fi

# 3. Measure startup time
echo
echo "[3/4] Measuring startup time..."

# Create a minimal config for startup test
TMPDIR=$(mktemp -d)
cat > "$TMPDIR/config.json" <<EOF
{
  "workspace": "$TMPDIR/workspace",
  "gateway": { "port": $PORT }
}
EOF
mkdir -p "$TMPDIR/workspace"

START_TIME=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')

# Start the gateway in background
"$BINARY" --config "$TMPDIR/config.json" gateway --port $PORT &
GW_PID=$!

# Wait for /health to respond
READY=false
for i in $(seq 1 $((TIMEOUT * 10))); do
    if curl -s -o /dev/null -w "%{http_code}" "http://localhost:$PORT/health" 2>/dev/null | grep -q "200"; then
        READY=true
        break
    fi
    sleep 0.1
done

END_TIME=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')

if [ "$READY" = true ]; then
    ELAPSED_NS=$((END_TIME - START_TIME))
    ELAPSED_MS=$((ELAPSED_NS / 1000000))
    ELAPSED_S=$(echo "scale=2; $ELAPSED_MS / 1000" | bc)
    echo "       Startup time: ${ELAPSED_S}s (${ELAPSED_MS}ms)"
    if (( $(echo "$ELAPSED_S < 2" | bc -l) )); then
        echo "       PASS: Under 2s target"
    else
        echo "       WARN: Over 2s target"
    fi
else
    echo "       FAIL: Gateway did not start within ${TIMEOUT}s"
fi

# 4. Measure memory (RSS)
echo
echo "[4/4] Measuring memory usage..."

if [ "$READY" = true ] && kill -0 "$GW_PID" 2>/dev/null; then
    # macOS: ps -o rss gives kilobytes
    RSS_KB=$(ps -o rss= -p "$GW_PID" 2>/dev/null | tr -d ' ')
    if [ -n "$RSS_KB" ]; then
        RSS_MB=$(echo "scale=1; $RSS_KB / 1024" | bc)
        echo "       RSS memory: ${RSS_MB} MB"
        if (( $(echo "$RSS_MB < 50" | bc -l) )); then
            echo "       PASS: Under 50 MB target"
        else
            echo "       WARN: Over 50 MB target"
        fi
    else
        echo "       Could not read RSS"
    fi
fi

# Cleanup
kill "$GW_PID" 2>/dev/null || true
wait "$GW_PID" 2>/dev/null || true
rm -rf "$TMPDIR"

echo
echo "=== Done ==="
