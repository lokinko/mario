"""把 `bench-run` 输出目录中各策略的成绩汇总成可离线打开的单文件 HTML 看板。

看板只读取成绩与策略说明，不读取输入或结果标签；图表由本模块统一生成，新增策略不需要改看板代码。
"""

import json
import math
from datetime import datetime

from .files import read_json, require
from .public_sources import SHANGHAI
from .scoring import series_stats
from .strategy_runner import completed_runs, failed_runs

POOL_LABELS = {"dividend": "红利池", "tech": "科技池"}
BENCHMARKS = {"pool_equal_weight": "对照：股票池等权", "csi300": "对照：沪深300"}
SPLIT_YEAR = 2018


def summary(stats, prefix=""):
    """总分的均值、中位数与区间，以及平均收益得分与胜率。"""
    value = stats[f"{prefix}score"]
    return {"mean": value["mean"], "median": value["median"], "ci": value["bootstrap_95"], "count": value["count"],
            "return_mean": stats[f"{prefix}return_score"]["mean"], "win_rate": stats[f"{prefix}win_rate"]}


def segment(decision_date):
    return "early" if int(decision_date[:4]) < SPLIT_YEAR else "late"


def segments(periods):
    """按参数选定段（SPLIT_YEAR 之前）与检验段切分；只列出有决策点的时间段。"""
    years = sorted({int(p["decision_date"][:4]) for p in periods})
    spans = {"early": [y for y in years if y < SPLIT_YEAR], "late": [y for y in years if y >= SPLIT_YEAR]}
    return [{"key": "all", "label": "全部年份"}] + [
        {"key": key, "label": f"{span[0]}–{span[-1]}" if span[0] != span[-1] else str(span[0])}
        for key, span in spans.items() if span]


