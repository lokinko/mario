"""把 `bench-run` 输出目录中各策略的成绩汇总成看板：页面、脚本与数据分为三个文件。

看板只读取成绩与策略说明，不读取输入或结果标签；图表由本模块统一生成，新增策略不需要改看板代码。
本机由 `bench-serve` 提供，部署后由 Mario 服务在 `/dashboard` 提供，数据只对已登录账号开放。
"""

import inspect
import json
import math
from datetime import datetime
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from .files import read_json, require
from .public_sources import SHANGHAI
from .scoring import WIN_RATE_WEIGHT, bootstrap_interval
from .strategy_runner import completed_runs, failed_runs

POOL_LABELS = {"dividend": "红利池", "tech": "科技池"}
BENCHMARKS = {"pool_equal_weight": ("对照：股票池等权", "每期等权买入股票池中有行情记录的全部成员，由打分引擎计算。"),
              "csi300": ("对照：沪深300", "沪深300 全收益指数（H00300），由打分引擎计算。")}
BASELINES = {"cash", "equal", "momentum", "size"}
SPLIT_YEAR = 2018
DASHBOARD_FILES = ("dashboard.html", "dashboard.js", "dashboard.json")


def stats_parameters():
    """页面按任意时间区间重算统计量时所用的参数，取自打分引擎，保证与 scoring.series_stats 同一口径。"""
    defaults = inspect.signature(bootstrap_interval).parameters
    return {"win_rate_weight": WIN_RATE_WEIGHT, "rounds": defaults["rounds"].default, "seed": defaults["seed"].default}


def segment(decision_date):
    return "early" if int(decision_date[:4]) < SPLIT_YEAR else "late"


def segments(periods):
    """按参数选定段（SPLIT_YEAR 之前）与检验段切分；只列出有决策点的时间段。"""
    years = sorted({int(p["decision_date"][:4]) for p in periods})
    spans = {"early": [y for y in years if y < SPLIT_YEAR], "late": [y for y in years if y >= SPLIT_YEAR]}
    return [{"key": "all", "label": "全部年份"}] + [
        {"key": key, "label": f"{span[0]}–{span[-1]}" if span[0] != span[-1] else str(span[0])}
        for key, span in spans.items() if span]


def positions(weights):
    """{样本: [[证券, 仓位, 卖出规则或网格]]}，供看板展示当期持仓。"""
    output = {}
    for row in weights:
        output.setdefault(row["sample_id"], []).append([row["symbol"], row["weight"], row.get("exit") or row.get("grid")])
    return output


def dashboard_data(runs, config=None, failures=None):
    """runs：{策略名: {"title", "description", "scorecard", "weights"}}；各成绩必须覆盖同一批决策点且对照得分一致。"""
    require(runs, "缺少策略成绩")
    names = sorted(runs)
    scorecards = {name: runs[name]["scorecard"] for name in names}
    first = scorecards[names[0]]
    ids = [period["sample_id"] for period in first["periods"]]
    pools = sorted({period["pool"] for period in first["periods"]})
    spans = segments(first["periods"])
    periods = {period["sample_id"]: {
        "id": period["sample_id"], "order": order, "pool": period["pool"], "decision_date": period["decision_date"],
        "segment": segment(period["decision_date"]), "entry_date": period["entry_date"], "scores": {},
        "segment_returns": {}, "holdings": {}, "blocked": {}, "exits": {}, "grid_trades": {}, "operations": {},
        "benchmarks": {"pool_equal_weight": period["pool_equal_weight_return_score"],
                       "csi300": period["csi300_return_score"]}}
        for order, period in enumerate(first["periods"])}
    for name in names:
        report, held = scorecards[name], positions(runs[name]["weights"])
        require([p["sample_id"] for p in report["periods"]] == ids, f"{name} 与其他策略的决策点不一致，可能来自不同发布包")
        for period in report["periods"]:
            require("operations" in period, f"{name} 缺少逐笔操作记录，请重新运行 bench-run")
            row = periods[period["sample_id"]]
            for key, value in (("pool_equal_weight", period["pool_equal_weight_return_score"]),
                               ("csi300", period["csi300_return_score"])):
                require(math.isclose(row["benchmarks"][key], value, abs_tol=1e-9), f"{name} 的对照得分不一致")
            row["scores"][name] = period["return_score"]
            row["segment_returns"][name] = period["segment_returns"]
            row["holdings"][name] = held.get(period["sample_id"], [])
            row["blocked"][name] = period["blocked_entries"]
            row["exits"][name] = period["exits"]
            row["operations"][name] = period["operations"]
            row["grid_trades"][name] = sum(t["buys"] + t["sells"] for t in period["grid_trades"])
    return {
        "strategies": [{"key": name, "label": runs[name]["title"], "description": runs[name]["description"],
                        "baseline": name in BASELINES} for name in names],
        "failures": [{"key": name, "error": error} for name, error in sorted((failures or {}).items())],
        "benchmarks": [{"key": key, "label": label, "description": text} for key, (label, text) in BENCHMARKS.items()],
        "pools": [{"key": pool, "label": POOL_LABELS.get(pool, pool)} for pool in pools], "segments": spans,
        "stats": stats_parameters(), "periods": sorted(periods.values(), key=lambda row: (row["decision_date"], row["pool"])),
        "metric": first["metric"], "cost_basis": first["cost_basis"], "limitations": first["limitations"],
        "release": {key: config[key] for key in ("version", "built_on", "pools", "reference_total_return_index")
                    if config and key in config},
        "generated_at": datetime.now(SHANGHAI).isoformat(timespec="seconds")}


def render(data):
    """看板拆成页面、脚本与数据三个文件：页面与脚本不含数据，可公开提供；数据由服务端按登录状态授权读取。"""
    return {"dashboard.html": TEMPLATE, "dashboard.js": STATS + SCRIPT,
            "dashboard.json": json.dumps(data, ensure_ascii=False, allow_nan=False)}


def write_dashboard(runs_dir, folder, release=None):
    config = read_json(release / "config.json") if release else None
    data = dashboard_data(completed_runs(runs_dir), config, failed_runs(runs_dir))
    folder.mkdir(parents=True, exist_ok=True)
    for name, text in render(data).items():
        part = folder / f"{name}.part"
        part.write_text(text, encoding="utf-8")
        part.replace(folder / name)
    return {"dashboard": str(folder / "dashboard.html"), "strategies": [s["key"] for s in data["strategies"]],
            "periods": len(data["periods"])}


class DashboardHandler(SimpleHTTPRequestHandler):
    """与部署后的 Mario 服务使用同一组路径；只暴露看板三个文件，数据响应带 X-Bench-Live 让页面开启自动刷新。"""
    ROUTES = {"/": "dashboard.html", "/dashboard": "dashboard.html", "/dashboard.html": "dashboard.html",
              "/dashboard.js": "dashboard.js", "/api/benchmark/dashboard": "dashboard.json"}

    def translate_path(self, path):
        name = self.ROUTES.get(path.split("?", 1)[0])
        return str(Path(self.directory) / name) if name else str(Path(self.directory) / "missing")

    def guess_type(self, path):
        return {".js": "text/javascript", ".json": "application/json"}.get(Path(path).suffix) or super().guess_type(path)

    def end_headers(self):
        if self.path.startswith("/api/"):
            self.send_header("X-Bench-Live", "1")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


def dashboard_server(folder, port):
    """只在本机提供 bench-run 输出目录中的看板；页面轮询数据文件的修改时间，重新生成后自动载入。"""
    require(all((folder / name).exists() for name in DASHBOARD_FILES), f"{folder} 下还没有看板，请先运行 bench-run")
    return ThreadingHTTPServer(("127.0.0.1", port), partial(DashboardHandler, directory=str(folder)))


