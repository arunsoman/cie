'use client';
import {AtlasSelectionBar,AtlasRegisteredView,AtlasEvidencePanel} from './atlas-views';
/** Persona composition only. No storage, selection reducer or event handlers. */
export function ReviewerAtlas(){return <div className="p-4 space-y-4" data-testid="atlas-core-reviewer">
  <h2 className="font-semibold">Reviewer · shared Atlas selection</h2>
  <p className="text-xs text-stone-600">Selection applies to the views below. Ask CIE currently uses its separate chat context.</p>
  <AtlasSelectionBar/>
  <div className="grid grid-cols-1 xl:grid-cols-3 gap-4">
    <div className="xl:col-span-2"><AtlasRegisteredView id="symbols"/></div>
    <AtlasRegisteredView id="kinds"/>
    <div className="xl:col-span-2"><AtlasRegisteredView id="calls"/></div>
    <AtlasRegisteredView id="files"/>
  </div>
  <AtlasEvidencePanel/>
</div>;}
