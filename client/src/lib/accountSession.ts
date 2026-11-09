/** All mutable transport state belongs to one account lifetime. */
export class AccountSession {
  revision: string | undefined;
  private readonly controller = new AbortController();
  private remoteUpdatePending = false;
  private running: Promise<void> | undefined;
  private finish: (() => void) | undefined;
  private syncCount = 0;
  private writes = 0;
  private queue: Promise<unknown> = Promise.resolve();

  constructor(
    readonly token: string | undefined,
    readonly baseUrl: string,
  ) {}

  get signal() {
    return this.controller.signal;
  }

  assertActive() {
    if (this.signal.aborted)
      throw new Error("账号已切换，请在当前账号中重新操作。");
  }

  acceptRevision(value?: string) {
    this.assertActive();
    this.revision = value;
  }

  beginSync() {
    this.assertActive();
    this.syncCount++;
    this.running ??= new Promise<void>((resolve) => {
      this.finish = resolve;
    });
    let released = false;
    return () => {
      if (released) return;
      released = true;
      this.endSync();
    };
  }

  endSync() {
    this.syncCount = Math.max(0, this.syncCount - 1);
    if (this.syncCount) return;
    this.finish?.();
    this.running = undefined;
    this.finish = undefined;
  }

  blockStaleWrites(value: boolean) {
    this.assertActive();
    this.remoteUpdatePending = value;
  }

  async beforeDataWrite() {
    this.assertActive();
    while (this.running) {
      await this.running;
      this.assertActive();
    }
    if (this.remoteUpdatePending)
      throw new Error(
        "其他设备已更新资料。请先点击‘查看最新资料’，核对后再保存；你的输入仍保留在页面中。",
      );
  }

  markDataWrite() {
    this.assertActive();
    this.writes++;
    let released = false;
    return () => {
      if (released) return;
      released = true;
      this.writes--;
    };
  }

  hasDataWrites() {
    return this.writes > 0;
  }

  /** Callers select which edits need ordering; queued work never survives its session. */
  enqueue<T>(work: () => Promise<T>): Promise<T> {
    const result = this.queue
      .catch(() => undefined)
      .then(() => {
        this.assertActive();
        return work();
      });
    this.queue = result;
    return result;
  }

  dispose() {
    this.controller.abort(new Error("账号已切换，请在当前账号中重新操作。"));
    this.revision = undefined;
    this.syncCount = 0;
    this.endSync();
  }
}

let active: AccountSession | undefined;

export function accountSession(token: string | undefined, baseUrl: string) {
  if (!active || active.token !== token || active.baseUrl !== baseUrl) {
    resetAccountSession();
    active = new AccountSession(token, baseUrl);
  }
  return active;
}

/** Also invalidate A → B → A requests, even when the token eventually matches. */
export function resetAccountSession() {
  active?.dispose();
  active = undefined;
}
