# Does a bigger tool surface hurt tool-selection accuracy?

**Run:** 2026-08-29, one pass. Raised directly by a fair pushback on the
project's own earlier framing, which called cie's 121 tools a "kitchen sink"
that's hard to market — but that
argument is about a human reading a README, not about the actual user of
a tool surface: an agent picking which tool to call. A large, specific
tool surface could just as easily be a real capability edge (the model
spends less reasoning budget improvising a workaround from generic
primitives) — and the honest question is whether the *cost* side of that
tradeoff (picking the wrong tool out of many) is real or assumed. This
measures it instead of asserting it either way.

## Methodology

**Ground truth, locked before running anything:** 14 natural-language
tasks, each with one predetermined correct tool from cie's real,
generated `tools/list` schemas (`cie.tool_schema.tool_schemas`,
readonly/`inspector`-policy set — the 81-tool read-only surface as of
the 08-30 run (a dated snapshot of an earlier, smaller surface; the
live read-only count has since grown with the R1 task/QA write-back
promotions — the *selection* finding is about relative surface size,
not the exact count),
descriptions truncated exactly as the real schema generation produces
them, not edited for this test). Several tasks were
deliberately chosen to be genuinely hard: cie's own tool surface has 5
different "coverage"-named tools (`coverage_gaps`, `test_coverage`,
`traceability_coverage`, `prd_traceability_coverage`,
`unified_coverage_report`, `element_coverage`) and two "who calls this"
tools (`callers` — static graph — vs `actual_callers` — runtime traces)
that a real agent has to disambiguate correctly, not just recognize a
category.

**Two conditions, same 14 tasks:**
- **full-81** — every readonly tool cie actually exposes.
- **core-14** — only the 14 tools that are the correct answers, a proxy
  for "what if cie shipped roughly CodeGraphContext's tool count (14)
  instead."

**Subject:** a fresh general-purpose agent (no prior context of this
project or conversation — a new `Agent` call per condition, not a fork,
specifically to avoid the bias of an agent that already knows cie's
internals) given only the tool JSON and the 14 tasks, asked to name the
one tool it would call per task — the selection step alone, not
execution, not argument construction.

## Results

| Condition | Tools available | Correct | Score |
|---|---|---|---|
| full-81 | 81 | 14 / 14 | **100%** |
| core-14 | 14 | 14 / 14 | **100%** |

Every task, both conditions, exact same tool chosen — including the
deliberately hard ones (`callers` vs `actual_callers`; the specific one
of five coverage-shaped tools that actually matched each phrasing).

## What this actually shows

**The hypothesis that prompted this test — that a large read-only tool
costs selection accuracy relative to a 14-tool subset — did not hold up.** Zero measured
cost in this run, even against tasks hand-picked to be confusable. That
directly supports the pushback this test was built to check: a large,
specific tool surface is not automatically a liability for the agent
actually using it, at least not at this scale, against this model, on
this task set. Worth saying plainly: this is now evidence *for* framing
121 tools as a capability strength, not just a marketing "kitchen sink"
problem to route around.

**Read the ceiling, don't over-read the number.** 100%/100% is a tie
that could mean "breadth genuinely doesn't cost anything here" or could
mean "these 14 tasks weren't hard enough to separate the two
conditions" — a perfect score in both arms can't distinguish a real null
result from a test that wasn't adversarial enough. The five-way
"coverage" ambiguity and the `callers`/`actual_callers` pair were the
sharpest tools available in cie's own surface for this purpose; the
model handled it, but a properly separating test would need it to fail
*somewhere* to prove the full-81 condition is actually harder.

## What this does not cover

- **N=1 per condition.** One run each, same "small first-pass, not a
  mature benchmark" standing this project's other measurements
  (`docs/benchmarks.md`, `docs/benchmarks-requests.md`) already hold
  themselves to — re-run before citing this as a stable result.
- **Tool NAME only, not the full call.** This measures which tool was
  picked, not whether the arguments constructed for it (e.g.
  `path_between(source="login", target="hash_password")`) would have
  been correct too — a real agentic turn can pick the right tool and
  still fill in the wrong argument.
- **One capable model.** The subject here is whatever model backs the
  general-purpose `Agent` type in this session (Claude, current
  generation) — a smaller/weaker model, or a different provider's
  function-calling implementation, could show the cost this test didn't
  find. The competitive claim ("more specific tools help") is model-
  dependent until tested against more than one.
- **Selection in isolation, not mid-task.** Real agentic use interleaves
  tool selection with everything else in context (prior turns, partial
  results, a long system prompt) — a clean, single-shot forced-choice
  test like this one is the easy case, not the realistic one.

## Reproducing this

The ground truth, task list, and raw tool schemas used are not yet
checked into a script (this was a one-off measurement, unlike
`docs/benchmarks-requests.md`'s `scripts/record_demo.sh`) — a real
follow-up would be promoting this into a repeatable eval, with harder
adversarial tasks specifically targeting cie's own near-duplicate tool
names, run across more than one model, more than once, and scored on the
full call (tool + arguments), not tool name alone.
