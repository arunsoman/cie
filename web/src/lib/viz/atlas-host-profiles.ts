import type {ViewSpec,RenderOptions} from 'viz-adapter';
import type {SemanticKind} from '../atlas-core/atlas-semantic';
/** Trusted application code may register G6 layouts/shapes/plugins here. Profiles
 * never carry storage queries or replace the semantic data / interaction owner. */
export interface G6PresentationExtension {id:string;kinds:SemanticKind[];options:(view:ViewSpec)=>RenderOptions}
const extensions=new Map<string,G6PresentationExtension>();
export function registerG6PresentationExtension(extension:G6PresentationExtension){if(!extension.id||extensions.has(extension.id)||!extension.kinds.length)throw new Error('Invalid/duplicate G6 presentation extension');extensions.set(extension.id,extension);}
export function g6PresentationOptions(id:string|undefined,kind:SemanticKind,view:ViewSpec):RenderOptions {
 if(!id)return {};const extension=extensions.get(id);if(!extension||!extension.kinds.includes(kind))throw new Error('G6 extension does not support this semantic view');
 const options=extension.options(view);for(const reserved of ['data','container','behaviors','requiredCapabilities','autoFit'])if(reserved in options)throw new Error(`G6 profile cannot override ${reserved}`);return options;
}
