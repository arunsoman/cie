import { hierarchyView, controlFlowView, sequenceView } from './atlas-semantic-producers';
import { reviewerViews } from './atlas-projections';
import { standardAtlasRegistry } from './atlas-registry';
import { foundationPlans } from './atlas-plans';
/** One application catalog. Persona components consume plans, not query definitions. */
export function createAtlasCatalog() { const registry = standardAtlasRegistry([...reviewerViews, hierarchyView, controlFlowView, sequenceView]); return { registry, plans: foundationPlans(registry) }; }
