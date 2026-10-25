const fs=require('node:fs/promises'),path=require('node:path');
/** Test-only synthetic captured measurements, never a production collector. */
module.exports=async root=>{
 const target=path.join(root,'.cie','revs','1','web-bundle.json'),bundle=JSON.parse(await fs.readFile(target,'utf8'));
 bundle.quality={project:'QA',collected_at:'2026-10-25T00:00:00Z',honesty:'Synthetic captured QA measurements',scopes:{sample:'QA fixture'},
 coverage_heatmap:{files:[{file:'Payments.java',percent:70},{file:'a.py',percent:0}]},complexity:{files:[{file:'Payments.java',avg_cc:4,max_cc:8},{file:'a.py',avg_cc:1,max_cc:1}]},
 hotspots:{formula:'captured fixture score',rows:[{file:'Payments.java',risk:90},{file:'a.py',risk:0}]},
 duplication:{pairs:[{a_file:'Payments.java',b_file:'Payments.java',a_line:1,b_line:2,lines:2}],scanned_files:['Payments.java','a.py']},
 security:{issues:[{file:'Payments.java',id:'qa-1'}],scanned_files:['Payments.java','a.py']}};
 await fs.writeFile(target,JSON.stringify(bundle));return bundle.quality;
};
