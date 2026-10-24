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
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased bg-background text-foreground`}
      >
        {children}
        <Toaster />
      </body>
    </html>
  );
}
