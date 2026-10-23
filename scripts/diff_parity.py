import json, re, sys

raw = open('/home/z/my-project/tool-results/bash_1792751750911_7da59ebbbf21.txt').read()
# The panic message embeds two Rust-escaped JSON strings: left: "..." right: "..."
m = re.search(r'left: "(.*?)", right: "(.*?)"\n\nfailures:', raw, re.S)
if not m:
    # fallback: left: "...."  right: "...." at end of block
    m = re.search(r'left: "(.*)"\n\s*right: "(.*)"\s*\n\s*note:', raw, re.S)
if not m:
    print("COULD NOT EXTRACT"); sys.exit(1)

def unesc(s):
    return s.encode().decode('unicode_escape')

left = json.loads(unesc(m.group(1)))
right = json.loads(unesc(m.group(2)))

diffs = 0
def walk(a, b, path):
    global diffs
    if diffs >= 12: return
    if type(a) != type(b):
        print(f"TYPE {path}: {type(a).__name__} vs {type(b).__name__}"); diffs += 1; return
    if isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a: print(f"ONLY-RIGHT {path}.{k}"); diffs += 1
            elif k not in b: print(f"ONLY-LEFT {path}.{k}"); diffs += 1
            else: walk(a[k], b[k], f"{path}.{k}")
    elif isinstance(a, list):
        if len(a) != len(b):
            print(f"LEN {path}: {len(a)} vs {len(b)}"); diffs += 1
            for i in range(min(len(a), len(b))):
                walk(a[i], b[i], f"{path}[{i}]")
        else:
            for i, (x, y) in enumerate(zip(a, b)):
                walk(x, y, f"{path}[{i}]")
    else:
        if a != b:
            print(f"VALUE {path}: {a!r} vs {b!r}"); diffs += 1

walk(left, right, "$")
print(f"total shown diffs: {diffs}")
