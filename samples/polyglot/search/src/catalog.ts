// the feature service — turns a raw query into a result page.
import { Berth, query, save } from "./store";

export function searchBerths(minLength: number): Berth[] {
  if (minLength <= 0) {
    throw new Error("length must be positive");
  }
  return query(minLength);
}

export function bookByCode(code: string): Berth {
  return save(code);
}
