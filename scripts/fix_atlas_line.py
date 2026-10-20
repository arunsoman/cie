import re

p = '/home/z/my-project/web/src/components/cie/atlas-view.tsx'
s = open(p, encoding='utf-8').read()
i = s.find('const sym = ')
# find the corrupted one specifically
target = None
for m in re.finditer(r'const sym = [^\n]*', s):
    txt = m.group(0)
    if '.main' in txt and '"main"' in txt:
        target = m
        break
assert target, "corrupted line not found"
line = target.group(0)
print("BEFORE:", [hex(ord(c)) for c in line[:24]])
print(repr(line))
fixed = 'const sym = [m.name, `${m.name}.main`, "main"]'
s2 = s[:target.start()] + fixed + s[target.end():]
open(p, 'w', encoding='utf-8').write(s2)
print("replaced with:", fixed)
