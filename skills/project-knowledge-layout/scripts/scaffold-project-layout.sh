#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: scaffold-project-layout.sh <project-dir>" >&2
  exit 1
fi

project_dir="$1"
project_name="$(basename "$project_dir")"
today="$(date -u +%F)"

mkdir -p "$project_dir/docs" "$project_dir/state" "$project_dir/notes" "$project_dir/artifacts"

if [[ ! -f "$project_dir/README.md" ]]; then
  cat > "$project_dir/README.md" <<EOF
# ${project_name}

## Purpose

TODO

## Status

active

## Canonical Files

- current state: \`state/current.md\`
- architecture: \`docs/architecture.md\`
- decisions: \`docs/decisions.md\`
EOF
fi

if [[ ! -f "$project_dir/docs/architecture.md" ]]; then
  cat > "$project_dir/docs/architecture.md" <<'EOF'
# Architecture

## Components

- TODO

## Data Flow

- TODO
EOF
fi

if [[ ! -f "$project_dir/docs/decisions.md" ]]; then
  cat > "$project_dir/docs/decisions.md" <<'EOF'
# Decisions

## YYYY-MM-DD — Initial setup

- decision: TODO
- why: TODO
- impact: TODO
EOF
fi

if [[ ! -f "$project_dir/state/current.md" ]]; then
  cat > "$project_dir/state/current.md" <<'EOF'
# Current State

## Active Work

- TODO

## Blockers

- none

## Next Steps

- TODO
EOF
fi

if [[ ! -f "$project_dir/state/backlog.md" ]]; then
  cat > "$project_dir/state/backlog.md" <<'EOF'
# Backlog

## Now

- TODO

## Later

- TODO
EOF
fi

touch "$project_dir/notes/${today}.md"

echo "scaffolded ${project_dir}"
