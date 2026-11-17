# lint 范围与验证边界

lint 负责可自动判定的代码质量、模块边界、契约一致性和文档有效性。业务正确性由测试验证，AI 回答质量单独评测。业务归属和允许的依赖见[架构](architecture.md)，不在本文维护另一套模块划分。

本文确定检查范围，并说明已有入口与缺口；不表示所有检查已经实现或本次已执行。

## 1. 代码质量

检查格式、类型与编译警告，复用现有工具：

- 前端用 Prettier 检查格式，TypeScript 检查类型和无用声明。
- Rust 用 rustfmt 检查格式，用 Clippy 检查代码问题并拒绝警告。
- 检查模式不自动修改业务代码；修复与检查分别执行。

现有工具配置以[根 package.json](../package.json)、[前端 package.json](../client/package.json)、[TypeScript 配置](../client/tsconfig.json)和 [Rust 配置](../server/Cargo.toml)为准。

## 2. 模块边界

检查依赖方向、循环依赖，以及对其他域内部实现的直接访问。用户、市场、AI 的职责归属由架构文档确定，技术层检查为其提供补充约束。

当前[架构检查](../scripts/check-architecture.mjs)覆盖前端跨页面依赖、共享模块反向依赖、循环、不可达前端模块，以及后端领域、用例、端口、数据库驱动、行情适配、AI 执行和认证等技术边界。检查器自身有[回归测试](../scripts/check-architecture.test.mjs)。

现有混合职责在架构文档中显式登记。检查通过不能掩盖尚未解决的业务边界，也不要求本轮立即重构全库。新增约束应针对明确的职责和依赖，不通过目录名称推断整个目录只属于一个业务域。

## 3. 契约一致性

检查前后端类型、请求策略，以及路由与 API 文档是否对应。业务规则引用所属域 spec，不将自然语言需求全部转成机械文本匹配。

现有[契约检查](../scripts/check-contracts.mjs)将 Rust 序列化夹具交给 TypeScript 检查，并核对部分请求版本策略，覆盖金额、成交、财务档案、观察规则、模型配置和流式事件等契约。它不等于所有接口已经覆盖。

该入口会运行 Rust 契约夹具，并通过一次性 PostgreSQL 启动器执行，需要本机 Rust 与 PostgreSQL 工具；它不是纯文本静态扫描。现有接口清单与调用契约见 [API 文档](api.md)，路由与该文档的持续自动对应检查尚未实现。

## 4. 文档有效性

检查文档引用的文件路径、链接、锚点、脚本入口，以及生成内容是否与来源一致。只检查已经确认保留的文档；历史内容不会因缺失引用而自动恢复。

现有[入口检查](../scripts/check-entrypoints.mjs)验证 npm 脚本与完整门禁引用的脚本路径，其[测试](../scripts/check-entrypoints.test.mjs)验证缺失入口会被发现。它尚未覆盖全部 Markdown 链接、锚点或 API 生成内容。

已确认保留的文档由[文档索引](README.md)提供查询入口。检查结果须说明实际覆盖的文件，不能将索引所列文档的局部链接检查通过描述为整个仓库文档已经通过检查。

## 5. 当前执行入口

以下是现有入口；仓库目前尚无统一的 `npm run lint` 命令。

| 类别 | 当前命令 | 覆盖说明 |
| --- | --- | --- |
| 前端格式 | `npm run format:check --prefix client` | 检查前端源码格式 |
| 前端类型与构建 | `npm run build --prefix client` | 当前将 TypeScript 检查与 Vite 构建一起执行 |
| Rust 格式 | `cargo fmt --manifest-path server/Cargo.toml -- --check` | 只检查格式 |
| Rust 代码质量 | `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings` | 检查所有目标，拒绝警告 |
| 技术架构 | `npm run check:architecture` | 现有边界检查及检查器回归 |
| 前后端契约 | `npm run check:contracts` | Rust 夹具、TypeScript 契约及请求策略的一部分 |
| 脚本入口 | `npm run check:entrypoints` | 脚本存在性及检查器回归 |
| 完整门禁 | `npm test` | 见[完整测试脚本](../scripts/test.sh)，包含构建、检查、单元及集成验收，范围大于 lint |

纯文档改动核对事实、链接、锚点、冲突标记并执行 `git diff --check`。代码或检查器发生变化时，补充相应类型检查和有意义的回归；不能把无关业务测试的执行次数当成文档或架构质量。

真实市场来源连通性、真实模型回答质量、OAuth 登录和独立部署恢复验收分别报告。lint 通过不代表这些外部行为或业务正确性已经验收。
