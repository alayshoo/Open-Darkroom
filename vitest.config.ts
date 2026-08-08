// vitest.config.ts
//
// Two projects:
//   unit    — pure TypeScript, no DOM, no GPU. Milliseconds.
//   browser — the frontend GPU pipeline against real WebGPU in Chromium.
//
// The heavy numerical work lives in the Rust suite (src-tauri/tests), which
// drives the same WGSL headlessly. The browser project exists to prove the
// TypeScript wiring — buffer sizes, bind groups, slider packing — agrees with
// it, so it stays deliberately small.

import { defineConfig } from "vitest/config";
import { playwright } from "@vitest/browser-playwright";
import { fileURLToPath } from "node:url";

const alias = {
  $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
};

export default defineConfig({
  resolve: { alias },
  test: {
    projects: [
      {
        resolve: { alias },
        test: {
          name: "unit",
          environment: "node",
          include: ["tests/unit/**/*.test.ts"],
        },
      },
      {
        resolve: { alias },
        test: {
          name: "browser",
          include: ["tests/browser/**/*.test.ts"],
          browser: {
            enabled: true,
            provider: playwright({
              launchOptions: {
                // Playwright's Chromium ships --disable-gpu, which leaves
                // navigator.gpu defined but with no adapter behind it.
                ignoreDefaultArgs: ["--disable-gpu"],
              },
            }),
            // Headed on purpose: headless Chromium returns no WebGPU adapter on
            // this platform under any flag combination tried (swiftshader,
            // Vulkan, ANGLE), while headed gets the real hardware device. A
            // browser window appears for a few seconds during the run.
            headless: false,
            instances: [{ browser: "chromium" }],
          },
        },
      },
    ],
  },
});
