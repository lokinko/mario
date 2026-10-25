const steps = ["准备资料", "检索", "分析", "校验", "完成"];
export function GenerationSteps({ stage }: { stage: string }) {
  const current = /完成/.test(stage)
    ? 4
    : /校验|验证|修复|审查|检查/.test(stage)
      ? 3
      : /分析|方案|研究计划|反思|报告|候选/.test(stage)
        ? 2
        : /检索|搜索|查找|筛选/.test(stage)
          ? 1
          : /准备|读取|资料/.test(stage)
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
