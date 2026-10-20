/**
 * Typed viz errors (PRD Part II §5 / Part I §2.3): every decline, tamper
 * and substitution is a NAMED, typed value — never a blank canvas.
 */

export type GateErrorCode =
  | 'ELEMENT_WITHOUT_EVIDENCE'
  | 'NON_FACT_WITHOUT_CLAIM'
  | 'HYPOTHESIS_WITHOUT_COUNTER'
  | 'FOG_WITHOUT_REASON'
  | 'EDGE_WITHOUT_EVIDENCE'
  | 'EDGE_UNKNOWN_ENDPOINT'
  | 'DUPLICATE_ELEMENT_ID'
  | 'ELEMENT_CAP_EXCEEDED'
  // RFC-0024 patch 3 gate checks (codes shared with the Rust mirror):
  | 'BODY_LINE_WITHOUT_EVIDENCE'
  | 'ALSO_EVIDENCE_MALFORMED';

export class VizError extends Error {
  readonly code: GateErrorCode;
  readonly elementId: string;

  constructor(code: GateErrorCode, elementId: string) {
    super(`${code} [${elementId}]`);
    this.name = 'VizError';
    this.code = code;
    this.elementId = elementId;
  }
}

/** Typed negotiation failure (Part VIII: if no engine can render the
 *  semantic shape, the shape is not producible — a typed error, never a
 *  silent fallback). */
export class NegotiationError extends Error {
  readonly reason: string;
  readonly request: unknown;

  constructor(reason: string, request: unknown) {
    super(`NO_ELIGIBLE_ENGINE: ${reason}`);
    this.name = 'NegotiationError';
    this.reason = reason;
    this.request = request;
  }
}
