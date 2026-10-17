# Code Intelligence — Detailed Product User Stories

Product vision catalogue converted from the feature requirements inventory · 18 October 2026

This catalogue converts the platform's feature requirements into user-visible capabilities. It describes intended outcomes, not audited implementation availability. Advanced simulation, exhaustive analysis and autonomous assistance remain bounded by supported languages, accessible evidence and authorized execution. Every story deliberately excludes architecture, interfaces, algorithms, component assignments and technology choices: a story states what a user can do and see, never how the product does it.

Stories consolidate overlapping requirements rather than repeat source-specific identifiers. US identifiers are new catalogue identifiers; every requirement family of the source inventory is represented, and Appendix A maps each family to the stories that cover it. No quantitative accuracy, novelty or bug-free guarantee is asserted.

## Universal user expectations

Whenever a user asks about a repository, the product answers from relevant accessible evidence and provides a complementary visual where it adds understanding. A visual explains the answer rather than decorating it: a flow for a journey, a timeline for events, a table for exact comparisons, a chart for measured quantities. A simple answer may use a compact evidence card instead of forcing an unnecessary graph, and the user can request another suitable representation without the underlying facts changing.

Answers distinguish code facts, runtime observations, historical evidence, interpretations, hypotheses and proposed future changes. Uncertainty remains visible in text, diagrams, summaries and exports. Missing evidence produces a scoped answer or an explicit unknown — never invented files, paths, measurements or causes. "Never hallucinate" is expressed through these observable obligations, not as an impossible guarantee about every generated sentence.

The user can inspect the evidence supporting any material claim, understand which revision and environment it concerns, see relevant coverage limitations and challenge an interpretation. Agreement never turns a hypothesis into proven behaviour, and saved evidence may remain historically valid while no longer describing the current system. All access, sharing, execution and publication respect the user's authorized scope; a useful partial result remains visibly partial, and no complete absence of defects, vulnerabilities, races or regressions is ever claimed from incomplete analysis.

## Repository access and orientation

### US-001: Connect a repository

**As a developer, I want to connect a local or hosted repository and choose the branch or revision to study, so I know exactly what every answer describes.**

**Expected experience:** The user sees what is available to analyse, which parts are ready, and what is unsupported or inaccessible. A repository summary identifies the selected revision; connecting a repository never implies it has been completely analysed.

**Complementary view:** Repository readiness summary and scope map.

### US-002: Know what the product understands

**As a technical lead, I want to see which parts of a mixed-language system the product understands, so I can weigh its answers correctly.**

**Expected experience:** Supported, partially analysed and unexamined areas are shown side by side. Boundaries between languages appear as gaps rather than being silently bridged by an invented path, and constructs the product cannot resolve stay unknown instead of guessed.

**Complementary view:** Coverage matrix and boundary view.

### US-003: Keep understanding current

**As a developer, I want to know when code changes affect my current answer or investigation, so I never act on silently outdated conclusions.**

**Expected experience:** Changed areas and stale conclusions are highlighted and affected views can be refreshed in place without re-doing everything. The studied revision and the working-copy state are always visible, with a warning when they differ. Old answers stay identifiable and are never relabelled as current.

**Complementary view:** Change highlights and freshness indicators.

### US-004: Work across several repositories

**As a platform engineer, I want to index and search every repository I am allowed to study from one place, so I can follow code across service boundaries.**

**Expected experience:** Search and reference-finding operate across all indexed repositories at once, while analysis views remain per repository. Identical names in different repositories stay separate, and version conflicts between them are named, never merged.

**Complementary view:** Search results and cross-repository relationship map.

### US-005: Connect a hosted source

**As a maintainer, I want to connect a hosted repository source and pull updates such as pull requests on demand, so my analysis covers work that happens outside my machine.**

**Expected experience:** Connection state, credential freshness and failed records are shown honestly — suspicious records are held aside for review, never silently dropped. Pauses and limits are disclosed rather than hidden.

**Complementary view:** Source connector health and ingestion state.

### US-006: Watch and control background work

**As a developer, I want long work such as indexing to run in the background with visible progress and a working cancel, so I stay in control of my time and my data.**

**Expected experience:** A job bar shows each task's label, state and progress; results appear when ready and the list survives a page reload. Cancelling before results are saved stops cleanly with nothing written; an interrupted run is reported as failed with "nothing was saved" rather than pretending otherwise, and jobs never resume silently after a restart.

**Complementary view:** Jobs bar and job history.

### US-007: Try a ready-made sample repository

**As an evaluator, I want a sample repository with realistic structure and history, so I can exercise the product end to end before connecting real code.**

**Expected experience:** A generated example application with real dated history can be created on demand, giving every view and investigation honest material to work on.

**Complementary view:** Demo repository.

## Asking questions and working with answers

### US-008: Ask a code-grounded question

**As a developer, I want to ask a repository question in ordinary language and receive an evidence-supported answer, so I can understand the system without reading all of it first.**

**Expected experience:** The response addresses the actual question, cites the code or other evidence behind it, and links any visual to the same evidence. Symbols, relationships and behaviour are never invented to complete an explanation.

**Complementary view:** Answer plus the visual that fits the question.

### US-009: See how my question was read

**As a developer, I want to see how my question was interpreted and pick an alternative reading, so a misunderstanding never sends me down the wrong path.**

**Expected experience:** The view states what the question was taken to mean and how confident that reading is, with the nearest alternatives as one-click options. A reading I chose is never second-guessed, and when no confident reading exists, a low-confidence general answer says so instead of guessing.

**Complementary view:** Interpretation line with alternative readings.

### US-010: Ask several conditions at once

**As an analyst, I want to combine several conditions in one question, so I can find exactly the population I care about.**

**Expected experience:** For "show endpoints that access customer data, lack an identified audit check, and changed recently," the result explains the population and each inclusion condition. Missing audit evidence remains a suspected gap unless absence is established within scope.

**Complementary view:** Filtered result table and relationship map.

### US-011: Combine several questions into one analysis

**As a developer, I want to request a multi-part analysis in one message, so I do not have to babysit each step.**

**Expected experience:** A request such as "explain this project, identify its riskiest module, and show me the tests covering it" runs as a sequence of read-only steps where later steps use earlier results. Failed steps are reported individually without discarding completed views; if no plan can be formed, the product says so and answers the question directly.

**Complementary view:** Step-by-step composed answer.

### US-012: Continue with follow-ups and steering

**As a developer, I want to say "show only these failures" or "what about this service?" without repeating myself, so the exchange flows like a conversation.**

**Expected experience:** The active selection, visible elements and recent conversation provide context, shown as removable chips — including what I am looking at in my editor. When several referents are plausible, the product asks rather than guesses. Steering phrases such as "ignore X", "pin X", "why isn't X shown?" or "zoom out" work as commands.

**Complementary view:** Updated focused view with context chips.

### US-013: Choose the audience and level of detail

**As a team member, I want the same system explained to an engineer, an architect, a security reviewer or a product owner, so everyone works from the same facts.**

**Expected experience:** Vocabulary and detail adapt to the chosen audience while facts, caveats and access boundaries remain identical. An audience lens re-weights what is prominent but never hides a safety-relevant fact.

**Complementary view:** Role-appropriate summary with shared evidence.

### US-014: Inspect and challenge a claim

**As a reviewer, I want to see why a claim was made and correct unsupported interpretations, so the product's knowledge improves instead of accumulating errors.**

**Expected experience:** Every material claim opens to show its evidence, assumptions and the counter-arguments considered against it. I can confirm, dispute or refute with a required explanation; refuting hides the claim and marks everything derived from it stale, while historical evidence is preserved. My agreement never upgrades an interpretation into proven fact.

**Complementary view:** Claim card with evidence and counter-argument.

### US-015: Distinguish proven, inferred, hypothesised and unknown

**As a reviewer, I want every displayed element to state how it is known, so I can weigh each part of an answer appropriately.**

