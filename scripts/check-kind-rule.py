#!/usr/bin/env python3
"""Prove the duplicates kind-rule diff: every pair present BEFORE but absent
AFTER is container × member (class/trait vs function/method); no same-kind
pair was removed and no new pair appeared.

Usage: check-kind-rule.py <before-dump> <after-dump>
Both dumps come from cie-polyglot/examples/dup_dump.rs (the after-side dump
supplies the symbol kinds; ids are deterministic across runs of the same
source tree)."""
import sys

def load(path):
    pairs, kinds = {}, {}
    with open(path) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if parts[0] == "duplicates":
                _, sid, a, b, sim, tier, det, epi, canon, counter = parts
                pairs[(int(a), int(b))] = (tier, sim, det, epi)
            elif parts[0] == "symbols":
                kinds[int(parts[1])] = parts[2]
    return pairs, kinds

CONTAINERS = {"class", "trait"}
MEMBERS = {"function", "method"}

before, kinds_before = load(sys.argv[1])
after, kinds = load(sys.argv[2])
# the before-side binary may predate the harness's symbols dump; ids are
# deterministic (same ingest code, same source tree), so kinds come from
# whichever dump carries them
assert not kinds_before or kinds_before == kinds, "symbol ids/kinds differ between runs"

removed = sorted(set(before) - set(after))
added = sorted(set(after) - set(before))
kept_diff = [(p, before[p], after[p]) for p in before.keys() & after.keys() if before[p] != after[p]]

bad = [p for p in removed if not (
    (kinds[p[0]] in CONTAINERS and kinds[p[1]] in MEMBERS) or
    (kinds[p[0]] in MEMBERS and kinds[p[1]] in CONTAINERS))]
same_kind_removed = [p for p in removed if kinds[p[0]] == kinds[p[1]]]

print(f"pairs: before={len(before)} after={len(after)} removed={len(removed)} added={len(added)}")
kinds_removed = {}
for a, b in removed:
    key = (kinds[a], kinds[b])
    kinds_removed[key] = kinds_removed.get(key, 0) + 1
print("removed by kind combination:", kinds_removed)
print(f"removed pairs that are NOT container×member: {len(bad)}")
print(f"same-kind pairs removed: {len(same_kind_removed)}")
print(f"pairs whose tier/sim changed while kept: {len(kept_diff)}")
for p, x, y in kept_diff[:5]:
    print("  KEPT-DIFF", p, x, y)
for p in bad[:5]:
    print("  BAD", p, kinds[p[0]], kinds[p[1]])
for p in added[:5]:
    print("  ADDED", p)

ok = not bad and not added and not same_kind_removed and not kept_diff
print("RESULT:", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
