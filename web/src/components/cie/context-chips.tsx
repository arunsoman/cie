"use client";
import { useCie } from "@/lib/cie-store";
export function ContextChips() {
  const { chatContext, removeChatContext, clearChatContext } = useCie();
  if (!chatContext.length) return null;
  return (
    <div className="px-4 py-2 border-t space-y-1" aria-label="Answer context">
      <p className="text-xs text-stone-500">
        Answers use these selections ·{" "}
        <button type="button" className="underline" onClick={clearChatContext}>
          Answer from the whole project
        </button>
      </p>
      <div className="flex gap-1 overflow-x-auto" role="list">
        {chatContext.map((c) => (
          <span
            role="listitem"
            key={c.id}
            className="shrink-0 rounded-full border border-emerald-200 bg-emerald-50 px-2 py-1 text-xs"
            title={
              c.files.length
                ? c.files.join(", ")
                : "No source files recorded; contributes no code space"
            }
          >
            {c.label}{" "}
            <button
              type="button"
              aria-label={`Remove ${c.label} from answer context`}
              onClick={() => removeChatContext(c.id)}
            >
              ×
            </button>
          </span>
        ))}
      </div>
    </div>
  );
}
