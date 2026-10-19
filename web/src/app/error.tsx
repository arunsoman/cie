"use client";

// Graceful recovery screen — if a render crashes anywhere in the dashboard,
// this boundary keeps the app alive with an honest explanation and a way
// back, instead of the browser's dead "Application error" page. Nothing is
// guessed here: the failure is named, the data behind it is untouched.

import { useEffect } from "react";

export default function ErrorBoundary({ error, reset }: { error: Error & { digest?: string }; reset: () => void }) {
  useEffect(() => {
    // Surfaced in the browser console for anyone debugging; the page itself
    // stays plain-words.
    console.error("CIE hit a rendering problem:", error);
  }, [error]);

  return (
    <div className="min-h-[60vh] flex items-center justify-center px-4">
      <div className="w-full max-w-md rounded-2xl border border-stone-200 bg-white shadow-sm p-6 space-y-3 text-center">
        <div className="mx-auto flex h-11 w-11 items-center justify-center rounded-full bg-rose-50 border border-rose-200">
          <svg viewBox="0 0 24 24" className="h-5 w-5 text-rose-600" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
            <circle cx="12" cy="12" r="9" />
            <path d="M12 8v4.5" strokeLinecap="round" />
            <circle cx="12" cy="16" r="0.5" fill="currentColor" />
          </svg>
        </div>
        <h2 className="text-lg font-semibold text-stone-900">Something here didn&apos;t render properly</h2>
        <p className="text-sm text-stone-600 leading-relaxed">
          Your project data is safe — this is a display problem, not lost work. You can retry this view, or go back
          to the start and pick a page that works.
        </p>
        {error?.digest && (
          <p className="text-xs text-stone-400 font-mono">ref: {error.digest}</p>
        )}
        <div className="flex flex-col sm:flex-row gap-2 justify-center pt-1">
          <button
            type="button"
            onClick={reset}
            className="h-10 rounded-xl bg-emerald-700 hover:bg-emerald-800 text-white text-sm font-medium px-5"
          >
            Try again
          </button>
          <button
            type="button"
            onClick={() => {
              window.location.href = "/";
            }}
            className="h-10 rounded-xl border border-stone-300 text-stone-700 hover:bg-stone-50 text-sm font-medium px-5"
          >
            Back to the start
          </button>
        </div>
      </div>
    </div>
  );
}