**Expected experience:** Proven facts, cited inferences, hypotheses shown with their counter-arguments, and unresolved areas are marked consistently in words and symbols across text, visuals and exports. Output from the product's own reasoning never displays as proven fact, and calibrated verbs separate what proves, shows, indicates, suggests and speculates — mixed evidence is stated as a mix, not as the strongest part.

**Complementary view:** Epistemic legend and summary footer.

### US-016: Keep several answers side by side

**As an investigator, I want my answers collected in one workspace with clear statuses, so I can compare views without losing track.**

**Expected experience:** Each answer is a tab with a status — ready, generating, queued, failed, stale — and can be closed, exported or highlighted back to its evidence. Exploration history is retained as breadcrumbs, work in progress is cancellable, and a view whose underlying evidence snapshot was lost refuses to display rather than bluff.

**Complementary view:** Response workspace with statuses and breadcrumbs.

### US-017: Recover when something goes wrong mid-answer

**As a developer, I want failures and interruptions to preserve my progress, so a crash never costs me an investigation.**

**Expected experience:** Where a request can be retried, the offer keeps prior progress; interrupted sessions resume after freshness checks. Provider or connectivity problems surface as plain guidance instead of raw errors, and completed work stays available while the failed part is retried.

**Complementary view:** Recovery options and progress report.
## Interactive visual exploration

### US-018: Receive a view that fits the question

**As a developer, I want the answer rendered in a form that actually helps my current question, so understanding arrives faster.**

**Expected experience:** A dependency question produces relationships; a latency question uses available measurements; a chronology uses a timeline. Numbers are never fabricated to satisfy a chart request, and a compact evidence card is preferred over an unnecessary graph.

**Complementary view:** Task-appropriate visual and explanatory text.

### US-019: Switch representation without changing meaning

**As an analyst, I want to switch between a graph, table, timeline or chart for the same answer, so I can pick the shape my audience understands.**

**Expected experience:** The same population, filters and evidence remain identifiable across representations. If a representation would need data the product does not have, it explains the gap rather than quietly changing the question.

**Complementary view:** Linked alternative views.

### US-020: Browse everything the product can draw

**As a team member, I want a gallery of every available visual — purpose-built maps plus standard system-design diagrams — so I can find the right form by name.**

**Expected experience:** Each entry shows a name, a short blurb and an editable question, and states plainly when it cannot be shown for this repository right now, with the reason. Search over the gallery says "matched so far" when coverage is partial. Standard diagrams — architecture, sequence, state machine, data flow, entity relationships and more — are kept apart from the product's own question-relative visuals.

**Complementary view:** Visuals gallery with availability reasons.

### US-021: Drill from system to code

**As a developer, I want to move from capabilities and services down to files, functions and statements, so I can land on the exact code behind a concept.**

**Expected experience:** The selected concept stays recognizable as detail increases, source locations remain reachable, and returning to the broader view preserves the investigation context. Intermediate levels never invent content to fill space.

**Complementary view:** Expandable architecture and source view.

### US-022: Zoom without losing readability

**As a developer, I want to zoom freely while labels and context stay useful, so no level becomes noise.**

**Expected experience:** Detail and label presentation adapt before essential information becomes unreadable, and level changes happen only when a zoom settles, so views never flicker between levels. Free zooming never silently relocates me to a different abstraction level, and off-screen content is disclosed with a bring-into-view action.

**Complementary view:** Readable multilevel map and off-screen indicator.

### US-023: Magnify locally without disturbing the map

**As a reviewer, I want to inspect a dense area with a magnifying lens, so I can read details without rearranging anything.**

**Expected experience:** Hovering a node previews its details and edge labels; hovering an aggregate expands its members into a local grid. The underlying layout provably stays put, and the lens can be paused or applied to tables and other canvases.

**Complementary view:** Hover-expansion lens.

### US-024: Point at things and ask about them

**As a developer, I want to select or lasso visual elements — even cells of a matrix — and ask about "these", so my question's scope is unambiguous.**

**Expected experience:** The response clearly identifies the selected entities and answers within that scope; selection can be refined or cleared. Even an empty cell can be selected, to ask why nothing is there.

**Complementary view:** Highlighted selection and scoped response.

### US-025: Control focus and hidden information

**As a developer, I want to know why something is shown, why something is not, and to keep what matters in front of me, so the map serves my question rather than a default.**

**Expected experience:** A per-element table lists the factors that earned it a place; hidden candidates are listed with reasons and can be pulled back in. I can pin, boost, demote or reset relevance, filters can be inspected and restored, and hiding an item never turns a caveated conclusion into an unconditional fact.

**Complementary view:** Focus controls, salience table and reveal list.

### US-026: Inspect an element's full story

**As a reviewer, I want one inspector for any element or relationship — its role, notes, claims, evidence and code — so I never lose the thread between picture and source.**

**Expected experience:** The inspector shows what the element is, how it is known in a plain sentence, its members or endpoints, attached claims with their evidence, and a code card with the exact location that opens in my editor. Unavailable or stale sources are announced, never faked.

**Complementary view:** Element inspector and code card.

### US-027: Keep multiple views aligned

**As an investigator, I want architecture, runtime and source views of the same subject to stay connected, so findings in one view carry into the others.**

**Expected experience:** Selecting an entity reveals corresponding material wherever a supported mapping exists. Unmapped items are identified instead of implying a relationship from similar names.

**Complementary view:** Linked views and source inspector.

### US-028: Express a proposed change visually

**As an architect, I want to group, move or reconnect components to describe a desired change, so I can think with my hands.**

**Expected experience:** The product explains its interpretation and presents a proposal with consequences and open questions. A gesture never mutates the real system or silently deploys the proposed shape.

**Complementary view:** Current/proposed comparison and intent summary.

### US-029: Overlay tests and runtime on any map

**As a developer, I want to layer test confidence and recorded runtime trouble onto structure I already understand, so hotspots stand out in context.**

**Expected experience:** Overlays offer time windows and refresh controls, and the legend states plainly where there is no data. Overlay evidence merges into the element inspector. Only signals the project's own tooling produced are read — nothing is executed to obtain them.

**Complementary view:** Map overlays with legends.

### US-030: Trust the drawing itself

**As a reader, I want positions, routes and crossings to never mislead me, so the layout carries no hidden meaning.**

**Expected experience:** Elements do not overlap and edges do not cut through unrelated elements; crossings are minimised with honestly documented limits. Layout may change positions but never meaning, claims or evidence.

**Complementary view:** Layout quality report.

## The visual catalogue

### US-031: See the architecture that answers my question

**As a new team member, I want an architecture view grouped around the concern I asked about, so the map matches my question instead of the folder tree.**

**Expected experience:** Code is grouped by the responsibility relevant to the question, from whole-system overview down to statement detail. Proposed groupings are labelled as claims with their grounding, unresolved boundaries render as unknown, and every grouping cites the code that justifies it.

**Complementary view:** Question-relative architecture map.

### US-032: Investigate a failure from a stack trace

**As a debugging engineer, I want to paste an exception and see ranked suspects with discriminating evidence, so triage starts immediately.**

**Expected experience:** A small set of suspects appears with a factor-by-factor breakdown of why each is suspected; I can ignore a suspect or ask why another is suspected, live. Refuted hypotheses stay visible but greyed.

**Complementary view:** Causal hypothesis graph.

### US-033: See every way a failure can arise

**As a developer, I want to ask "what could cause this payment to fail?" or "how could this value become wrong?", so failure modes are enumerated rather than remembered.**

**Expected experience:** Failure sites and every writer of the value are listed with citations; paths that cross asynchronous boundaries are flagged. Purely static branches are shown as code-proven; async-only failure routes are labelled hypotheses.

**Complementary view:** Failure-space and invariant map.

### US-034: Walk through a transaction step by step

**As a new team member, I want a journey view of an operation, so I can follow what happens in order.**

**Expected experience:** Steps appear in the order the calling code performs them, with decision points where it can fail, asynchronous hand-offs marked, and transaction scope visible on the nodes. Whether a branch is taken at runtime, or how often a loop runs, is marked unknown rather than guessed.

