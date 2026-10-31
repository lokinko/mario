"""把 `bench-run` 输出目录中各策略的成绩汇总成可离线打开的单文件 HTML 看板。

看板只读取成绩与策略说明，不读取输入或结果标签；图表由本模块统一生成，新增策略不需要改看板代码。
"""

import json
import math
from datetime import datetime

from .files import read_json, require
from .public_sources import SHANGHAI
from .strategy_runner import completed_runs, failed_runs

POOL_LABELS = {"dividend": "红利池", "tech": "科技池"}
BENCHMARKS = {"pool_equal_weight": "对照：股票池等权", "csi300": "对照：沪深300"}


def summary(value):
    return {"mean": value["mean"], "median": value["median"], "ci": value["bootstrap_95"], "count": value["count"]}


def dashboard_data(runs, config=None, failures=None):
    """runs：{策略名: {"title", "description", "scorecard"}}；各成绩必须覆盖同一批决策点且对照得分一致。"""
    require(runs, "缺少策略成绩")
    names = sorted(runs)
    scorecards = {name: runs[name]["scorecard"] for name in names}
    first = scorecards[names[0]]
    ids = [period["sample_id"] for period in first["periods"]]
    periods = {period["sample_id"]: {
        "id": period["sample_id"], "pool": period["pool"], "decision_date": period["decision_date"],
        "entry_date": period["entry_date"], "scores": {}, "blocked": {},
        "benchmarks": {"pool_equal_weight": period["pool_equal_weight_score"], "csi300": period["csi300_score"]}}
        for period in first["periods"]}
    summaries = {}
    for name in names:
        report = scorecards[name]
        require([p["sample_id"] for p in report["periods"]] == ids, f"{name} 与其他策略的决策点不一致，可能来自不同发布包")
        for period in report["periods"]:
            row = periods[period["sample_id"]]
            for key, value in (("pool_equal_weight", period["pool_equal_weight_score"]),
                               ("csi300", period["csi300_score"])):
                require(math.isclose(row["benchmarks"][key], value, abs_tol=1e-9), f"{name} 的对照得分不一致")
            row["scores"][name] = period["score"]
            row["blocked"][name] = len(period["blocked_entries"])
        summaries[name] = {"all": summary(report["score"]),
                           **{pool: summary(value["score"]) for pool, value in report["by_pool"].items()}}
    for key, field in (("pool_equal_weight", "pool_equal_weight_score"), ("csi300", "csi300_score")):
        summaries[key] = {"all": summary(first[field]),
                          **{pool: summary(value[field]) for pool, value in first["by_pool"].items()}}
    return {
        "strategies": [{"key": name, "label": runs[name]["title"], "description": runs[name]["description"]}
                       for name in names],
        "failures": [{"key": name, "error": error} for name, error in sorted((failures or {}).items())],
        "benchmarks": [{"key": key, "label": label} for key, label in BENCHMARKS.items()],
        "pools": [{"key": pool, "label": POOL_LABELS.get(pool, pool)} for pool in sorted(first["by_pool"])],
        "summaries": summaries, "periods": sorted(periods.values(), key=lambda row: (row["decision_date"], row["pool"])),
        "metric": first["metric"], "cost_basis": first["cost_basis"], "limitations": first["limitations"],
        "release": {key: config[key] for key in ("version", "built_on", "pools", "reference_total_return_index")
                    if config and key in config},
        "generated_at": datetime.now(SHANGHAI).isoformat(timespec="seconds")}


def render(data):
    payload = json.dumps(data, ensure_ascii=False, allow_nan=False).replace("</", "<\\/")
    return TEMPLATE.replace("__DATA__", payload)


def write_dashboard(runs_dir, destination, release=None):
    config = read_json(release / "config.json") if release else None
    data = dashboard_data(completed_runs(runs_dir), config, failed_runs(runs_dir))
    destination.parent.mkdir(parents=True, exist_ok=True)
    part = destination.with_suffix(destination.suffix + ".part")
    part.write_text(render(data), encoding="utf-8")
    part.replace(destination)
    return {"dashboard": str(destination), "strategies": [s["key"] for s in data["strategies"]],
            "periods": len(data["periods"])}


