/// <reference types="vitest/config" />
import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import { clientSource } from "../scripts/client-source.mjs";
import { readFileSync } from "node:fs";

export default defineConfig(({ mode }) => {
  const source = clientSource();
  const base = loadEnv(mode, ".", "VITE_").VITE_BASE_PATH || "/";
  if (!/^\/(?:[A-Za-z0-9_-]+\/)*$/.test(base))
    throw new Error(
      "VITE_BASE_PATH 必须为 / 或 /mario/ 这样的路径，且以 / 结尾",
    );
  return {
    base,
    plugins: [
      react(),
      {
        name: "mario-build-info",
        generateBundle() {
          this.emitFile({
            type: "asset",
            fileName: "build-info.json",
            source:
              JSON.stringify(
                {
                  version: JSON.parse(
                    readFileSync(
                      new URL("./package.json", import.meta.url),
                      "utf8",
                    ),
                  ).version,
                  ...source,
                  target: "web",
                  basePath: base,
                  builtAt: new Date().toISOString(),
                },
                null,
                2,
              ) + "\n",
          });
        },
      },
    ],
    test: {
      setupFiles: ["./src/test/setup.ts"],
    },
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
    },
  };
});
