// "What CIE can't do yet" — the honest menu of the 33 remaining Planned
// stories from the engine's own story registry (github.com/arunsoman/cie,
// cie-stories/src/registry.rs, v1.2.0: 114 delivered / 33 planned).
//
// Every entry carries: the plain-words title, what you would get, and WHY it
// is not here yet — mirroring the engine's honesty rules (a Planned story
// never claims capabilities, and its note always says why). Titles/notes are
// hand-translated into plain words; story ids point back at the registry.

export interface NotYetStory {
  id: string; // registry id, e.g. "US-005"
  theme: ThemeId;
  title: string; // plain-words title
  get: string; // what you would get, in plain words
  why: string; // why it is not here yet — always honest
}

export type ThemeId =
  | "bring"
  | "chat"
  | "maps"
  | "runtime"
  | "speed"
  | "team"
  | "experiments"
  | "plan"
  | "build"
  | "ship"
  | "release"
  | "editor";

export const THEMES: { id: ThemeId; label: string; blurb: string }[] = [
  { id: "bring", label: "Bringing code in", blurb: "Getting code and updates into CIE" },
  { id: "chat", label: "Talking with CIE", blurb: "Back-and-forth conversation" },
  { id: "maps", label: "Maps and pictures", blurb: "More ways to see the code" },
  { id: "runtime", label: "Watching it run", blurb: "The live system, not just the files" },
  { id: "speed", label: "Speed and slowness", blurb: "Measured performance stories" },
  { id: "team", label: "Team history and privacy", blurb: "People, knowledge and privacy" },
  { id: "experiments", label: "What-if experiments", blurb: "Trying changes in a sandbox" },
  { id: "plan", label: "From idea to plan", blurb: "Starting a feature from plain words" },
  { id: "build", label: "Building the change", blurb: "Turning a plan into a reviewable change" },
  { id: "ship", label: "Checking and shipping", blurb: "Validation, patches and pull requests" },
  { id: "release", label: "Release day", blurb: "Approvals, rollouts and after" },
  { id: "editor", label: "Your editor", blurb: "Working where you already work" },
];

