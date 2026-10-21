import path from "node:path";
import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "standalone",
  /* config options here */
  typescript: {
    ignoreBuildErrors: true,
  },
  reactStrictMode: false,
  // RFC-0024 stage 1: the adapter package ships TS source; the page
  // bundles it (headless-pure — no DOM, no engine import inside it).
  transpilePackages: ["viz-adapter"],
  // The repo root is the workspace root: viz-adapter lives beside web/.
  // ABSOLUTE on purpose — a relative "../.." resolves against process.cwd() at
  // runtime, so a dev server started from anywhere else silently re-roots the
  // CSS pipeline at a directory without node_modules and every compile dies
  // with "Can't resolve 'tailwindcss' in /home/arun/2cie/cie".
  turbopack: {
    root: path.resolve(__dirname, ".."),
  },
};

export default nextConfig;
