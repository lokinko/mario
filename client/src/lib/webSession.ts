import { serviceUrl } from "./service";
const key = () => `mario.session:${serviceUrl()}`;
const accountsKey = () => `mario.accounts:${serviceUrl()}`;
export interface WebAccount {
  token: string;
  expiresAt: number;
  email?: string;
  userId?: string;
}
function validAccount(value: unknown): value is WebAccount {
  const account = value as WebAccount | null;
  return (
    !!account &&
    typeof account.token === "string" &&
    !!account.token &&
    Number.isFinite(account.expiresAt) &&
    account.expiresAt * 1000 > Date.now() &&
    (account.email === undefined || typeof account.email === "string") &&
    (account.userId === undefined || typeof account.userId === "string")
  );
}
export function savedWebAccounts(): WebAccount[] {
  let accounts: WebAccount[] = [];
  try {
    const stored: unknown = JSON.parse(
      localStorage.getItem(accountsKey()) ?? "[]",
    );
    if (Array.isArray(stored)) accounts = stored.filter(validAccount);
  } catch {
    /* Ignore damaged account storage. */
  }
  return accounts;
}

export function switchWebAccount(token: string): boolean {
  const account = savedWebAccounts().find((value) => value.token === token);
  if (!account) return false;
  setWebToken(account.token, account.expiresAt, account);
  return true;
}

export function forgetWebAccount(token: string) {
  const accounts = savedWebAccounts().filter(
    (account) => account.token !== token,
  );
  if (webToken() === token) setWebToken();
  localStorage.setItem(accountsKey(), JSON.stringify(accounts));
}
export function webToken(): string | undefined {
  const stored = localStorage.getItem(key());
  if (stored !== null) {
    try {
      const session = JSON.parse(stored);
      if (validAccount(session)) return session.token;
    } catch {
      /* Discard unreadable sessions instead of blocking the login screen. */
    }
    setWebToken();
    return undefined;
  }
  return undefined;
}
export function setWebToken(
  token?: string,
  expiresAt = Math.floor(Date.now() / 1000) + 7 * 24 * 3600,
  identity?: Pick<WebAccount, "email" | "userId">,
) {
  if (token) {
    const account: WebAccount = {
      token,
      expiresAt,
      ...(identity?.email
        ? { email: identity.email.trim().toLowerCase() }
        : {}),
      ...(identity?.userId ? { userId: identity.userId } : {}),
    };
    const accounts = savedWebAccounts().filter(
      (saved) =>
        saved.token !== token &&
        !(account.userId && saved.userId === account.userId) &&
        !(account.email && saved.email === account.email),
    );
    localStorage.setItem(accountsKey(), JSON.stringify([...accounts, account]));
    localStorage.setItem(key(), JSON.stringify(account));
  } else localStorage.removeItem(key());
  sessionStorage.removeItem(key());
}
