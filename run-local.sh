#!/usr/bin/env bash
set -euo pipefail

export STUFF_DATABASE_URL="${STUFF_DATABASE_URL:-postgres://root:root@127.0.0.1:5432/stuff}"
cd "$(dirname "$0")/stuff-app"
exec npm --prefix ui exec tauri -- dev