**Complementary view:** Transaction journey with swim lanes.

### US-035: Trace who reads and writes data

**As a developer, I want to see all readers and writers of a field together with the transaction boundary and ordering risks, so data invariants become discussable.**

**Expected experience:** Access is tracked per owning object, so same-named fields on different objects stay apart. Reads, writes and order-of-application risks are distinguished, and the view states its limits for multiple instances of one class.

**Complementary view:** Data lineage and mutation map.

### US-036: Compare code before and after a change

**As a reviewer, I want a before/after view of what a revision changed and what it means, so review starts at the consequences.**

**Expected experience:** Both revisions are shown side by side with a plain-words list — new failure modes, removed protections, changed test meaning, affected concepts — and the commits behind the change. A rename counts as a change and is labelled as such; revisions that were never indexed cannot be compared.

**Complementary view:** Semantic diff timeline.

### US-037: Ask why unusual code exists

**As a developer, I want the history of the lines in question beside the comments that still bind them, so context survives author turnover.**

**Expected experience:** Commits that touched those lines appear oldest-first next to the binding comments. Events are commits and constraints are comments — the product adds no narration of intent, and "which commit introduced this" remains a claim unless evidenced.

**Complementary view:** Archaeology chain.

### US-038: See trust boundaries and privilege walls

**As a security reviewer, I want the walls between outside and inside, the gates that can refuse a request, and any unprotected paths, so exposure is visible structurally.**

**Expected experience:** Gates are proposed only where code can actually refuse — a guard's name alone is never enough. Unprotected or escape routes appear as hypotheses with their counter-arguments, covering frameworks, proxies and configuration invisible to static analysis.

**Complementary view:** Trust-boundary and privilege map.

### US-039: See recent trouble on the structure

**As an operator, I want reported exceptions and failing tests tinted onto the architecture for a window I choose, so "what has been going wrong" has a place.**

**Expected experience:** Overlays offer time windows and a replay of recorded runtime with observed/error highlighting. Sampling and attribution gaps remain visible, and frames outside the repository are drawn as unknown, not guessed.

**Complementary view:** Runtime overlay with replay.

### US-040: Find where races are possible

**As a developer, I want execution paths that share data drawn as lanes with flagged interleaving windows, so race discussions start from structure.**

**Expected experience:** The view shows where interleaving is possible, citing both statements in the window — never that a race happened. Locks, queues and scheduler settings are not read to decorate the picture.

**Complementary view:** Concurrency and race-window lanes.

### US-041: Preview removing a component

**As an architect, I want to ask "what if we remove X?" and see the consequences attached to a ghost of the removed part, so deletion debates are grounded.**

**Expected experience:** Broken callers, dropped events, lost protections, orphaned writers and tests that would break are attached to the outline. "No consequence found" is displayed as an outcome, not a safety guarantee, and in this release only removal is simulated — moves and merges are not.

**Complementary view:** Counterfactual overlay.

### US-042: See how well behaviour is tested

**As a technical lead, I want a behaviours-by-tests matrix, so test gaps are visible by behaviour, not by file.**

**Expected experience:** Fill shows which tests reach a behaviour's code, failing tests are marked, and behaviours that no test asserts are called out. "Reaching code is not asserting its behaviour" is stated in the legend; matches made from names alone are inferences.

**Complementary view:** Test-confidence matrix.

### US-043: See ownership and knowledge risk

**As a team lead, I want code arranged by owner with thin and stale knowledge highlighted, so bus-factor risk is a picture, not a hunch.**

**Expected experience:** Owners come from declared policy or de facto history, and de facto ownership is an inference needing human confirmation. Import links stay faint until selected — context, not answer.

**Complementary view:** Ownership and knowledge map.

### US-044: See implicit concepts in the code

**As a business analyst, I want concept cards pinned to the code that implements them, so domain language and code can be reconciled.**

**Expected experience:** Each instance cites its sites, and places where a concept lacks enforcement are shown. Cards must exist for the current revision before the atlas can render.

**Complementary view:** Implicit-concept atlas.

### US-045: Audit which policies are enforced

**As a policy owner, I want a routes-by-rules matrix showing enforced, bypassable, convention-only and unrelated, so compliance talk starts from evidence.**

**Expected experience:** "Enforced" is an inference from refusal sites; "can get around" names every check the route skips; convention-only rules are marked as such. Rows with the most ways around sort first, with a plain-words count.

**Complementary view:** Policy enforcement matrix.

### US-046: Judge where change is risky

**As a technical lead, I want a terrain of change risk combining coupling, churn, incidents, test gaps and thin knowledge, so prioritisation debates have a shared picture.**

**Expected experience:** The composite formula is shown and its weights are adjustable sliders; missing data renders as a neutral hatched value, colours are relative to this repository, and the "composite, not fact" label is permanent.

**Complementary view:** Change-risk terrain.

### US-047: Connect profiling data to code

**As a performance engineer, I want profile hotspots attributed to code with uncertainty stated, so optimisation targets come from measurements.**

**Expected experience:** Hotspot shares are measured from imported profiles with uncertainty intervals; unattributed or withheld portions render as unknown. Every metric cites its source artifact and states how strongly it correlates with the referenced window.

**Complementary view:** Profile-attributed hotspot view.

### US-048: See routes and what guards them

**As a reviewer, I want the system's routes and their guards in one matrix, so entry points are reviewable at a glance.**

**Expected experience:** Routes and guards come from recognised framework evidence. When no current indexed evidence supports elements for the diagram, the view says so in plain words instead of rendering an empty frame.

**Complementary view:** Route and guard matrix.

### US-049: Ask for a diagram of my own

**As a developer, I want to say "draw it as a diagram" and get a bounded chart over the indexed code, so custom explanations do not require a drawing tool.**

**Expected experience:** The generated plan uses only indexed entities within a bounded size, is reviewable as a description, and never executes anything. Explicit chart requests win over automatic concern matching, and a repeated question reuses its plan.

**Complementary view:** Generated chart from a bounded plan.
## Behaviour, concepts and business rules

### US-050: Extract and curate concept cards

**As a business analyst, I want candidate domain concepts extracted from the code as reviewable cards, so the product learns our vocabulary from us.**

**Expected experience:** Extraction runs as a cancellable background job, and each new version of the card set can be compared with the previous one. I judge each card — confirm, dispute or refute — and refuted cards stop influencing results. The product's own confidence is compared with my verdicts so I can see whether it means anything, and it abstains from calibrating until enough cards have been judged.

**Complementary view:** Concept card list with version diffs.

### US-051: Browse the business-concept hierarchy

**As an architect, I want concepts organised both by where they live and by what they specialise, so structural and domain views reinforce each other.**

**Expected experience:** One axis follows architectural containment; the other derives specialisation patterns from behaviour. Invariants carry an honesty tier — verified by explicit assertion, supported by guards, or speculative across asynchronous or dynamic code. Assistance may name things, but naming never asserts a claim.

**Complementary view:** Two-axis concept hierarchy browser.

### US-052: Locate an existing feature

**As a developer, I want to find where a business capability is implemented across layers, so change discussions start from the full picture.**

**Expected experience:** The result connects relevant screens, endpoints, rules, persistence and integrations with evidence, and explains partial coverage. A named feature is distinguished from a verified working capability.

**Complementary view:** Feature-to-code map.

### US-053: Discover distributed business rules

**As a business analyst, I want rules scattered across validation, workflows and data constraints collected in one catalogue, so policy and code can be compared.**

**Expected experience:** The catalogue shows candidate rules, enforcement locations and supporting examples. Extracted behaviour is separated from approved policy, and inconsistent paths are highlighted rather than averaged away.

**Complementary view:** Rule catalogue and enforcement map.

### US-054: Compare intended and actual behaviour

**As a product owner, I want to see where implementation differs from approved requirements, so mismatches are named precisely.**

