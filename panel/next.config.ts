import type { NextConfig } from "next";

const config: NextConfig = {
    output: "export",
    poweredByHeader: false,
    images: { unoptimized: true },
    experimental: { cpus: 2 },
};

export default config;
