import { spawn } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { dirname, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = join(root, "server/target/release", process.platform === "win32" ? "mario-server.exe" : "mario-server");
if (!existsSync(binary) || !existsSync(join(root, "client/dist/index.html"))) {
  console.error("请先运行 npm run web:build"); process.exit(1);
}
// Web uses a separate profile by default; its auth token never enters the web bundle.
const dataDir = process.env.MARIO_DATA_DIR ?? join(root, "outputs/web-data");
mkdirSync(dataDir, { recursive: true, mode: 0o700 });
const tokenFile = join(dataDir, "web-access-key");
let token = process.env.MARIO_AUTH_TOKEN;
if (!token) {
  if (!existsSync(tokenFile)) writeFileSync(tokenFile, randomBytes(32).toString("hex"), { mode: 0o600, flag: "wx" });
  token = readFileSync(tokenFile, "utf8").trim();
  console.log(`访问密钥文件：${tokenFile}`);
}
const port = process.env.MARIO_PORT ?? "4217";
console.log(`Web 地址：http://${process.env.MARIO_HOST ?? "127.0.0.1"}:${port}`);
const child = spawn(binary, ["--port", port], {
  cwd: root,
  env: { ...process.env, MARIO_AUTH_TOKEN: token, MARIO_DATA_DIR: dataDir, MARIO_WEB_DIR: join(root, "client/dist") },
  stdio: "inherit",
});
child.on("error", error => { console.error(error.message); process.exitCode = 1; });
child.on("exit", code => { process.exitCode = code ?? 0; });
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => child.kill(signal));