**Expected experience:** The response names the applicable requirement, the code-supported behaviour and the mismatch scenario. Missing expectations and conflicting requirements are classified separately from a confirmed defect.

**Complementary view:** Requirement-to-behaviour comparison.

### US-055: Understand workflow states and failures

**As an analyst, I want valid transitions, invalid actions and recovery paths for a workflow, so edge cases become discussable.**

**Expected experience:** Triggers, guards, terminal states and known cancellation or reversal differences are shown. A plausible inferred state transition is never displayed as an observed fact.

**Complementary view:** State diagram and exception paths.

### US-056: Detect misleading names

**As a reviewer, I want to know whether code actually performs the responsibility implied by its name, so deceptive naming cannot survive review.**

**Expected experience:** If an "authorize" function only prints text, the observed implementation and the unmet expectation are shown side by side. A misleading name alone is a concern; a confirmed violation requires an established behavioural obligation.

**Complementary view:** Responsibility-versus-behaviour comparison.

## Architecture, dependencies and impact

### US-057: Explore system structure

**As an architect, I want responsibilities, boundaries and dependencies rather than only directory structure, so architecture reviews start from real relationships.**

**Expected experience:** The view distinguishes declared architecture, discovered relationships and inferred groupings. Boundaries can be challenged down to the code supporting them.

**Complementary view:** Architecture map.

### US-058: Find the change blast radius

**As a developer, I want to understand what removing or changing an entity could affect, so impact is evidence, not folklore.**

**Expected experience:** Direct references, transitive dependencies, known consumers and inferred consequences are separated. Unexamined consumers are identified rather than promising that everything outside the map is safe.

**Complementary view:** Impact map with affected source list.

### US-059: Compare architectural alternatives

**As an architect, I want to compare extending a module, introducing a service or retaining compatibility support, so trade-offs are explicit before commitment.**

**Expected experience:** Each alternative lists affected capabilities, contracts, data boundaries, risks and unresolved assumptions. Proposed consequences remain distinct from measured outcomes.

**Complementary view:** Side-by-side alternatives and tradeoff table.

### US-060: Discover potential service boundaries

**As an architect, I want candidate separations with their trade-offs, so decomposition debates have a starting point.**

**Expected experience:** Supporting responsibilities, shared data and coupling are shown together with the migration questions they raise. Suggested boundaries remain recommendations, not a uniquely correct architecture.

**Complementary view:** Candidate boundary and coupling view.

### US-061: Follow cross-repository contracts

**As a platform engineer, I want producer/consumer relationships across services and internal libraries, so contract changes are navigable.**

**Expected experience:** Source versions, interface assumptions and affected consumers are inspectable. Consumers that are unavailable or out of bounds appear through coverage statements, not silence.

**Complementary view:** Cross-repository contract map.

### US-062: Judge dependency importance and upgrades

**As a maintainer, I want to see where a package matters and what an upgrade or replacement affects, so upgrades stop being leaps of faith.**

**Expected experience:** Usage evidence, advisories, compatibility concerns and proposed replacement work are separated. An available fixed version is never presented as automatically compatible with the application.

**Complementary view:** Dependency usage/risk map and upgrade comparison.

### US-063: Ask questions of a built artifact

**As a maintainer, I want to inspect a compiled artifact and learn what its bytes support, so third-party binaries can be questioned too.**

**Expected experience:** Formats, embedded metadata, linked libraries, exported symbols and high-signal strings are reported — each claim marked observed, derived or inferred, with limitations and the location probed. Unanswerable questions are listed as unresolved rather than answered creatively.

**Complementary view:** Artifact probe report.

## Debugging and investigations

### US-064: Start an investigation from a symptom

**As a debugging engineer, I want to provide an exception, failed request, log or description and get a structured investigation, so I begin from facts instead of hunches.**

**Expected experience:** Known facts, plausible causes and missing evidence are separated from the start, and a useful starting point is offered without pretending the root cause is already known. Exceptions reported by running applications collect in one inbox where repeats collapse into a counted entry; any entry can be sent straight into an investigation or dismissed.

**Complementary view:** Failure and hypothesis map; exceptions inbox.

### US-065: Compare competing explanations

**As an investigator, I want evidence for and against each candidate cause, so the strongest story does not win by repetition.**

**Expected experience:** Hypotheses remain individually inspectable and can be supported, challenged or eliminated as evidence arrives. Ranking is explained rather than dressed up as calibrated probability.

**Complementary view:** Competing-hypothesis view.

### US-066: Direct an investigation while it runs

**As an investigator, I want to set a goal, constrain scope and redirect work in progress, so the investigation serves my question.**

**Expected experience:** Ongoing, completed and blocked work are visible; I can pause, steer or stop within budget limits. Excluding an area shrinks the final conclusion's scope, and the limitation stays visible. Every stop has a stated reason.

**Complementary view:** Live investigation board.

### US-067: Trust the investigation's bookkeeping

**As a reviewer, I want the investigation's rules to be strict, so its conclusions deserve trust.**

**Expected experience:** Late results from cancelled or superseded work are stored but never published. Reasoning may propose hypotheses but never grants itself authority — only retrieved evidence counts. Absence counts against a hypothesis only when coverage was exhaustive for the window; one description repeated ten times is one observation; priority is never presented as probability.

**Complementary view:** Assessment ledger with coverage certificates.

### US-068: Reproduce an intermittent defect

**As a developer, I want a suspected issue turned into a repeatable failing example where possible, so "works on my machine" ends.**

**Expected experience:** Inputs, environment, relevant ordering and results are recorded. Failure to reproduce is reported as exactly that — never as evidence that the defect is absent.

**Complementary view:** Reproduction report and event sequence.

### US-069: Ask for the next discriminating step

**As an investigator, I want to know which observation would separate the remaining explanations, so effort goes where it changes the answer.**

**Expected experience:** Each recommended check explains what outcome would support or challenge which hypothesis. Unavailable tools or evidence become concrete blockers, never fabricated results.

**Complementary view:** Evidence-gap map and next-step list.

### US-070: Resume an investigation later

**As a developer, I want to return to prior findings, rejected hypotheses and open questions, so investigations can outlast a workday.**

**Expected experience:** The workspace preserves evidence, decisions and context. After code changes, a banner lists affected findings and offers a refresh against the latest revision while keeping identities stable; stale findings remain distinguishable from current ones.

**Complementary view:** Saved investigation with change summary.

### US-071: Assemble an incident explanation

**As an incident lead, I want an evidence-supported account of what happened and why, so the postmortem stands on citations.**

**Expected experience:** Event chronology is separated from causal explanation; alternative and unresolved causes stay in the report, linked to the code changes and follow-up actions involved.

**Complementary view:** Incident timeline and causal evidence view.

### US-072: Triage defect candidates honestly

**As a developer, I want a defects panel of triageable candidates with their limits attached, so findings get decisions without false certainty.**

**Expected experience:** Every finding carries its rule, evidence location and stated coverage gaps; findings are candidates, never verdicts, and "no finding" never means "safe". I can dismiss with a reason, mark reviewed, or export the set. Security alarms appear only with deterministic proof or two authorized confirmations.

**Complementary view:** Defects panel with severity chips and witness paths.

### US-073: Back candidates with runtime evidence and isolated runs

**As a maintainer, I want static candidates checked against runtime tools where the stack allows, inside sandboxes that refuse to overreach, so corroboration is real.**

**Expected experience:** Race detectors, deadlock reports and hang watchdogs are used on reviewed sources when available, with uncovered cases named openly. Experiments run in confined profiles with explicit grants, and a runner refuses a requested guarantee it cannot provide rather than silently dropping it.

**Complementary view:** Runtime corroboration report.

### US-074: Collect performance candidates

**As a developer, I want suspicious performance patterns — repeated per-element queries, input/output under locks, per-iteration locking — collected as candidates, so optimisation starts from a list.**

**Expected experience:** Each candidate names its pattern and location. A performance claim is called measured only when timing comes from a named workload, build and revision.