TEMPLATE = """<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>mario_bench 策略得分看板</title>
<style>
:root { --fg:#1f2328; --muted:#656d76; --line:#d0d7de; --bg:#f6f8fa; --up:#cf222e; --down:#1a7f37; --pick:#fff8c5; }
* { box-sizing:border-box; }
[hidden] { display:none !important; }
body { margin:0; font:14px/1.5 -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif; color:var(--fg); background:#fff; }
header { padding:14px 24px 10px; border-bottom:1px solid var(--line); }
h1 { font-size:20px; margin:0 0 2px; }
h2 { font-size:15px; margin:0 0 10px; }
.meta { color:var(--muted); font-size:12px; }
.layout { display:grid; grid-template-columns:340px minmax(0, 1fr); gap:20px; padding:16px 24px 40px; align-items:start; }
aside { position:sticky; top:12px; max-height:calc(100vh - 24px); display:flex; flex-direction:column; gap:12px; }
main { display:grid; gap:20px; min-width:0; }
@media (max-width:1100px) { .layout { grid-template-columns:minmax(0, 1fr); } aside { position:static; max-height:none; } }
.bar { display:flex; gap:6px; flex-wrap:wrap; align-items:center; }
.bar + .bar { margin-top:8px; }
.bar > span:first-child { color:var(--muted); font-size:12px; width:44px; }
button { border:1px solid var(--line); background:#fff; border-radius:6px; padding:2px 10px; cursor:pointer; font:inherit; font-size:13px; }
button.on { background:var(--fg); color:#fff; border-color:var(--fg); }
button.link { border:0; padding:0; color:#0969da; background:none; font-size:12px; }
.card { border:1px solid var(--line); border-radius:8px; padding:14px 16px; min-width:0; }
.grid { display:grid; grid-template-columns:repeat(auto-fit, minmax(440px, 1fr)); gap:20px; }
#list { overflow:auto; max-height:46vh; padding:8px 10px; flex:none; }
#card { overflow:auto; flex:1 1 auto; min-height:180px; }
@media (max-width:1100px) { #list { max-height:none; } }
.group { font-size:12px; color:var(--muted); margin:6px 4px 2px; }
.group:first-child { margin-top:0; }
.item { display:grid; grid-template-columns:16px 24px 24px minmax(0, 1fr) auto; gap:6px; align-items:center; padding:2px 6px; border-radius:6px; cursor:pointer; }
.item:hover { background:var(--bg); }
.item .name { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.item input { margin:0; }
.off { opacity:.4; }
.num { font-variant-numeric:tabular-nums; }
.rank { display:inline-block; min-width:20px; padding:0 4px; text-align:center; font-size:11px; line-height:18px; border-radius:9px;
  background:var(--bg); color:var(--muted); font-variant-numeric:tabular-nums; }
.rank.top { background:var(--fg); color:#fff; }
.sw { vertical-align:middle; flex:none; }
#card h2 { display:flex; gap:6px; align-items:flex-start; }
#card p.rules { white-space:pre-wrap; margin:10px 0 8px; font-size:13px; }
.kpi { display:grid; grid-template-columns:1fr 1fr; gap:6px 12px; font-size:12px; color:var(--muted); margin-top:10px; }
.kpi b { display:block; font-size:15px; color:var(--fg); font-variant-numeric:tabular-nums; }
table { border-collapse:collapse; width:100%; font-variant-numeric:tabular-nums; }
th, td { padding:5px 8px; border-bottom:1px solid var(--line); text-align:right; white-space:nowrap; }
th.l, td.l { text-align:left; }
thead th { background:var(--bg); position:sticky; top:0; z-index:1; }
th[data-sort] { cursor:pointer; user-select:none; } th[data-sort]:hover { background:#eaeef2; }
#summary tbody tr { cursor:pointer; }
#periods tbody tr { cursor:pointer; } #periods tbody tr:hover { background:var(--bg); }
#periods tbody tr.picked { background:var(--pick); }
#periods th { font-weight:500; font-size:12px; cursor:pointer; }
.scroll { max-height:520px; overflow:auto; }
.wide { overflow-x:auto; }
.up { color:var(--up); } .down { color:var(--down); }
svg text { font-size:11px; fill:var(--muted); }
svg text.lbl { fill:var(--fg); font-size:12px; }
svg.chart { display:block; cursor:crosshair; }
.hov { cursor:pointer; }
.s { transition:opacity .12s; }
.note { color:var(--muted); font-size:12px; margin:8px 0 0; }
.failed { border-color:var(--up); } .failed li { white-space:pre-wrap; font-family:ui-monospace, monospace; font-size:12px; }
code { font-size:12px; }
.tip { position:fixed; pointer-events:none; z-index:10; background:#fff; border:1px solid var(--line); border-radius:6px;
  box-shadow:0 4px 12px rgba(31,35,40,.15); padding:6px 10px; font-size:12px; max-width:420px; }
.tip b { display:block; margin-bottom:2px; } .tip div { display:flex; justify-content:space-between; gap:16px; padding:0 2px; }
.tip div.hit { background:var(--pick); font-weight:600; }
#range { position:sticky; top:0; z-index:5; background:#fff; padding:10px 16px 6px; box-shadow:0 2px 6px rgba(31,35,40,.06); }
#range .bar > span:first-child { width:auto; }
.dates { display:inline-flex; gap:4px; align-items:center; font-size:12px; color:var(--muted); margin-left:8px; }
.dates input { font:inherit; font-size:12px; border:1px solid var(--line); border-radius:6px; padding:1px 4px; color:var(--fg); }
#timeline { margin-top:6px; touch-action:none; user-select:none; }
#timeline svg { display:block; cursor:crosshair; }
#timeline .window { cursor:grab; } #timeline .grip { cursor:ew-resize; }
.holdings { display:grid; grid-template-columns:repeat(auto-fill, minmax(min(100%, 640px), 1fr)); gap:12px; }
.holdings > div { border:1px solid var(--line); border-radius:6px; padding:10px; min-width:0; }
.holdings h3 { font-size:13px; margin:0 0 4px; } .holdings table { font-size:12px; min-width:640px; }
.holdings td.l { white-space:normal; }
.holdings summary { cursor:pointer; margin-top:10px; }
</style>
<style id="focus"></style>
</head>
<body>
<header>
  <h1>mario_bench 策略得分看板</h1>
  <div class="meta"><span id="meta">正在载入看板数据…</span> · <span id="live"></span></div>
</header>
<div class="layout">
  <aside>
    <section class="card">
      <div class="bar"><span>股票池</span><span class="bar" id="pools"></span></div>
      <div class="bar" id="quick"><span>显示</span><button data-show="all">全部</button>
        <button data-show="top5">前 5 名 + 市场对照</button><button data-show="strategy">只看候选策略</button></div>
    </section>
    <section class="card" id="list"></section>
    <section class="card" id="card"></section>
  </aside>
  <main>
    <section class="card" id="range">
      <div class="bar"><span>时间区间</span><span class="bar" id="presets"></span>
        <label class="dates"><input type="date" id="from" aria-label="开始日期"> 至 <input type="date" id="to" aria-label="结束日期"></label>
        <span class="meta" id="range-note"></span></div>
      <div id="timeline"></div>
    </section>
    <section class="card failed" id="failures" hidden><h2>运行失败的策略（未计入看板）</h2><ul></ul></section>
    <section class="card"><h2>策略汇总</h2><div class="wide"><table id="summary"></table></div>
      <p class="note">名次按当前股票池与时间区间的总分排列，对照基线与市场对照不占名次。统计量按打分引擎的同一公式在页面计算，任意区间与打分报告一致；95% 区间在拖动结束后补算。点击表头排序，点击一行在左侧查看该策略的规则。</p></section>
    <section class="card"><h2>累计收益得分（逐期收益得分求和，不是净值）</h2><div id="cumulative"></div>
      <p class="note">曲线右端标出名次与策略名；悬停时高亮最近的曲线，点击查看该策略规则与当期持仓。</p></section>
    <div class="grid">
      <section class="card"><h2>总分与 95% 区间</h2><div id="means"></div></section>
      <section class="card"><h2>逐年平均收益得分</h2><div id="yearly"></div></section>
    </div>
    <section class="card"><h2>逐期收益得分</h2><div class="scroll"><table id="periods"></table></div>
      <p class="note">数值为当期收益得分，大于 0 计为胜；红色为正、绿色为负（A 股习惯）。买不进的股票（停牌或一字涨停）按规则留作现金，括号内为该期买不进的只数；“卖 n”为该期按卖出规则提前卖出的只数。点击一行查看当期持仓。</p></section>
    <section class="card" id="detail" hidden><h2 id="detail-title"></h2><div id="detail-body"></div></section>
    <section class="card"><h2>口径与限制</h2><p class="note" id="metric"></p><ul id="limits" class="note"></ul></section>
  </main>
</div>
<div class="tip" id="tip" hidden></div>
<script type="module" src="dashboard.js"></script>
</body>
</html>
"""

