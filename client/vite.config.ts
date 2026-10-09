import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const base = loadEnv(mode, ".", "VITE_").VITE_BASE_PATH || "/";
  if (!/^\/(?:[A-Za-z0-9_-]+\/)*$/.test(base))
    throw new Error(
      "VITE_BASE_PATH 必须为 / 或 /mario/ 这样的路径，且以 / 结尾",
    );
  return {
    base,
    plugins: [react()],
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      watch: { ignored: ["**/src-tauri/**"] },
    },
  };
});
