// DS-EVAL verification: the web read path against the promoted WAL-mode DB.
// Proves: (1) node:sqlite opens a cleanly-checkpointed WAL DB read-only
// (no -wal/-shm sidecars present); (2) the duplicates indexes now exist at
// build time (idx_dup_detection/epistemic were previously web-side writes);
// (3) a filtered duplicates query works and uses an index.
import { DatabaseSync } from "node:sqlite";

const dbPath = process.argv[2];
const db = new DatabaseSync(dbPath, { readOnly: true });

const mode = db.prepare("PRAGMA journal_mode").get();
console.log("journal_mode:", JSON.stringify(mode));

const idx = db.prepare("PRAGMA index_list(duplicates)").all();
console.log("duplicates indexes:", idx.map((r) => r.name).sort().join(", "));

const plans = db
  .prepare(
    "EXPLAIN QUERY PLAN SELECT * FROM duplicates WHERE detection = 'canonical-hash' AND epistemic = 'fact'"
  )
  .all();
console.log("query plan:", plans.map((r) => r.detail).join(" | "));

const tiers = db
  .prepare("SELECT tier, COUNT(*) AS n FROM duplicates GROUP BY tier")
  .all();
console.log("tiers:", JSON.stringify(tiers));

const files = db.prepare("SELECT COUNT(*) AS n FROM files").get();
const syms = db.prepare("SELECT COUNT(*) AS n FROM symbols").get();
console.log(`rows: files=${files.n} symbols=${syms.n}`);

db.close();
console.log("READONLY-WAL-OK");