STATS = """
function pythonRandom(seed) {
  const N = 624, M = 397, mt = new Uint32Array(N);
  let index = N;
  const init = s => { mt[0] = s; for (let i = 1; i < N; i++) mt[i] = Math.imul(1812433253, mt[i - 1] ^ (mt[i - 1] >>> 30)) + i; };
  const key = [];
  for (let rest = Math.abs(seed); rest > 0 || !key.length; rest = Math.floor(rest / 4294967296)) key.push(rest % 4294967296);
  init(19650218);
  let i = 1, j = 0;
  for (let k = Math.max(N, key.length); k; k--) {
    mt[i] = (mt[i] ^ Math.imul(mt[i - 1] ^ (mt[i - 1] >>> 30), 1664525)) + key[j] + j;
    if (++i >= N) { mt[0] = mt[N - 1]; i = 1; }
    if (++j >= key.length) j = 0;
  }
  for (let k = N - 1; k; k--) {
    mt[i] = (mt[i] ^ Math.imul(mt[i - 1] ^ (mt[i - 1] >>> 30), 1566083941)) - i;
    if (++i >= N) { mt[0] = mt[N - 1]; i = 1; }
  }
  mt[0] = 0x80000000;
  const next = () => {
    if (index >= N) {
      for (let k = 0; k < N; k++) {
        const y = (mt[k] & 0x80000000) | (mt[(k + 1) % N] & 0x7fffffff);
        mt[k] = mt[(k + M) % N] ^ (y >>> 1) ^ (y & 1 ? 0x9908b0df : 0);
      }
      index = 0;
    }
    let y = mt[index++];
    y ^= y >>> 11; y ^= (y << 7) & 0x9d2c5680; y ^= (y << 15) & 0xefc60000; y ^= y >>> 18;
    return y >>> 0;
  };
  return {below(n) {
    const bits = 32 - Math.clz32(n);
    let r = next() >>> (32 - bits);
    while (r >= n) r = next() >>> (32 - bits);
    return r;
  }};
}

function fsum(values) {
  const partials = [];
  for (let x of values) {
    let i = 0;
    for (let y of partials) {
      if (Math.abs(x) < Math.abs(y)) [x, y] = [y, x];
      const hi = x + y, lo = y - (hi - x);
      if (lo) partials[i++] = lo;
      x = hi;
    }
    partials.length = i;
    partials.push(x);
  }
  let n = partials.length, hi = 0, lo = 0;
  if (n) {
    hi = partials[--n];
    while (n > 0) {
      const x = hi, y = partials[--n];
      hi = x + y;
      lo = y - (hi - x);
      if (lo) break;
    }
    if (n > 0 && ((lo < 0 && partials[n - 1] < 0) || (lo > 0 && partials[n - 1] > 0))) {
      const y = lo * 2, x = hi + y;
      if (y === x - hi) hi = x;
    }
  }
  return hi;
}

function bootstrap(values, rounds, seed) {
  if (values.length < 2) return null;
  const random = pythonRandom(seed), n = values.length, means = new Float64Array(rounds), draw = new Array(n);
  for (let r = 0; r < rounds; r++) {
    for (let i = 0; i < n; i++) draw[i] = values[random.below(n)];
    means[r] = fsum(draw) / n;
  }
  means.sort();
  return [means[Math.trunc(0.025 * rounds)], means[Math.trunc(0.975 * rounds) - 1]];
}

const totalScores = (returns, params) => returns.map(v => v + params.win_rate_weight * (v > 0));
const interval = (returns, params) => bootstrap(totalScores(returns, params), params.rounds, params.seed);

function summarize(returns, params, withInterval) {
  if (!returns.length) return undefined;
  const n = returns.length, totals = totalScores(returns, params), sorted = totals.slice().sort((a, b) => a - b);
  return {mean: fsum(totals) / n, median: n % 2 ? sorted[(n - 1) / 2] : (sorted[n / 2 - 1] + sorted[n / 2]) / 2,
    ci: withInterval ? interval(returns, params) : undefined, count: n,
    return_mean: fsum(returns) / n, win_rate: returns.filter(v => v > 0).length / n};
}
"""

