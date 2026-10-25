'use client';
import { AtlasLayoutView } from './atlas-layout-view';
import { AtlasWorkspaceControls } from './atlas-workspace-controls';
/** Persona composition only: semantic producers, profiles and layout are registered. */
export function ReviewerAtlas() {
    return <div className="p-4 space-y-4" data-testid="atlas-core-reviewer">
 <h2 className="font-semibold">Reviewer · shared Atlas selection</h2>
 <p className="text-xs text-stone-600">Selection applies to the views below. Ask CIE currently uses its separate chat context.</p>
 <AtlasWorkspaceControls />
 <AtlasLayoutView />
    </div>;
}
