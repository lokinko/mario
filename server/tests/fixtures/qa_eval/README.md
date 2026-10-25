# 理财问答基线评测

对应仓库根目录 [精简方案](../../../../finance_qa.md) 行动 0。只通过显式忽略测试调用真实模型，不连接账号数据库，不改变产品行为。

- `snapshot.json` 是完全虚构的固定快照：成长基金 80 万元、债券基金 15 万元、缺少汇率的美元基金、人民币现金和待核对港币现金。应急金不足 1 个月，每月结余 8000 元。
- 原币金额与精确会计值并存，沿用当前生产输入。缺汇率时总资产为 `null`，投资资产已知小计不冒充完整合计；港币现金待核对不阻止其他现金计算。
- `questions.json` 包含 12 题；`previousCaseId` 仅引用前面成功完成的题目。v7 至行动 3 的已保存评测复现当时前端的 6000 个 UTF-16 单元截取及字段拼接；当前与前端一致通过 previousTurn 携带800字以内的上一轮问题、结论与动作，不新增总结调用。
- 默认沿用 `AnalysisSettings::default()` 的工作流、资料组和分析选项；Chat Completions 与前端一致关闭不支持的原生搜索。行动 1 更改默认值后，同一题集自然走新路径。

## 运行

需要本机 PostgreSQL 工具及真实模型配置。测试启动器只创建一次性数据库，不读取部署 `.env`、开发集群或生产数据。输出目录应先创建；报告必须使用新路径，防止覆盖基线或中断后的付费调用结果。

```sh
mkdir -p .runtime/qa-eval
# 在当前终端安全配置以下环境变量，勿把密钥写进代码或报告：
# MARIO_EVAL_BASE_URL、MARIO_EVAL_MODEL、MARIO_EVAL_API_KEY
# 可选 MARIO_EVAL_PROVIDER：openai-responses（默认）、openai-chat、anthropic
MARIO_EVAL_REPORT_PATH="$PWD/.runtime/qa-eval/baseline-v7.json" npm run eval:qa
npm run eval:qa:score -- .runtime/qa-eval/baseline-v7.json
```

也可以把配置保存到被 Git 忽略的 `.runtime/qa-eval.env`（建议权限 `600`），内容为上述环境变量以及绝对路径的 `MARIO_EVAL_REPORT_PATH`，然后运行：

```sh
node --env-file=.runtime/qa-eval.env scripts/postgres-cluster.mjs \
  cargo test --manifest-path server/Cargo.toml live_evidence_advice_evaluation -- --ignored --nocapture
```

密钥不进入提示词或报告。报告保存工作流版本、模型名、默认选项、题目、冻结输入、完整结果及调用轨迹。每题结束后写入检查点；失败继续记录，依赖失败题目的追问标为跳过，最后测试报错。失败题目没有完整 `trace.calls`，不计入完成题目的资源均值；不能把局部成功报告当成合格基线。

## 人工评分

首次评分生成 `<报告>.manual.csv` 和 `<报告>.score.json`。再次评分不会覆盖已有 CSV；修改评分后须显式传入 CSV 路径：

```sh
npm run eval:qa:score -- .runtime/qa-eval/baseline-v7.json \
  .runtime/qa-eval/baseline-v7.json.manual.csv
```

逐题评阅完整回答及冻结输入，在 CSV 填写：

| 字段 | 含义 |
| --- | --- |
| `suggestionCount` | 建议总数，模板用结构化行动数预填，可按实际展示修正 |
| `usefulSuggestionCount` | 同时有对象、方向和幅度、数字依据、适用前提的建议数 |
| `directAnswer` | 第一句直接回答本题为 `1`，否则 `0` |
| `fabricated` | 出现资料中没有依据的事实数字或来源为 `1`，否则 `0`；合理且可复核的计算与明确提出的目标幅度不算编造 |
| `notes` | 数字错配、依据缺口和具体评分理由 |

空白表示未评阅，不代表 `0`。只有所有题目执行完成且人工评分齐全，才输出三项质量率；建议率按建议总数加权，另两项按回答数计算。任何编造均不符合方案的零编造要求。

机器指标包含 Unicode 字符数（包括正文中的标题与告警）、阶段数、按 `requestCount` 汇总的实际请求次数、输入/输出 token、调用累计耗时和题目端到端耗时。供应商未返回某次 token 时，该题对应 token 合计为 `null`；均值只计用量完整的题目，同时输出覆盖数量，不按零估算。应使用相同模型和接口比较，并记录搜索开启时实时网页资料的变动。

离线验证：`npm run test:qa-eval`，以及 `npm run test:server -- ai::qa_eval`。完整 `npm test` 包含评分回归及虚构快照、追问截取回归；不会隐式调用真实模型。
