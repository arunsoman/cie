import { containerView, componentView } from './atlas-architecture';
import { routeView, applicationEntryView } from './atlas-entries';
import { conceptView } from './atlas-concepts';
import { hierarchyView, controlFlowView, sequenceView } from './atlas-semantic-producers';
import { reviewerViews } from './atlas-projections';
import { standardAtlasRegistry } from './atlas-registry';
import { foundationPlans } from './atlas-plans';
/** One application catalog. Persona components consume plans, not query definitions. */
export function createAtlasCatalog() { const registry = standardAtlasRegistry([...reviewerViews, hierarchyView, controlFlowView, sequenceView, conceptView, routeView, applicationEntryView, containerView, componentView]); return { registry, plans: foundationPlans(registry) }; }
