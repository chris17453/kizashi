#!/usr/bin/env bash
# Create a new numbered Architecture Decision Record from the template.
#
# Usage: scripts/adr-new.sh "<title>"
set -euo pipefail

TITLE="${1:?usage: adr-new.sh \"<title>\"}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ADR_DIR="$ROOT/docs/adr"
mkdir -p "$ADR_DIR"

SLUG="$(echo "$TITLE" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-//;s/-$//')"

LAST=$(find "$ADR_DIR" -maxdepth 1 -name '[0-9][0-9][0-9][0-9]-*.md' 2>/dev/null \
  | sed -E 's#.*/([0-9]{4})-.*#\1#' | sort -n | tail -1)
if [ -z "${LAST:-}" ]; then
  NEXT="0001"
else
  NEXT=$(printf "%04d" $((10#$LAST + 1)))
fi

FILE="$ADR_DIR/${NEXT}-${SLUG}.md"
TODAY=$(date +%Y-%m-%d 2>/dev/null || echo "unknown")

cat > "$FILE" <<EOF
# ADR-${NEXT}: ${TITLE}

- **Status:** proposed
- **Date:** ${TODAY}

## Context

<!-- What forces are at play: technical, business, spec references. Link the spec section. -->

## Decision

<!-- The change we're making, stated plainly. -->

## Consequences

<!-- What becomes easier or harder as a result. Trade-offs accepted. -->
EOF

echo "Created $FILE"
