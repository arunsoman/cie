#!/usr/bin/env bash
# bankapp setup — creates the local database and logs directory.
# CIE reads this file now: it sees the sourced file, the psql call, and the
# network + file effects, and draws them on the map.

set -euo pipefail

source "$(dirname "$0")/lib/common.sh"

DB_URL="${DATABASE_URL:-postgresql://localhost/bankapp}"

mkdir -p logs
psql "$DB_URL" -f db/schema.sql
psql "$DB_URL" -f db/migrations/001_init.sql
psql "$DB_URL" -f db/migrations/002_audit_indexes.sql

curl -fsS "http://localhost:8080/health" || echo "service not running yet"
echo "bankapp setup done"
