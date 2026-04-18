/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: false, // Disabled for BlockNote compatibility
  // Static export only for production builds (consumed by Tauri).
  // In dev mode, this would cause hydration mismatches and ChunkLoadErrors in the WebView.
  output: process.env.NODE_ENV === 'production' ? 'export' : undefined,
  images: {
    unoptimized: true,
  },
  // basePath stays empty; assetPrefix removed because '/' produced odd asset URLs
  // in the Tauri WebView and is unnecessary for both dev and the static export.
  basePath: '',

  // Add webpack configuration for Tauri.
  // Note: when running with `next dev --turbo` (default dev script), this whole webpack
  // function is ignored — Turbopack uses its own resolver. The block below only applies
  // to `npm run dev:webpack` and to production builds (`next build`).
  webpack: (config, { isServer, dev }) => {
    if (!isServer) {
      config.resolve.fallback = {
        ...config.resolve.fallback,
        fs: false,
        path: false,
        os: false,
      };
      if (dev) {
        // Tauri's WebView opens as soon as localhost:3118 responds, but Next.js compiles
        // routes lazily on first request. Heavy deps (BlockNote/Remirror/TipTap/Radix)
        // can take a while to compile on the first hit, especially after a refactor that
        // invalidated the .next cache. Bump the chunk-load timeout to 5 min so we don't
        // get a misleading "ChunkLoadError: timeout" mid-compile.
        config.output.chunkLoadTimeout = 300_000;
      }
    }
    return config;
  },
}

module.exports = nextConfig
