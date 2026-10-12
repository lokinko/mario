import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  rmSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  readdirSync,
} from "node:fs";
import path from "node:path";
import { clientSource, projectRoot } from "./client-source.mjs";

const npmCli = process.env.npm_execpath;
if (!npmCli) throw new Error("请使用 npm run web:build 构建 Web。");
execFileSync(process.execPath, [npmCli, "run", "build", "--prefix", "client"], {
  cwd: projectRoot,
  stdio: "inherit",
});
const build = JSON.parse(
  readFileSync(path.join(projectRoot, "client/dist/build-info.json"), "utf8"),
);
const current = clientSource();
if (
  build.target !== "web" ||
  build.sourceFingerprint !== current.sourceFingerprint
)
  throw new Error("源码在前端构建后发生变化，请重新构建。");

const output = path.join(projectRoot, "outputs/web");
rmSync(output, { recursive: true, force: true });
mkdirSync(path.dirname(output), { recursive: true });
cpSync(path.join(projectRoot, "client/dist"), output, { recursive: true });
const hash = createHash("sha256");
let bytes = 0;
function digest(directory, prefix = "") {
  for (const entry of readdirSync(directory, { withFileTypes: true }).sort(
    (a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0),
  )) {
    const relative = `${prefix}${entry.name}`;
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) digest(fullPath, `${relative}/`);
    else {
      const data = readFileSync(fullPath);
      bytes += data.length;
      hash.update(relative).update("\0").update(data).update("\0");
    }
  }
}
digest(output);
writeFileSync(
  path.join(projectRoot, "outputs/build-manifest.json"),
  JSON.stringify(
    {
      schemaVersion: 2,
      expectedSource: current,
      artifacts: [
        {
          file: "web",
          platform: "web",
          kind: "directory",
          bytes,
          sha256: hash.digest("hex"),
          source: build,
          status: "current",
        },
      ],
    },
    null,
    2,
  ) + "\n",
);
console.log(
  "Web artifacts: outputs/web; checksum: outputs/build-manifest.json",
);
