#!/usr/bin/env python3
"""Regenerate static evidence with the actual CLI, never manufacture runtime data."""
import argparse,json,os,pathlib,shutil,subprocess
root=pathlib.Path(__file__).resolve().parents[1]
binary=os.environ.get('CIE_BINARY',str(root/'target/debug/cie'))
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--fresh',action='store_true',help='Discard only the three bundled samples’ generated .cie analysis caches before regeneration')
options=parser.parse_args()
def run(args,cwd=root):
 p=subprocess.run([binary,*map(str,args)],cwd=cwd,capture_output=True,text=True,timeout=120)
 if p.returncode:raise RuntimeError(f'{args[0]} failed: {p.stderr[:1000]}')
 return p.stdout
def save(path,value):path.write_text(json.dumps(value,indent=1,ensure_ascii=False)+'\n')
for source,destination in [('examples/bankapp','web/public/data'),('samples/spring','web/public/data/spring'),('samples/polyglot','web/public/data/polyglot')]:
 project=root/source;out=root/destination;out.mkdir(parents=True,exist_ok=True)
 if options.fresh and (project/'.cie').is_dir():shutil.rmtree(project/'.cie')
 run(['ingest',project])
 report=json.loads(run(['report',project,'--json']))
 save(out/'report.json',report)
 for name,args in [('fault',['fault',project,'--json']),('stack',['stack',project,'--json']),('concepts',['concepts',project,'--format','json']),('deps',['deps',project,'--json']),('overlap',['overlap',project,'--json'])]:save(out/f'{name}.json',json.loads(run(args)))
 run(['viz',project,'--out',out/'viewspec.json'])
 # Include only source files named by this exact report. Missing stays null.
 files={rel:((project/rel).read_text() if (project/rel).is_file() and (project/rel).stat().st_size<=512*1024 else None) for rel in report['files']}
 save(out/'sources.json',dict(project=source,note='Exact static source inputs; runtime captures are separately supplied and not regenerated.',files=files))
 docs=list(project.glob('*.md'))
 if docs:save(out/'drift.json',json.loads(run(['drift',*[d.name for d in docs],'--code',project,'--format','json'],cwd=project)))
 print(f'{source}: revision {report["rev"]}, {len(files)} files, static artifacts refreshed')
