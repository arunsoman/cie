import type { AtlasEvidencePolicy } from './atlas-types';
import type { AtlasRegistry } from './atlas-registry';
import { assertLayout, reviewerLayout, fresherLayout, architectLayout, type AtlasLayout } from './atlas-layout';
export interface AtlasPersonaPlan {
    id: string;
    layout: AtlasLayout;
    views: string[];
    policy: AtlasEvidencePolicy;
    withChangeContext?: string;
}
export class AtlasPlanRegistry {
    private plans = new Map<string, AtlasPersonaPlan>();
    constructor(private views: AtlasRegistry) { }
    register(plan: AtlasPersonaPlan) { assertLayout(plan.layout); if (this.plans.has(plan.id))
        throw new Error('Duplicate persona plan'); this.views.viewsFor(plan.views); for (const s of plan.layout.slots)
        if (!plan.views.includes(s.viewId))
            throw new Error('Layout references inactive view'); this.plans.set(plan.id, plan); return this; }
    layout(id: string) { const layout = [...this.plans.values()].find(p => p.layout.id === id)?.layout; if (!layout) throw new Error('Unknown saved layout'); return layout; }
    resolve(persona: string, hasChangeContext = false) { const plan = this.plans.get(persona); if (!plan)
        throw new Error(`Unregistered persona plan: ${persona}`); if (hasChangeContext && plan.withChangeContext) {
        const alternate = this.plans.get(plan.withChangeContext);
        if (!alternate)
            throw new Error('Missing change-context plan');
        return alternate;
    } return plan; }
}
/** These are shared infrastructure defaults, not claims of completed persona PRDs. */
export function foundationPlans(registry: AtlasRegistry) { const plans = new AtlasPlanRegistry(registry); for (const id of ['fresher', 'reviewer', 'architect', 'auditor', 'consultant', 'performance'])
    plans.register({ id, layout: id === 'fresher' ? fresherLayout : id === 'architect' ? architectLayout : { ...reviewerLayout, id: `${id}.reference` }, views: ['symbols', 'kinds', 'files', 'calls', 'hierarchy', 'controlflow', 'sequence', 'concepts', 'routes', 'entries', 'containers', 'components', 'scorecard', 'depgraph', 'boundedcontexts', 'duplication', 'driftboard', 'timeline', 'risktreemap'], policy: { factsOnly: id === 'auditor' || id === 'reviewer', locked: id === 'auditor' }, ...(id === 'consultant' ? { withChangeContext: 'reviewer' } : {}) }); return plans; }
