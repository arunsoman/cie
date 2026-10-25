'use client';
import { createContext, useContext, useEffect, useState, useSyncExternalStore, type ReactNode } from 'react';
import { createAtlasCore, type AtlasCore, type AtlasState } from '@/lib/atlas-core/atlas-core';
import { atlasStorage } from '@/lib/atlas-core/atlas-storage';
import { createAtlasCatalog } from '@/lib/atlas-core/atlas-catalog';
import { createAtlasInteraction } from '@/lib/atlas-core/atlas-interaction';
const Context = createContext<{
    core: AtlasCore;
    interaction: ReturnType<typeof createAtlasInteraction>;
    root: string;
    catalog: ReturnType<typeof createAtlasCatalog>;
} | null>(null);
export function AtlasProvider({ root, revision, persona, children }: {
    root: string;
    revision: string;
    persona: string;
    children: ReactNode;
}) {
    const [loaded, setLoaded] = useState<{
        core: AtlasCore;
        interaction: ReturnType<typeof createAtlasInteraction>;
        root: string;
        catalog: ReturnType<typeof createAtlasCatalog>;
        key: string;
    } | null>(null), [error, setError] = useState('');
    useEffect(() => {
        const catalog = createAtlasCatalog(), storage = atlasStorage(root, revision), abort = new AbortController();
        let core: AtlasCore | undefined, interaction: ReturnType<typeof createAtlasInteraction> | undefined;
        setLoaded(null);
        setError('');
        void storage.retrieve({ select: { kind: 'symbol' } }, [{ id: 'probe', output: { entities: { page: { limit: 1 } } } }], abort.signal).then(result => {
            if (abort.signal.aborted)
                return;
            core = createAtlasCore({ pin: { repository: result.repository, revision }, base: { select: { kind: 'symbol' } }, storage, views: catalog.registry.viewsFor(catalog.plans.resolve(persona).views), registry: catalog.registry, plans: catalog.plans, persona });
            interaction = createAtlasInteraction(core);
            setLoaded({ core, interaction, root, catalog, key: JSON.stringify([root, revision]) });
            void core.start();
        }).catch(e => { if (!abort.signal.aborted)
            setError(String(e)); });
        return () => { abort.abort(); interaction?.dispose(); core?.dispose(); if (!core)
            void storage.close().catch(() => { }); };
        // Persona changes are presentation changes; do not recreate the session.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [root, revision]);
    useEffect(() => { loaded?.core.dispatch({ type: 'persona', persona }); }, [loaded, persona]);
    if (error)
        return <p role="alert">Atlas storage unavailable: {error}</p>;
    if (!loaded || loaded.key !== JSON.stringify([root, revision]))
        return <p role="status">Opening captured Atlas revision…</p>;
    return <Context.Provider value={loaded}>{children}</Context.Provider>;
}
export function useAtlas() { const ctx = useContext(Context); if (!ctx)
    throw new Error('Atlas views require AtlasProvider'); return ctx; }
export function useAtlasState(): AtlasState { const { core } = useAtlas(); return useSyncExternalStore(core.subscribe, core.getState, core.getState); }
