const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const load=require('./helpers/load-ts.cjs')(),get=name=>load(path.resolve(__dirname,'../src/lib/atlas-core/'+name+'.ts'));
const {riskRectangles,driftBoardView}=get('atlas-architect-panels'),{dependencyView}=get('atlas-dependencies');
const pin={repository:'r',revision:'1'},context={pin,query:{select:{kind:'container'}},version:1};
test('risk treemap preserves area proportions, excludes unknown/zero area and does not overlap',()=>{
 const rectangles=riskRectangles([{key:'a',value:9},{key:'b',value:3},{key:'zero',value:0},{key:'unknown'},{key:'c',value:2}]);
 assert.equal(rectangles.length,3);const weights={a:9,b:3,c:2};
 for(const rect of rectangles){assert.ok(Math.abs(rect.width*rect.height/10000-weights[rect.key]/14)<1e-9);assert.ok(rect.x>=0&&rect.y>=0&&rect.x+rect.width<=100.00001&&rect.y+rect.height<=100.00001);}
 for(let i=0;i<rectangles.length;i++)for(let j=i+1;j<rectangles.length;j++){const a=rectangles[i],b=rectangles[j];assert.ok(a.x+a.width<=b.x+1e-9||b.x+b.width<=a.x+1e-9||a.y+a.height<=b.y+1e-9||b.y+b.height<=a.y+1e-9);}
});
test('dependency graph keeps zero-edge modules and declares static evidence rather than runtime calls',()=>{
 const response={...pin,matches:2,outputs:[{id:'graph',schema_version:2,status:'ready',data:{nodes:[{id:'empty',kind:'container',fields:{name:'empty'}},{id:'app',kind:'container',fields:{name:'app'}},{id:'dep',kind:'dependency',fields:{name:'lib',source:'pom.xml'}}],edges:[{source:'app',target:'dep',relation:'declares_dependency'}]}}]};
 const view=dependencyView.project(response,context);assert.ok(view.items.some(i=>i.key==='empty'));assert.equal(view.semantic.kind,'dependency');assert.equal(view.links[0].label,'Declares');assert.equal(view.links[0].ref.entityKind,'dependency');assert.deepEqual(view.items.find(i=>i.key==='dep').ref.evidence,[{file:'pom.xml'}]);assert.match(view.gaps.join(' '),/do not prove runtime/);
});
test('drift uses recorded document and code references and never treats incomplete verification as clean',()=>{
 const response={...pin,matches:1,outputs:[{id:'rows',schema_version:2,status:'ready',data:{total:1,continuation:null,items:[{id:'claim',fields:{claim:'must save',status:'Partial',complete:false,document:{doc:'README.md',line:3},sources:[{location:{file:'App.java',start:5,end:20}}]}}]}}]};
 const view=driftBoardView.project(response,context);assert.equal(view.items[0].ref.evidence.length,2);assert.equal(view.items[0].fields.status,'Partial');assert.match(view.gaps.join(' '),/incomplete/);
});
test('Architect layout follows the PRD seven-panel grid and registers shared producers',()=>{
 const catalog=get('atlas-catalog').createAtlasCatalog(),layout=catalog.plans.resolve('architect').layout;
 assert.equal(layout.id,'architect.default');assert.deepEqual(layout.slots.map(s=>[s.area,s.viewId]),[['hero','depgraph'],['bctx','boundedcontexts'],['score','scorecard'],['dup','duplication'],['drift','driftboard'],['time','timeline'],['tree','risktreemap']]);
 assert.equal(layout.areas.length,12);assert.equal(layout.dock.widthPx,280);
});
