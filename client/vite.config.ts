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
    build: {
      rollupOptions: {
        output: {
          manualChunks(id) {
            if (!id.includes("node_modules")) return;
            if (
              id.includes("/node_modules/react/") ||
              id.includes("/node_modules/react-dom/") ||
              id.includes("/node_modules/scheduler/")
            ) {
              return "react-vendor";
            }
            if (
              id.includes("/node_modules/@ant-design/cssinjs") ||
              id.includes("/node_modules/@ant-design/colors") ||
              id.includes("/node_modules/@ant-design/fast-color") ||
              id.includes("/node_modules/@ant-design/icons") ||
              id.includes("/node_modules/antd/es/theme/") ||
              id.includes("/node_modules/antd/es/config-provider/") ||
              id.includes("/node_modules/antd/es/button/") ||
              id.includes("/node_modules/antd/es/input/") ||
              id.includes("/node_modules/antd/es/alert/") ||
              id.includes("/node_modules/antd/es/tag/") ||
              id.includes("/node_modules/antd/es/locale/") ||
              id.includes("/node_modules/antd/es/_util/") ||
              id.includes("/node_modules/antd/es/style/")
            ) {
              return "antd-core";
            }
            if (id.includes("/node_modules/lucide-react/")) {
              return "icons-vendor";
            }
          },
        },
      },
    },
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
      // Bound jsdom/Ant Design workers so UI tests do not compete for all host CPUs.
      maxWorkers: 4,
    },
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
    },
  };
});
