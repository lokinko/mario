"use strict";
// Public client configuration. No privileged key or user data is stored here.
const projectUrl = "https://haqvpuukgsxuroeqohsk.supabase.co";
const publishableKey = "sb_publishable_fPtEQ0_ny5JKVVS5djfGFw_HlzhV0FL";
const parameters = new URLSearchParams(location.hash.slice(1));
// Remove email credentials before any network request or external navigation.
history.replaceState(null, "", location.pathname);
let accessToken = parameters.get("access_token") || "";
let tokenHash = parameters.get("token_hash") || "";
const flow = parameters.get("type") || "";
const callbackError = parameters.get("error_code") || parameters.get("error");
const el = (id) => document.getElementById(id);
let verified = false;

function showError(message) { el("error").textContent = message; el("error").hidden = false; }
function clearError() { el("error").textContent = ""; el("error").hidden = true; }
function errorMessage(code) {
  if (["otp_expired", "access_denied", "bad_jwt", "session_not_found"].includes(code)) return "链接已过期或已使用。请回到 mario 重新发送邮件，再打开最新一封。";
  if (code === "same_password") return "新密码不能与旧密码相同，请换一个密码。";
  if (code === "weak_password") return "新密码不符合要求，请增加长度并组合字母、数字和符号。";
  if (code === "over_request_rate_limit") return "操作过于频繁，请稍后重试。";
  return "验证服务暂时未能完成请求，请稍后重试；若链接失效，请回到 mario 重新发送邮件。";
}

async function request(path, options = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 15000);
  try {
    const response = await fetch(`${projectUrl}/auth/v1${path}`, {
      ...options, signal: controller.signal, cache: "no-store", credentials: "omit",
      headers: { "apikey": publishableKey, "Content-Type": "application/json", ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}) },
    });
    const data = await response.json();
    if (!response.ok) throw new Error(errorMessage(data.error_code));
    return data;
  } catch (error) {
    if (error instanceof TypeError || error.name === "AbortError") throw new Error("无法连接邮箱验证服务，请检查网络后重试。");
    throw error;
  } finally { clearTimeout(timer); }
}

async function checkUser() {
  clearError(); el("retry").hidden = true;
  try {
    const user = await request("/user");
    if (!user.email_confirmed_at) throw new Error("邮箱尚未确认，请回到 mario 重新发送确认邮件。");
    verified = true;
    if (flow === "recovery") {
      el("title").textContent = "设置新密码";
      el("message").textContent = `已验证 ${user.email}，请输入新密码。`;
      el("password-form").hidden = false;
      el("help").textContent = "修改登录密码不会更换云端投资数据的同步恢复密钥。";
    } else {
      el("title").textContent = "邮箱确认成功";
      el("message").textContent = `${user.email} 已完成验证。请返回 mario，使用注册时设置的密码登录。`;
      accessToken = "";
    }
  } catch (error) {
    el("title").textContent = "暂时无法确认验证结果";
    el("message").textContent = "我们尚未确认此次操作成功。";
    showError(error.message); el("retry").hidden = false;
  }
}

el("retry").addEventListener("click", checkUser);
el("verify").addEventListener("click", async () => {
  el("verify").disabled = true; clearError();
  try {
    const session = await request("/verify", { method: "POST", body: JSON.stringify({ type: flow, token_hash: tokenHash }) });
    if (!session.access_token) throw new Error("未取得有效的验证结果，请重新发送邮件。");
    accessToken = session.access_token; tokenHash = ""; el("verify").hidden = true;
    await checkUser();
  } catch (error) { showError(error.message); }
  finally { el("verify").disabled = false; }
});

el("password-form").addEventListener("submit", async (event) => {
  event.preventDefault(); clearError();
  const password = el("password").value;
  if (password !== el("confirm").value) { showError("两次输入的新密码不一致。"); return; }
  if (!verified || !accessToken || flow !== "recovery") { showError("请重新打开最新的重置邮件验证身份。"); return; }
  el("save").disabled = true;
  try {
    await request("/user", { method: "PUT", body: JSON.stringify({ password }) });
    accessToken = ""; el("password").value = ""; el("confirm").value = "";
    el("password-form").hidden = true; el("title").textContent = "密码已更新";
    el("message").textContent = "请返回 mario，使用新密码登录。";
  } catch (error) { showError(error.message); }
  finally { el("save").disabled = false; }
});

if (callbackError) {
  accessToken = ""; tokenHash = ""; el("title").textContent = "确认链接未能完成验证";
  el("message").textContent = errorMessage(callbackError);
} else if (tokenHash && ["signup", "email", "recovery"].includes(flow)) {
  el("title").textContent = flow === "recovery" ? "验证密码重置请求" : "确认你的邮箱";
  el("message").textContent = "点击下方按钮，完成邮件中的验证请求。";
  el("verify").textContent = flow === "recovery" ? "验证并设置新密码" : "确认邮箱";
  el("verify").hidden = false;
} else if (accessToken && ["signup", "email", "recovery"].includes(flow)) {
  checkUser();
} else {
  accessToken = ""; tokenHash = ""; el("title").textContent = "请从最新的确认邮件打开";
  el("message").textContent = "此页面需要邮件中的一次性验证信息。请回到 mario 发送确认邮件或重置邮件，再打开邮件中的链接。";
}
