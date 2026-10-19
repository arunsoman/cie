// the search API — express routes wired to the feature service.
import express, { Request, Response } from "express";
import { searchBerths, bookByCode } from "./catalog";

const app = express();

app.get("/berths/search", (req: Request, res: Response) => {
  const minLength = Number(req.query.min_length ?? "0");
  try {
    res.json(searchBerths(minLength));
  } catch (e) {
    res.status(400).json({ error: String(e) });
  }
});

app.post("/berths/:code/book", (req: Request, res: Response) => {
  try {
    res.json(bookByCode(req.params.code));
  } catch (e) {
    res.status(409).json({ error: String(e) });
  }
});

app.listen(3000, () => {
  console.log("search up on 3000");
});
