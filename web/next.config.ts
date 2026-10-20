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
  turbopack: {
    root: "..",
  },
};

export default nextConfig;
