'use client';
import { AtlasLayoutView } from './atlas-layout-view';
import { AtlasWorkspaceControls } from './atlas-workspace-controls';
/** Onboarding copy and composition only. All data and interactions belong to core. */
export function FresherAtlas() {
    return <div className="p-4 space-y-4" data-testid="atlas-core-fresher">
        <h2 className="font-semibold">Onboarding · explore the captured code</h2>
        <nav aria-label="Atlas onboarding guide" className="rounded border p-3 text-sm space-y-2">
            <p><strong>Start here:</strong> choose a file in File contributors. Every chart will narrow to its recorded contributors.</p>
            <p>Next, choose a function in Selected symbols. Read its captured control flow and static call sequence below.</p>
            <p>Click a selected element again to remove that constraint. Double-click an element to inspect its captured source. You can also remove constraints in the selection bar.</p>
            <p>Use each Chart menu to place any registered chart in any slot. Moving charts keeps your selection.</p>
        </nav>
        <details className="text-xs text-stone-600 rounded border p-2">
            <summary>What this onboarding view covers</summary>
            <p>Files, declaration hierarchy, symbols, symbol counts, native function control flow, and static calls share one storage-backed selection. Declaration hierarchy is not the concept tree. Static calls do not prove runtime execution.</p>
            <p>C4 containers, application and service entry tables, concept hierarchy, and read/write data flow still need core producers. These legacy panels are not included in this slice. Ask CIE currently uses its separate chat context.</p>
        </details>
        <AtlasWorkspaceControls />
        <AtlasLayoutView />
    </div>;
}