export const NOT_YET: NotYetStory[] = [
  // ---- Bringing code in ----
  {
    id: "US-005",
    theme: "bring",
    title: "Pull updates straight from GitHub or GitLab",
    get: "CIE would fetch new commits and pull requests from the hosting service on request, and show whether the connection and keys are healthy — anything suspicious held aside for you, never silently dropped.",
    why: "CIE is offline-first by design: today it only reads what is on your machine. Reaching out to the hosting service is scheduled work, not shipped yet.",
  },
  // ---- Talking with CIE ----
  {
    id: "US-012",
    theme: "chat",
    title: "Follow-ups like “show only these” or “what about this file?”",
    get: "You could keep talking without repeating yourself. Context chips would show what CIE thinks you mean, and phrases like “ignore X”, “pin X” or “zoom out” would work as commands.",
    why: "The engine already has the scoped queries this would drive; the conversation steering on top is product work that isn't built yet.",
  },
  // ---- Maps and pictures ----
  {
    id: "US-043",
    theme: "maps",
    title: "A map of who knows which part",
    get: "Code arranged by owner, with thinly-known and stale areas highlighted — so “what if Alice leaves?” becomes a picture instead of a hunch.",
    why: "CIE reads code, not people: there is no ownership or authorship analysis yet, and a fair version would need your declared ownership policy as input.",
  },
  {
    id: "US-047",
    theme: "maps",
    title: "Point performance measurements at the exact code",
    get: "Hotspots from real profiler runs drawn onto the map, with uncertainty shown and unmeasured parts left honestly unknown.",
    why: "There is no profile import yet — and CIE refuses to invent hotspots it did not measure.",
  },
  {
    id: "US-063",
    theme: "maps",
    title: "Question a compiled binary, not just source code",
    get: "Formats, embedded metadata, linked libraries and exported symbols of a built file — every claim marked as seen, derived or guessed, and unanswerable questions listed as unresolved rather than answered creatively.",
    why: "CIE is a static source reader; probing binary bytes is outside its current scope.",
  },
  // ---- Watching it run ----
  {
    id: "US-075",
    theme: "runtime",
    title: "Lay live system activity over the code map",
    get: "You would see which parts actually ran in a chosen time window — exact matches labelled exact, everything weaker labelled unknown.",
    why: "CIE cannot watch a running system yet. A structure-only overlay would pretend to knowledge it does not have, so it stays unbuilt.",
  },
  {
    id: "US-076",
    theme: "runtime",
    title: "Replay a past request, step by step",
    get: "Recorded executions played back next to the source — clearly marked as recordings, with unrecorded values never made up as facts.",
    why: "No execution recorder or replay exists in the engine yet.",
  },
  {
    id: "US-077",
    theme: "runtime",
    title: "Explain “it works in staging” precisely",
    get: "Flags, configuration and deployed versions compared side by side across environments — and exploring a flag change would never touch the real ones.",
    why: "CIE today reads one checkout of code, not your deployments; environment and flag comparison isn't shipped.",
  },
  {
    id: "US-081",
    theme: "runtime",
    title: "Spot how retries and duplicates can break data rules",
    get: "Readers, writers and guarantees linked with candidate failure scenarios — data races kept separate from logic errors, with no promise of finding everything.",
    why: "Distributed-systems hazard modelling is not in the engine yet.",
  },
  // ---- Speed and slowness ----
  {
    id: "US-082",
    theme: "speed",
    title: "Trace why something is slow, down to the code",
    get: "CPU work, waiting, data work and outside calls told apart, with hotspots carrying uncertainty and guesses labelled as guesses.",
    why: "This needs imported traces and profiles. CIE refuses to fabricate slowness stories, so until that import lands, it stays an honest silence.",
  },
  {
    id: "US-083",
    theme: "speed",
    title: "Prove “faster” with the conditions stated",
    get: "Two measured runs compared under a declared equivalence — unfair comparisons refused with evidence, and failed runs kept in the count.",
    why: "No profile import yet, so there is nothing measured to compare.",
  },
  {
    id: "US-084",
    theme: "speed",
    title: "Demonstrate a slowdown before and after a change",
    get: "Baseline versus candidate with workload differences, errors and uncertainty visible — dropping work could never be dressed up as an improvement.",
    why: "Same missing piece: performance profiles. The comparison rules exist as stories; the data feed does not.",
  },
  // ---- Team history and privacy ----
  {
    id: "US-088",
    theme: "team",
    title: "History insights without exposing anyone",
    get: "Analysis that protects identities at the source, never ranks individuals, and counts denied paths without naming them.",
    why: "History analysis itself isn't shipped — there are no contributor fields anywhere to protect yet. Privacy-at-source is a standing design rule for when it lands.",
  },
  // ---- What-if experiments ----
  {
    id: "US-101",
    theme: "experiments",
    title: "Compare sandbox experiments before deciding",
    get: "Baseline and candidate runs (failures included) compared fairly; a certificate only when the evidence meets the bar — inconclusive stays inconclusive.",
    why: "Running sandboxed experiments is beyond the offline engine. It refuses overreach rather than fake it.",
  },
  // ---- From idea to plan ----
  {
    id: "US-104",
    theme: "plan",
    title: "Start a feature from a plain-language prompt",
    get: "Type a rough idea — one sentence, a short brief or a full spec — and CIE would identify the target repo and deliverable without making you learn internal names.",
    why: "Feature-request intake is product layer, not engine.",
  },
  {
    id: "US-105",
    theme: "plan",
    title: "Be asked only what actually matters",
    get: "Clarification questions that each explain why they matter, with independent work continuing while you answer. Silence would never count as a yes for an important decision.",
    why: "The clarification flow is product layer.",
  },
  {
    id: "US-108",
    theme: "plan",
    title: "An approval-worthy brief before any code",
    get: "Scope, actors, rules, failure paths and acceptance criteria — requirements separated from guesses, every line carrying its origin, correctable at any time.",
    why: "Feature-contract assembly is product layer.",
  },
  {
    id: "US-110",
    theme: "plan",
    title: "Change your mind without losing work",
    get: "Edit an answer mid-build and see exactly which tasks and results are affected — finished, reusable work stays usable with its scope explained.",
    why: "The scope-revision workflow is product layer.",
  },
  // ---- Building the change ----
  {
    id: "US-111",
    theme: "build",
    title: "A reviewable change built in isolation",
    get: "A candidate built away from your work, with target, progress and failures visible — staying inside the authorized scope, never touching your files uninvited.",
    why: "The engine never mutates working trees by design; the candidate-build workflow around that guarantee isn't built yet.",
  },
  {
    id: "US-112",
    theme: "build",
    title: "A six-step wizard that survives interruptions",
    get: "Describe → Clarify → Plan → Changes → Validate → Deliver as saved stages; viewing a stage is never confused with approving an action, and work stops checkpoint-safely.",
    why: "The feature wizard is product layer.",
  },
  {
    id: "US-115",
    theme: "build",
    title: "Try the feature before accepting it",
    get: "Successful, denied and failure journeys you can walk through, with role views, sample data and mock-ups always labelled as such.",
    why: "Running a preview isn't in the engine — CIE will not claim behaviour it has not executed.",
  },
  {
    id: "US-116",
    theme: "build",
    title: "The bill of consequences a feature adds",
    get: "New packages, configuration, migrations and their recovery limits explained — a local test migration never sold as production-proof.",
    why: "CIE reads today's dependencies (the Dependency page), but reviewing the before/after a feature introduces isn't shipped yet.",
  },
  // ---- Checking and shipping ----
  {
    id: "US-117",
    theme: "ship",
    title: "Real build results for every target",
    get: "Actual compile outcomes per target with the exact commands shown — one target's success could not hide another target's failure.",
    why: "Building code is outside the offline static engine; it would honestly refuse rather than guess at results.",
  },
  {
    id: "US-121",
    theme: "ship",
    title: "A disciplined fix-and-recheck loop",
    get: "Failures investigated, the candidate revised, stale results invalidated — and a passing rerun unable to hide other failing checks.",
    why: "The fix-request loop is product layer.",
  },
  {
    id: "US-122",
    theme: "ship",
    title: "Take the change as a patch file",
    get: "A downloadable patch with base context and a change inventory, unmistakably labelled if review-only or incomplete.",
    why: "Patch export is product layer.",
  },
  {
    id: "US-123",
    theme: "ship",
    title: "Apply a patch with eyes open",
    get: "Conflicts, divergence and local edits explained before anything changes — a clean textual apply never mistaken for a behaviour match.",
    why: "Apply-to-destination is product layer.",
  },
  {
    id: "US-124",
    theme: "ship",
    title: "A draft pull request where the code lives",
    get: "Draft-only publishing with reviewer feedback mapped back to requirements; merging and deploying stay separate, separately-authorized actions.",
    why: "Draft-PR creation is product layer.",
  },
  {
    id: "US-125",
    theme: "ship",
    title: "An audit trail from request to file",
    get: "A root trail connecting the prompt, decisions, files, tests, commits and pull requests — sensitive input redacted, untracked changes never called verified.",
    why: "Mutation tracing is product layer.",
  },
  {
    id: "US-126",
    theme: "ship",
    title: "Several related changes, managed without chaos",
    get: "Dependencies, conflicting work, child changes and partial failures all visible — unmerged work never presented as shipped.",
    why: "Multi-change coordination is product layer.",
  },
  // ---- Release day ----
  {
    id: "US-129",
    theme: "release",
    title: "Approvals that can't be self-served",
    get: "A release board where nobody marks their own work done — approving requires someone other than the author, and release builds start from the release's context.",
    why: "The approval workflow is product layer.",
  },
  {
    id: "US-130",
    theme: "release",
    title: "Launch-day checklists that assume real failure",
    get: "Flags, observation criteria, stop conditions and a realistic revert plan — with a kill switch never described as undoing what is already done.",
    why: "Rollout and recovery planning is product layer.",
  },
  {
    id: "US-131",
    theme: "release",
    title: "“Was it always like this?” — answered",
    get: "Production anomalies linked to the changes and expectations that preceded them, with monitoring gaps never shown as healthy status.",
    why: "This needs the runtime feed (see “Lay live system activity over the code map” above) — no runtime ingest yet.",
  },
  // ---- Your editor ----
  {
    id: "US-132",
    theme: "editor",
    title: "CIE follows your focus — only when you allow it",
    get: "A visible toggle: when on, only file locations and line numbers are shared, never file contents, and only to things running on your own machine.",
    why: "Editor integrations aren't shipped yet.",
  },
];

// Registry facts as of v1.2.0 (cie-stories/src/registry.rs): 147 stories —
// 113 delivered + 1 honest partial (licences, US-098) + 33 planned.
export const REGISTRY_TOTAL = 147;
export const REGISTRY_DELIVERED = 113;
export const REGISTRY_PARTIAL = 1;

export const NOT_YET_COUNT = NOT_YET.length; // 33
