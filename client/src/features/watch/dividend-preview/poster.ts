import type { DividendCard } from "./card";

const ink = "#172c47";
const muted = "#506581";
const left = 60;
const right = 900;
const blocks = {
  outlook: { fill: "#f5ecd9", edge: "rgba(146, 101, 26, 0.16)" },
  drivers: { fill: "#e6ecf0", edge: "rgba(61, 95, 120, 0.16)" },
  performance: { fill: "#e5eee6", edge: "rgba(59, 107, 77, 0.16)" },
};

export function wrapPosterText(value: string, fits: (line: string) => boolean) {
  const tokens = value.match(/[A-Za-z0-9.+-]+|./gu) ?? [];
  const lines: string[] = [];
  let line = "";
  for (const token of tokens) {
    if (line && !fits(line + token) && !/^[，。；、？！）]$/.test(token)) {
      lines.push(line.trimEnd());
      line = token.trimStart();
    } else {
      line += token;
    }
  }
  if (line) lines.push(line.trimEnd());
  return lines;
}

export function createDividendPoster(
  card: DividendCard,
  qr?: HTMLCanvasElement,
): Promise<Blob> {
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return Promise.reject(new Error("Canvas unavailable"));
  const font =
    getComputedStyle(document.documentElement)
      .getPropertyValue("--font-sans")
      .trim() || "system-ui, sans-serif";
  const { name, symbol, sections } = card;
  const headingSize = 28;
  const bodySize = 26;
  const lineHeight = 50;
  const blockGap = 24;
  const blockPadding = 32;
  const blockLeft = left - 20;
  const blockWidth = right - left + 40;
  const headerTop = 36;
  const qrSize = 128;
  const qrLeft = right - qrSize;
  const qrTop = headerTop + 20;
  const qrCaptionBaseline = qrTop + qrSize + 26;
  const headerBottom = qrCaptionBaseline + 18;

  let nextTop = headerBottom + blockGap;
  ctx.font = `400 ${bodySize}px ${font}`;
  const sectionLayout = sections.map((section) => {
    const lines = wrapPosterText(
      section.text,
      (line) => ctx.measureText(line).width <= right - left - bodySize,
    );
    const top = nextTop;
    const heading = top + blockPadding + headingSize;
    const body = heading + lineHeight;
    let last = body + (lines.length - 1) * lineHeight;
    let metrics;
    if (section.kind === "performance") {
      const label = last + 60;
      const value = label + 40;
      const note = value + 30;
      metrics = { items: section.metrics, label, value, note };
      last = section.metrics.some(({ date }) => date) ? note + 29 : note;
    }
    const bottom = last + blockPadding - 4;
    nextTop = bottom + blockGap;
    return { ...section, lines, top, bottom, heading, body, metrics };
  });
  canvas.width = 960;
  canvas.height = nextTop - blockGap + headerTop;

  ctx.textBaseline = "alphabetic";
  const text = (
    value: string,
    x: number,
    y: number,
    size: number,
    color = ink,
    weight = 400,
  ) => {
    ctx.fillStyle = color;
    ctx.font = `${weight} ${size}px ${font}`;
    ctx.fillText(value, x, y);
  };

  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#f6e7de";
  ctx.beginPath();
  ctx.roundRect(
    left - 20,
    headerTop,
    right - left + 40,
    headerBottom - headerTop,
    20,
  );
  ctx.fill();
  const headerText = "#743f2f";
  text("mario  红利观察", left, headerTop + 58, 24, headerText, 500);
  text(name, left, headerTop + 118, 36, ink, 600);
  text(`A 股 ${symbol}`, left, headerTop + 162, 24, headerText);
  if (qr) {
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(qr, qrLeft, qrTop, qrSize, qrSize);
    ctx.textAlign = "center";
    text(
      "扫码查看详情",
      qrLeft + qrSize / 2,
      qrCaptionBaseline,
      18,
      headerText,
    );
    ctx.textAlign = "left";
  }

  for (const section of sectionLayout) {
    const { fill, edge } = blocks[section.kind];
    ctx.beginPath();
    ctx.roundRect(
      blockLeft,
      section.top,
      blockWidth,
      section.bottom - section.top,
      20,
    );
    ctx.fillStyle = fill;
    ctx.fill();
    ctx.strokeStyle = edge;
    ctx.lineWidth = 2;
    ctx.stroke();
    text(section.title, left, section.heading, headingSize, ink, 600);
    section.lines.forEach((line, index) =>
      text(line, left, section.body + lineHeight * index, bodySize, ink),
    );
    if (!section.metrics) continue;
    const { items, label, value, note } = section.metrics;
    const metricWidth = (right - left) / items.length;
    for (const [index, metric] of items.entries()) {
      const x = left + index * metricWidth;
      text(metric.label, x, label, 20, muted);
      text(metric.value, x, value, 30, ink, 600);
      text(metric.note, x, note, 20, muted);
      if (metric.date) {
        text(
          `${metric.date.label} ${metric.date.value}`,
          x,
          note + 29,
          20,
          muted,
        );
      }
    }
  }

  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) resolve(blob);
      else reject(new Error("PNG export failed"));
    }, "image/png");
  });
}