**Complementary view:** Performance candidate list.
## Runtime, concurrency and data consistency

### US-075: Compare structure with observed runtime

**As an operator, I want runtime activity laid over code structure with honesty about exactness, so the live system and the maps can be compared.**

**Expected experience:** The view identifies the environment and observation window. Joins are labelled exact only when revision, deployment and code location agree; everything weaker is stated or drawn as unknown. A component with no observed activity is not declared unused under every workload.

**Complementary view:** Runtime activity overlay.

### US-076: Replay a recorded request

**As a developer, I want to replay recorded execution with source context and timing, so an incident can be walked through after the fact.**

**Expected experience:** Available spans, events and data snapshots are inspectable with appropriate redaction. The replay controls state plainly that this is recorded data, not live telemetry, and unrecorded variables are never reconstructed as certain facts.

**Complementary view:** Execution timeline and source highlights.

### US-077: Compare environments and flags

**As an operator, I want flags, configuration and deployed versions compared across environments, so "it works in staging" becomes precise.**

**Expected experience:** Known path differences are explained, and configured possibilities are separated from observed execution. Exploring a flag change never applies it to any environment.

**Complementary view:** Environment comparison and flag topology.

### US-078: Distinguish causes from coincidence

**As an investigator, I want events that caused or enabled a failure separated from events that merely occurred nearby, so root-cause claims mean something.**

**Expected experience:** The answer states the strength and limits of the causal evidence. Timing proximity or a parent-child relationship alone never establishes a business-level cause, and contradictory ordering proposals are quarantined rather than averaged in.

**Complementary view:** Causal explanation with timeline evidence.

### US-079: Find possible race windows

**As a developer, I want shared-state and interleaving risks identified with a witness where supported, so races get argued from evidence.**

**Expected experience:** Static suspicion and observed failing executions are distinguished, with the affected invariant and the scope of exploration stated. Passing bounded tests is never a race-free certificate.

**Complementary view:** Shared-resource and interleaving view.

### US-080: Find deadlocks and lock contention

**As a developer, I want waiting tasks, held resources and problematic lock relationships mapped, so deadlock risks are structural, not anecdotal.**

**Expected experience:** Observed cycles and suspected ordering risks are labelled separately, with locations, traces and relevant scheduling evidence inspectable.

**Complementary view:** Wait/resource graph and contention view.

### US-081: Investigate distributed consistency

**As an architect, I want ways retries, ordering, duplicate messages or transaction boundaries can violate data rules, so consistency hazards are designed against.**

**Expected experience:** Readers, writers, relevant guarantees and candidate failing scenarios are linked. Data races are distinguished from logical consistency errors, and no exhaustive discovery is claimed.

**Complementary view:** Consistency hazard and message-flow map.

## Performance and technical health

### US-082: Explain a slow operation

**As a performance engineer, I want slow requests connected to code and resource evidence, so the story of the slowness is traceable.**

**Expected experience:** CPU, waits, data work, external calls and missing observations are distinguished. Hotspots come with uncertainty and attribution, speculative mechanisms are labelled as such, and imported profiles show what was collected and what was dropped.

**Complementary view:** Trace waterfall, profile or hotspot table.

### US-083: Compare two profiles

**As a developer, I want two profiles compared under a declared equivalence, so "faster" comes with stated conditions.**

**Expected experience:** The comparison states its verdict with reasons and limitations. Populations that cannot be fairly compared are rejected with evidence, and failed runs stay in the population.

**Complementary view:** Profile comparison with metric table.

### US-084: Investigate a performance regression

**As a developer, I want behaviour compared before and after a change under equivalent conditions, so regressions are demonstrated, not felt.**

**Expected experience:** Workload and environment differences, errors, throughput, latency and uncertainty are visible. A candidate that looks faster by dropping work or errors cannot be presented as an improvement.

**Complementary view:** Baseline/candidate charts and complete outcome table.

### US-085: Identify optimization opportunities

**As a developer, I want avoidable repeated work, inefficient queries and serialization/locking concerns surfaced with their constraints, so improvements do not break correctness.**

**Expected experience:** Each suggestion explains the affected behaviour, the evidence and the correctness constraints. Potential improvements are never described as measured speedups without experiments.

**Complementary view:** Hotspot map and opportunity comparison.

### US-086: Prioritize maintenance work

**As a technical lead, I want code ranked by where change frequency, complexity or weak tests create practical risk, so maintenance effort goes where it matters.**

**Expected experience:** Rankings expose every contributing factor and its weight, the effect of each exclusion such as bulk edits, formatting or bot commits, and how stable the top ranks are under re-weighting. Scores are labelled heuristics — never calibrated defect probabilities — and history-based scores state their lower-bound nature.

**Complementary view:** Maintenance hotspot view with rank explainer.

### US-087: Find code that changes together

**As an architect, I want historical co-change relationships that cross directory boundaries, so hidden coupling becomes visible.**

**Expected experience:** Supporting commits and sample sizes are inspectable, and bulk edits and renames are accounted for. Co-change is distinguished from runtime or static dependency and always shown with its support count.

**Complementary view:** Change-coupling map.

## History, collaboration and knowledge

### US-088: Protect contributor privacy

**As a team lead, I want history analysis without exposing individual contributors, so insight never costs privacy.**

**Expected experience:** Contributor identities are protected at the source; display names appear only where policy and an explicit grant allow, and no surface ranks individuals. Denied paths are counted in coverage, never named.

**Complementary view:** Contributor policy summary.

### US-089: Track how an interface evolved

**As a maintainer, I want an interface's change history with affected clients, so breaking changes are announced with evidence.**

**Expected experience:** Breaking-change concerns cite versions and contracts; unknown consumers remain a visible coverage gap. Review discussions stay attached to the code through merges, renames and deletions.

**Complementary view:** Contract timeline and consumer impact.

### US-090: Find documentation gaps

**As a team lead, I want important behaviour with missing or outdated documentation identified, so documentation debt is a list, not a feeling.**

**Expected experience:** The product links current behaviour to the documents that describe it, explains its search scope and proposes reviewable corrections. Undocumented behaviour is never called approved by default.

**Complementary view:** Documentation gap list and behaviour links.

### US-091: Collaborate without widening access

**As a team member, I want to share views, attach evidence and record agreement or dispute, so decisions accumulate with their context.**

**Expected experience:** Sharing reaches only people who can already read the code; shared views are cut to what each recipient may see, and what is invisible is listed without being revealed. Notes can be private or team-visible and travel with their elements into future views; disagreement is recorded, not erased.

**Complementary view:** Shared workspace and decision trail.

### US-092: Export and bookmark understanding

**As a team member, I want to save or share an answer, visual or report with its context intact, so knowledge survives the session.**

**Expected experience:** Exports retain revision, evidence references, limitations and speculative labels, and evidence bundles can travel with the export and be verified on import. Bookmarks reopen with a clear account of what changed since.

**Complementary view:** Report, bookmarked view or annotated visual.

### US-093: Save and resume a workspace

**As a developer, I want my open views, selections, pins and notes restored as I left them, so context reloads with me.**

**Expected experience:** A workspace restores conclusions and beliefs, not just window layout. Saved states are annotated with what has gone stale, returning shows what changed since I left, and starting fresh is one explicit action.

**Complementary view:** Workspace list with staleness banners.

## Security, permissions and supply chain

### US-094: Understand authorization

**As a security reviewer, I want to see who can perform an action on which resources and fields, so access decisions are auditable.**

**Expected experience:** The view includes ownership scope and relevant conditional restrictions. Unexplored areas are shown as unknown, never as permission-free access.

**Complementary view:** Actor/action/resource matrix and enforcement map.

### US-095: Trace sensitive information

**As a security reviewer, I want sensitive data followed through transformations, storage, logs and external destinations, so exposure paths are concrete.**

**Expected experience:** Supporting source evidence and uncertain classifications are shown, with witness paths for suspected flows; sanitizers that were considered are listed with their outcome. Missing or inaccessible paths limit the conclusion, and the limit is stated.

