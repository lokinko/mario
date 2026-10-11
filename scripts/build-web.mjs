import { execFileSync } from "node:child_process";
import { cpSync, rmSync, mkdirSync } from "node:fs";
import path from "node:path";
import { projectRoot } from "./client-source.mjs";

execFileSync(
  process.platform === "win32" ? "npm.cmd" : "npm",
  ["run", "build", "--prefix", "client"],
  {
    cwd: projectRoot,
    stdio: "inherit",
  },
);
const output = path.join(projectRoot, "outputs/web");
rmSync(output, { recursive: true, force: true });
mkdirSync(path.dirname(output), { recursive: true });
cpSync(path.join(projectRoot, "client/dist"), output, { recursive: true });
execFileSync(
  process.execPath,
  ["scripts/record-client-build.mjs", "web", "outputs/web"],
  {
    cwd: projectRoot,
    stdio: "inherit",
  },
);
