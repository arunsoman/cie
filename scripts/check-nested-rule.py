#!/usr/bin/env python3
"""Prove the duplicates span-rule diff: every pair present BEFORE but absent
AFTER is a same-file strict span containment (one symbol's byte span strictly
contains the other's) and NOT a container × member kind pair (those belong to
the kind rule's bucket — a kind-bucket removal here would mean double
counting); no other pair was removed and no new pair appeared.

Usage: check-nested-rule.py <before-dump> <after-dump>
Both dumps come from cie-polyglot/examples/dup_dump.rs; the after-side dump
supplies the byte spans (the before-side binary may predate the harness's
spans dump). Ids are deterministic across runs of the same source tree
(same ingest code, same sources)."""
import sys

def load(path):
    pairs, spans, kinds = {}, {}, {}
    with open(path) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if parts[0] == "duplicates":
                _, sid, a, b, sim, tier, det, epi, canon, counter = parts
                pairs[(int(a), int(b))] = (tier, sim, det, epi)
            elif parts[0] == "symbols":
                kinds[int(parts[1])] = parts[2]
            elif parts[0] == "spans":
                spans[int(parts[1])] = (parts[2], int(parts[3]), int(parts[4]))
    return pairs, spans, kinds

def nested(sp, a, b):
    fa, sa, ea = sp[a]
    fb, sb, eb = sp[b]
    if fa != fb:
        return False
    return (sa <= sb and eb <= ea and (sa < sb or eb < ea)) or \
           (sb <= sa and ea <= eb and (sb < sa or ea < eb))

CONTAINERS = {"class", "trait"}
MEMBERS = {"function", "method"}

before, spans_before, kinds_before = load(sys.argv[1])
after, spans, kinds = load(sys.argv[2])
# ids are deterministic (same ingest, same tree), so whichever dump carries
# the spans/kinds must agree with the other
assert not spans_before or spans_before == spans, "symbol ids/spans differ between runs"
assert not kinds_before or kinds_before == kinds, "symbol ids/kinds differ between runs"

removed = sorted(set(before) - set(after))
added = sorted(set(after) - set(before))
kept_diff = [(p, before[p], after[p]) for p in before.keys() & after.keys() if before[p] != after[p]]

def kind_pair(a, b):
    return (kinds[a] in CONTAINERS and kinds[b] in MEMBERS) or \
           (kinds[a] in MEMBERS and kinds[b] in CONTAINERS)

bad = [p for p in removed if not nested(spans, p[0], p[1])]
kind_bucket = [p for p in removed if kind_pair(p[0], p[1])]

print(f"pairs: before={len(before)} after={len(after)} removed={len(removed)} added={len(added)}")
print("removed pairs (each must be same-file span containment, non-kind):")
for a, b in removed:
    fa, sa, ea = spans[a]
    fb, sb, eb = spans[b]
    print(f"  ({a},{b}) kinds=({kinds[a]},{kinds[b]}) file={fa.split('/')[-1]} "
          f"outer=({sa},{ea}) inner=({sb},{eb})")
print(f"removed pairs NOT span-nested: {len(bad)}")
print(f"removed pairs that are container×member (kind bucket — must be 0): {len(kind_bucket)}")
print(f"pairs whose tier/sim changed while kept: {len(kept_diff)}")
for p in bad[:5]:
    print("  BAD", p)
for p in added[:5]:
    print("  ADDED", p)
for p, x, y in kept_diff[:5]:
    print("  KEPT-DIFF", p, x, y)

ok = not bad and not added and not kind_bucket and not kept_diff
print("RESULT:", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
