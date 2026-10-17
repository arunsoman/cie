export function handler(req: Request): boolean {
  const q = req.query.q;
  db.query("SELECT * FROM t WHERE q = " + q);
  return true;
}
