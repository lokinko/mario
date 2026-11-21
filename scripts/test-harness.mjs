// Shared end-to-end HTTP test harness, fault-injection model server, and invariant guards.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import { once } from "node:events";
import { existsSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer as createNetServer } from "node:net";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { root, serverBinary } from "./server-artifacts.mjs";

export const DEFAULT_FORBIDDEN_LEAKS = [
  "panicked at",
  "RUST_BACKTRACE",
  "sqlx::",
  "postgres://",
  "postgresql://",
];

export async function allocatePort() {
  const probe = createNetServer().listen(0, "127.0.0.1");
  await once(probe, "listening");
  const { port } = probe.address();
  await new Promise((resolve) => probe.close(resolve));
  return port;
}

export async function startMarioServer({
  database,
  masterKey = randomBytes(32).toString("base64"),
  port,
  webDir = join(root, "client/dist"),
  extraEnv = {},
  includeWebDir = true,
  keepAgentUrl = false,
} = {}) {
  assert(
    database,
    "Set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database",
  );
  const actualPort = port ?? (await allocatePort());
  const origin = `http://127.0.0.1:${actualPort}`;
  const base = `${origin}/api`;
  const env = {
    ...process.env,
    DATABASE_URL: database,
    MARIO_MASTER_KEY: masterKey,
    MARIO_HOST: "127.0.0.1",
    OPENAI_API_KEY: "",
    OPENAI_MODEL: "",
    ...(includeWebDir && existsSync(join(webDir, "index.html"))
      ? { MARIO_WEB_DIR: webDir }
      : {}),
    ...extraEnv,
  };
  if (!keepAgentUrl && !("MARIO_AGENT_URL" in extraEnv)) {
    delete env.MARIO_AGENT_URL;
  }

  let log = "";
  const child = spawn(serverBinary(), ["--port", String(actualPort)], {
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (chunk) => {
    log += chunk;
  });
  child.stderr.on("data", (chunk) => {
    log += chunk;
  });

  const ready = await waitForServerReady(base, child, () => log);
  if (!ready) {
    throw new Error(`Test server failed to start on ${origin}:\n${log}`);
  }

  let stopped = false;
  async function stop() {
    if (stopped) return;
    stopped = true;
    if (child.exitCode === null && child.signalCode === null) {
      const exited = once(child, "exit");
      child.kill("SIGTERM");
      const winner = await Promise.race([
        exited.then(() => "exited"),
        sleep(5000).then(() => "timeout"),
      ]);
      if (winner === "timeout" && child.exitCode === null) {
        child.kill("SIGKILL");
        await once(child, "exit");
      }
    }
  }

  return {
    origin,
    base,
    port: actualPort,
    masterKey,
    child,
    stop,
    logs: () => log,
  };
}

async function waitForServerReady(base, child, getLog) {
  for (let i = 0; i < 150; i += 1) {
    try {
      if ((await fetch(`${base}/server`)).ok) return true;
    } catch {}
    if (child.exitCode !== null) {
      throw new Error(`Test server exited early (${child.exitCode}):\n${getLog()}`);
    }
    await sleep(50);
  }
  return false;
}

/**
 * Controllable local OpenAI-compatible mock server for deterministic fault injection.
 */
export async function createFaultModelServer() {
  const requests = [];
  const queue = [];
  let defaultResponder = () => ({
    status: 200,
    json: {
      choices: [{ message: { role: "assistant", content: "OK" }, finish_reason: "stop" }],
      usage: { prompt_tokens: 12, completion_tokens: 4 },
    },
  });

  const server = createHttpServer(async (req, res) => {
    let rawBody = "";
    for await (const chunk of req) rawBody += chunk;
    let body = null;
    try {
      body = rawBody ? JSON.parse(rawBody) : null;
    } catch {
      body = null;
    }
    const record = {
      method: req.method,
      path: req.url,
      headers: { ...req.headers },
      rawBody,
      body,
    };
    requests.push(record);

    const next = queue.length > 0 ? queue.shift() : defaultResponder;
    const action = typeof next === "function" ? await next(record) : next;

    if (action?.delayMs) {
      await sleep(action.delayMs);
    }
    if (action?.abort) {
      req.socket.destroy();
      return;
    }
    const status = action?.status ?? 200;
    const headers = {
      "Content-Type": action?.contentType ?? "application/json",
      ...(action?.headers ?? {}),
    };
    res.writeHead(status, headers);
    if (action?.raw !== undefined) {
      res.end(action.raw);
    } else {
      res.end(JSON.stringify(action?.json ?? {}));
    }
  });

  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const { port } = server.address();

  return {
    baseUrl: `http://127.0.0.1:${port}/v1`,
    requests,
    reset() {
      requests.length = 0;
      queue.length = 0;
    },
    setDefault(responder) {
      defaultResponder = responder;
    },
    enqueue(...items) {
      queue.push(...items);
    },
    chatOk(content, usage = { prompt_tokens: 40, completion_tokens: 20 }) {
      return {
        status: 200,
        json: {
          choices: [{ message: { role: "assistant", content }, finish_reason: "stop" }],
          usage,
        },
      };
    },
    httpError(status, message = "upstream injected fault") {
      return {
        status,
        json: { error: { message, type: "injected_error" } },
      };
    },
    malformedJson(raw = "{not-valid-json") {
      return {
        status: 200,
        raw,
      };
    },
    abortSocket() {
      return { abort: true };
    },
    async close() {
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}

export function assertNoForbiddenLeaks(text, forbidden = []) {
  for (const secret of [...DEFAULT_FORBIDDEN_LEAKS, ...forbidden]) {
    if (!secret) continue;
    assert.equal(
      text.includes(secret),
      false,
      `Response leaked forbidden fragment: ${JSON.stringify(secret)}`,
    );
  }
}

export class ApiAccount {
  constructor(base, { token, userId, email, revision = null, secrets = [] } = {}) {
    this.base = base;
    this.token = token;
    this.userId = userId;
    this.email = email;
    this.revision = revision;
    this.secrets = [...secrets];
  }

  static async register(base, { tag = "test", email, password, secrets = [] } = {}) {
    const actualEmail = email ?? `${tag}-${randomUUID()}@example.test`;
    const actualPassword = password ?? "adversarial-password-0123456789";
    const response = await fetch(`${base}/auth/register`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email: actualEmail, password: actualPassword }),
    });
    const text = await response.text();
    assert.equal(response.status, 200, `register ${actualEmail} failed: ${text}`);
    const payload = JSON.parse(text);
    const account = new ApiAccount(base, {
      token: payload.token,
      userId: payload.userId,
      email: actualEmail,
      secrets: [...secrets, actualPassword],
    });
    await account.syncVersion();
    return account;
  }

  async rawRequest(
    path,
    {
      method = "GET",
      body,
      rawBody,
      ifMatch,
      useRevision = false,
      headers = {},
      contentType,
      token = this.token,
      signal,
    } = {},
  ) {
    const resolvedHeaders = {
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(body !== undefined && rawBody === undefined
        ? { "Content-Type": contentType ?? "application/json" }
        : contentType
          ? { "Content-Type": contentType }
          : {}),
      ...(ifMatch !== undefined
        ? ifMatch === null
          ? {}
          : { "If-Match": String(ifMatch) }
        : useRevision && this.revision
          ? { "If-Match": String(this.revision) }
          : {}),
      ...headers,
    };

    const response = await fetch(`${this.base}${path}`, {
      method,
      headers: resolvedHeaders,
      body:
        rawBody !== undefined
          ? rawBody
          : body === undefined
            ? undefined
            : JSON.stringify(body),
      signal,
    });
    const text = await response.text();
    let json;
    if (text && response.headers.get("content-type")?.includes("application/json")) {
      try {
        json = JSON.parse(text);
      } catch {
        json = undefined;
      }
    }
    if (
      response.ok &&
      token === this.token &&
      response.headers.get("x-data-revision")
    ) {
      this.revision = response.headers.get("x-data-revision");
    }
    return {
      status: response.status,
      ok: response.ok,
      headers: response.headers,
      text,
      json,
    };
  }

  async syncVersion() {
    const res = await this.rawRequest("/sync/version");
    assert.equal(res.status, 200, `GET /sync/version failed: ${res.text}`);
    return res.json;
  }

  async expectOk(path, options = {}) {
    const method = options.method ?? "GET";
    const useRevision =
      options.useRevision ?? (method !== "GET" && method !== "HEAD");
    const res = await this.rawRequest(path, { ...options, useRevision });
    assert.equal(res.status, 200, `${method} ${path} expected 200, got ${res.status}: ${res.text}`);
    assertNoForbiddenLeaks(res.text, this.secrets);
    return res;
  }

  /**
   * Invariant guard: a rejected request MUST return one of expectedStatuses,
   * MUST NOT leak secrets/stack traces, and MUST leave targetAccount's revision untouched.
   */
  async assertRejectedWithoutMutation(
    path,
    options = {},
    expectedStatuses = [400],
    { targetAccount = this, extraForbidden = [] } = {},
  ) {
    const allowed = Array.isArray(expectedStatuses)
      ? expectedStatuses
      : [expectedStatuses];
    const beforeTarget = await targetAccount.syncVersion();
    const beforeCaller =
      targetAccount === this ? beforeTarget : await this.syncVersion();
    const method = options.method ?? "GET";
    const useRevision =
      options.useRevision ??
      (options.ifMatch === undefined && method !== "GET" && method !== "HEAD");
    const res = await this.rawRequest(path, { ...options, useRevision });
    assert(
      allowed.includes(res.status),
      `${method} ${path} expected status in [${allowed.join(", ")}], got ${res.status}: ${res.text}`,
    );
    assertNoForbiddenLeaks(res.text, [
      ...this.secrets,
      ...targetAccount.secrets,
      ...extraForbidden,
    ]);
    const afterTarget = await targetAccount.syncVersion();
    assert.equal(
      afterTarget.revision,
      beforeTarget.revision,
      `${method} ${path} failed (${res.status}) yet mutated target revision ${beforeTarget.revision} -> ${afterTarget.revision}`,
    );
    if (targetAccount !== this) {
      const afterCaller = await this.syncVersion();
      assert.equal(
        afterCaller.revision,
        beforeCaller.revision,
        `${method} ${path} failed (${res.status}) yet mutated caller revision ${beforeCaller.revision} -> ${afterCaller.revision}`,
      );
    }
    return res;
  }
}

