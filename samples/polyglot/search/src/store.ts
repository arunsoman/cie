// the module that knows where the catalogue lives.
export interface Berth {
  code: string;
  lengthMeters: number;
  occupied: boolean;
}

const berths: Berth[] = [
  { code: "A1", lengthMeters: 12, occupied: false },
  { code: "A2", lengthMeters: 15, occupied: true },
  { code: "B1", lengthMeters: 20, occupied: false },
];

export function query(minLength: number): Berth[] {
  return berths.filter((b) => b.lengthMeters >= minLength);
}

export function save(code: string): Berth {
  const found = berths.find((b) => b.code === code);
  if (!found) {
    throw new Error("no berth " + code);
  }
  found.occupied = true;
  return found;
}
