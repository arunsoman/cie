// Story demos → chart specs. Every one of the 33 remaining registry stories
// carries real data from the sample project (collected for real by
// examples/bankapp/scripts/collect_stories.py). This module turns each
// story's demo payload into the same ChatChartSpec the rest of the app
// renders, so bars/donuts/tables/stat cards all keep the engine+layout
// disclosure, the fisheye lens and keyboard parity for free.
//
// Nothing here invents numbers: the demo payload IS the collected data.

import type { CieStory, CieStories, StoryDemo } from "./cie-types";
import { engineFor, layoutFor, type ChatChartKind, type ChatChartSpec } from "./chat-chart";

export interface StoryCard {
  story: CieStory;
  chart: ChatChartSpec | null; // null → honest empty state
}

function cut(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

function cell(v: unknown, n = 90): string {
  if (v === null || v === undefined) return "—";
  const s = String(v);
  return s.length > n ? cut(s, n) : s;
}

/** Stat items arrive as display strings ("84", "1,234", "12.5%") — parse the
 *  real collected number so the headline card shows the measured value, not
 *  a fabricated 0. Non-numeric values ("yes", "n/a") fall back to 0 and the
 *  table below the card still shows the original string verbatim. */
function statNumber(v: string): number {
  const n = Number(String(v).replace(/[,%\s]/g, ""));
  return Number.isFinite(n) ? n : 0;
}

/** Build the chart spec for one story's demo payload. */
export function storyChart(story: CieStory): ChatChartSpec | null {
  const d: StoryDemo = story.demo;

  const base = (kind: ChatChartKind, title: string, tell: string): ChatChartSpec => ({
    kind,
    engine: engineFor(kind),
    layout: layoutFor(kind),
    title,
    tell,
  });

  if (d.shape === "hbar" && d.data?.length) {
    return {
      ...base("hbar", d.title ?? story.title, "From the sample project's collected data — bars sorted biggest first."),
      data: d.data,
      unit: d.unit,
      lens: true,
    };
  }

  if (d.shape === "table" && d.rows?.length && d.columns?.length) {
    return {
      ...base("table", d.columns.length ? story.title : story.title, "Real values from the sample project's collected data."),
      columns: d.columns.map((c) => cut(c, 34)),
      rows: d.rows.map((r) => r.map((c) => cell(c))),
    };
  }

  if (d.shape === "stat" && d.items?.length) {
    return {
      ...base("stat", story.title, d.headline ?? "Measured on the sample project."),
      data: d.items.map((i) => ({ name: i.label, value: statNumber(i.value) })),
      columns: ["Measure", "Value"],
      rows: d.items.map((i) => [i.label, cell(i.value), i.note ?? ""]),
    };
  }

  // steps / timeline / kv / diff all read best as a table too
  if (d.rows?.length && d.columns?.length) {
    return {
      ...base("table", story.title, "Recorded from the sample project's walkthrough."),
      columns: d.columns.map((c) => cut(c, 34)),
      rows: d.rows.map((r) => r.map((c) => cell(c))),
    };
  }

  return null;
}

/** All 33 stories, in registry order, as renderable cards. */
export function buildStoryCards(stories: CieStories | null): StoryCard[] {
  if (!stories?.stories) return [];
  return Object.values(stories.stories)
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((story) => ({ story, chart: storyChart(story) }));
}

/** A one-line "where these numbers came from" for the card header. */
export function sourceLine(story: CieStory): string {
  const kind = story.kind === "measured" ? "Measured" : "Recorded";
  return `${kind}: ${story.source.label}`;
}

/** The extract → transform → load line, in one plain sentence. */
export function etlLine(story: CieStory): string {
  return `From your repo: extract ${story.etl.extract} → transform: ${story.etl.transform} → load: ${story.etl.load}`;
}

export interface StoryAnswer {
  answer: string;
  chart: ChatChartSpec | null;
}

/**
 * Click-to-chat: answer a question about one story straight from the
 * collected data — no AI round-trip, nothing invented. Matches a story id
 * (US-043) or a story's own words ("who knows which part", "spending
 * limit", "was it always like this").
 */
export function tryAnswerStory(question: string, stories: CieStories | null): StoryAnswer | null {
  if (!stories?.stories) return null;
  const all = Object.values(stories.stories);
  const q = question.toLowerCase();

  let story: CieStory | undefined;
  const idMatch = question.match(/US[- ]?(\d{3})/i);
  if (idMatch) {
    const id = `US-${idMatch[1]}`;
    story = all.find((s) => s.id === id);
  }
  if (!story) {
    // fuzzy title match: most distinguishing words of one title appear in the question
    let best: { story: CieStory; score: number } | null = null;
    for (const s of all) {
      const words = s.title
        .toLowerCase()
        .replace(/[^a-z0-9 ]/g, " ")
        .split(/\s+/)
        .filter((w) => w.length > 4);
      if (!words.length) continue;
      const hits = words.filter((w) => q.includes(w)).length;
      const score = hits / words.length;
      if (score >= 0.6 && (!best || score > best.score)) best = { story: s, score };
    }
    if (best) story = best.story;
  }
  if (!story) return null;

  const chart = storyChart(story);
  const lines: string[] = [];
  lines.push(`**${story.id} — ${story.title}**`);
  lines.push(
    story.kind === "measured"
      ? "The numbers here were **measured** by real tools on the sample project — the chart above shows exactly what was collected."
      : "This one is a **recorded walkthrough**: the workflow was really carried out on the sample project and its real artifacts were saved. The record above is that session, not a mock-up."
  );
  lines.push(`**Where it came from:** ${story.source.label} — ${story.source.detail}`);
  lines.push(`**From your repo:** extract ${story.etl.extract}; transform — ${story.etl.transform}; load — ${story.etl.load}.`);
  if (story.real_world_equivalents?.length) {
    lines.push(`**On production projects the same data comes from:** ${story.real_world_equivalents.join(", ")}.`);
  }
  lines.push(`**Still honest work left:** ${story.gap}`);
  return { answer: lines.join("\n\n"), chart };
}
