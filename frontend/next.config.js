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

  // Add webpack configuration for Tauri
  webpack: (config, { isServer }) => {
    if (!isServer) {
      config.resolve.fallback = {
        ...config.resolve.fallback,
        fs: false,
        path: false,
        os: false,
      };
    }
    return config;
  },
}

module.exports = nextConfig
