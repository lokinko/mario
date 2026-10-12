export function allocationGradient(allocation: { pct: number }[]) {
  const colors = ["#cf5c3b", "#23443b", "#c99a45", "#80948f", "#774936"];
  let cursor = 0;
  const stops = allocation.map((item, index) => {
    const start = cursor;
    cursor += item.pct;
    return `${colors[index % colors.length]} ${start}% ${cursor}%`;
  });
  return `conic-gradient(${stops.join(",")})`;
}
