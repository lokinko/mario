const modularSteps = ["准备资料", "研究", "分模块讨论", "策略汇总", "完成"];
const fastSteps = ["准备资料", "场景路由", "直接作答", "完成"];

export function GenerationSteps({ stage }: { stage: string }) {
  const isFast = /精简|直接作答/.test(stage);
  const steps = isFast ? fastSteps : modularSteps;
  const current = isFast
    ? /完成/.test(stage)
      ? 3
      : /作答/.test(stage)
        ? 2
        : /识别|路由|精简|检索|搜索|查找/.test(stage)
          ? 1
          : /准备|读取|资料|整理/.test(stage)
            ? 0
            : -1
    : /完成/.test(stage)
      ? 4
      : /汇总|策略总监|裁决/.test(stage)
        ? 3
        : /模块/.test(stage)
          ? 2
          : /研究|检索|搜索|查找|识别|路由/.test(stage)
            ? 1
            : /准备|读取|资料|整理/.test(stage)
              ? 0
              : -1;
  return (
    <div role="status">
      <ol className="generation-steps" aria-label="生成进度">
        {steps.map((label, i) => (
          <li key={label} aria-current={current === i ? "step" : undefined}>
            {i + 1} · {label}
          </li>
        ))}
      </ol>
      <small>{stage}</small>
    </div>
  );
}
