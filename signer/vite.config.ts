import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.VITE_HOST ?? "0.0.0.0";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host,
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**", "**/.devenv/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: process.env.TAURI_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
  optimizeDeps: {
    include: ["@ngraveio/bc-ur"],
    exclude: ["konsta/react"],
  },
  resolve: {
    alias: {
      buffer: "buffer/",
    },
  },
});
