import type { ElementLoc } from "./chat-chart";
export interface ChatContextElement {
  id: string;
  label: string;
  kind: string;
  revision: number;
  loc?: ElementLoc;
  /** Only recorded files, resolved at selection time. Empty means no code space. */
  files: string[];
  /** The panel/chart the chip was picked from (RFC-0024 stage 4), shown on the chip. */
  sourceTitle?: string;
}
export const CHAT_CONTEXT_CAP = 24;
export function addContext(
  current: ChatContextElement[],
  next: ChatContextElement,
): ChatContextElement[] {
  const sameRevision = current.filter((c) => c.revision === next.revision);
  return [...sameRevision.filter((c) => c.id !== next.id), next].slice(
    -CHAT_CONTEXT_CAP,
  );
}
/** The store's single add path (RFC-0024 stage 4 disclosures over the RFC-0023
 *  primitive): a repeat click never duplicates — it pulses, and the notice says
 *  so; the cap of 24 evicts the OLDEST chip and says so; chips from an older
 *  project revision drop (stale selections are not code space). */
export function addChipState(current: ChatContextElement[], next: ChatContextElement): { chatContext: ChatContextElement[]; chipNotice: string | null } {
  if (current.some(c => c.id === next.id)) {
    return { chatContext: current, chipNotice: `${next.label} is already in context — pulsed` };
  }
  const sameRevision = current.filter(c => c.revision === next.revision);
  const list = [...sameRevision, next];
  if (list.length > CHAT_CONTEXT_CAP) {
    const evicted = list.shift()!;
    return { chatContext: list, chipNotice: `cap of ${CHAT_CONTEXT_CAP} reached — evicted the oldest chip (${evicted.label})` };
  }
  return { chatContext: list, chipNotice: null };
}
