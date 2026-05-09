#!/bin/bash
# Launch Codex agents in parallel to fill spec files
# Each agent reads its PRD and uses GitNexus + ABCoder MCP to analyze code

WORKSPACE="/Volumes/mayrain/workspace/Aether"
TASKS_DIR="$WORKSPACE/.trellis/tasks"
LOG_DIR="$WORKSPACE/.trellis/workspace/mayrain/codex-logs"
mkdir -p "$LOG_DIR"

PROMPT_TEMPLATE='Read the file .trellis/tasks/TASK_SLUG/prd.md and execute the task described in it. Use the GitNexus and ABCoder MCP tools to analyze the codebase, then fill all spec files listed in the PRD with real code examples and project-specific guidelines. Remember: specify repo="Aether" for GitNexus tools and repo_name="CRATE_NAME" for ABCoder tools.'

# All 24 tasks
TASKS=(
  "01-spec-aether-crypto:aether-crypto"
  "02-spec-aether-wallet:aether-wallet"
  "03-spec-aether-cache:aether-cache"
  "04-spec-aether-http:aether-http"
  "05-spec-aether-contracts:aether-contracts"
  "06-spec-aether-data-schema:aether-data-schema"
  "07-spec-aether-ai-formats:aether-ai-formats"
  "08-spec-aether-data-contracts:aether-data-contracts"
  "09-spec-aether-oauth:aether-oauth"
  "10-spec-aether-runtime:aether-runtime"
  "11-spec-aether-runtime-state:aether-runtime-state"
  "12-spec-aether-scheduler-core:aether-scheduler-core"
  "13-spec-aether-video-tasks-core:aether-video-tasks-core"
  "14-spec-aether-task-runtime:aether-task-runtime"
  "15-spec-aether-data:aether-data"
  "16-spec-aether-provider-transport:aether-provider-transport"
  "17-spec-aether-ai-serving:aether-ai-serving"
  "18-spec-aether-model-fetch:aether-model-fetch"
  "19-spec-aether-usage-runtime:aether-usage-runtime"
  "20-spec-aether-billing:aether-billing"
  "21-spec-aether-admin:aether-admin"
  "22-spec-aether-gateway:aether-gateway"
  "23-spec-aether-testkit:aether-testkit"
  "24-spec-aether-proxy:aether-proxy"
)

# Concurrency control - run N agents at a time
MAX_PARALLEL=${1:-6}
RUNNING=0
PIDS=()

echo "=== Launching Codex Spec Agents ==="
echo "Parallelism: $MAX_PARALLEL"
echo "Total tasks: ${#TASKS[@]}"
echo ""

for entry in "${TASKS[@]}"; do
  SLUG="${entry%%:*}"
  CRATE="${entry##*:}"

  PROMPT="${PROMPT_TEMPLATE//TASK_SLUG/$SLUG}"
  PROMPT="${PROMPT//CRATE_NAME/$CRATE}"

  echo "[$(date +%H:%M:%S)] Starting: $SLUG"

  codex exec -s workspace-write -C "$WORKSPACE" "$PROMPT" > "$LOG_DIR/${SLUG}.log" 2>&1 &
  PIDS+=($!)

  RUNNING=$((RUNNING + 1))

  if [ $RUNNING -ge $MAX_PARALLEL ]; then
    # Wait for any one child to finish (zsh-compatible)
    for pid in "${PIDS[@]}"; do
      if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid"
        PIDS=("${PIDS[@]/$pid}")
        RUNNING=$((RUNNING - 1))
        break
      fi
    done
    # If none finished yet, wait for the first one
    if [ $RUNNING -ge $MAX_PARALLEL ]; then
      wait "${PIDS[0]}"
      PIDS=("${PIDS[@]:1}")
      RUNNING=$((RUNNING - 1))
    fi
  fi
done

# Wait for all remaining
wait

echo ""
echo "=== All agents completed ==="
echo "Logs: $LOG_DIR/"

# Quick validation
echo ""
echo "=== Validation ==="
PLACEHOLDER_COUNT=$(grep -rl "To be filled" .trellis/spec/ 2>/dev/null | wc -l)
echo "Files still with placeholders: $PLACEHOLDER_COUNT"
find .trellis/spec -name "*.md" -exec sh -c 'lines=$(wc -l < "$1"); if [ "$lines" -lt 20 ]; then echo "  SHORT: $1 ($lines lines)"; fi' _ {} \;