def grouped_summaries(periods, segment_keys, pools, prefixes):
    """{前缀: {时间段: {股票池: 汇总}}}；统计口径与打分报告相同，由 scoring.series_stats 计算。"""
    output = {prefix: {} for prefix in prefixes}
    for key in segment_keys:
        for pool in ["all", *pools]:
            subset = [p for p in periods if key in ("all", segment(p["decision_date"])) and pool in ("all", p["pool"])]
            if not subset:
                continue
            stats = series_stats(subset, prefixes)
            for prefix in prefixes:
                output[prefix].setdefault(key, {})[pool] = summary(stats, prefix)
    return output


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
        "id": period["sample_id"], "pool": period["pool"], "decision_date": period["decision_date"],
        "segment": segment(period["decision_date"]), "entry_date": period["entry_date"], "scores": {},
        "segment_returns": {}, "holdings": {}, "blocked": {}, "exits": {}, "grid_trades": {},
        "benchmarks": {"pool_equal_weight": period["pool_equal_weight_return_score"],
                       "csi300": period["csi300_return_score"]}}
        for period in first["periods"]}
    summaries = {}
    for name in names:
        report, held = scorecards[name], positions(runs[name]["weights"])
        require([p["sample_id"] for p in report["periods"]] == ids, f"{name} 与其他策略的决策点不一致，可能来自不同发布包")
        for period in report["periods"]:
            row = periods[period["sample_id"]]
            for key, value in (("pool_equal_weight", period["pool_equal_weight_return_score"]),
                               ("csi300", period["csi300_return_score"])):
                require(math.isclose(row["benchmarks"][key], value, abs_tol=1e-9), f"{name} 的对照得分不一致")
            row["scores"][name] = period["return_score"]
            row["segment_returns"][name] = period["segment_returns"]
            row["holdings"][name] = held.get(period["sample_id"], [])
            row["blocked"][name] = period["blocked_entries"]
            row["exits"][name] = period["exits"]
            row["grid_trades"][name] = sum(t["buys"] + t["sells"] for t in period["grid_trades"])
        summaries[name] = grouped_summaries(report["periods"], [s["key"] for s in spans], pools, [""])[""]
    benchmarks = grouped_summaries(first["periods"], [s["key"] for s in spans], pools, [f"{k}_" for k in BENCHMARKS])
    summaries.update({key: benchmarks[f"{key}_"] for key in BENCHMARKS})
    return {
        "strategies": [{"key": name, "label": runs[name]["title"], "description": runs[name]["description"]}
                       for name in names],
        "failures": [{"key": name, "error": error} for name, error in sorted((failures or {}).items())],
        "benchmarks": [{"key": key, "label": label} for key, label in BENCHMARKS.items()],
        "pools": [{"key": pool, "label": POOL_LABELS.get(pool, pool)} for pool in pools], "segments": spans,
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
:root { --fg:#1f2328; --muted:#656d76; --line:#d0d7de; --bg:#f6f8fa; --up:#cf222e; --down:#1a7f37; --pick:#fff8c5; }
* { box-sizing:border-box; }
body { margin:0; font:14px/1.5 -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif; color:var(--fg); background:#fff; }
header { padding:20px 28px 12px; border-bottom:1px solid var(--line); }
h1 { font-size:20px; margin:0 0 4px; }
h2 { font-size:15px; margin:0 0 10px; }
.meta { color:var(--muted); font-size:12px; }
main { padding:16px 28px 40px; display:grid; gap:20px; }
.bar { display:flex; gap:8px; flex-wrap:wrap; align-items:center; }
.bar + .bar { margin-top:-12px; }
button { border:1px solid var(--line); background:#fff; border-radius:6px; padding:4px 12px; cursor:pointer; font:inherit; }
button.on { background:var(--fg); color:#fff; border-color:var(--fg); }
.card { border:1px solid var(--line); border-radius:8px; padding:16px; }
.grid { display:grid; grid-template-columns:repeat(auto-fit, minmax(420px, 1fr)); gap:20px; }
table { border-collapse:collapse; width:100%; font-variant-numeric:tabular-nums; }
th, td { padding:5px 8px; border-bottom:1px solid var(--line); text-align:right; white-space:nowrap; }
th:first-child, td:first-child, th.l, td.l { text-align:left; }
thead th { background:var(--bg); position:sticky; top:0; }
th[data-sort] { cursor:pointer; user-select:none; } th[data-sort]:hover { background:#eaeef2; }
#periods tbody tr { cursor:pointer; } #periods tbody tr:hover { background:var(--bg); }
#periods tbody tr.picked { background:var(--pick); }
.scroll { max-height:520px; overflow:auto; }
.up { color:var(--up); } .down { color:var(--down); }
.swatch { display:inline-block; width:10px; height:10px; border-radius:2px; margin-right:6px; vertical-align:-1px; }
.legend label { margin-right:14px; cursor:pointer; user-select:none; }
svg text { font-size:11px; fill:var(--muted); }
svg.pick { cursor:pointer; }
.note { color:var(--muted); font-size:12px; margin:8px 0 0; }
.failed { border-color:var(--up); } .failed li { white-space:pre-wrap; font-family:ui-monospace, monospace; font-size:12px; }
.desc h3 { font-size:14px; margin:12px 0 4px; } .desc p { margin:0; white-space:pre-wrap; color:var(--muted); }
.desc code { font-size:12px; }
.tip { position:fixed; pointer-events:none; z-index:10; background:#fff; border:1px solid var(--line); border-radius:6px;
  box-shadow:0 4px 12px rgba(31,35,40,.15); padding:6px 10px; font-size:12px; max-width:360px; }
.tip b { display:block; margin-bottom:2px; } .tip div { display:flex; justify-content:space-between; gap:16px; }
.holdings { display:grid; grid-template-columns:repeat(auto-fill, minmax(400px, 1fr)); gap:12px; }
.holdings > div { border:1px solid var(--line); border-radius:6px; padding:10px; min-width:0; }
.holdings h3 { font-size:13px; margin:0 0 4px; } .holdings table { font-size:12px; }
.holdings td.l { white-space:normal; }
</style>
</head>
<body>
<header>
  <h1>mario_bench 策略得分看板</h1>
  <div class="meta" id="meta"></div>
</header>
<main>
  <div class="bar"><span>股票池：</span><span id="pools"></span></div>
  <div class="bar"><span>时间段：</span><span id="segments"></span></div>
  <div class="legend" id="legend"></div>
  <section class="card failed" id="failures" hidden><h2>运行失败的策略（未计入看板）</h2><ul></ul></section>
  <section class="card"><h2>策略汇总</h2><table id="summary"></table>
    <p class="note">点击表头按该列排序，再次点击反向；时间段按决策日年份切分。</p>
    <p class="note" id="metric"></p></section>
  <div class="grid">
    <section class="card"><h2>总分与 95% 区间</h2><div id="means"></div></section>
    <section class="card"><h2>逐年平均收益得分</h2><div id="yearly"></div></section>
  </div>
  <section class="card"><h2>累计收益得分（逐期收益得分求和，不是净值）</h2><div id="cumulative"></div>
    <p class="note">悬停查看该期各策略得分，点击某一期查看当期持仓。</p></section>
  <section class="card"><h2>逐期收益得分</h2><div class="scroll"><table id="periods"></table></div>
    <p class="note">数值为当期收益得分，大于 0 计为胜；红色为正、绿色为负（A 股习惯）。买不进的股票（停牌或一字涨停）按规则留作现金，括号内为该期买不进的只数；“卖 n”为该期按卖出规则提前卖出的只数。点击一行查看当期持仓。</p></section>
  <section class="card" id="detail" hidden><h2 id="detail-title"></h2><div id="detail-body"></div></section>
  <section class="card"><h2>策略说明</h2><div id="descriptions"></div></section>
  <section class="card"><h2>口径与限制</h2><ul id="limits" class="note"></ul></section>
</main>
<div class="tip" id="tip" hidden></div>
<script type="application/json" id="data">__DATA__</script>
<script>
const DATA = JSON.parse(document.getElementById('data').textContent);
const COLORS = ['#0969da', '#bf3989', '#9a6700', '#57606a', '#8250df', '#1b7c83', '#bc4c00', '#2da44e',
  '#6639ba', '#0550ae', '#953800', '#4d2d00'];
const SERIES = [...DATA.strategies.map((s, i) => ({...s, kind: 'strategy', color: COLORS[i % COLORS.length]})),
  {...DATA.benchmarks[0], kind: 'benchmark', color: '#d4a72c', dash: '5 3'},
  {...DATA.benchmarks[1], kind: 'benchmark', color: '#cf222e', dash: '2 3'}];
const RULES = {stop_loss: '止损', trailing_stop: '移动止损', take_profit: '止盈', max_hold_days: '最长持有'};
const state = {pool: 'all', segment: 'all', hidden: new Set(), sort: null, picked: null};
const fmt = v => v == null ? '—' : v.toFixed(2);
const pct = v => `${+(Number(v) * 100).toFixed(2)}%`;
const tone = v => v > 0 ? 'up' : v < 0 ? 'down' : '';
const esc = s => String(s).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
const value = (row, s) => s.kind === 'strategy' ? row.scores[s.key] : row.benchmarks[s.key];
const visible = () => SERIES.filter(s => !state.hidden.has(s.key));
const rows = () => DATA.periods.filter(p => (state.pool === 'all' || p.pool === state.pool) &&
  (state.segment === 'all' || p.segment === state.segment));
const stats = s => (DATA.summaries[s.key][state.segment] || {})[state.pool];
const poolLabel = key => (DATA.pools.find(p => p.key === key) || {label: key}).label;
const mean = a => a.length ? a.reduce((x, y) => x + y, 0) / a.length : null;
const swatch = s => `<span class="swatch" style="background:${s.color}"></span>`;

const tip = document.getElementById('tip');
function showTip(event, html) {
  tip.innerHTML = html;
  tip.hidden = false;
  const box = tip.getBoundingClientRect(), x = event.clientX + 14, y = event.clientY + 14;
  tip.style.left = `${Math.min(x, innerWidth - box.width - 8)}px`;
  tip.style.top = `${y + box.height > innerHeight ? event.clientY - box.height - 10 : y}px`;
}
const hideTip = () => { tip.hidden = true; };

function buttons(target, choices, field) {
  document.getElementById(target).innerHTML = choices.map(c =>
    `<button data-${field}="${c.key}" class="${c.key === state[field] ? 'on' : ''}">${esc(c.label)}</button>`).join(' ');
  document.querySelectorAll(`[data-${field}]`).forEach(b => b.onclick = () => { state[field] = b.dataset[field]; draw(); });
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
  document.getElementById('descriptions').innerHTML = SERIES.filter(s => s.kind === 'strategy').map(s =>
    `<div class="desc"><h3>${swatch(s)}${esc(s.label)} <code>${esc(s.key)}</code></h3>
    <p>${esc(s.description)}</p></div>`).join('');
  buttons('pools', [{key: 'all', label: '全部'}, ...DATA.pools], 'pool');
  buttons('segments', DATA.segments, 'segment');
  document.getElementById('legend').innerHTML = SERIES.map(s => `<label><input type="checkbox" data-series="${s.key}"
    ${state.hidden.has(s.key) ? '' : 'checked'}>${swatch(s)}${esc(s.label)}</label>`).join('');
  document.querySelectorAll('[data-series]').forEach(b => b.onchange = () => {
    b.checked ? state.hidden.delete(b.dataset.series) : state.hidden.add(b.dataset.series); draw(); });
}

function summaryTable() {
  const data = rows(), strategy = s => s.kind === 'strategy';
  const count = (s, field) => data.reduce((n, p) => n + (Array.isArray(p[field][s.key]) ? p[field][s.key].length : p[field][s.key]), 0);
  const beat = (s, k) => data.filter(p => p.scores[s.key] > p.benchmarks[k]).length;
  const columns = [
    {label: '策略', key: s => s.label, cell: s => `${swatch(s)}${esc(s.label)}`},
    {label: '总分', key: (s, m) => m.mean, cell: (s, m) => `<span class="${tone(m.mean)}">${fmt(m.mean)}</span>`},
    {label: '总分中位数', key: (s, m) => m.median, cell: (s, m) => fmt(m.median)},
    {label: '总分 95% 区间', key: (s, m) => m.ci && m.ci[0], cell: (s, m) => m.ci ? `[${fmt(m.ci[0])}, ${fmt(m.ci[1])}]` : '—'},
    {label: '平均收益得分', key: (s, m) => m.return_mean, cell: (s, m) => `<span class="${tone(m.return_mean)}">${fmt(m.return_mean)}</span>`},
    {label: '胜率（正得分期数）', key: (s, m) => m.win_rate,
     cell: (s, m) => `${(m.win_rate * 100).toFixed(0)}%（${data.filter(p => value(p, s) > 0).length}/${data.length}）`},
    {label: '胜沪深300', key: s => strategy(s) ? beat(s, 'csi300') : null,
     cell: s => strategy(s) ? `${beat(s, 'csi300')}/${data.length}` : '—'},
    {label: '胜股票池等权', key: s => strategy(s) ? beat(s, 'pool_equal_weight') : null,
     cell: s => strategy(s) ? `${beat(s, 'pool_equal_weight')}/${data.length}` : '—'},
    ...[['买不进只次', 'blocked'], ['卖出只次', 'exits'], ['网格成交笔数', 'grid_trades']].map(([label, field]) =>
      ({label, key: s => strategy(s) ? count(s, field) : null, cell: s => strategy(s) ? count(s, field) : '—'}))];
  let list = SERIES.map(s => ({s, m: stats(s)})).filter(x => x.m);
  if (state.sort) {
    const {col, dir} = state.sort, key = x => columns[col].key(x.s, x.m);
    list = list.slice().sort((a, b) => {
      const x = key(a), y = key(b);
      if (x == null || y == null) return (x == null) - (y == null);
      return (typeof x === 'string' ? x.localeCompare(y, 'zh') : x - y) * dir;
    });
  }
  const arrow = i => state.sort && state.sort.col === i ? (state.sort.dir > 0 ? ' ▲' : ' ▼') : '';
  const table = document.getElementById('summary');
  table.innerHTML = `<thead><tr>${columns.map((c, i) => `<th data-sort="${i}">${c.label}${arrow(i)}</th>`).join('')}</tr></thead>
    <tbody>${list.map(({s, m}) => `<tr>${columns.map(c => `<td>${c.cell(s, m)}</td>`).join('')}</tr>`).join('')}</tbody>`;
  table.querySelectorAll('[data-sort]').forEach(th => th.onclick = () => {
    const col = +th.dataset.sort, same = state.sort && state.sort.col === col;
    state.sort = {col, dir: same ? -state.sort.dir : col === 0 ? 1 : -1};
    summaryTable();
  });
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
  const series = visible().filter(stats), sums = series.map(stats);
  const lo = Math.min(0, ...sums.map(x => x.ci ? x.ci[0] : x.mean)), hi = Math.max(0, ...sums.map(x => x.ci ? x.ci[1] : x.mean));
  const width = 560, height = 300, pad = {l: 44, r: 12, t: 12, b: 96}, f = frame(width, height, pad, lo, hi);
  const band = (width - pad.l - pad.r) / Math.max(series.length, 1);
  const bars = series.map((s, i) => {
    const sum = sums[i], x = pad.l + band * i + band * 0.2, w = band * 0.6, top = f.y(Math.max(0, sum.mean)), base = f.y(Math.min(0, sum.mean));
    const ci = sum.ci ? `<line x1="${x + w / 2}" x2="${x + w / 2}" y1="${f.y(sum.ci[0])}" y2="${f.y(sum.ci[1])}" stroke="#24292f"/>
      <line x1="${x + w / 3}" x2="${x + w * 2 / 3}" y1="${f.y(sum.ci[0])}" y2="${f.y(sum.ci[0])}" stroke="#24292f"/>
      <line x1="${x + w / 3}" x2="${x + w * 2 / 3}" y1="${f.y(sum.ci[1])}" y2="${f.y(sum.ci[1])}" stroke="#24292f"/>` : '';
    return `<g data-bar="${i}"><rect x="${pad.l + band * i}" y="${pad.t}" width="${band}" height="${height - pad.t - pad.b}" fill="transparent"/>
      <rect x="${x}" y="${top}" width="${w}" height="${Math.max(base - top, 1)}" fill="${s.color}" opacity="0.85"/>${ci}
      <text transform="translate(${x + w / 2},${height - pad.b + 12}) rotate(-30)" text-anchor="end">${esc(s.label.replace(/^对照：/, ''))}</text></g>`;
  }).join('');
  const box = document.getElementById('means');
  box.innerHTML = `<svg viewBox="0 0 ${width} ${height}" width="100%">${f.grid}${bars}</svg>`;
  box.querySelectorAll('[data-bar]').forEach(g => {
    const s = series[+g.dataset.bar], sum = sums[+g.dataset.bar];
    g.onmousemove = e => showTip(e, `<b>${swatch(s)}${esc(s.label)}</b><div><span>总分</span><span>${fmt(sum.mean)}</span></div>
      <div><span>95% 区间</span><span>${sum.ci ? `[${fmt(sum.ci[0])}, ${fmt(sum.ci[1])}]` : '—'}</span></div>
      <div><span>平均收益得分</span><span>${fmt(sum.return_mean)}</span></div>
      <div><span>胜率</span><span>${(sum.win_rate * 100).toFixed(0)}%（${sum.count} 期）</span></div>`);
    g.onmouseleave = hideTip;
  });
}

function lineChart(target, points, labels, titles, width, height, pick) {
  const series = visible(), all = series.flatMap(s => points[s.key].filter(v => v != null));
  const lo = Math.min(0, ...all), hi = Math.max(0, ...all), pad = {l: 48, r: 16, t: 12, b: 28};
  const f = frame(width, height, pad, lo, hi), n = labels.length, x = i => pad.l + (n > 1 ? i / (n - 1) : 0.5) * (width - pad.l - pad.r);
  const every = Math.ceil(n / (width / 70));
  const axis = labels.map((l, i) => i % every ? '' : `<text x="${x(i)}" y="${height - 8}" text-anchor="middle">${esc(l)}</text>`).join('');
  const lines = series.map(s => {
    const pts = points[s.key].map((v, i) => v == null ? null : `${x(i)},${f.y(v)}`).filter(Boolean).join(' ');
    const dots = n > 40 ? '' : points[s.key].map((v, i) => v == null ? '' : `<circle cx="${x(i)}" cy="${f.y(v)}" r="2.5" fill="${s.color}"/>`).join('');
    return `<polyline points="${pts}" fill="none" stroke="${s.color}" stroke-width="1.8" ${s.dash ? `stroke-dasharray="${s.dash}"` : ''}/>${dots}`;
  }).join('');
  const box = document.getElementById(target);
  box.innerHTML = `<svg viewBox="0 0 ${width} ${height}" width="100%" class="${pick ? 'pick' : ''}">${f.grid}${axis}${lines}
    <line class="cross" y1="${pad.t}" y2="${height - pad.b}" stroke="#8c959f" stroke-dasharray="3 3" visibility="hidden"/></svg>`;
  const svg = box.querySelector('svg'), cross = svg.querySelector('.cross');
  const index = e => {
    const rect = svg.getBoundingClientRect(), sx = (e.clientX - rect.left) * width / rect.width;
    return Math.max(0, Math.min(n - 1, Math.round((sx - pad.l) / (width - pad.l - pad.r) * (n - 1))));
  };
  svg.onmousemove = e => {
    if (!n) return;
    const i = index(e), ranked = series.map(s => [s, points[s.key][i]]).filter(([, v]) => v != null).sort((a, b) => b[1] - a[1]);
    cross.setAttribute('x1', x(i)); cross.setAttribute('x2', x(i)); cross.setAttribute('visibility', 'visible');
    showTip(e, `<b>${esc(titles[i])}</b>${ranked.map(([s, v]) =>
      `<div><span>${swatch(s)}${esc(s.label)}</span><span class="${tone(v)}">${fmt(v)}</span></div>`).join('')}`);
  };
  svg.onmouseleave = () => { cross.setAttribute('visibility', 'hidden'); hideTip(); };
  if (pick) svg.onclick = e => n && pick(index(e));
}

function yearly() {
  const data = rows(), years = [...new Set(data.map(p => p.decision_date.slice(0, 4)))].sort(), points = {};
  SERIES.forEach(s => points[s.key] = years.map(y => mean(data.filter(p => p.decision_date.startsWith(y)).map(p => value(p, s)))));
  lineChart('yearly', points, years, years.map(y => `${y} 年平均收益得分`), 560, 260);
}

function cumulative() {
  const data = rows(), points = {};
  SERIES.forEach(s => { let total = 0; points[s.key] = data.map(p => total += value(p, s)); });
  lineChart('cumulative', points, data.map(p => p.decision_date),
    data.map(p => `${p.decision_date} ${poolLabel(p.pool)}（累计）`), 1100, 300, i => pickPeriod(data[i].id, true));
}

function periodTable() {
  const series = visible(), data = rows().slice().reverse();
  const head = `<thead><tr><th>决策日</th><th class="l">股票池</th><th>建仓日</th>${series.map(s => `<th>${esc(s.label)}</th>`).join('')}</tr></thead>`;
  const body = data.map(p => `<tr data-period="${esc(p.id)}" class="${p.id === state.picked ? 'picked' : ''}"><td>${p.decision_date}</td>
    <td class="l">${esc(poolLabel(p.pool))}</td><td>${p.entry_date}</td>
    ${series.map(s => { const v = value(p, s), b = s.kind === 'strategy' ? p.blocked[s.key].length : 0;
      const x = s.kind === 'strategy' ? p.exits[s.key].length : 0;
      return `<td class="${tone(v)}">${fmt(v)}${b ? ` <span class="meta">(${b})</span>` : ''}${x ? ` <span class="meta">卖${x}</span>` : ''}</td>`; }).join('')}</tr>`).join('');
  const table = document.getElementById('periods');
  table.innerHTML = head + `<tbody>${body}</tbody>`;
  table.querySelectorAll('[data-period]').forEach(tr => tr.onclick = () => pickPeriod(tr.dataset.period, false));
}

function ruleText(rule) {
  if (!rule) return '持有到期';
  if (rule.step) return `网格 线距 ${pct(rule.step)}，${rule.lots} 份，建仓 ${rule.initial} 份`;
  return Object.entries(rule).map(([k, v]) => k === 'max_hold_days' ? `最长持有 ${v} 天` : `${RULES[k] || k} ${pct(v)}`).join('，');
}

function detail() {
  const box = document.getElementById('detail'), p = DATA.periods.find(x => x.id === state.picked);
  box.hidden = !p;
  if (!p) return;
  const benchmarks = SERIES.filter(s => s.kind === 'benchmark').map(s => `${esc(s.label.replace(/^对照：/, ''))} ${fmt(value(p, s))}`).join('，');
  document.getElementById('detail-title').textContent = `当期明细：${p.decision_date} ${poolLabel(p.pool)}，建仓日 ${p.entry_date}`;
  const cards = visible().filter(s => s.kind === 'strategy').map(s => {
    const v = p.scores[s.key], seg = p.segment_returns[s.key], held = p.holdings[s.key];
    const exits = Object.fromEntries(p.exits[s.key].map(e => [e.symbol, e])), blocked = new Set(p.blocked[s.key]);
    const outcome = code => { const e = exits[code];
      if (blocked.has(code)) return '买不进，留作现金';
      return e ? `${RULES[e.reason] || e.reason} ${e.signal_date} 触发，${e.sell_date ? `${e.sell_date} 卖出` : '窗口内未能卖出'}` : ''; };
    const body = held.length ? `<table><thead><tr><th>证券</th><th>仓位</th><th class="l">卖出规则</th><th class="l">执行</th></tr></thead>
      <tbody>${held.map(([code, weight, rule]) => `<tr><td>${esc(code)}</td><td>${pct(weight)}</td>
        <td class="l">${esc(ruleText(rule))}</td><td class="l">${esc(outcome(code))}</td></tr>`).join('')}</tbody></table>`
      : '<p class="note">全部持有现金</p>';
    return `<div><h3>${swatch(s)}${esc(s.label)}：<span class="${tone(v)}">${fmt(v)}</span></h3>
      <p class="note">0–7 天 ${pct(seg[0])} · 7–30 天 ${pct(seg[1])} · 30–90 天 ${pct(seg[2])}</p>${body}</div>`;
  }).join('');
  document.getElementById('detail-body').innerHTML = `<p class="note">对照收益得分：${benchmarks}。只列出当前勾选的策略。</p>
    <div class="holdings">${cards}</div>`;
}

function pickPeriod(id, scroll) {
  state.picked = state.picked === id && !scroll ? null : id;
  periodTable(); detail();
  if (scroll && state.picked) document.getElementById('detail').scrollIntoView({behavior: 'smooth', block: 'start'});
}

function draw() {
  if (state.picked && !rows().some(p => p.id === state.picked)) state.picked = null;
  header(); summaryTable(); meansChart(); yearly(); cumulative(); periodTable(); detail();
}
draw();
</script>
</body>
</html>
"""
