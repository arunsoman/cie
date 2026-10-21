import type { ElementLoc } from './chat-chart';
export interface ChatContextElement {
  id: string;
  label: string;
  kind: string;
  revision: number;
  loc?: ElementLoc;
  /** Only recorded files, resolved at selection time. Empty means no code space. */
  files: string[];
}
export const CHAT_CONTEXT_CAP = 24;
export function addContext(current: ChatContextElement[], next: ChatContextElement): ChatContextElement[] {
  const sameRevision = current.filter(c => c.revision === next.revision);
  return [...sameRevision.filter(c => c.id !== next.id), next].slice(-CHAT_CONTEXT_CAP);
}
