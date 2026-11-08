# 买点观察趋势图验收记录（2026-11-08）

本记录保存本次实现的验证结果，不代表已部署；产品契约见[股价趋势与金叉图](../buy-watch.md#股价趋势与金叉图)。

本次在自选与公共公司详情共用两张图：月线 BOLL 中轨与周线 BOLL 三轨，以及 20／60 周 BOLL 中轨的金叉。价格与指标采用同一观察日归一的复权单位，行情历史扩到四年，输出最多 104 个周点。月线不参与买点条件；历史周点不引用未来月末，缺口只影响相关指标，已知端点收盘继续显示。旧观察缺字段时等待下一次成功检查补齐，没有数据迁移或新表。

隔离工作区从原主分支建立，随后重放到已经发布的 `72b0a72` 国内行情修复上；自动合并无冲突，重新验证最终组合。未提交或覆盖原工作区中的其他任务改动。

## 通过的检查

- `node scripts/check-architecture.mjs`：通过，94 个前端文件、27 个服务端模块，边界及循环依赖扫描通过。
- `npm run build --prefix client`、`npm run format:check --prefix client`：通过。构建保留既有大于 500 kB 的包体提示，没有放宽配置。
- `npm test --prefix client`：52 个文件、345 项测试通过。
- `npm run test:server`：353 项通过，6 项需要联网、登录或真实模型的既有测试保持忽略。数据库由 `scripts/postgres-cluster.mjs` 创建一次性 PostgreSQL 集群，完成后自动清理，未连接开发或生产数据库。
- `cargo fmt --manifest-path server/Cargo.toml -- --check`、`cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`、`cargo build --manifest-path server/Cargo.toml`：通过。
- 指标回归包含月周聚合、与原 BOLL 计算器一致、均线窗口独立、历史时点无未来月末价、实际上穿与暂态、缺口后的金叉抑制、旧观察缺字段，以及同源行情的归一单位。JSON 浮点启用精确往返，原观察迁移与账号备份回归通过。
- 浏览器以合成数据检查 1200px 桌面、768px 平板、390px 手机及手机深色模式；两图无横向溢出，日期滑块键盘操作、范围切换正常，无浏览器脚本错误。检查了截图，窄屏图表保持可读；此验收没有使用真实账号数据。
- 本次新增文档引用、冲突标记检查及 `git diff --check`：通过。

## 未完成的完整门禁

执行了 `npm test` 和 `npm run check:architecture`。二者在生产架构扫描通过后，均因仓库中不存在 `scripts/check-architecture.test.mjs` 而退出。该文件在本次修改前即缺失；没有补造或跳过该门禁，也没有修改测试入口。完整脚本后续引用的 `test-functional-api.mjs`、`test-web.mjs` 等集成脚本同样未包含在当前仓库，相关整套 HTTP／Web／桥接／财务脚本验收未完成。以上分项通过不能表述为完整 `npm test` 通过。

未执行真实联网来源、付费模型、真实 OAuth 或生产部署。来源回归使用本机 HTTP 夹具，实际可展示历史仍取决于供应商覆盖。

## 产物清理

所有构建与检查进程结束后，确认隔离工作区 `server/target` 的绝对路径位于本次工作区内，并用 `lsof` 确认没有打开的文件，再运行 `cargo clean --manifest-path server/Cargo.toml --target-dir server/target`。清除了 8395 个文件、约 5.0 GiB，确认目录已删除或为空；未清理原主工作区的编译目录。临时 Vite 验收进程已停止、合成页面已删除，`.runtime/watch-trends-qa` 中四张截图已逐项删除并移除空目录。未触及 `.runtime/dev`、`.runtime/native`、数据库、配置或其他任务的文件。
