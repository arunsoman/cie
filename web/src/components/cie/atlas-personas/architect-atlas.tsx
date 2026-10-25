'use client';
import { AtlasLayoutView } from './atlas-layout-view';
import { AtlasWorkspaceControls } from './atlas-workspace-controls';
/** Persona copy and layout composition; queries and gestures stay in core. */
export function ArchitectAtlas() {
    return <div className="p-4 space-y-4" data-testid="atlas-core-architect">
        <h2 className="font-semibold">Architect · captured architecture orientation</h2>
        <p className="text-sm">Choose an application container or component to explore its captured code. Every analytical view shares the selection; click again to remove a constraint, or double-click to inspect representative source.</p>
        <details className="text-xs text-stone-600 rounded border p-2">
            <summary>Architecture evidence and remaining coverage</summary>
            <p>C4 application boundaries come from captured build records. Components include captured framework roles and stored generated groupings. Architectural grouping remains an interpretation; deployment and runtime communication are unverified. Missing external systems and relationships are not invented.</p>
            <p>This orientation slice does not complete the Architect PRD: dependency analysis, duplication, drift, timelines and risk charts still need shared producers. The scorecard shows captured quality measurements and explicit collection gaps. Ask CIE currently uses its separate chat context.</p>
        </details>
        <AtlasWorkspaceControls />
        <AtlasLayoutView />
    </div>;
}
