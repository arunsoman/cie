'use client';
import { AtlasLayoutView } from './atlas-layout-view';
import { AtlasWorkspaceControls } from './atlas-workspace-controls';
/** Onboarding copy and composition only. All data and interactions belong to core. */
export function FresherAtlas() {
    return <div className="p-4 space-y-4" data-testid="atlas-core-fresher">
        <h2 className="font-semibold">Onboarding · explore the captured code</h2>
        <details className="text-xs text-stone-600 rounded border p-2">
            <summary>What this onboarding view covers</summary>
            <p>Files, generated concepts, declaration hierarchy, symbols, symbol counts, native function control flow, and static calls share one storage-backed selection. Concept interpretations are separate from captured declaration structure. Static calls do not prove runtime execution.</p>
            <p>Application and service entry tables show uniquely resolved captured targets; unsupported or ambiguous entries remain excluded from analytical scope. C4 application/container and component charts are available from the Chart menus. Read/write data flow still needs a core producer. These legacy panels are not included in this slice. Ask CIE currently uses its separate chat context.</p>
        </details>
        <AtlasWorkspaceControls />
        <AtlasLayoutView />
    </div>;
}