SCRIPT = """
const SOURCE = 'api/benchmark/dashboard';

function sessionToken() {
  try {
    const session = JSON.parse(localStorage.getItem(`mario.session:${new URL('api', location.href).pathname}`));
    return session && typeof session.token === 'string' && session.expiresAt * 1000 > Date.now() ? session.token : null;
  } catch {
    return null;
  }
}

function stop(message, login = false) {
  document.querySelector('.layout').hidden = true;
  document.getElementById('meta').textContent = message;
  if (login) document.getElementById('live').innerHTML = '<a href="./">前往 Mario 登录</a>';
  throw Error(message);
}

async function loadData() {
  const token = sessionToken();
  let response;
  try {
    response = await fetch(SOURCE, {cache: 'no-store', headers: token ? {Authorization: `Bearer ${token}`} : {}});
  } catch {
    stop('无法连接看板服务');
  }
  if (response.status === 401) stop('看板只对已登录账号开放：请先在 Mario 登录，再回到本页', true);
  if (response.status === 404) stop('服务器上还没有生成看板，请检查部署日志');
  if (!response.ok) stop(`看板数据读取失败（HTTP ${response.status}）`);
  return {data: await response.json(), live: response.headers.get('X-Bench-Live') === '1'};
}

const {data: DATA, live: LIVE} = await loadData();
const PALETTE = ['#0969da', '#cf222e', '#1a7f37', '#8250df', '#e16f24', '#1b7c83', '#bf3989', '#6c8c00',
  '#4c2889', '#a40e26', '#54aeff', '#7d4e00', '#2da44e', '#d2a8ff', '#ff8182', '#0550ae'];
const GREYS = ['#8c959f', '#6e7781', '#afb8c1', '#57606a'];
const counter = {strategy: 0, baseline: 0};
const SERIES = [
  ...DATA.strategies.map(s => {
    const kind = s.baseline ? 'baseline' : 'strategy', i = counter[kind]++;
    return kind === 'baseline'
      ? {...s, kind, color: GREYS[i % GREYS.length], dash: ['4 3', '1 3', '7 3', '3 2 1 2'][i % 4], width: 1.3}
      : {...s, kind, color: PALETTE[i % PALETTE.length], dash: ['', '7 3', '2 2'][Math.floor(i / PALETTE.length) % 3], width: 2};
  }),
  ...DATA.benchmarks.map((b, i) => ({...b, kind: 'benchmark', color: ['#bf8700', '#24292f'][i % 2], dash: '6 3', width: 1.6}))];
const BY_KEY = Object.fromEntries(SERIES.map(s => [s.key, s]));
const GROUPS = {strategy: '候选策略', baseline: '对照基线（不占名次）', benchmark: '市场对照'};
const RULES = {stop_loss: '止损', trailing_stop: '移动止损', take_profit: '止盈', max_hold_days: '最长持有'};
const DATES = [...new Set(DATA.periods.map(p => p.decision_date))].sort();
const PRESETS = DATA.segments.map(s => {
  const dates = DATA.periods.filter(p => s.key === 'all' || p.segment === s.key).map(p => p.decision_date);
  return {...s, from: dates[0], to: dates[dates.length - 1]};
});
const MARKET_KEY = (DATA.benchmarks.find(b => b.key === 'csi300') || DATA.benchmarks[0]).key;
const state = {pool: 'all', range: {from: DATES[0], to: DATES[DATES.length - 1]}, dragging: false, hidden: new Set(),
  sort: null, picked: null, focus: null, hover: null, reveal: false};
let view = {rows: [], scored: [], rank: {}, width: {}, stats: {}, preset: 'all'};
const INTERVALS = new Map();
const fmt = v => v == null ? '—' : v.toFixed(2);
const pct = v => `${+(Number(v) * 100).toFixed(2)}%`;
const tone = v => v > 0 ? 'up' : v < 0 ? 'down' : '';
const esc = s => String(s).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
const runs = s => s.kind !== 'benchmark';
const value = (row, s) => runs(s) ? row.scores[s.key] : row.benchmarks[s.key];
const visible = () => SERIES.filter(s => !state.hidden.has(s.key));
const stats = s => view.stats[s.key];
const poolLabel = key => (DATA.pools.find(p => p.key === key) || {label: key}).label;
const rangeLabel = () => view.preset ? PRESETS.find(p => p.key === view.preset).label : `${state.range.from} 至 ${state.range.to}`;
const scopeLabel = () => `${state.pool === 'all' ? '全部股票池' : poolLabel(state.pool)} · ${rangeLabel()}`;
const ciText = m => m.ci === undefined ? '计算中…' : m.ci ? `[${fmt(m.ci[0])}, ${fmt(m.ci[1])}]` : '—';
const mean = a => a.length ? a.reduce((x, y) => x + y, 0) / a.length : null;
const plain = s => s.label.replace(/^对照：/, '');
const short = (text, n) => text.length > n ? `${text.slice(0, Math.ceil((n - 1) / 2))}…${text.slice(text.length - Math.floor((n - 1) / 2))}` : text;
const swatch = s => `<svg class="sw" width="22" height="10"><line x1="1" x2="21" y1="5" y2="5" stroke="${s.color}"
  stroke-width="${s.width + 1}" ${s.dash ? `stroke-dasharray="${s.dash}"` : ''}/></svg>`;
const rankBadge = s => view.rank[s.key] ? `<span class="rank ${view.rank[s.key] <= 3 ? 'top' : ''}">${view.rank[s.key]}</span>`
  : '<span class="rank">—</span>';
const rankText = s => view.rank[s.key] ? `${view.rank[s.key]} · ` : '';

const intervalKey = s => `${s.key}|${state.pool}|${state.range.from}|${state.range.to}`;
const returnsOf = s => view.scored.map(p => value(p, s));

function derive() {
  const {from, to} = state.range;
  view.rows = DATA.periods.filter(p => (state.pool === 'all' || p.pool === state.pool) && p.decision_date >= from && p.decision_date <= to);
  view.preset = (PRESETS.find(p => p.from === from && p.to === to) || {}).key;
  view.scored = view.rows.slice().sort((a, b) => a.order - b.order);
  view.stats = {};
  for (const s of SERIES) {
    const m = summarize(returnsOf(s), DATA.stats, false);
    if (m && INTERVALS.has(intervalKey(s))) m.ci = INTERVALS.get(intervalKey(s));
    view.stats[s.key] = m;
  }
  const ranked = SERIES.filter(s => s.kind === 'strategy' && stats(s)).sort((a, b) => stats(b).mean - stats(a).mean);
  view.rank = Object.fromEntries(ranked.map((s, i) => [s.key, i + 1]));
  if (state.picked && !view.rows.some(p => p.id === state.picked)) state.picked = null;
}

function ordered() {
  const score = s => (stats(s) || {mean: -Infinity}).mean;
  return Object.keys(GROUPS).flatMap(kind => SERIES.filter(s => s.kind === kind).sort((a, b) => score(b) - score(a)));
}

const tip = document.getElementById('tip');
function showTip(event, html) {
  tip.innerHTML = html;
  tip.hidden = false;
  const box = tip.getBoundingClientRect(), x = event.clientX + 14, y = event.clientY + 14;
  tip.style.left = `${Math.min(x, innerWidth - box.width - 8)}px`;
  tip.style.top = `${y + box.height > innerHeight ? Math.max(4, event.clientY - box.height - 10) : y}px`;
}
const hideTip = () => { tip.hidden = true; };

const focusStyle = document.getElementById('focus');
function applyFocus() {
  const key = state.hover || state.focus;
  focusStyle.textContent = key ? `.s:not([data-s="${key}"]) { opacity:.12; } polyline.s[data-s="${key}"] { stroke-width:3.4px; }
    .row[data-s="${key}"] { background:var(--pick); }` : '';
  if (key) document.querySelectorAll(`g.lines > [data-s="${key}"]`).forEach(el => el.parentNode.appendChild(el));
}
function setHover(key) {
  if (state.hover === (key || null)) return;
  state.hover = key || null;
  applyFocus();
}
function focusOn(key) {
  state.focus = key || null;
  update(['card']);
}

const RENDER = {timeline, list: seriesList, card: strategyCard, summary: summaryTable, means: meansChart, yearly, cumulative,
  periods: periodTable, picked: markPicked, detail};
const EVERYTHING = Object.keys(RENDER);
const VISIBILITY = ['list', 'summary', 'means', 'yearly', 'cumulative', 'periods', 'detail'];
const dirty = new Set();
let pending = 0;
function update(parts, filtered) {
  if (filtered) dirty.add('derive');
  parts.forEach(part => dirty.add(part));
  if (!pending) pending = requestAnimationFrame(flush);
}
function flush() {
  pending = 0;
  const derived = dirty.delete('derive');
  if (derived) derive();
  ['timeline', 'means', 'yearly', 'cumulative'].forEach(id => { if (dirty.has(id)) view.width[id] = Math.max(document.getElementById(id).clientWidth, 320); });
  for (const [name, render] of Object.entries(RENDER)) if (dirty.has(name)) render();
  dirty.clear();
  applyFocus();
  saveHash();
  if (derived) fillIntervals();
  if (state.reveal && state.picked) document.getElementById('detail').scrollIntoView({behavior: 'smooth', block: 'start'});
  state.reveal = false;
}

let filling = 0;
function fillIntervals() {
  clearTimeout(filling);
  if (state.dragging) return;
  filling = setTimeout(function step() {
    const start = performance.now();
    for (const s of SERIES) {
      const m = view.stats[s.key];
      if (!m || m.ci !== undefined) continue;
      m.ci = interval(returnsOf(s), DATA.stats);
      INTERVALS.set(intervalKey(s), m.ci);
      if (performance.now() - start > 30) { filling = setTimeout(step, 0); return; }
    }
    update(['summary', 'means', 'card']);
  }, 0);
}

function setRange(from, to) {
  if (from === state.range.from && to === state.range.to) return;
  state.range = {from, to};
  update(state.dragging ? EVERYTHING.filter(part => !['periods', 'detail'].includes(part)) : EVERYTHING, true);
}

function loadHash() {
  const q = new URLSearchParams(location.hash.slice(1));
  if (DATA.pools.some(p => p.key === q.get('pool'))) state.pool = q.get('pool');
  const [from, to] = [q.get('from'), q.get('to')];
  if (DATES.includes(from) && DATES.includes(to) && from <= to) state.range = {from, to};
  (q.get('hide') || '').split(',').filter(key => BY_KEY[key]).forEach(key => state.hidden.add(key));
  if (BY_KEY[q.get('focus')]) state.focus = q.get('focus');
  if (DATA.periods.some(p => p.id === q.get('period'))) state.picked = q.get('period');
  const [col, dir] = (q.get('sort') || '').split(':');
  if (col && ['1', '-1'].includes(dir)) state.sort = {col, dir: +dir};
}
function saveHash() {
  const q = new URLSearchParams();
  if (state.pool !== 'all') q.set('pool', state.pool);
  if (view.preset !== 'all') [['from', state.range.from], ['to', state.range.to]].forEach(([k, v]) => q.set(k, v));
  if (state.hidden.size) q.set('hide', [...state.hidden].join(','));
  if (state.focus) q.set('focus', state.focus);
  if (state.picked) q.set('period', state.picked);
  if (state.sort) q.set('sort', `${state.sort.col}:${state.sort.dir}`);
  const hash = q.toString();
  history.replaceState(null, '', hash ? `#${hash}` : location.pathname + location.search);
}

function header() {
  const r = DATA.release, pools = r.pools ? Object.entries(r.pools).map(([k, y]) =>
    `${poolLabel(k)} ${y[0]}–${y[y.length - 1]}`).join('，') : '';
  document.getElementById('meta').textContent = [r.version && `发布包 ${r.version}`, r.built_on && `构建于 ${r.built_on}`,
    pools, `${DATA.periods.length} 个决策点`, `看板生成于 ${DATA.generated_at}`].filter(Boolean).join(' · ');
  document.getElementById('metric').textContent = `主指标：${DATA.metric}。${DATA.cost_basis}。`;
  document.getElementById('limits').innerHTML = DATA.limitations.map(x => `<li>${esc(x)}</li>`).join('');
  const failed = document.getElementById('failures');
  failed.hidden = !DATA.failures.length;
  failed.querySelector('ul').innerHTML = DATA.failures.map(x => `<li><b>${esc(x.key)}</b>：${esc(x.error)}</li>`).join('');
}

function controls() {
  const choose = (target, choices, field) => {
    const box = document.getElementById(target);
    box.innerHTML = choices.map(c =>
      `<button data-key="${esc(c.key)}" class="${c.key === state[field] ? 'on' : ''}">${esc(c.label)}</button>`).join('');
    box.onclick = e => {
      const b = e.target.closest('button');
      if (!b || b.dataset.key === state[field]) return;
      state[field] = b.dataset.key;
      controls();
      update(EVERYTHING, true);
    };
  };
  choose('pools', [{key: 'all', label: '全部'}, ...DATA.pools], 'pool');
}

const BRUSH = {l: 10, r: 10, t: 4, b: 18, height: 70};
const MARKET = (() => {
  let total = 0;
  return DATES.map(d => total += mean(DATA.periods.filter(p => p.decision_date === d).map(p => p.benchmarks[MARKET_KEY])));
})();
const brushX = i => BRUSH.l + (DATES.length > 1 ? i / (DATES.length - 1) : 0.5) * (view.width.timeline - BRUSH.l - BRUSH.r);

function timeline() {
  const width = view.width.timeline, {t, b, height} = BRUSH, n = DATES.length, x = brushX, bottom = height - b;
  const lo = Math.min(0, ...MARKET), hi = Math.max(0, ...MARKET), y = v => t + (hi - v) / (hi - lo || 1) * (bottom - t);
  const fi = DATES.indexOf(state.range.from), ti = DATES.indexOf(state.range.to), left = x(fi), right = x(ti);
  const line = MARKET.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(' ');
  const years = DATES.map((d, i) => [i, d.slice(0, 4)]).filter(([i, year]) => !i || DATES[i - 1].slice(0, 4) !== year);
  const every = Math.max(1, Math.ceil(years.length / ((width - BRUSH.l - BRUSH.r) / 44)));
  const ticks = years.filter((_, k) => k % every === 0).map(([i, year]) => `<line x1="${x(i)}" x2="${x(i)}" y1="${bottom}"
    y2="${bottom + 4}" stroke="#8c959f"/><text x="${x(i) + 2}" y="${height - 4}">${year}</text>`).join('');
  const splits = PRESETS.filter(p => p.key !== 'all' && p.from !== DATES[0]).map(p => `<line x1="${x(DATES.indexOf(p.from))}"
    x2="${x(DATES.indexOf(p.from))}" y1="${t}" y2="${bottom}" stroke="#8c959f" stroke-dasharray="2 3"/>`).join('');
  const grip = at => `<rect class="grip" x="${at - 4}" y="${(t + bottom) / 2 - 11}" width="8" height="22" rx="3" fill="#fff" stroke="#0969da"/>`;
  document.getElementById('timeline').innerHTML = `<svg viewBox="0 0 ${width} ${height}" width="100%" height="${height}">
    <polygon points="${x(0)},${y(0)} ${line} ${x(n - 1)},${y(0)}" fill="#d0d7de" fill-opacity=".55"/>
    <polyline points="${line}" fill="none" stroke="#8c959f"/>${splits}${ticks}
    <rect x="0" y="${t}" width="${Math.max(left, 0)}" height="${bottom - t}" fill="#fff" fill-opacity=".7"/>
    <rect x="${right}" y="${t}" width="${Math.max(width - right, 0)}" height="${bottom - t}" fill="#fff" fill-opacity=".7"/>
    <rect class="window" x="${left}" y="${t}" width="${Math.max(right - left, 1)}" height="${bottom - t}" fill="#0969da"
      fill-opacity=".08" stroke="#0969da"/>${grip(left)}${grip(right)}</svg>`;
  document.getElementById('presets').innerHTML = PRESETS.map(p =>
    `<button data-from="${p.from}" data-to="${p.to}" class="${p.key === view.preset ? 'on' : ''}">${esc(p.label)}</button>`).join('');
  const [fromInput, toInput] = [document.getElementById('from'), document.getElementById('to')];
  [fromInput, toInput].forEach(input => Object.assign(input, {min: DATES[0], max: DATES[n - 1]}));
  fromInput.value = state.range.from;
  toInput.value = state.range.to;
  document.getElementById('range-note').textContent = `${view.rows.length} 个决策点${view.preset ? '' : ' · 自定义区间'}` +
    ` · 背景为${plain(BY_KEY[MARKET_KEY])}累计收益得分，拖动两端或中间调整，空白处拖出新区间`;
}

(() => {
  const box = document.getElementById('timeline');
  let drag = null;
  const locate = e => {
    const r = box.getBoundingClientRect(), sx = (e.clientX - r.left) * view.width.timeline / r.width;
    const i = Math.round((sx - BRUSH.l) / (view.width.timeline - BRUSH.l - BRUSH.r) * (DATES.length - 1));
    return {sx, i: Math.max(0, Math.min(DATES.length - 1, i))};
  };
  box.onpointerdown = e => {
    const {sx, i} = locate(e), fi = DATES.indexOf(state.range.from), ti = DATES.indexOf(state.range.to);
    const mode = Math.abs(sx - brushX(fi)) <= 8 ? 'from' : Math.abs(sx - brushX(ti)) <= 8 ? 'to' : i >= fi && i <= ti ? 'move' : 'new';
    drag = {mode, anchor: i, fi, ti};
    state.dragging = true;
    box.setPointerCapture(e.pointerId);
  };
  box.onpointermove = e => {
    if (!drag) return;
    const {i} = locate(e), last = DATES.length - 1;
    let {fi, ti} = drag;
    if (drag.mode === 'from') fi = Math.min(i, ti);
    else if (drag.mode === 'to') ti = Math.max(i, fi);
    else if (drag.mode === 'move') { const shift = Math.max(-fi, Math.min(last - ti, i - drag.anchor)); fi += shift; ti += shift; }
    else [fi, ti] = [Math.min(drag.anchor, i), Math.max(drag.anchor, i)];
    setRange(DATES[fi], DATES[ti]);
  };
  box.onpointerup = box.onpointercancel = () => {
    if (!drag) return;
    drag = null;
    state.dragging = false;
    update(EVERYTHING, true);
  };
  const snap = (value, forward) => forward ? DATES.find(d => d >= value) || DATES[DATES.length - 1]
    : DATES.slice().reverse().find(d => d <= value) || DATES[0];
  document.getElementById('from').onchange = e => {
    if (!e.target.value) return update(['timeline']);
    const from = snap(e.target.value, true);
    setRange(from, from > state.range.to ? from : state.range.to);
    update(['timeline']);
  };
  document.getElementById('to').onchange = e => {
    if (!e.target.value) return update(['timeline']);
    const to = snap(e.target.value, false);
    setRange(to < state.range.from ? to : state.range.from, to);
    update(['timeline']);
  };
  document.getElementById('presets').onclick = e => {
    const b = e.target.closest('button');
    if (b) setRange(b.dataset.from, b.dataset.to);
  };
})();

function seriesList() {
  let html = '', kind = null;
  for (const s of ordered()) {
    if (s.kind !== kind) html += `<div class="group">${GROUPS[kind = s.kind]}</div>`;
    const m = stats(s), off = state.hidden.has(s.key);
    html += `<div class="item row hov ${off ? 'off' : ''}" data-s="${s.key}" title="${esc(s.label)}">
      <input type="checkbox" data-toggle="${s.key}" ${off ? '' : 'checked'} aria-label="显示 ${esc(s.label)}">
      ${swatch(s)}${rankBadge(s)}<span class="name">${esc(plain(s))}</span>
      <span class="num ${tone(m && m.mean)}">${fmt(m && m.mean)}</span></div>`;
  }
  document.getElementById('list').innerHTML = html;
}

function strategyCard() {
  const box = document.getElementById('card'), s = BY_KEY[state.focus];
  if (!s) {
    box.innerHTML = `<h2>策略规则</h2><p class="note">点击左侧列表、汇总表、柱状图或曲线上的任一策略，
      在这里查看它的买卖规则与当前切片的成绩；再次点击或按 Esc 取消。</p>`;
    return;
  }
  const m = stats(s), place = {strategy: view.rank[s.key] ? `第 ${view.rank[s.key]} 名` : '当前切片无成绩',
    baseline: '对照基线，不占名次', benchmark: '市场对照'}[s.kind];
  const kpi = m ? `<div class="kpi"><div>总分<b class="${tone(m.mean)}">${fmt(m.mean)}</b></div>
      <div>95% 区间<b>${ciText(m)}</b></div>
      <div>平均收益得分<b class="${tone(m.return_mean)}">${fmt(m.return_mean)}</b></div>
      <div>胜率<b>${(m.win_rate * 100).toFixed(0)}%（${m.count} 期）</b></div></div>` : '<p class="note">当前切片没有决策点</p>';
  box.innerHTML = `<h2>${swatch(s)}<span>${esc(s.label)}</span></h2>
    <div class="meta">${s.kind === 'benchmark' ? '' : `<code>${esc(s.key)}</code> · `}${place} · ${esc(scopeLabel())}</div>
    ${kpi}<p class="rules">${esc(s.description)}</p><button class="link" data-unfocus>取消聚焦</button>`;
}

function summaryTable() {
  const data = view.rows;
  const count = (s, field) => data.reduce((n, p) => n + (Array.isArray(p[field][s.key]) ? p[field][s.key].length : p[field][s.key]), 0);
  const beat = (s, k) => data.filter(p => p.scores[s.key] > p.benchmarks[k]).length;
  const columns = [
    {id: 'rank', label: '名次', key: s => view.rank[s.key], cell: s => rankBadge(s)},
    {id: 'name', label: '策略', cls: 'l', key: s => s.label, cell: s => `${swatch(s)} ${esc(s.label)}`},
    {id: 'mean', label: '总分', key: (s, m) => m.mean, cell: (s, m) => `<span class="${tone(m.mean)}">${fmt(m.mean)}</span>`},
    {id: 'median', label: '总分中位数', key: (s, m) => m.median, cell: (s, m) => fmt(m.median)},
    {id: 'ci', label: '总分 95% 区间', key: (s, m) => m.ci && m.ci[0], cell: (s, m) => ciText(m)},
    {id: 'return', label: '平均收益得分', key: (s, m) => m.return_mean,
     cell: (s, m) => `<span class="${tone(m.return_mean)}">${fmt(m.return_mean)}</span>`},
    {id: 'win', label: '胜率（正得分期数）', key: (s, m) => m.win_rate,
     cell: (s, m) => `${(m.win_rate * 100).toFixed(0)}%（${data.filter(p => value(p, s) > 0).length}/${data.length}）`},
    {id: 'csi300', label: '胜沪深300', key: s => runs(s) ? beat(s, 'csi300') : null,
     cell: s => runs(s) ? `${beat(s, 'csi300')}/${data.length}` : '—'},
    {id: 'pool', label: '胜股票池等权', key: s => runs(s) ? beat(s, 'pool_equal_weight') : null,
     cell: s => runs(s) ? `${beat(s, 'pool_equal_weight')}/${data.length}` : '—'},
    ...[['blocked', '买不进只次'], ['exits', '卖出只次'], ['grid_trades', '网格成交笔数']].map(([field, label]) =>
      ({id: field, label, key: s => runs(s) ? count(s, field) : null, cell: s => runs(s) ? count(s, field) : '—'}))];
  const sort = state.sort && columns.some(c => c.id === state.sort.col) ? state.sort : {col: 'mean', dir: -1};
  const column = columns.find(c => c.id === sort.col), key = x => column.key(x.s, x.m);
  const list = SERIES.map(s => ({s, m: stats(s)})).filter(x => x.m).sort((a, b) => {
    const x = key(a), y = key(b);
    if (x == null || y == null) return (x == null) - (y == null);
    return (typeof x === 'string' ? x.localeCompare(y, 'zh') : x - y) * sort.dir;
  });
  const arrow = c => c.id === sort.col ? (sort.dir > 0 ? ' ▲' : ' ▼') : '';
  const table = document.getElementById('summary');
  table.innerHTML = `<thead><tr>${columns.map(c => `<th data-sort="${c.id}" class="${c.cls || ''}">${c.label}${arrow(c)}</th>`).join('')}</tr></thead>
    <tbody>${list.map(({s, m}) => `<tr class="row hov ${state.hidden.has(s.key) ? 'off' : ''}" data-s="${s.key}">${columns.map(c =>
      `<td class="${c.cls || ''}">${c.cell(s, m)}</td>`).join('')}</tr>`).join('')}</tbody>`;
  table.querySelector('thead').onclick = e => {
    const th = e.target.closest('[data-sort]');
    if (!th) return;
    const col = th.dataset.sort;
    state.sort = {col, dir: col === sort.col ? -sort.dir : ['rank', 'name'].includes(col) ? 1 : -1};
    update(['summary']);
  };
}

function ticks(lo, hi) {
  const span = hi - lo || 1, magnitude = Math.pow(10, Math.floor(Math.log10(span / 5)));
  const step = [1, 2, 5, 10].map(k => k * magnitude).find(k => span / k <= 6);
  const output = [];
  for (let v = Math.ceil(lo / step) * step; v <= hi + 1e-9; v += step) output.push(+v.toFixed(10));
  return output;
}

function meansChart() {
  const box = document.getElementById('means'), series = ordered().filter(s => !state.hidden.has(s.key) && stats(s));
  const sums = series.map(stats), width = view.width.means, row = 22;
  const pad = {l: Math.min(200, width * 0.4), r: 16, t: 4, b: 22}, height = pad.t + pad.b + row * Math.max(series.length, 1);
  const lo = Math.min(0, ...sums.map(m => m.ci ? m.ci[0] : m.mean)), hi = Math.max(0, ...sums.map(m => m.ci ? m.ci[1] : m.mean));
  const x = v => pad.l + (v - lo) / (hi - lo || 1) * (width - pad.l - pad.r), chars = Math.floor((pad.l - 12) / 12.5);
  const grid = ticks(lo, hi).map(v => `<line x1="${x(v)}" x2="${x(v)}" y1="${pad.t}" y2="${height - pad.b}"
    stroke="${Math.abs(v) < 1e-9 ? '#8c959f' : '#eaeef2'}"/><text x="${x(v)}" y="${height - 6}" text-anchor="middle">${v}</text>`).join('');
  const bars = series.map((s, i) => {
    const m = sums[i], y = pad.t + row * i, mid = y + row / 2, cap = (v, h) => `<line x1="${x(v)}" x2="${x(v)}" y1="${mid - h}" y2="${mid + h}" stroke="#24292f"/>`;
    const ci = m.ci ? `<line x1="${x(m.ci[0])}" x2="${x(m.ci[1])}" y1="${mid}" y2="${mid}" stroke="#24292f"/>${cap(m.ci[0], 4)}${cap(m.ci[1], 4)}` : '';
    return `<g class="s hov" data-s="${s.key}"><rect x="0" y="${y}" width="${width}" height="${row}" fill="transparent"/>
      <text class="lbl" x="${pad.l - 8}" y="${mid + 4}" text-anchor="end">${esc(rankText(s) + short(plain(s), chars - rankText(s).length))}</text>
      <rect x="${Math.min(x(0), x(m.mean))}" y="${y + 5}" width="${Math.max(Math.abs(x(m.mean) - x(0)), 1)}" height="${row - 10}"
        fill="${s.color}" opacity="${s.kind === 'strategy' ? 0.9 : 0.6}"/>${ci}</g>`;
  }).join('');
  box.innerHTML = `<svg viewBox="0 0 ${width} ${height}" width="100%">${grid}${bars}</svg>`;
  box.querySelector('svg').onmousemove = e => {
    const g = e.target.closest('[data-s]');
    if (!g) return hideTip();
    const s = BY_KEY[g.dataset.s], m = stats(s);
    showTip(e, `<b>${swatch(s)} ${esc(rankText(s) + s.label)}</b><div><span>总分</span><span>${fmt(m.mean)}</span></div>
      <div><span>95% 区间</span><span>${ciText(m)}</span></div>
      <div><span>平均收益得分</span><span>${fmt(m.return_mean)}</span></div>
      <div><span>胜率</span><span>${(m.win_rate * 100).toFixed(0)}%（${m.count} 期）</span></div>`);
  };
  box.querySelector('svg').onmouseleave = hideTip;
}

function lineChart(target, points, labels, titles, height, pick, tagged) {
  const box = document.getElementById(target), width = view.width[target], series = visible();
  const all = series.flatMap(s => points[s.key].filter(v => v != null)), lo = Math.min(0, ...all), hi = Math.max(0, ...all);
  const pad = {l: 44, r: tagged ? Math.round(Math.min(260, Math.max(170, width * 0.2))) : 14, t: 10, b: 26}, right = width - pad.r;
  const n = labels.length, x = i => pad.l + (n > 1 ? i / (n - 1) : 0.5) * (right - pad.l);
  const y = v => pad.t + (hi - v) / (hi - lo || 1) * (height - pad.t - pad.b);
  const grid = ticks(lo, hi).map(v => `<line x1="${pad.l}" x2="${right}" y1="${y(v)}" y2="${y(v)}"
    stroke="${Math.abs(v) < 1e-9 ? '#8c959f' : '#eaeef2'}"/><text x="${pad.l - 6}" y="${y(v) + 4}" text-anchor="end">${v}</text>`).join('');
  const every = Math.max(1, Math.ceil(n / ((right - pad.l) / 72)));
  const axis = labels.map((l, i) => i % every ? '' : `<text x="${x(i)}" y="${height - 8}" text-anchor="middle">${esc(l)}</text>`).join('');
  const lines = series.map(s => {
    const pts = points[s.key].map((v, i) => v == null ? null : `${x(i).toFixed(1)},${y(v).toFixed(1)}`).filter(Boolean).join(' ');
    const dots = n > 40 ? '' : `<g class="s" data-s="${s.key}">${points[s.key].map((v, i) => v == null ? ''
      : `<circle cx="${x(i)}" cy="${y(v)}" r="2.5" fill="${s.color}"/>`).join('')}</g>`;
    return `<polyline class="s" data-s="${s.key}" points="${pts}" fill="none" stroke="${s.color}" stroke-width="${s.width}"
      stroke-linejoin="round" ${s.dash ? `stroke-dasharray="${s.dash}"` : ''}/>${dots}`;
  }).join('');
  let tags = '';
  if (tagged && n) {
    const gap = 14, top = pad.t + 4, bottom = height - pad.b, chars = Math.floor((pad.r - 28) / 12);
    const items = series.map(s => {
      const v = points[s.key].filter(p => p != null).pop();
      return v == null ? null : {s, v, at: y(v)};
    }).filter(Boolean).sort((a, b) => a.at - b.at);
    items.forEach((it, i) => { it.ty = Math.max(it.at, i ? items[i - 1].ty + gap : top); });
    for (let i = items.length - 1; i >= 0; i--) items[i].ty = Math.min(items[i].ty, i === items.length - 1 ? bottom : items[i + 1].ty - gap);
    tags = items.map(({s, at, ty}) => `<g class="s hov" data-s="${s.key}">
      <path d="M${right + 2},${at} L${right + 10},${ty}" stroke="${s.color}" stroke-opacity=".6" fill="none"/>
      <rect x="${right + 10}" y="${ty - 7}" width="${pad.r - 12}" height="14" fill="transparent"/>
      <rect x="${right + 12}" y="${ty - 4}" width="8" height="8" rx="2" fill="${s.color}"/>
      <text class="lbl" x="${right + 24}" y="${ty + 4}">${esc(rankText(s) + short(plain(s), chars - rankText(s).length))}</text></g>`).join('');
  }
  box.innerHTML = `<svg viewBox="0 0 ${width} ${height}" width="100%" class="chart">${grid}${axis}<g class="lines">${lines}</g>${tags}
    <line class="cross" y1="${pad.t}" y2="${height - pad.b}" stroke="#8c959f" stroke-dasharray="3 3" visibility="hidden"/></svg>`;
  const svg = box.querySelector('svg'), cross = svg.querySelector('.cross');
  const locate = e => { const r = svg.getBoundingClientRect(); return [(e.clientX - r.left) * width / r.width, (e.clientY - r.top) * height / r.height]; };
  const index = sx => Math.max(0, Math.min(n - 1, Math.round((sx - pad.l) / (right - pad.l) * (n - 1))));
  const leave = () => { cross.setAttribute('visibility', 'hidden'); hideTip(); };
  svg.onmousemove = e => {
    const [sx, sy] = locate(e), tag = e.target.closest('g.hov');
    if (tag) {
      const s = BY_KEY[tag.dataset.s];
      cross.setAttribute('visibility', 'hidden');
      const v = points[s.key].filter(p => p != null).pop();
      return showTip(e, `<b>${swatch(s)} ${esc(rankText(s) + s.label)}</b><div><span>期末</span><span class="${tone(v)}">${fmt(v)}</span></div>
        <p class="note">点击查看规则</p>`);
    }
    if (!n || sx < pad.l - 4 || sx > right + 4) { setHover(null); return leave(); }
    const i = index(sx), ranked = series.map(s => [s, points[s.key][i]]).filter(([, v]) => v != null).sort((a, b) => b[1] - a[1]);
    let near = null, best = 12;
    ranked.forEach(([s, v]) => { const d = Math.abs(y(v) - sy); if (d < best) { best = d; near = s; } });
    setHover(near && near.key);
    cross.setAttribute('x1', x(i)); cross.setAttribute('x2', x(i)); cross.setAttribute('visibility', 'visible');
    showTip(e, `<b>${esc(titles[i])}</b>${ranked.map(([s, v]) => `<div class="${s === near ? 'hit' : ''}"><span>${swatch(s)}
      ${esc(rankText(s) + s.label)}</span><span class="${tone(v)}">${fmt(v)}</span></div>`).join('')}
      <p class="note">${[near && '点击查看高亮策略的规则', pick && '点击查看当期持仓'].filter(Boolean).join('，') || '靠近曲线可高亮该策略'}</p>`);
  };
  svg.onmouseleave = () => { leave(); setHover(null); };
  svg.onclick = e => {
    if (e.target.closest('g.hov')) return;
    const [sx] = locate(e);
    if (state.hover) focusOn(state.hover);
    if (pick && n && sx >= pad.l - 4 && sx <= right + 4) pick(index(sx));
  };
}

function yearly() {
  const data = view.rows, years = [...new Set(data.map(p => p.decision_date.slice(0, 4)))].sort(), points = {};
  SERIES.forEach(s => points[s.key] = years.map(y => mean(data.filter(p => p.decision_date.startsWith(y)).map(p => value(p, s)))));
  lineChart('yearly', points, years, years.map(y => `${y} 年平均收益得分`), 280);
}

function cumulative() {
  const data = view.rows, points = {};
  SERIES.forEach(s => { let total = 0; points[s.key] = data.map(p => total += value(p, s)); });
  lineChart('cumulative', points, data.map(p => p.decision_date), data.map(p => `${p.decision_date} ${poolLabel(p.pool)}（累计）`),
    340, i => pickPeriod(data[i].id, true), true);
}

function periodTable() {
  const series = visible(), data = view.rows.slice().reverse();
  const head = `<thead><tr><th>决策日</th><th class="l">股票池</th><th>建仓日</th>${series.map(s =>
    `<th class="row hov" data-s="${s.key}" title="${esc(s.label)}">${swatch(s)}<br>${esc(rankText(s) + short(plain(s), 8))}</th>`).join('')}</tr></thead>`;
  const body = data.map(p => `<tr data-period="${esc(p.id)}"><td>${p.decision_date}</td>
    <td class="l">${esc(poolLabel(p.pool))}</td><td>${p.entry_date}</td>
    ${series.map(s => { const v = value(p, s), b = runs(s) ? p.blocked[s.key].length : 0, x = runs(s) ? p.exits[s.key].length : 0;
      return `<td class="row ${tone(v)}" data-s="${s.key}">${fmt(v)}${b ? ` <span class="meta">(${b})</span>` : ''}${x ? ` <span class="meta">卖${x}</span>` : ''}</td>`; }).join('')}</tr>`).join('');
  const table = document.getElementById('periods');
  table.innerHTML = head + `<tbody>${body}</tbody>`;
  table.querySelector('tbody').onclick = e => { const tr = e.target.closest('[data-period]'); if (tr) pickPeriod(tr.dataset.period, false); };
  markPicked();
}

function markPicked() {
  document.querySelectorAll('#periods [data-period]').forEach(tr => tr.classList.toggle('picked', tr.dataset.period === state.picked));
}

function initialWeight(weight, rule) {
  return Number(weight) * (rule && rule.step ? rule.initial / rule.lots : 1);
}

function ruleText(rule, weight) {
  if (!rule) return '首买预算的 100%，窗口内持有到期';
  if (rule.step) return `网格线距 ${pct(rule.step)}；首买预算的 ${pct(rule.initial / rule.lots)}；每格加仓预算的 ${pct(1 / rule.lots)}（期初资产的 ${pct(Number(weight) / rule.lots)}）；每格减持 1 份，卖出比例按当时持有份数计算`;
  const trigger = Object.entries(rule).filter(([k]) => k !== 'sell_fraction').map(([k, v]) =>
    k === 'max_hold_days' ? `最长持有 ${v} 天` : `${RULES[k] || k} ${pct(v)}`).join('，');
  return `${trigger}；触发后一次卖出该股持股数量的 ${pct(rule.sell_fraction)}，余仓持有到期`;
}

function operationsTable(operations) {
  if (!operations.length) return '<p class="note">本期无实际买卖成交</p>';
  return `<details><summary>每次买卖操作（${operations.length} 笔）</summary>
    <p class="note">成交资金与成交后持仓市值均以期初总资产为分母；买入资金含买入费用预算，卖出为扣费前金额。成交后持仓按该次开盘价计，不是组合当时权重。</p>
    <div class="wide"><table><thead><tr><th>成交日</th><th class="l">证券</th><th class="l">操作与触发</th><th class="l">本次操作比例</th><th>成交资金/期初资产</th><th>成交后持仓/期初资产</th></tr></thead>
    <tbody>${operations.map(t => `<tr><td>${esc(t.execution_date)}</td><td class="l">${esc(t.symbol)}</td>
      <td class="l">${t.side === 'buy' ? '买入' : '卖出'} · ${esc(t.reason === 'entry' ? '首次建仓' : t.reason === 'grid' ? '网格' : RULES[t.reason] || t.reason)}${t.signal_date ? `（${esc(t.signal_date)} 触发）` : ''}</td>
      <td class="l">${t.side === 'sell' ? `卖出当时持股的 ${pct(t.position_fraction)}` : `使用该股预算的 ${pct(Number(t.cash_fraction) / Number(t.budget_fraction))}`}</td>
      <td>${pct(t.cash_fraction)}</td><td>${pct(t.remaining_value_fraction)}</td></tr>`).join('')}</tbody></table></div></details>`;
}

function detail() {
  const box = document.getElementById('detail'), p = DATA.periods.find(x => x.id === state.picked);
  box.hidden = !p;
  if (!p) return;
  const benchmarks = SERIES.filter(s => s.kind === 'benchmark').map(s => `${esc(plain(s))} ${fmt(value(p, s))}`).join('，');
  document.getElementById('detail-title').textContent = `当期明细：${p.decision_date} ${poolLabel(p.pool)}，建仓日 ${p.entry_date}`;
  const cards = ordered().filter(s => runs(s) && !state.hidden.has(s.key)).map(s => {
    const v = p.scores[s.key], seg = p.segment_returns[s.key], held = p.holdings[s.key];
    const exits = Object.fromEntries(p.exits[s.key].map(e => [e.symbol, e])), blocked = new Set(p.blocked[s.key]);
    const outcome = code => { const e = exits[code];
      if (blocked.has(code)) return '买不进，留作现金';
      return e ? `${RULES[e.reason] || e.reason} ${e.signal_date} 触发，${e.sell_date ? `${e.sell_date} 卖出持股的 ${pct(e.sell_fraction)}` : '窗口内未能卖出'}` : ''; };
    const initial = held.reduce((sum, [code, weight, rule]) => sum + (blocked.has(code) ? 0 : initialWeight(weight, rule)), 0);
    const body = held.length ? `<p class="note">计划首买占期初总资产 ${pct(held.reduce((sum, [, weight, rule]) => sum + initialWeight(weight, rule), 0))}；实际用于首买 ${pct(initial)}，初始现金 ${pct(1 - initial)}。初始现金包含网格预留资金及买不进的预算。</p>
      <div class="wide"><table><thead><tr><th class="l">证券</th><th>预算/期初资产</th><th>首买/期初资产</th><th class="l">每次操作规则</th><th class="l">执行</th></tr></thead>
      <tbody>${held.map(([code, weight, rule]) => `<tr><td class="l">${esc(code)}</td><td>${pct(weight)}</td><td>${pct(initialWeight(weight, rule))}</td>
        <td class="l">${esc(ruleText(rule, weight))}</td><td class="l">${esc(outcome(code))}</td></tr>`).join('')}</tbody></table></div>`
      : '<p class="note">全部持有现金</p>';
    return `<div class="row" data-s="${s.key}"><h3 class="hov" data-s="${s.key}">${swatch(s)} ${esc(rankText(s) + s.label)}：
      <span class="${tone(v)}">${fmt(v)}</span></h3>
      <p class="note">0–7 天 ${pct(seg[0])} · 7–30 天 ${pct(seg[1])} · 30–90 天 ${pct(seg[2])}</p>${body}${operationsTable(p.operations[s.key])}</div>`;
  }).join('');
  document.getElementById('detail-body').innerHTML = `<p class="note">对照收益得分：${benchmarks}。只列出当前显示的策略，按名次排列。</p>
    <div class="holdings">${cards}</div>`;
}

function pickPeriod(id, reveal) {
  state.picked = state.picked === id && !reveal ? null : id;
  state.reveal = reveal;
  update(['picked', 'detail']);
}

document.getElementById('list').addEventListener('change', e => {
  const key = e.target.dataset.toggle;
  if (!key) return;
  e.target.checked ? state.hidden.delete(key) : state.hidden.add(key);
  update(VISIBILITY);
});
document.getElementById('quick').onclick = e => {
  const mode = e.target.dataset.show;
  if (!mode) return;
  const keep = s => mode === 'all' || (mode === 'top5' ? s.kind === 'benchmark' || view.rank[s.key] <= 5 : s.kind === 'strategy');
  state.hidden = new Set(SERIES.filter(s => !keep(s)).map(s => s.key));
  update(VISIBILITY);
};
document.addEventListener('mouseover', e => {
  const el = e.target.closest('.hov');
  if (el) setHover(el.dataset.s);
  else if (!e.target.closest('svg.chart')) setHover(null);
});
document.addEventListener('click', e => {
  if (e.target.closest('[data-unfocus]')) return focusOn(null);
  const el = e.target.closest('.hov');
  if (el && !e.target.closest('input')) focusOn(state.focus === el.dataset.s ? null : el.dataset.s);
});
document.addEventListener('keydown', e => { if (e.key === 'Escape' && state.focus) focusOn(null); });

let measured = 0;
new ResizeObserver(([entry]) => {
  const width = Math.round(entry.contentRect.width);
  if (width !== measured) { measured = width; update(['timeline', 'means', 'yearly', 'cumulative']); }
}).observe(document.querySelector('main'));

function watch() {
  const live = document.getElementById('live');
  if (!LIVE) {
    live.textContent = '数据随部署更新';
    return;
  }
  live.textContent = '自动刷新已开启：bench-run 重新生成看板后自动载入';
  let stamp = null;
  const poll = async () => {
    if (!document.hidden) {
      try {
        const r = await fetch(SOURCE, {method: 'HEAD', cache: 'no-store'});
        const next = `${r.headers.get('Last-Modified')}|${r.headers.get('Content-Length')}`;
        if (r.ok && stamp && next !== stamp) return location.reload();
        if (r.ok) [stamp, live.textContent] = [next, '自动刷新已开启：bench-run 重新生成看板后自动载入'];
      } catch (error) {
        live.textContent = '看板服务已停止，自动刷新暂停';
      }
    }
    setTimeout(poll, 3000);
  };
  poll();
}

loadHash();
header();
controls();
watch();
update(EVERYTHING, true);
"""
