import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";
import os from "node:os";

/**
 * 测试配置（从 pastePanda 的精简版）。
 *
 * 两个 project 而不是一个大环境：
 * - `node`：纯逻辑（编解码、状态机、字节契约）——跑得快，也**逼着**这些模块不许
 *   偷偷摸 window/document（摸到就红，这是好事）。
 * - `jsdom`：组件与 hook。
 *
 * `.tsx` 一律进 jsdom；`.ts` 默认进 node，需要 DOM 的少数文件列在 DOM_NEEDED_TS
 * 里（判据是实测跑一遍看谁红，不是按文件名猜）。
 */
const GENERIC_EXCLUDE = ["**/node_modules/**", "**/dist/**", "**/target/**", "design/**"];

/** 需要真实 DOM 的 .ts 测试（不在表里的 .ts 跑 node 环境）。 */
const DOM_NEEDED_TS = [
  "src/lib/rcAudio.test.ts",
  "src/lib/rcClipPushErr.test.ts",
  "src/lib/rcFocusRelease.test.ts",
  "src/lib/rcJpegSink.test.ts",
  "src/lib/rcWindowOps.test.ts",
  "src/hooks/useRcAudio.test.ts",
  "src/hooks/useRcBackgroundPause.test.ts",
  "src/hooks/useRcClipboardAuto.test.ts",
  "src/hooks/useRcFrames.gate.test.ts",
  "src/hooks/useRcFrames.native.test.ts",
  "src/hooks/useRcFrames.recovery.test.ts",
  "src/hooks/useRcFrames.wait.test.ts",
  "src/hooks/useRcInput.test.ts",
  "src/hooks/useRcNearbyPair.test.ts",
  "src/hooks/useRcSessionPrefs.test.ts",
  "src/hooks/useRcSessionAudio.test.ts",
  "src/hooks/useWindowVisible.test.ts",
];

const alias = {
  "@": path.resolve(__dirname, "./src"),
  "@tauri-apps/api/core": path.resolve(__dirname, "./src/__mocks__/@tauri-apps-api-core.ts"),
  "@tauri-apps/api/event": path.resolve(__dirname, "./src/__mocks__/@tauri-apps-api-event.ts"),
  "@tauri-apps/api/window": path.resolve(__dirname, "./src/__mocks__/@tauri-apps-api-window.ts"),
};

const SETUP_SHARED = path.resolve(__dirname, "./src/test-setup.ts");
const SETUP_DOM = path.resolve(__dirname, "./src/test-setup.dom.ts");

export default defineConfig({
  plugins: [react()],
  resolve: { alias },
  test: {
    globals: true,
    exclude: GENERIC_EXCLUDE,
    pool: "forks",
    // 按内存而不是核数封顶：默认 cpus-1 个 fork 在内存紧张的机器上会撞 vitest
    // 硬编码的 worker 启动闸，表现是随机判红（详见 pastePanda 的 vitest.config.ts）。
    maxWorkers: Math.min(10, Math.max(1, (os.availableParallelism?.() ?? os.cpus().length) - 1)),
    // 默认 5s 在满载时会被「fake timers 推进 60s 虚拟时间」这类 CPU 密集用例撞穿，
    // 而超时是**中断**执行：残留 fake timers 会让同文件后续用例全渲染成空壳。
    testTimeout: 15_000,
    projects: [
      {
        extends: true,
        test: {
          name: "node",
          environment: "node",
          setupFiles: [SETUP_SHARED],
          include: ["**/*.test.ts", "**/*.spec.ts"],
          exclude: [...GENERIC_EXCLUDE, ...DOM_NEEDED_TS],
        },
      },
      {
        extends: true,
        test: {
          name: "jsdom",
          environment: "jsdom",
          setupFiles: [SETUP_SHARED, SETUP_DOM],
          include: ["**/*.test.tsx", "**/*.spec.tsx", ...DOM_NEEDED_TS],
          exclude: GENERIC_EXCLUDE,
        },
      },
    ],
  },
});