export function createSuite(suiteName, { filter } = {}) {
  const activeFilter =
    filter ??
    process.argv
      .find((arg) => arg.startsWith("--filter="))
      ?.slice("--filter=".length)
      ?.toLowerCase();
  const cases = [];
  let assertions = 0;

  return {
    add(name, fn) {
      cases.push({ name, fn });
    },
    check(condition, message) {
      assert(condition, message);
      assertions += 1;
    },
    count(n = 1) {
      assertions += n;
    },
    async run({ onError } = {}) {
      const selected = activeFilter
        ? cases.filter((entry) => entry.name.toLowerCase().includes(activeFilter))
        : cases;
      assert(
        selected.length > 0,
        `No test cases matched filter "${activeFilter}" in ${suiteName}`,
      );
      const started = Date.now();
      const results = [];
      for (const entry of selected) {
        const caseStart = Date.now();
        try {
          await entry.fn();
          const elapsed = Date.now() - caseStart;
          results.push({ name: entry.name, ok: true, elapsed });
          console.log(`  PASS [${elapsed}ms] ${entry.name}`);
        } catch (error) {
          const elapsed = Date.now() - caseStart;
          console.error(`  FAIL [${elapsed}ms] ${entry.name}`);
          console.error(`       ${error.message}`);
          if (onError) await onError(error, entry.name);
          throw error;
        }
      }
      const totalMs = Date.now() - started;
      console.log(
        `PASS ${suiteName}: ${results.length}/${selected.length} suites, ${assertions} tracked invariant checks (${totalMs}ms)`,
      );
      return { results, assertions, totalMs };
    },
  };
}
