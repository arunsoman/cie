'use client';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { RenderHandle } from 'viz-adapter';
import type { AtlasViewSpec } from '@/lib/atlas-core';
import { atlasVisualization } from '@/lib/atlas-core/atlas-visualization';
import { useAtlas, useAtlasState } from './atlas-provider';
import { DiagramNavigation } from '../diagram-navigation';
import { g6PresentationOptions } from '@/lib/viz/atlas-host-profiles';
export function AtlasCanvas({ spec, disabled }: {
    spec: AtlasViewSpec;
    disabled: boolean;
}) {
    const { core, interaction } = useAtlas(), state = useAtlasState(), container = useRef<HTMLDivElement>(null), handle = useRef<RenderHandle | null>(null), [notice, setNotice] = useState('Preparing visualization…');
    const compiled = useMemo(() => { try {
        return { view: atlasVisualization(spec, state.pin), error: null };
    }
    catch (e) {
        return { view: null, error: String(e) };
    } }, [spec, state.pin]);
    const view = compiled.view;
    const enabled = useRef(!disabled);
    enabled.current = !disabled;
    useEffect(() => {
        let stopped = false, owned: RenderHandle | undefined, off: (() => void) | undefined;
        if (!container.current || !view || !view.elements.length) {
            setNotice(compiled.error ?? 'No recorded canvas elements. Use the accessible outline.');
            return;
        }
        void import('@/lib/viz/g6-engine').then(async ({ renderViaNegotiator }) => {
            const result = await renderViaNegotiator(container.current!, view, { ...g6PresentationOptions(spec.presentation?.hostExtension, spec.semantic?.kind ?? 'dependency', view), requiredCapabilities: { interactive: true, customNodes: true, customEdges: spec.semantic?.kind === 'sequence', customLayouts: spec.semantic?.kind === 'hierarchy' } });
            owned = result.handle;
            if (stopped) {
                await owned.dispose();
                return;
            }
            handle.current = owned;
            off = owned.onInteraction?.(event => { if (!enabled.current)
                return; const item = spec.items.find(i => i.key === event.id)?.ref ?? spec.links?.find(e => e.key === event.id)?.ref; if (item) {
                if (event.type === 'doubleClick')
                    interaction.doubleClick(item);
                else
                    interaction.click(item, event.detail);
            } });
            setNotice(result.disclosure.headline);
        }).catch(e => { if (!stopped)
            setNotice(`Canvas unavailable: ${String(e)}. Use the accessible outline.`); });
        return () => { stopped = true; off?.(); handle.current = null; void owned?.dispose(); };
    }, [view, spec, interaction, compiled.error]);
    useEffect(() => { handle.current?.setSelected([...spec.items, ...(spec.links ?? [])].filter(i => core.isSelected(i.ref.id) || state.focusedItemId === i.ref.id).map(i => i.key), 'programmatic'); }, [state.selection, state.focusedItemId, spec, core]);
    return <><div ref={container} role="img" aria-label={spec.title} className="h-[420px] border rounded"/><DiagramNavigation handle={handle}/><p role="status" className="text-xs">{notice}</p></>;
}
