#!/usr/bin/env bash
# Create a new branch with an auto-incremented, type-scoped number, and register it in
# docs/branch-registry.md. Source of truth for branch numbering — never hand-pick a number.
#
# Usage: scripts/new-branch.sh <feature|fix|debug|docs|chore> <short-desc-with-dashes>
set -euo pipefail

TYPE="${1:?usage: new-branch.sh <feature|fix|debug|docs|chore> <short-desc>}"
DESC="${2:?usage: new-branch.sh <feature|fix|debug|docs|chore> <short-desc>}"
REGISTRY="$(dirname "$0")/../docs/branch-registry.md"

case "$TYPE" in
  feature|fix|debug|docs|chore) ;;
  *) echo "error: type must be one of feature|fix|debug|docs|chore" >&2; exit 1 ;;
esac

if [ ! -f "$REGISTRY" ]; then
  echo "error: $REGISTRY not found" >&2
  exit 1
fi

# Always read the counter from a fresh, clean main so branch numbering never depends on
# whatever branch/working-tree state this script happens to be invoked from.
git checkout main
git pull --ff-only origin main || true

NEXT=$(awk -F'|' -v t="$TYPE" '
  { gsub(/^[ \t]+|[ \t]+$/, "", $2) }
  $2 == t { gsub(/[^0-9]/, "", $4); print $4 }
' "$REGISTRY")

if [ -z "$NEXT" ]; then
  echo "error: could not find counter row for type '$TYPE' in $REGISTRY" >&2
  exit 1
fi

BRANCH="${TYPE}/${NEXT}-${DESC}"
TODAY=$(date +%Y-%m-%d 2>/dev/null || echo "unknown")

NEXT_NUM=$(printf "%04d" $((10#$NEXT + 1)))

git checkout -b "$BRANCH"

# Bump the counter for this type
tmp=$(mktemp)
awk -F'|' -v OFS='|' -v t="$TYPE" -v newnum="$NEXT_NUM" '
  {
    line = $2; gsub(/^[ \t]+|[ \t]+$/, "", line)
    if (line == t) { $4 = " " newnum "         "; print; next }
    print
  }
' "$REGISTRY" > "$tmp" && mv "$tmp" "$REGISTRY"

# Append a row to the branch log table (last table in the file)
printf "| %s | \`%s\` | %s | %s | open | pending | |\n" "$NEXT" "$BRANCH" "$TYPE" "$TODAY" >> "$REGISTRY"

echo "Created branch: $BRANCH"
echo "Registered in $REGISTRY (next $TYPE number is now $NEXT_NUM)"
echo "Remember: docs/features.md gets an entry in the PR that lands this branch."
