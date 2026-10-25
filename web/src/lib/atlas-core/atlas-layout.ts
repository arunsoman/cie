export interface PanelSlot {
    viewId: string;
    area: string;
    minHeightPx: number;
    isHero?: boolean;
    chrome: 'card' | 'canvas' | 'table-dense';
}
export interface AtlasLayout {
    id: string;
    columns: 8;
    rowHeightPx: number;
    areas: string[];
    areasMd: string[];
    areasBase: string[];
    slots: PanelSlot[];
    dock: {
        defaultOpen: boolean;
        widthPx: number;
    };
}
/** Validate every breakpoint, including CSS's rectangular named-area rule. */
export function assertLayout(layout: AtlasLayout) {
    if (layout.columns !== 8 || layout.rowHeightPx <= 0 || layout.slots.filter(s => s.isHero).length !== 1)
        throw new Error('Atlas layout requires eight columns and one hero');
    const names = layout.slots.map(s => s.area);
    if (new Set(names).size !== names.length || names.some(n => !/^[a-z][a-z0-9]*$/.test(n)))
        throw new Error('Invalid layout areas');
    for (const [rows, width] of [[layout.areas, 8], [layout.areasMd, 2], [layout.areasBase, 1]] as const) {
        const cells = rows.map(row => row.trim().split(/\s+/));
        if (!cells.length || cells.some(r => r.length !== width) || cells[0].some(c => c !== 'r4'))
            throw new Error('Layout requires full-width R4 first row');
        for (const row of cells)
            for (const token of row)
                if (token !== '.' && !names.includes(token) && token !== 'r4')
                    throw new Error(`Unmapped layout area ${token}`);
        for (const name of [...names, 'r4']) {
            const points = cells.flatMap((r, y) => r.flatMap((c, x) => c === name ? [{ x, y }] : []));
            if (!points.length)
                throw new Error(`Missing layout area ${name}`);
            const xs = points.map(p => p.x), ys = points.map(p => p.y);
            if ((Math.max(...xs) - Math.min(...xs) + 1) * (Math.max(...ys) - Math.min(...ys) + 1) !== points.length)
                throw new Error(`Nonrectangular area ${name}`);
        }
    }
    if (layout.dock.widthPx < 160)
        throw new Error('Dock too narrow');
}
export const reviewerLayout: AtlasLayout = { id: 'reviewer.reference', columns: 8, rowHeightPx: 110,
    areas: ['r4 r4 r4 r4 r4 r4 r4 r4', 'hero hero hero hero hero kinds kinds kinds', 'hero hero hero hero hero kinds kinds kinds', 'hero hero hero hero hero kinds kinds kinds', 'files files files tree tree tree tree tree', 'files files files tree tree tree tree tree', 'files files files tree tree tree tree tree', 'flow flow flow flow seq seq seq seq', 'flow flow flow flow seq seq seq seq', 'flow flow flow flow seq seq seq seq', 'flow flow flow flow seq seq seq seq', 'calls calls calls calls calls calls calls calls', 'calls calls calls calls calls calls calls calls', 'calls calls calls calls calls calls calls calls'],
    areasMd: ['r4 r4', 'hero hero', 'hero hero', 'kinds files', 'tree tree', 'tree tree', 'flow flow', 'flow flow', 'seq seq', 'seq seq', 'calls calls', 'calls calls'], areasBase: ['r4', 'hero', 'kinds', 'files', 'tree', 'flow', 'seq', 'calls'],
    slots: [{ viewId: 'symbols', area: 'hero', minHeightPx: 330, isHero: true, chrome: 'table-dense' }, { viewId: 'kinds', area: 'kinds', minHeightPx: 220, chrome: 'card' }, { viewId: 'files', area: 'files', minHeightPx: 330, chrome: 'card' }, { viewId: 'hierarchy', area: 'tree', minHeightPx: 330, chrome: 'canvas' }, { viewId: 'controlflow', area: 'flow', minHeightPx: 440, chrome: 'canvas' }, { viewId: 'sequence', area: 'seq', minHeightPx: 440, chrome: 'canvas' }, { viewId: 'calls', area: 'calls', minHeightPx: 330, chrome: 'canvas' }], dock: { defaultOpen: true, widthPx: 280 } };