**Complementary view:** Sensitive-data flow and trust-boundary map.

### US-096: Find policy enforcement gaps

**As a policy owner, I want approved access, audit or data rules compared with implementation, so gaps are found before auditors do.**

**Expected experience:** Confirmed violations and suspected gaps are labelled separately. The presence of a guard by name is not accepted as evidence that the policy is enforced.

**Complementary view:** Policy/enforcement/gap view.

### US-097: Review code-security and secret findings

**As a reviewer, I want applicable code-security and exposed-secret findings with scope and exclusions, so security review starts from honest input.**

**Expected experience:** Protected secret values never appear in reports; blocking issues, exclusions and unresolved coverage are visible. A clean scan does not imply universal security.

**Complementary view:** Security findings with source evidence.

### US-098: Assess packages and licences

**As a maintainer, I want dependency additions, vulnerabilities, source integrity and licence obligations reviewed, so supply chain risk is managed knowingly.**

**Expected experience:** Direct and transitive changes are visible, unavailable provenance is stated, and known attribution duties are recorded. Exhaustive legal or malicious-package detection is never promised.

**Complementary view:** Dependency diff and policy findings.
## Counterfactuals and digital twin

### US-099: Explore a hypothetical change

**As an architect, I want to ask what happens if a component is removed, scaled, split or made asynchronous, so decisions get a dry run.**

**Expected experience:** Current structure and the proposed change stay visually distinct; assumptions, required adaptations and predicted consequences are shown, with each consequence labelled structural, measured or inferred. The scenario is never presented as already implemented.

**Complementary view:** Current/proposed overlay with typed consequences.

### US-100: Explore capacity and failure scenarios

**As an operator, I want outcomes compared under changed traffic, capacity or external failures, so capacity planning has a sandbox.**

**Expected experience:** Workload assumptions, the supported prediction domain and unknowns are identified. Unsupported scenarios stay exploratory instead of receiving invented measured numbers.

**Complementary view:** Scenario comparison and uncertainty chart.

### US-101: Compare isolated experiments

**As a developer, I want baseline and candidate outcomes compared before deciding whether a change helps, so evidence, not enthusiasm, decides.**

**Expected experience:** Correctness, latency, errors and resource usage are compared for the tested population, with failed and timed-out runs retained in that population. A certificate is issued only when the recorded evidence meets the applicable reliability criteria, and inconclusive evidence remains inconclusive.

**Complementary view:** Experiment comparison and run summary.

### US-102: Study a workflow digital twin

**As an architect, I want a model of a critical workflow bound to a revision and environment, so "what would happen" has a disciplined home.**

**Expected experience:** What is observed, modelled, assumed and unmodelled is separable, and synchronisation and freshness are inspectable. A replay timeline is explicitly a model run, not production timing, and a diagram alone is never presented as an executable validated twin.

**Complementary view:** Workflow model with observation and scenario views.

### US-103: Know the limits of predictions

**As a decision maker, I want to know whether a forecast is validated for my proposed use, so predictions inform without hypnotising.**

**Expected experience:** Measured runs, validated model predictions and untested hypotheses are distinguishable, and results are never relabelled upward. Predictions that leave their validated domain, or whose sensitivity analysis shows fragility, say so before influencing a decision.

**Complementary view:** Prediction scope and evidence summary.

## Prompt-to-feature discovery and planning

### US-104: Describe a new feature naturally

**As a feature requester, I want to start from a short prompt, a feature brief, a detailed specification or examples, so the barrier to asking is low.**

**Expected experience:** The product accepts different levels of detail, identifies the target repository and the desired deliverable — plan, preview candidate or draft pull request — and never requires internal framework or component names to begin.

**Complementary view:** Build feature workspace and request summary.

### US-105: Answer only material questions

**As a feature requester, I want to be asked only about decisions that affect behaviour, so my time goes to what matters.**

**Expected experience:** Existing conventions are adopted where appropriate; every question explains why it matters, and independent work continues while answers are pending. Silence never authorizes a material business or access decision.

**Complementary view:** Focused clarification cards and task status.

### US-106: Discover equivalent or overlapping features

**As a feature requester, I want to know whether the requested capability already exists, so I do not rebuild what is there.**

**Expected experience:** The response compares behaviour, permissions, availability and evidence. For "B is X plus Y, and existing A is X", the proposal explains what can be reused and what Y adds; it does not duplicate X solely because names differ.

**Complementary view:** Overlap and reuse comparison.

### US-107: Resolve conflicting requirements

**As a product owner, I want contradictions, ambiguities and incompatible policies surfaced before implementation, so conflicts are decided, not discovered.**

**Expected experience:** Source statements, applicability, a concrete conflict scenario and resolution options are shown. Decisions have an owner and a history, and resolving a conflict updates the affected acceptance expectations.

**Complementary view:** Conflict comparison and blocked-task view.

### US-108: Review a complete feature contract

**As a product owner, I want a complete brief — scope, actors, rules, failure paths, assumptions and observable acceptance criteria — so my approval means something.**

**Expected experience:** Requirements are distinguished from inferred defaults, optional improvements and necessary prerequisites, and every requirement carries its origin and authority. Material omissions stay visible, and the brief can be corrected before or during implementation.

**Complementary view:** Feature contract and criterion checklist.

### US-109: Understand feature impact before building

**As a reviewer, I want the expected changes to code, permissions, data, interfaces, operations and performance, so review covers consequences, not just code.**

**Expected experience:** Planned changes are always marked as planned. Compatibility, migrations, external dependencies and unresolved risks are visible without a false promise that planning proves safety.

**Complementary view:** Planned impact graph and risk summary.

### US-110: Revise scope while work proceeds

**As a feature requester, I want to change an answer or requirement without losing useful prior work, so iteration is cheap.**

**Expected experience:** The product explains which tasks, code and results are affected, preserves decisions, and marks obsolete validation stale. Unchanged reusable work stays available with its scope justified.

**Complementary view:** Contract comparison and refreshed task map.

## Feature implementation and inspection

### US-111: Build an isolated candidate

**As a developer, I want the clarified request turned into a reviewable change that does not disturb unrelated work, so building feels safe.**

**Expected experience:** The exact target and base, progress, affected artifacts and failures are visible. Implementation stays within authorized scope, missing prerequisites are stated plainly, and nothing is ever applied to my working tree uninvited.

**Complementary view:** Candidate workspace and progress report.

### US-112: Follow a dedicated feature wizard

**As a feature requester, I want a persistent workspace that walks through Describe, Clarify, Plan, Changes, Validate and Deliver, so a long build survives interruptions.**

**Expected experience:** Earlier decisions can be revisited, progress is saved, and viewing a stage is always distinct from authorizing an action — every effectful step is its own explicit button. Blockers and candidate status remain visible, and work can be stopped checkpoint-safely.

**Complementary view:** Six-stage wizard with status digest.

### US-113: Inspect where changes occur

**As a reviewer, I want to know which requirements changed which components, files and symbols, so accountability is structural.**

**Expected experience:** Added, modified, deleted, renamed, reused and affected-but-unchanged elements are distinguishable, available as a searchable list as well as a graph.

**Complementary view:** Requirement/component/file change graph.

### US-114: Inspect final code and exact differences

**As a developer, I want complete candidate files and exact comparisons with the baseline, so nothing is judged from a summary.**

**Expected experience:** Full-code, unified and side-by-side views are switchable, reasons are inspectable, and truncation or binary limitations are disclosed. Planned code is never confused with applied or validated code.

**Complementary view:** Full-code and diff viewer.

### US-115: Preview feature behaviour

**As a product owner, I want to try representative successful, denied and failure journeys before accepting, so acceptance is experienced, not imagined.**

**Expected experience:** Role-specific views, sample data and mocked dependencies are labelled as such; appearance alone never establishes behaviour, and backend-only changes come with usable request/response examples.

**Complementary view:** Runnable preview or interface demonstration.

### US-116: Review new dependencies and data changes

