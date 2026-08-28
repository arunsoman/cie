# Adding a language cie has no built-in support for

**29 Aug 2026.** cie ships tree-sitter extraction for Python, JavaScript,
TypeScript, and Java out of the box — every other language needs an
adapter. This doc is that worked example: a real, runnable
`LanguageAdapter` for a language cie has never heard of, verified end to
end, not a sketch.

Two ways to get here, same registry either side:

- **This doc** — a from-scratch adapter with no existing parser at all
  (regex over a toy grammar), when nothing else already understands your
  language.
- **The `nirdosha` adapter** (see `docs/competitive-landscape.md`'s
  strengths section) — the same registry, but wrapping an *existing*
  compiler's own AST dump (`nirdosha emit-ast`) instead of writing a
  parser from scratch. If your language already has a compiler, LSP
  server, or tree-sitter grammar you can shell out to or import, prefer
  wrapping that over writing a new parser — the registry doesn't care
  which.

## The whole interface

`cie.lang_adapter.LanguageAdapter` is two methods:

```python
class LanguageAdapter(Protocol):
    def supported_suffixes(self) -> set[str]: ...
    def extract_file(self, path: Path) -> Extraction: ...
```

`Extraction` is plain dicts — `nodes` (one per file/class/function/method,
schema: `id`, `label`, `source_file`, `source_location`, `kind`,
`signature`, `line_start`, `line_end`, `docstring`, `decorators`) and
`edges` (`source`, `target`, `relation`, `confidence`). That's the same
shape `cie/extract.py`'s tree-sitter walker already produces — an adapter
just needs to fill it in by whatever means its language supports.

## The worked example

[`examples/adapters/toy_regex_adapter.py`](../examples/adapters/toy_regex_adapter.py)
is a complete adapter for a fictional `.toy` language (`fn name(args):`
definitions, nothing else) using a regex instead of a real parser — the
point is that *no tree-sitter grammar, no LSP, and no cie/ code change*
are required, not that regex is a good parsing strategy in general (it
isn't, past toy syntax; a real adapter for a real language should use
that language's own tokenizer/parser/compiler where one exists).

Run it yourself:

```bash
cd cie   # repo root
PYTHONPATH=. python examples/adapters/toy_regex_adapter.py
```

Verified output (checked into the file's own docstring so it can't drift
silently from what the code actually does):

```
3 nodes, 2 edges
  FILE   hello.toy
  FUNC   greet      fn greet(name):
  FUNC   main       fn main():
```

## The two registration paths

1. **Explicit, in your own code**: `cie.lang_adapter.register_adapter(MyAdapter())`
   before you call anything in `cie.extract`/`cie.query`/`cie index`.
   Later registrations win over earlier ones for an overlapping suffix,
   so this also works to override the built-in tree-sitter adapter for a
   suffix you want special handling for.
2. **Entry point, for a pip-installable package**: register a
   `cie.language_adapters` entry point in your package's `pyproject.toml`
   pointing at a zero-arg factory function returning your adapter — cie
   discovers it automatically via `importlib.metadata`, no explicit call
   needed, the same way a plugin would hook into any entry-point-based
   registry.

## What this doesn't cover

`cie.lang_adapter`'s own module docstring flags this honestly: only
`cie.extract`'s Extraction-producing path is adapter-based today. A few
consumers (`cie.source_analysis`, `cie.sync`) call
`cie.extract.parse_file` directly for the raw tree-sitter tree, not just
the walked `Extraction` — those stay tree-sitter-only for now, so a
non-tree-sitter adapter's files are invisible to them the same way any
other unsupported suffix already is. Closing that second seam is a
separate, tracked piece of work, not something this adapter pattern
already solves end to end.
