import type {AtlasViewDefinition,AtlasViewSpec} from './atlas-types';
import {assertDefinition} from './atlas-conformance';
export interface RendererCapability {id:string;kinds:string[];interactive:boolean;validate?:(spec:AtlasViewSpec)=>void}
/** Headless registrations. React renderers and layout components live outside core. */
export class AtlasRegistry {
 private views=new Map<string,AtlasViewDefinition>();
 private renderers=new Map<string,RendererCapability>();
 registerRenderer(renderer:RendererCapability){
  if(!renderer.id||!renderer.kinds.length||this.renderers.has(renderer.id))throw new Error('Invalid/duplicate Atlas renderer');
  this.renderers.set(renderer.id,{...renderer,kinds:[...renderer.kinds]});return this;
 }
 registerView(view:AtlasViewDefinition){assertDefinition(view);if(this.views.has(view.id))throw new Error('Duplicate Atlas view');this.views.set(view.id,view);return this;}
 view(id:string){const view=this.views.get(id);if(!view)throw new Error(`Unregistered Atlas view: ${id}`);return view;}
 viewsFor(ids:string[]){return ids.map(id=>this.view(id));}
 renderer(spec:AtlasViewSpec){const candidates=[...this.renderers.values()].filter(r=>r.kinds.includes(spec.kind)&&(!spec.renderer||r.id===spec.renderer));
  if(!candidates.length)throw new Error(`No registered renderer for ${spec.kind}`);const renderer=candidates[0];if(!renderer.interactive&&spec.items.length)throw new Error('Data renderer must support Atlas interactions');renderer.validate?.(spec);return renderer;
 }
}
export function standardAtlasRegistry(views:AtlasViewDefinition[]=[]){const r=new AtlasRegistry()
 .registerRenderer({id:'atlas-table',kinds:['table','pivot'],interactive:true})
 .registerRenderer({id:'atlas-bars',kinds:['bars'],interactive:true})
 .registerRenderer({id:'viz-adapter',kinds:['graph','flow','sequence','tree','treemap','heatmap','flamegraph','chord','timeline','stat'],interactive:true});
 views.forEach(v=>r.registerView(v));return r;
}
