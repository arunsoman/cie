'use client';
import {useAtlasDetails} from './atlas-display';
import { useEffect, useMemo, useRef, useState } from 'react';
import type { RenderHandle } from 'viz-adapter';
import type { AtlasViewSpec } from '@/lib/atlas-core';
import { atlasVisualization } from '@/lib/atlas-core/atlas-visualization';
import { useAtlas, useAtlasState } from './atlas-provider';
import { DiagramNavigation } from '../diagram-navigation';
import { g6PresentationOptions } from '@/lib/viz/atlas-host-profiles';
export function AtlasCanvas({ spec, disabled }: {spec: AtlasViewSpec; disabled: boolean}) {
    const {core, interaction} = useAtlas(), state = useAtlasState();
    const details=useAtlasDetails(), canvasHeight=state.persona==='architect'?330:420;
    const container = useRef<HTMLDivElement>(null), handle = useRef<RenderHandle | null>(null);
    const [render, setRender] = useState({hash: '', status: 'loading', notice: 'Preparing visualization…'});
    const compiled = useMemo(() => {try {return {view: atlasVisualization(spec, state.pin), error: null};} catch(e) {return {view: null, error: String(e)};}}, [spec, state.pin]);
    const view = compiled.view, hash = view?.renderHash ?? '';
    const current = render.hash === hash ? render : {hash, status: 'loading', notice: 'Preparing visualization…'};
    const enabled = useRef(!disabled), latestHash = useRef(hash);
    enabled.current = !disabled; latestHash.current = hash;
    useEffect(() => {
        let stopped = false, owned: RenderHandle | undefined, off: (() => void) | undefined;
        if (!container.current || !view || !view.elements.length) {
            container.current?.replaceChildren();
            setRender({hash, status: 'unavailable', notice: compiled.error ?? 'No recorded canvas elements. Use the accessible outline.'});
            return;
        }
        // Every asynchronous render owns its own DOM host. A late disposal or
        // render from a previous chart can never clear or repaint the next one.
        const stage = document.createElement('div');
        stage.style.width = '100%'; stage.style.height = `${canvasHeight}px`;
        container.current.replaceChildren(stage);
        setRender({hash, status: 'loading', notice: 'Preparing visualization…'});
        void import('@/lib/viz/g6-engine').then(async ({renderViaNegotiator}) => {
            if (stopped) return;
            const result = await renderViaNegotiator(stage, view, {...g6PresentationOptions(spec.presentation?.hostExtension, spec.semantic?.kind ?? 'dependency', view), requiredCapabilities: {interactive: true, customNodes: true, customEdges: spec.semantic?.kind === 'sequence', customLayouts: spec.semantic?.kind === 'hierarchy'}});
            owned = result.handle;
            if (stopped) {await owned.dispose(); stage.remove(); return;}
            handle.current = owned;
            off = owned.onInteraction?.(event => {
                if (!enabled.current || latestHash.current !== hash) return;
                const item = spec.items.find(i => i.key === event.id)?.ref ?? spec.links?.find(e => e.key === event.id)?.ref;
                if (item) {if (event.type === 'doubleClick') interaction.doubleClick(item); else interaction.click(item, event.detail);}
            });
            setRender({hash, status: 'ready', notice: result.disclosure.headline});
        }).catch(e => {if (!stopped) setRender({hash, status: 'unavailable', notice: `Canvas unavailable: ${String(e)}. Use the accessible outline.`});});
        return () => {
            stopped = true; off?.();
            if (handle.current === owned) handle.current = null;
            stage.remove(); void owned?.dispose();
        };
    }, [view, spec, interaction, compiled.error, hash, canvasHeight]);
    useEffect(() => {
        if (current.status === 'ready') handle.current?.setSelected([...spec.items, ...(spec.links ?? [])].filter(i => core.isSelected(i.ref.id) || state.focusedItemId === i.ref.id).map(i => i.key), 'programmatic');
    }, [state.selection, state.focusedItemId, spec, core, current.status]);
    return <><div ref={container} role="img" aria-label={spec.title} data-render-state={current.status} data-render-hash={hash} className="border rounded" style={{minHeight:canvasHeight,visibility: current.status === 'ready' ? 'visible' : 'hidden'}}/>{details&&<DiagramNavigation handle={handle}/>}{(details||current.status!=='ready')&&<p role="status" className="text-xs">{current.notice}</p>}</>;
}
