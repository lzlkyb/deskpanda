import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;

// ⚠️ 端口用 1430 而不是壳子模板的 1420：本机同时开着 pastePanda 与 nc-workbench
// 两个同源模板的项目，它们的 `vite` 都占 1420，`strictPort` 下 dev 直接起不来
// （报 "Port 1420 is already in use"，很容易被误读成代码问题）。
// 改这里必须同步 `src-tauri/tauri.conf.json` 的 `build.devUrl`，两处不一致的表现是
// 窗口白屏（webview 去连一个没人监听的端口）。
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // `@/` 别名与 tsconfig 的 paths 必须一致：vite 不读 tsconfig，
  // 少了这一行 tsc 绿而 dev/build 全红（"Failed to resolve import"）。
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1431 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // E-P2-6: 代码分割——vendor 拆离主 bundle
  build: {
    target: "esnext",
    rollupOptions: {
      output: {
        manualChunks: {
          "vendor-react": ["react", "react-dom"],
          "vendor-query": ["@tanstack/react-query"],
          "vendor-tauri": ["@tauri-apps/api"],
        },
      },
    },
  },
});
