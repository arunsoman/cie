import {duplicationView, driftBoardView, timelineView, riskTreemapView} from './atlas-architect-panels';
import {dependencyView} from './atlas-dependencies';
import {scorecardView} from './atlas-scorecard';
import { containerView, componentView, boundedContextView } from './atlas-architecture';
import { routeView, applicationEntryView } from './atlas-entries';
import { conceptView } from './atlas-concepts';
import { hierarchyView, controlFlowView, sequenceView } from './atlas-semantic-producers';
import { reviewerViews } from './atlas-projections';
import { standardAtlasRegistry } from './atlas-registry';
import { foundationPlans } from './atlas-plans';
/** One application catalog. Persona components consume plans, not query definitions. */
export function createAtlasCatalog() { const registry = standardAtlasRegistry([...reviewerViews, hierarchyView, controlFlowView, sequenceView, conceptView, routeView, applicationEntryView, containerView, componentView, boundedContextView, dependencyView, scorecardView, duplicationView, driftBoardView, timelineView, riskTreemapView]); return { registry, plans: foundationPlans(registry) }; }
