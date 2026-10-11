import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const projectRoot = fileURLToPath(new URL("../", import.meta.url));

export function clientSource() {
  const git = (...args) =>
    execFileSync("git", args, { cwd: projectRoot, encoding: "utf8" }).trim();
  const files = [
    ...new Set(
      git("ls-files", "--cached", "--others", "--exclude-standard", "-z").split(
        "\0",
      ),
    ),
  ]
    .filter((name) =>
      /^(client\/|server\/|scripts\/|\.github\/workflows\/|\.gitattributes$|package(?:-lock)?\.json$)/.test(
        name,
      ),
    )
    .sort();
  const hash = createHash("sha256");
  for (const name of files) {
    hash.update(name).update("\0");
    const fullPath = path.join(projectRoot, name);
    if (existsSync(fullPath)) hash.update(readFileSync(fullPath));
    else hash.update("<deleted>");
    hash.update("\0");
  }
  return {
    sourceCommit: git("rev-parse", "HEAD"),
    sourceDirty: !!git("status", "--porcelain"),
    sourceFingerprint: hash.digest("hex"),
  };
}
