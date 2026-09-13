import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const env = { ...process.env };
if (process.platform === "darwin") {
  env.DEVELOPER_DIR = "/Library/Developer/CommandLineTools";
  env.SDKROOT = env.DEVELOPER_DIR + "/SDKs/MacOSX.sdk";
  env.CC = env.DEVELOPER_DIR + "/usr/bin/clang";
  env.CXX = env.DEVELOPER_DIR + "/usr/bin/clang++";
}
for (const [command, args] of [[process.platform === "win32" ? "npm.cmd" : "npm", ["run", "build", "--prefix", "client"]], ["cargo", ["build", "--release", "--locked", "--manifest-path", "server/Cargo.toml"]]]) {
  const result = spawnSync(command, args, { cwd: root, env, stdio: "inherit", shell: process.platform === "win32" });
  if (result.error || result.status !== 0) process.exit(result.status || 1);
}
