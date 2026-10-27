import type { Metadata } from "next";
import { GeistSans as geistSans } from "geist/font/sans";
import { GeistMono as geistMono } from "geist/font/mono";
import "./globals.css";
import { Toaster } from "@/components/ui/toaster";

export const metadata: Metadata = {
  title: "CIE — Your code, in plain words",
  description:
    "CIE reads your code and explains it in plain words: a map of how it fits together, the promises it makes, and what to fix first. No technical words needed.",
  keywords: ["CIE", "code intelligence", "plain English", "code map", "code health"],
  authors: [{ name: "Arun Soman" }],
  openGraph: {
    title: "CIE — Your code, in plain words",
    description: "See what your code really does, without reading a single line.",
    siteName: "CIE",
    type: "website",
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      {/* suppressHydrationWarning on body too: browser extensions (Dark
          Reader, translators, …) inject style/class/data attributes onto
          <body> before React hydrates, and React would otherwise report
          "attributes of the server rendered HTML didn't match" at this
          element. It only covers this element's own attributes — anything
          our components render inside children still reports mismatches
          normally, so real bugs in the app are not silenced by this. */}
      <body
        suppressHydrationWarning
        className={`${geistSans.variable} ${geistMono.variable} antialiased bg-background text-foreground`}
      >
        {children}
        <Toaster />
      </body>
    </html>
  );
}
