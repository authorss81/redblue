#!/usr/bin/env bash
# Securely uploads and distributes OpenCode API keys across GitHub Organizations.
# Keys are read from a local file (e.g. keys.txt) and piped via standard input to
# `gh secret set --org <org> --visibility all`. Secrets are NEVER printed or logged.

set -euo pipefail

KEY_FILE="${1:-keys.txt}"

ORGS=(
    "rb-pkg-sys"
    "rb-pkg-web"
    "rb-pkg-net"
    "rb-pkg-data"
    "rb-pkg-math"
    "rb-pkg-ai"
    "rb-pkg-crypto"
    "rb-pkg-media"
    "rb-pkg-cli"
    "rb-apps-org"
)

echo "=========================================================="
echo " Redblue Ecosystem Organization Secret Provisioner"
echo "=========================================================="

if ! command -v gh &> /dev/null; then
    echo "Error: GitHub CLI ('gh') is not installed. Please install from https://cli.github.com"
    exit 1
fi

if ! gh auth status &> /dev/null; then
    echo "Error: GitHub CLI is not logged in. Please run 'gh auth login' first."
    exit 1
fi

if [ ! -f "$KEY_FILE" ]; then
    echo "Error: Key file '$KEY_FILE' not found! Create '$KEY_FILE' with one key per line."
    exit 1
fi

# Read non-empty, non-comment lines into array
mapfile -t KEYS < <(grep -v '^[[:space:]]*#' "$KEY_FILE" | grep -v '^[[:space:]]*$' || true)
TOTAL_KEYS=${#KEYS[@]}

if [ "$TOTAL_KEYS" -eq 0 ]; then
    echo "Error: Key file '$KEY_FILE' contains 0 valid keys."
    exit 1
fi

TOTAL_ORGS=${#ORGS[@]}
echo "[OK] Loaded $TOTAL_KEYS API keys from '$KEY_FILE'."
echo "[OK] Target Organizations: $TOTAL_ORGS organizations."
echo "----------------------------------------------------------"

SUCCESS_COUNT=0
FAIL_COUNT=0

for i in "${!KEYS[@]}"; do
    KEY="${KEYS[$i]}"
    ORG_INDEX=$((i % TOTAL_ORGS))
    TARGET_ORG="${ORGS[$ORG_INDEX]}"
    
    KEY_SLOT=$(( (i / TOTAL_ORGS) + 1 ))
    SECRET_NAME="OPENCODE_KEY_${KEY_SLOT}"

    echo -n "Setting $SECRET_NAME in $TARGET_ORG... "
    
    # Pipe secret through stdin into gh secret set so it is never exposed in process args
    if echo -n "$KEY" | gh secret set "$SECRET_NAME" --org "$TARGET_ORG" --visibility all &> /dev/null; then
        echo "[OK]"
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    else
        echo "[FAILED]"
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
done

echo "=========================================================="
echo " Provisioning Complete!"
echo " Successfully set: $SUCCESS_COUNT secrets"
if [ "$FAIL_COUNT" -gt 0 ]; then
    echo " Failed: $FAIL_COUNT secrets (ensure organizations exist on GitHub)"
fi
echo "=========================================================="
