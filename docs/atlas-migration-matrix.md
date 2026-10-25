# Atlas migration matrix

Code inventory: 2026-10-25. Governing rule: [any chart, anywhere](atlas-semantic-architecture.md). A rendered legacy chart is not a migrated core chart. Every analytical producer must send and receive shared selection, retain contributor identity, and support evidence navigation before its old path is retired.

## Persona routing

| Persona | Current route | Delivered slice | Remaining objective gaps |
|---|---|---|---|
| Fresher | `FresherAtlas` → shared core | Guided file → declaration → function exploration; distinct onboarding layout; shared filtering and evidence | Containers, boot/service entries, concepts, data flow, automated tour and start-point ranking |
| Reviewer | `ReviewerAtlas` → shared core | Symbol, file, kind, declaration, native CFG and static call views | Entry/route-led review, read/write effects, findings and change-specific review |
| Architect | `LegacyAtlasView` | Legacy charts; not core conformant | Register container/component/concept and boundary producers, then compose its layout |
| Auditor | `LegacyAtlasView` | Legacy facts preference; core has locked facts policy but this screen is not migrated | Evidence-backed routes, externals, findings, boundary writes and locked policy in rendered UI |
| Consultant | `LegacyAtlasView` | Legacy scoped exploration | Shared producers, busiest-container evidence and change-context composition |
| Performance | No selectable persona in `PERSONA_ORDER` | PRD and foundation plan only | Measurements, performance semantic producers and an explicitly supported UI route |

The five selectable personas are defined in `web/src/lib/atlas-presets.ts`; the sixth has a PRD but is not currently offered by the menu. Migrated slices do not imply full PRD completion.

## Chart-by-chart inventory

Legacy panel membership: F=Fresher, R=Reviewer, A=Architect, U=Auditor, C=Consultant. Membership below records the legacy preset, including panels replaced by the partial core slices above.

| Legacy analytical item | Personas | Existing source | Core status and required work |
|---|---|---|---|
| Containers / C4 | F A U C | `buildC4Spec`, stack apps/externals + report | Missing; register build-evidence entities and contributor mapping. Never relabel declaration hierarchy as C4. |
| Application entries | F R C | stack mains | Missing producer, exact symbol resolution and file fallback evidence |
| Service entries | F R U | stack routes | Missing producer, route → handler mapping and API-root call expansion |
| Components | F R A U C | `buildComponentSpec`, concepts/report | Missing concept/component producer; preserve fallback disclosure |
| Concept hierarchy | F R A U C | `buildConceptTreeSpec`, concepts | Missing. Current core hierarchy is declarations, not concepts; validate generated concept membership/labels separately |
| Flow | F R A U C | `buildFlowSpec`, report/selection | Native function CFG exists in core; aggregate component/API flow not yet migrated |
| Sequence | R A | `callSiteSequence`, report graph | Typed static sequence exists in core; occurrence order, dispatch uncertainty, depth limits disclosed; not runtime proof |
| Data flow | F R A U C | `buildDataFlowSpec`, recorded reads/writes | Missing typed producer and storage relations; do not substitute declared calls |
| Test coverage | Conditional legacy quality footer | quality coverage heatmap | Missing metric producer with file/test contributor mapping |
| Complexity | Conditional legacy quality footer | quality function metrics | Missing metric producer with exact symbol contributors |
| Churn per file | Conditional legacy quality footer | quality churn sample | Missing revision-bounded metric producer |
| Hot spots | Conditional legacy quality footer | quality hotspot rows | Missing ranked metric producer, score provenance and contributor mapping |
| Cross-service evidence | Legacy screen | `CrossServiceEvidence`, stack | Missing registered boundary view and relationship selection |

All legacy quality charts render through `QualityPanel`; their existing click behavior is not proof of universal core conformance. Existing sources are implementation receipts, not a claim that native storage exposes every required relation.

## Registered core catalog (available to either migrated persona in any slot)

| ID | Meaning | Selection / evidence |
|---|---|---|
| files | Symbol counts grouped by captured file | Aggregate contributors; captured file on double-click |
| symbols | Selected symbol table with pagination | Stable symbol IDs; declaration span and file |
| kinds | Selected symbols grouped by kind | Aggregate contributor descriptor; source only where supplied |
| hierarchy | Captured file/declaration parent structure | Entity contributors and recorded parent evidence |
| controlflow | Native IR for one selected function | Blocks/branches point to captured code; unavailable IR is explicit |
| sequence | Structured static call occurrences | Symbols, call evidence and disclosed unresolved boundaries |
| calls | Declared call dependency graph | Symbol/relationship contributors; static inference policy applies |

## Checkpoint acceptance

For each migrated persona: select a file, select a function, remove just the function, remove the file; verify every analytical view population. Double-click must open captured evidence without toggling selection. Duplicate or move a chart while selected; scope must survive. Switch personas and back; scope must survive. Validate semantic chart shape separately from the shared-set invariant.

Do not delete `LegacyAtlasView`, the legacy preset vocabulary or shared builders yet: Architect, Auditor and Consultant still depend on them. Retire each producer only after all callers have migrated. Ask CIE/MCP still need explicit shared-context integration.
