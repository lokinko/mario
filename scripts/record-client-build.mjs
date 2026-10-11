import { createHash } from "node:crypto";
import {
  readFileSync,
  writeFileSync,
  existsSync,
  statSync,
  readdirSync,
} from "node:fs";
import path from "node:path";
import { clientSource, projectRoot } from "./client-source.mjs";

const [platform, ...files] = process.argv.slice(2);
if (!["macos", "android", "windows", "web"].includes(platform) || !files.length)
  throw new Error(
    "Usage: record-client-build.mjs macos|android|windows|web outputs/file ...",
  );
const build = JSON.parse(
  readFileSync(path.join(projectRoot, "client/dist/build-info.json"), "utf8"),
);
const current = clientSource();
if (build.sourceFingerprint !== current.sourceFingerprint)
  throw new Error(
    "源码在前端构建后发生变化，请重新构建，避免标记错误的客户端版本。",
  );
if (build.target !== (platform === "web" ? "web" : "native"))
  throw new Error("客户端构建目标不匹配，请使用对应的平台构建入口。");

const manifestPath = path.join(projectRoot, "outputs/build-manifest.json");
const previous = existsSync(manifestPath)
  ? JSON.parse(readFileSync(manifestPath, "utf8"))
  : {};
const artifacts = (previous.artifacts || []).map((artifact) => ({
  ...artifact,
  source: artifact.source || {
    sourceCommit: previous.sourceCommit,
    sourceFingerprint: null,
  },
}));

function digest(fullPath) {
  if (!statSync(fullPath).isDirectory()) {
    const data = readFileSync(fullPath);
    return {
      bytes: data.length,
      sha256: createHash("sha256").update(data).digest("hex"),
    };
  }
  const hash = createHash("sha256");
  let bytes = 0;
  const walk = (directory, prefix = "") => {
    for (const name of readdirSync(directory).sort()) {
      const file = path.join(directory, name);
      const relative = `${prefix}${name}`;
      if (statSync(file).isDirectory()) walk(file, `${relative}/`);
      else {
        const data = readFileSync(file);
        bytes += data.length;
        hash.update(relative).update("\0").update(data).update("\0");
      }
    }
  };
  walk(fullPath);
  return { bytes, sha256: hash.digest("hex"), kind: "directory" };
}

for (const file of files) {
  const fullPath = path.resolve(projectRoot, file);
  const relative = path.relative(path.join(projectRoot, "outputs"), fullPath);
  if (relative.startsWith("..") || path.isAbsolute(relative))
    throw new Error("Artifact must be inside outputs/.");
  const entry = {
    file: relative.replaceAll("\\", "/"),
    platform,
    ...digest(fullPath),
    source: build,
  };
  const index = artifacts.findIndex((artifact) => artifact.file === entry.file);
  if (index >= 0) artifacts[index] = entry;
  else artifacts.push(entry);
}
for (const artifact of artifacts)
  artifact.status =
    artifact.source.sourceFingerprint === current.sourceFingerprint
      ? "current"
      : "stale";
writeFileSync(
  manifestPath,
  JSON.stringify(
    {
      schemaVersion: 2,
      expectedSource: current,
      artifacts,
    },
    null,
    2,
  ) + "\n",
);
console.log(`Recorded ${platform} artifacts in outputs/build-manifest.json`);
