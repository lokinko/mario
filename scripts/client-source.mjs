import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, existsSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const projectRoot = fileURLToPath(new URL("../", import.meta.url));

export function clientSource(directory = projectRoot) {
  const git = (...args) =>
    execFileSync("git", args, { cwd: directory, encoding: "utf8" }).trim();
  const files = [
    ...new Set(
      // Query tracked and untracked paths separately: combined output may include
      // directory placeholders on Git for Windows, omitting tracked descendants.
      [
        ...git("ls-files", "--cached", "-z").split("\0"),
        ...git("ls-files", "--others", "--exclude-standard", "-z").split("\0"),
      ],
    ),
  ]
    .filter((name) =>
      /^(client\/|server\/|scripts\/|\.gitattributes$|package(?:-lock)?\.json$)/.test(
        name,
      ),
    )
    .filter(
      (name) =>
        !existsSync(path.join(directory, name)) ||
        statSync(path.join(directory, name)).isFile(),
    )
    .sort();
  const hash = createHash("sha256");
  for (const name of files) {
    hash.update(name).update("\0");
    const fullPath = path.join(directory, name);
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
