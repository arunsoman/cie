import re, ast

log = open('/home/z/my-project/eq2_copy.log', encoding='utf-8').read()

def grab(tag):
    i = log.index(tag)
    j = log.index('\n', i)
    s = log[i + len(tag):j].strip()
    return '[' + s.rstrip(']') + ']'

TUP = re.compile(
    r'\((\d+), (\d+), ([0-9.eE+-]+|NaN|inf(?:inity)?), "([a-z]+)", "([a-z-]+)", "(\w+)", (?:Some\((\d+)\)|None), (?:Some\("([^"]*)"\)|None)\)'
)

def parse(s):
    out = []
    for m in TUP.finditer(s):
        a, b, sim, tier, det, epi, canon, counter = m.groups()
        out.append((int(a), int(b), float(sim), tier, det, epi,
                    int(canon) if canon else None, counter))
    return out

left = parse(grab('  left: ['))
right = parse(grab('  right: ['))
print('left rows:', len(left), ' right rows:', len(right))

ls = {(a, b): r for a, b, *r in left}
rs = {(a, b): r for a, b, *r in right}
print('left unique keys:', len(ls), ' right unique keys:', len(rs))
only_l = sorted(set(ls) - set(rs))
only_r = sorted(set(rs) - set(ls))
print('pairs only in NEW:', only_l[:10])
print('pairs only in REF:', only_r[:10])
diff = [(k, ls[k], rs[k]) for k in sorted(set(ls) & set(rs)) if ls[k] != rs[k]]
print('pairs with differing payload:', len(diff))
for k, a, b in diff[:10]:
    print(' key', k)
    print('   new:', a)
    print('   ref:', b)