TEMPLATE = """<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>mario_bench 策略得分看板</title>
<style>
:root { --fg:#1f2328; --muted:#656d76; --line:#d0d7de; --bg:#f6f8fa; --up:#cf222e; --down:#1a7f37; }
* { box-sizing:border-box; }
body { margin:0; font:14px/1.5 -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif; color:var(--fg); background:#fff; }
header { padding:20px 28px 12px; border-bottom:1px solid var(--line); }
h1 { font-size:20px; margin:0 0 4px; }
h2 { font-size:15px; margin:0 0 10px; }
.meta { color:var(--muted); font-size:12px; }
main { padding:16px 28px 40px; display:grid; gap:20px; }
.bar { display:flex; gap:8px; flex-wrap:wrap; align-items:center; }
button { border:1px solid var(--line); background:#fff; border-radius:6px; padding:4px 12px; cursor:pointer; font:inherit; }
button.on { background:var(--fg); color:#fff; border-color:var(--fg); }
.card { border:1px solid var(--line); border-radius:8px; padding:16px; }
.grid { display:grid; grid-template-columns:repeat(auto-fit, minmax(420px, 1fr)); gap:20px; }
table { border-collapse:collapse; width:100%; font-variant-numeric:tabular-nums; }
th, td { padding:5px 8px; border-bottom:1px solid var(--line); text-align:right; white-space:nowrap; }
th:first-child, td:first-child, th.l, td.l { text-align:left; }
thead th { background:var(--bg); position:sticky; top:0; }
.scroll { max-height:520px; overflow:auto; }
.up { color:var(--up); } .down { color:var(--down); }
.swatch { display:inline-block; width:10px; height:10px; border-radius:2px; margin-right:6px; vertical-align:-1px; }
.legend label { margin-right:14px; cursor:pointer; user-select:none; }
svg text { font-size:11px; fill:var(--muted); }
.note { color:var(--muted); font-size:12px; margin:8px 0 0; }
.failed { border-color:var(--up); } .failed li { white-space:pre-wrap; font-family:ui-monospace, monospace; font-size:12px; }
.desc h3 { font-size:14px; margin:12px 0 4px; } .desc p { margin:0; white-space:pre-wrap; color:var(--muted); }
.desc code { font-size:12px; }
</style>
</head>
<body>
<header>
  <h1>mario_bench 策略得分看板</h1>
  <div class="meta" id="meta"></div>
</header>
<main>
  <div class="bar"><span>股票池：</span><span id="pools"></span></div>
  <div class="legend" id="legend"></div>
  <section class="card failed" id="failures" hidden><h2>运行失败的策略（未计入看板）</h2><ul></ul></section>
  <section class="card"><h2>策略汇总</h2><table id="summary"></table>
    <p class="note" id="metric"></p></section>
  <div class="grid">
    <section class="card"><h2>平均得分与 95% 区间</h2><div id="means"></div></section>
    <section class="card"><h2>逐年平均得分</h2><div id="yearly"></div></section>
  </div>
  <section class="card"><h2>累计得分（逐期得分求和，不是净值）</h2><div id="cumulative"></div></section>
  <section class="card"><h2>逐期明细</h2><div class="scroll"><table id="periods"></table></div>
    <p class="note">红色为正、绿色为负（A 股习惯）。买不进的股票（停牌或一字涨停）按规则留作现金，括号内为该期买不进的只数。</p></section>
  <section class="card"><h2>策略说明</h2><div id="descriptions"></div></section>
  <section class="card"><h2>口径与限制</h2><ul id="limits" class="note"></ul></section>
</main>
<script type="application/json" id="data">__DATA__</script>
<script>
const DATA = JSON.parse(document.getElementById('data').textContent);
const COLORS = ['#0969da', '#bf3989', '#9a6700', '#57606a', '#8250df', '#1b7c83', '#bc4c00', '#2da44e',
  '#6639ba', '#0550ae', '#953800', '#4d2d00'];
const SERIES = [...DATA.strategies.map((s, i) => ({...s, kind: 'strategy', color: COLORS[i % COLORS.length]})),
  {...DATA.benchmarks[0], kind: 'benchmark', color: '#d4a72c', dash: '5 3'},
  {...DATA.benchmarks[1], kind: 'benchmark', color: '#cf222e', dash: '2 3'}];
const state = {pool: 'all', hidden: new Set()};
const fmt = v => v == null ? '—' : v.toFixed(2);
const tone = v => v > 0 ? 'up' : v < 0 ? 'down' : '';
const esc = s => String(s).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
const value = (row, s) => s.kind === 'strategy' ? row.scores[s.key] : row.benchmarks[s.key];
const visible = () => SERIES.filter(s => !state.hidden.has(s.key));
const rows = () => DATA.periods.filter(p => state.pool === 'all' || p.pool === state.pool);
const mean = a => a.length ? a.reduce((x, y) => x + y, 0) / a.length : null;

function header() {
  const r = DATA.release, pools = r.pools ? Object.entries(r.pools).map(([k, y]) =>
    `${(DATA.pools.find(p => p.key === k) || {label: k}).label} ${y[0]}–${y[y.length - 1]}`).join('，') : '';
  document.getElementById('meta').textContent = [r.version && `发布包 ${r.version}`, r.built_on && `构建于 ${r.built_on}`,
    pools, `${DATA.periods.length} 个决策点`, `看板生成于 ${DATA.generated_at}`].filter(Boolean).join(' · ');
  document.getElementById('metric').textContent = `主指标：${DATA.metric}。${DATA.cost_basis}。`;
  document.getElementById('limits').innerHTML = DATA.limitations.map(x => `<li>${esc(x)}</li>`).join('');
  const failed = document.getElementById('failures');
  failed.hidden = !DATA.failures.length;
  failed.querySelector('ul').innerHTML = DATA.failures.map(x => `<li><b>${esc(x.key)}</b>：${esc(x.error)}</li>`).join('');
  document.getElementById('descriptions').innerHTML = SERIES.filter(s => s.kind === 'strategy').map(s =>
    `<div class="desc"><h3><span class="swatch" style="background:${s.color}"></span>${esc(s.label)} <code>${esc(s.key)}</code></h3>
    <p>${esc(s.description)}</p></div>`).join('');
  const choices = [{key: 'all', label: '全部'}, ...DATA.pools];
  document.getElementById('pools').innerHTML = choices.map(p =>
    `<button data-pool="${p.key}" class="${p.key === state.pool ? 'on' : ''}">${esc(p.label)}</button>`).join(' ');
  document.querySelectorAll('[data-pool]').forEach(b => b.onclick = () => { state.pool = b.dataset.pool; draw(); });
  document.getElementById('legend').innerHTML = SERIES.map(s => `<label><input type="checkbox" data-series="${s.key}"
    ${state.hidden.has(s.key) ? '' : 'checked'}><span class="swatch" style="background:${s.color}"></span>${esc(s.label)}</label>`).join('');
  document.querySelectorAll('[data-series]').forEach(b => b.onchange = () => {
    b.checked ? state.hidden.delete(b.dataset.series) : state.hidden.add(b.dataset.series); draw(); });
}

function summaryTable() {
  const data = rows(), blocked = s => data.reduce((n, p) => n + (p.blocked[s.key] || 0), 0);
  const body = SERIES.map(s => {
    const sum = DATA.summaries[s.key][state.pool], scores = data.map(p => value(p, s));
    const beat = k => s.kind === 'strategy' ? `${data.filter(p => p.scores[s.key] > p.benchmarks[k]).length}/${data.length}` : '—';
    return `<tr><td><span class="swatch" style="background:${s.color}"></span>${esc(s.label)}</td>
      <td class="${tone(sum.mean)}">${fmt(sum.mean)}</td><td>${fmt(sum.median)}</td>
      <td>${sum.ci ? `[${fmt(sum.ci[0])}, ${fmt(sum.ci[1])}]` : '—'}</td>
      <td>${beat('csi300')}</td><td>${beat('pool_equal_weight')}</td>
      <td>${scores.filter(v => v > 0).length}/${data.length}</td><td>${s.kind === 'strategy' ? blocked(s) : '—'}</td></tr>`;
  }).join('');
  document.getElementById('summary').innerHTML = `<thead><tr><th>策略</th><th>平均得分</th><th>中位数</th><th>95% 区间</th>
    <th>胜沪深300</th><th>胜股票池等权</th><th>正得分期数</th><th>买不进只次</th></tr></thead><tbody>${body}</tbody>`;
}

function frame(width, height, pad, ymin, ymax) {
  const y = v => pad.t + (ymax - v) / (ymax - ymin || 1) * (height - pad.t - pad.b);
  const step = Math.pow(10, Math.floor(Math.log10((ymax - ymin) / 4 || 1)));
  const ticks = [];
  for (let v = Math.ceil(ymin / step) * step; v <= ymax + 1e-9; v += step * (((ymax - ymin) / step > 8) ? 2 : 1)) ticks.push(v);
  const grid = ticks.map(v => `<line x1="${pad.l}" x2="${width - pad.r}" y1="${y(v)}" y2="${y(v)}" stroke="${Math.abs(v) < 1e-9 ? '#8c959f' : '#eaeef2'}"/>
    <text x="${pad.l - 6}" y="${y(v) + 4}" text-anchor="end">${+v.toFixed(2)}</text>`).join('');
  return {y, grid};
}

function meansChart() {
  const series = visible(), sums = series.map(s => DATA.summaries[s.key][state.pool]);
  const lo = Math.min(0, ...sums.map(x => x.ci ? x.ci[0] : x.mean)), hi = Math.max(0, ...sums.map(x => x.ci ? x.ci[1] : x.mean));
  const width = 560, height = 300, pad = {l: 44, r: 12, t: 12, b: 96}, f = frame(width, height, pad, lo, hi);
  const band = (width - pad.l - pad.r) / Math.max(series.length, 1);
  const bars = series.map((s, i) => {
    const sum = sums[i], x = pad.l + band * i + band * 0.2, w = band * 0.6, top = f.y(Math.max(0, sum.mean)), base = f.y(Math.min(0, sum.mean));
    const ci = sum.ci ? `<line x1="${x + w / 2}" x2="${x + w / 2}" y1="${f.y(sum.ci[0])}" y2="${f.y(sum.ci[1])}" stroke="#24292f"/>
      <line x1="${x + w / 3}" x2="${x + w * 2 / 3}" y1="${f.y(sum.ci[0])}" y2="${f.y(sum.ci[0])}" stroke="#24292f"/>
      <line x1="${x + w / 3}" x2="${x + w * 2 / 3}" y1="${f.y(sum.ci[1])}" y2="${f.y(sum.ci[1])}" stroke="#24292f"/>` : '';
    return `<g><title>${esc(s.label)}：${fmt(sum.mean)}${sum.ci ? ` [${fmt(sum.ci[0])}, ${fmt(sum.ci[1])}]` : ''}</title>
      <rect x="${x}" y="${top}" width="${w}" height="${Math.max(base - top, 1)}" fill="${s.color}" opacity="0.85"/>${ci}
      <text transform="translate(${x + w / 2},${height - pad.b + 12}) rotate(-30)" text-anchor="end">${esc(s.label.replace(/^对照：/, ''))}</text></g>`;
  }).join('');
  document.getElementById('means').innerHTML = `<svg viewBox="0 0 ${width} ${height}" width="100%">${f.grid}${bars}</svg>`;
}

function lineChart(target, points, labels, width, height) {
  const series = visible(), all = series.flatMap(s => points[s.key].filter(v => v != null));
  const lo = Math.min(0, ...all), hi = Math.max(0, ...all), pad = {l: 48, r: 16, t: 12, b: 28};
  const f = frame(width, height, pad, lo, hi), n = labels.length, x = i => pad.l + (n > 1 ? i / (n - 1) : 0.5) * (width - pad.l - pad.r);
  const every = Math.ceil(n / (width / 70));
  const axis = labels.map((l, i) => i % every ? '' : `<text x="${x(i)}" y="${height - 8}" text-anchor="middle">${esc(l)}</text>`).join('');
  const lines = series.map(s => {
    const pts = points[s.key].map((v, i) => v == null ? null : `${x(i)},${f.y(v)}`).filter(Boolean).join(' ');
    const dots = points[s.key].map((v, i) => v == null ? '' : `<circle cx="${x(i)}" cy="${f.y(v)}" r="2.5" fill="${s.color}">
      <title>${esc(s.label)} ${esc(labels[i])}：${fmt(v)}</title></circle>`).join('');
    return `<polyline points="${pts}" fill="none" stroke="${s.color}" stroke-width="1.8" ${s.dash ? `stroke-dasharray="${s.dash}"` : ''}/>${dots}`;
  }).join('');
  document.getElementById(target).innerHTML = `<svg viewBox="0 0 ${width} ${height}" width="100%">${f.grid}${axis}${lines}</svg>`;
}

function yearly() {
  const data = rows(), years = [...new Set(data.map(p => p.decision_date.slice(0, 4)))].sort(), points = {};
  SERIES.forEach(s => points[s.key] = years.map(y => mean(data.filter(p => p.decision_date.startsWith(y)).map(p => value(p, s)))));
  lineChart('yearly', points, years, 560, 260);
}

function cumulative() {
  const data = rows(), points = {};
  SERIES.forEach(s => { let total = 0; points[s.key] = data.map(p => total += value(p, s)); });
  lineChart('cumulative', points, data.map(p => p.decision_date), 1100, 300);
}

function periodTable() {
  const series = visible(), data = rows().slice().reverse();
  const head = `<thead><tr><th>决策日</th><th class="l">股票池</th><th>建仓日</th>${series.map(s => `<th>${esc(s.label)}</th>`).join('')}</tr></thead>`;
  const body = data.map(p => `<tr><td>${p.decision_date}</td>
    <td class="l">${esc((DATA.pools.find(x => x.key === p.pool) || {label: p.pool}).label)}</td><td>${p.entry_date}</td>
    ${series.map(s => { const v = value(p, s), b = s.kind === 'strategy' ? p.blocked[s.key] : 0;
      return `<td class="${tone(v)}">${fmt(v)}${b ? ` <span class="meta">(${b})</span>` : ''}</td>`; }).join('')}</tr>`).join('');
  document.getElementById('periods').innerHTML = head + `<tbody>${body}</tbody>`;
}

function draw() { header(); summaryTable(); meansChart(); yearly(); cumulative(); periodTable(); }
draw();
</script>
</body>
</html>
"""
