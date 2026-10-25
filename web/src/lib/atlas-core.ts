/** Public Atlas API. Persona files compose registered views; renderers emit commands. */
export * from './atlas-core/atlas-core';
export {createAtlasInteraction} from './atlas-core/atlas-interaction';
export {assertDefinition,assertView} from './atlas-core/atlas-conformance';

export type {PopulationSpec,SemanticViewSpec,SemanticEvidence,VisualBinding} from './atlas-core/atlas-semantic';
export {compileSemanticView} from './atlas-core/atlas-visualization';
export {presentations,PresentationRegistry,planPresentation} from './atlas-core/atlas-presentation';
export type {PresentationSpec,PresentationProfile} from './atlas-core/atlas-presentation';
export {AtlasRegistry,standardAtlasRegistry} from './atlas-core/atlas-registry';
export {AtlasPlanRegistry} from './atlas-core/atlas-plans';