**As a maintainer, I want the packages, configuration, migrations and integration effects a feature introduces, so the bill of consequences is explicit.**

**Expected experience:** Necessity, compatibility, historical-data implications and recovery limits are explained. A successful local migration is never presented as production-scale validation.

**Complementary view:** Dependency/configuration/data change summary.

## Validation and safe delivery

### US-117: See whether the candidate builds

**As a developer, I want actual build outcomes for every applicable supported target, so "it compiles" means exactly what was run.**

**Expected experience:** Baseline and candidate results identify the commands and configuration used and their currentness. One successful target cannot hide another failing target.

**Complementary view:** Build and diagnostic dashboard.

### US-118: Inspect tests related to the requirements

**As a reviewer, I want test source, meaningful assertions and execution against the changed behaviour, so "tested" is inspectable.**

**Expected experience:** Existing, added and modified tests are distinguishable, with relation and coverage limits, unexecuted cases and assertion changes visible — never a file-tested badge based on imports alone.

**Complementary view:** Criterion/file/test matrix and test-source viewer.

### US-119: Trust independent acceptance evidence

**As a reviewer, I want to know the checks did not simply repeat the implementation's assumptions, so acceptance evidence is independent.**

**Expected experience:** Expected-result sources, original regression obligations and any unauthorized weakening of assertions are exposed. A requested business change to an expectation is shown separately from a code repair, and generated expectations count only after a person confirms them.

**Complementary view:** Oracle and assertion comparison.

### US-120: Inspect security and performance readiness

**As a technical lead, I want security, dependency, performance and operational checks alongside functional results, so readiness is one picture.**

**Expected experience:** Mandatory security checks run on the exact candidate, and browser journeys require real interactions and accessibility assertions — a screenshot alone never passes. Missing tools and nonrepresentative environments remain incomplete, and a waiver is disclosed rather than relabelling missing evidence as passed.

**Complementary view:** Readiness dashboard and metric comparisons.

### US-121: Request fixes and rerun checks

**As a developer, I want failures investigated, the candidate revised and relevant validation rerun, so iteration stays disciplined.**

**Expected experience:** Changes invalidate affected results and the whole current decision is refreshed. A passing rerun cannot hide other mandatory stale or failed checks, and exhausted budgets return inspectable partial work.

**Complementary view:** Failure diagnostics and current validation dashboard.

### US-122: Download a diff or patch

**As a developer, I want the change as a patch I can inspect and apply myself, so delivery fits my workflow.**

**Expected experience:** The delivery includes base context, a change inventory and validation limitations. Review-only or incomplete patches are unmistakably labelled, and prohibited content cannot bypass export policy.

**Complementary view:** Diff, patch and delivery summary.

### US-123: Check and apply against my destination

**As a developer, I want to know whether a patch fits my current code and whether the result still works, so applying is a decision, not a gamble.**

**Expected experience:** Divergence, local edits and application conflicts are explained before anything changes. Clean textual application does not imply semantic compatibility, and the resulting code gets appropriately current validation.

**Complementary view:** Destination comparison and application report.

### US-124: Create and iterate a draft pull request

**As a developer, I want an authorized draft published and review handled without losing the feature trail, so collaboration happens where the code lives.**

**Expected experience:** Publication is draft-only — the product never merges, approves or deploys. Reviewer feedback maps to requirements and changes, evidence is invalidated where due, and publication, merge and deployment remain separate visible actions that each need their own authority.

**Complementary view:** Draft PR workspace and updated diff.

### US-125: Trace prompt-caused mutations

**As a maintainer, I want to see which request and decision caused each file change, so generated work is auditable end to end.**

**Expected experience:** A root trail connects the prompt, clarifications, actions, files, tests, commits and pull requests. Sensitive input is redacted for the destination, interrupted synchronisation is disclosed, and untracked changes are never silently called verified.

**Complementary view:** Request-to-file origin timeline.

### US-126: Coordinate several related changes

**As a technical lead, I want overlapping or multi-repository requests managed without losing individual outcomes, so programmes do not collapse into chaos.**

**Expected experience:** Dependencies, conflicting work, child changes and partial failures are visible. Unmerged work is never presented as shipped, independently validated branches do not automatically certify their combination, and dry runs change nothing.

**Complementary view:** Campaign and integration-status view.
## Release and delivery oversight

### US-127: Freeze a release scope

**As a release manager, I want a milestone-frozen scope with disciplined handling of late additions, so scope creep has a visible cost.**

**Expected experience:** Items already in scope are trusted; items appearing after the freeze are marked for assessment, with committed-versus-stretch counts and an explicit drift warning. The scope can be replayed from its own recorded history.

**Complementary view:** Release scope wizard and checklist.

### US-128: Read readiness through stakeholder lenses

**As a decision maker, I want one release evidence set viewed through the lens of my role, so each stakeholder sees their worst risks first.**

**Expected experience:** Every lens sorts by what blocks its concern, keeping pass, attention, blocked and not-determined distinct. Unknowns appear as unknowns, inference tags are visible, and the overall tone — ready, conditional, blocked or unknown — is stated plainly.

**Complementary view:** Release readiness lens.

### US-129: Approve release items safely

**As a reviewer, I want release item approval with self-approval blocked, so no one marks their own work done.**

**Expected experience:** Item rows show candidate status and open requests. Approving requires someone other than the author, and building for a release starts from the release's context.

**Complementary view:** Release item board.

### US-130: Prepare rollout and recovery

**As an operator, I want flags, observation criteria, stop conditions and a realistic revert plan, so launch day has fewer surprises.**

**Expected experience:** Authorization needs, data-recovery limitations and applicable operational readiness are identified. A kill switch is never described as undoing completed external effects.

**Complementary view:** Release readiness and recovery checklist.

### US-131: Connect production anomalies to prior changes

**As an operator, I want anomalies linked to the changes and expectations that preceded them, so "was it always like this?" has an answer path.**

**Expected experience:** Observations link to the originating request, the deployment and the evidence. Historical passing tests remain recorded while current claims may become challenged, and monitoring gaps are never shown as healthy status.

**Complementary view:** Deployment/incident/change timeline.

## Editor and agent integrations

### US-132: Share editor context on my terms

**As a developer, I want the product to know what I am looking at in my editor only when I allow it, so focus follows me without surveillance.**

**Expected experience:** A visible toggle turns sharing on or off with a clear status indicator. When on, only file locations and line numbers are shared — never file contents — and the product refuses to talk to anything not running on my machine.

**Complementary view:** Privacy status and context chips.

### US-133: Navigate definitions and references from the editor

**As a developer, I want definitions and references reachable without leaving my editor, so context switches stay cheap.**

**Expected experience:** Results show confidence tiers and the package path taken, and never fabricate a symbol. Unindexed words offer an honest fallback, and compatibility between editor and server is checked before things misbehave.

**Complementary view:** Reference picker with tiers.

### US-134: Give coding agents evidence-shaped answers

**As a tool builder, I want my coding agent to ask impact, callers, tests and explanation questions and get answers that state how they are known, so automation inherits the honesty rules.**

**Expected experience:** Agent-facing tools are read-only, budgeted and refuse anything outside their declared scope. Results lead with claim classes, citations, gaps, the revision they describe and what was withheld, and staleness is stated rather than hidden.

**Complementary view:** Agent tool results with claim-class headers.

## Privacy, models and product control

### US-135: Work locally by default

**As a repository owner, I want the product fully functional without any cloud dependency, so my code never leaves my machine by accident.**

**Expected experience:** Analysis, routing and answers work offline. With no model available the product degrades to a deterministic mode that says what it can and cannot do — it never stops being honest.

**Complementary view:** Local mode status.

### US-136: Decide what may leave my machine

**As a repository owner, I want per-repository control over any external model use, with every send logged, so egress is a decision, not a default.**

**Expected experience:** Until approved, nothing is sent to hosted providers. Approval explains what is sent and what never is, removes secret-shaped values rather than masking them, never sends history or authorship, and warns visibly when a choice would move data off the machine. An inspectable, tamper-evident log records every send.

