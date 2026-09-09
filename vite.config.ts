import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { codeInspectorPlugin } from "code-inspector-plugin";

const tauriMockDir = path.resolve(__dirname, "./src/lib/api/tauri-mock");

export default defineConfig(({ command }) => ({
  root: "src",
  plugins: [
    command === "serve" &&
      codeInspectorPlugin({
        bundler: "vite",
      }),
    react(),
  ].filter(Boolean),
  base: "./",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
  server: {
    port: 3000,
    strictPort: true,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:2891",
        changeOrigin: true,
      },
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      // Redirect Tauri imports to browser-compatible mocks
      "@tauri-apps/api/core": path.join(tauriMockDir, "core.ts"),
      "@tauri-apps/api/event": path.join(tauriMockDir, "event.ts"),
      "@tauri-apps/api/window": path.join(tauriMockDir, "window.ts"),
      "@tauri-apps/api/app": path.join(tauriMockDir, "app.ts"),
      "@tauri-apps/api/path": path.join(tauriMockDir, "path.ts"),
      "@tauri-apps/plugin-dialog": path.join(tauriMockDir, "plugin-dialog.ts"),
      "@tauri-apps/plugin-process": path.join(tauriMockDir, "plugin-process.ts"),
      "@tauri-apps/plugin-updater": path.join(tauriMockDir, "plugin-updater.ts"),
    },
  },
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_"],
}));
