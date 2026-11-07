// Build and acceptance scripts share a Cargo target rooted at this checkout.
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { existsSync, realpathSync } from 'node:fs';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export function cargoTargetDir(env = process.env, fallback = 'server/target') {
  const configured = env.CARGO_TARGET_DIR;
  return resolve(root, configured?.trim() ? configured : fallback);
}

export function serverBinary({ env = process.env, targetDir = cargoTargetDir(env), platform = process.platform, binary = env.MARIO_TEST_SERVER_BIN } = {}) {
  // Preserve caller-relative explicit overrides; derived paths always use the checkout root.
  return binary?.trim() ? resolve(binary) : resolve(root, targetDir, 'debug', 'mario-server' + (platform === 'win32' ? '.exe' : ''));
}

if (process.argv[1] && existsSync(process.argv[1]) && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  console.log(cargoTargetDir());
}