**Complementary view:** Privacy settings and audit log.

### US-137: Switch models and understand model failures

**As a developer, I want to choose among available models live and get plain guidance when they fail, so model trouble is an inconvenience, not a mystery.**

**Expected experience:** A status chip lists what is available and switches without restart. Every model output is checked against expectations before use, and provider errors map to plain guidance without leaking raw responses.

**Complementary view:** Model status chip and error guidance.

### US-138: Control cost, progress and cancellation

**As a developer, I want to understand work status and stop expensive or unhelpful analysis, so the product respects my budget.**

**Expected experience:** Running, ready and blocked work is visible with budget limits and a cost meter. Progress is saved on cancellation, and effects already completed are reported honestly — cancellation never claims to undo the irreversible.

**Complementary view:** Progress, resource usage and cancellation report.

### US-139: Manage access, retention and deletion

**As an administrator, I want to manage who can view, share, execute and retain repository-derived information, so governance reaches everything derived, not just the source.**

**Expected experience:** Permission changes apply to accessible answers, artifacts and pending actions; deletion propagates to everything derived; withheld content appears only as generic gaps, and invisible things are counted, never named. Different tenants stay structurally separated.

**Complementary view:** Access, retention and audit views.

## Accessibility and interaction parity

### US-140: Operate everything from the keyboard

**As a keyboard-first developer, I want every interaction reachable without a pointer, so speed and accessibility never conflict.**

**Expected experience:** The canvas is a single tab stop with spatial navigation; matrices, tables, tabs and search follow consistent key patterns, and a complete investigation can be finished with keys alone.

**Complementary view:** Keyboard help for every canvas.

### US-141: Use the product with a screen reader

**As a blind or low-vision developer, I want every element announced with what it is, how it is known and how it connects, so the maps are mine too.**

**Expected experience:** A text outline lists every element and link in reading order with its epistemic state in words, doubling as the fallback when graphics fail. Updates are announced politely and completely.

**Complementary view:** Text outline and accessible tables.

### US-142: Never depend on colour alone

**As a colour-blind developer, I want meaning carried by shape, pattern and words as well as colour, so nothing important is invisible to me.**

**Expected experience:** Provenance and states are multi-encoded everywhere — stroke styles, glyphs, borders and words — with an expandable legend explaining the notation.

**Complementary view:** Notation legend.

### US-143: Read comfortably in any theme

**As a developer, I want light, dark and high-contrast themes with real contrast and calm motion, so long sessions do not hurt.**

**Expected experience:** Contrast targets hold in all themes, including a persistent high-contrast mode that also recolours visuals. Motion can be reduced, and keyboard focus is always clearly visible.

**Complementary view:** Theme settings.

## Honest operation

### US-144: Never see stale answers as current

**As a developer, I want staleness, partialness and failure always distinguishable from certainty, so my trust stays calibrated.**

**Expected experience:** "No results", "not indexed yet", "no language support" and "timed out" are four different answers. Interrupted work publishes nothing, superseded answers are marked, and invisible or denied areas are counted, never named.

**Complementary view:** Freshness and coverage indicators.

### US-145: Get graceful degradation instead of guesses

**As a developer, I want missing data and missing components to produce honest displayed states, so the product never fakes competence.**

**Expected experience:** Without a model, a deterministic mode answers within its limits; without data, views render neutral hatched values; "not determined" is a first-class result. Nothing is silently guessed to fill a gap.

**Complementary view:** Honest empty and degraded states.

### US-146: Trust that interruptions never corrupt state

**As an administrator, I want crashes, retries and restarts to leave state consistent, so reliability is a property, not a hope.**

**Expected experience:** Repeated submissions do not duplicate effects, interrupted runs leave nothing half-applied, updates can be rolled back, and backups and restores are verifiable. Health and version compatibility are visible.

**Complementary view:** Health and lifecycle dashboard.

### US-147: See the product verify itself

**As a maintainer, I want the product's honesty continuously audited by its own tests, so trust is earned by construction.**

**Expected experience:** Acceptance items count as done only when named tests prove them; an executable demo bar re-checks the flagship journeys and that no displayed element is silently mislabelled; evaluation runs report uncertainty and keep synthetic results out of expert statistics. Known gaps and defects are documented as first-class citizens, not hidden.

**Complementary view:** Evaluation dashboard and status report.

## Product-wide acceptance expectations

- Every material repository claim has inspectable evidence or an explicit inference/unknown label.
- Text, visuals and exported summaries preserve the same meaning, scope and caveats.
- Visuals never invent metrics or imply causal certainty from proximity alone.
- The user can always identify the repository/revision, relevant environment and coverage limitations.
- New input or code changes make affected saved conclusions and validations visibly stale.
- No-change, partial, blocked, failed, cancelled and unvalidated outcomes remain distinct from verified completion.
- Readability, keyboard access, clear status wording and recoverable progress apply across the product.
- Generated changes are inspectable before application or publication; builds, checks and tests are evidence, never a guarantee of zero bugs.
- Stories describe outcomes only: architecture, interfaces, algorithms and technology choices are deliberately out of scope.

## Appendix A — Coverage traceability

Map from the source feature-requirements catalogue to this catalogue. Families consolidate their members; purely technical requirements (internal protocols, storage mechanics, test tooling) are covered through the user-visible outcomes they enable rather than as standalone stories.

| Source family | Requirement IDs | Covered by |
|---|---|---|
| Repository ingestion & indexing | IDX-01…11 | US-001, US-002, US-003, US-006, US-007, US-144 |
| Language & framework analysis | ANL-01…20 | US-002, US-015, US-031, US-033, US-035, US-047, US-050, US-051, US-063, US-095, US-136 |
| Evidence, claims & epistemic model | EV-01…09 | US-014, US-015, US-026, US-029, US-092, US-147 |
| Question routing, chat & conversational analysis | RT-01…12 | US-008…US-017, US-025 |
| Sixteen native visualizations | VIS-01…19 | US-031…US-049 |
| Extended chart catalogue | CH-01…30 | US-020 |
| Semantic zoom, canvas & view interaction | ZM-01…16 | US-021…US-026, US-051, US-076, US-102, US-141 |
| Layout engine & quality gates | LY-01…05 | US-030 |
| Investigations — hypothesis & agentic engine | INV-01…09 | US-064…US-070 |
| Runtime & test signals | SIG-01…07 | US-029, US-064, US-075…US-078 |
| Search & cross-repository navigation | SRCH-01…06 | US-004, US-008, US-133, US-144 |
| History, hotspots & archaeology | HIS-01…06 | US-086…US-089 |
| Defects, security & performance analysis | DEF-01…08 | US-072…US-074, US-080, US-097 |
| Profiling | PRF-01…03 | US-082, US-083 |
| Digital twin & counterfactual simulation | TWN-01…08 | US-099…US-103 |
| Concepts, concept cards & hierarchy | CON-01…04 | US-050, US-051 |
| Workspaces, persistence & collaboration | WS-01…05 | US-013, US-091, US-093 |
| Access control, privacy, tenancy & audit | SEC-01…07 | US-132, US-136, US-139 |
| Background jobs, events & reliability | JOB-01…05 | US-006, US-146 |
| Storage, persistence & operations | STO-01…05 | US-139, US-146 |
| Prompt-to-feature | P2F-01…13 | US-104…US-126 |
| PR analysis, releases & campaigns | PR-01…14 | US-124…US-131 |
| VS Code extension | VSC-01…04 | US-132, US-133 |
| MCP agent context server | MCP-01…02 | US-134 |
| Model layer & configuration | MOD-01…05 | US-009, US-135…US-137 |
| Accessibility & keyboard interaction | A11Y-01…05 | US-140…US-143 |
| Evaluation, QA & honesty tooling | QA-01…07 | US-147 |
| Non-functional requirements | NFR-P/A/V/R/C | Acceptance expectations; US-135…US-146 |
