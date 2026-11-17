# mario

mario 是个人投资辅助工具，围绕用户财务与研究资料、公共市场数据、AI 分析三个功能域组织，帮助记录自身情况、跟踪市场并基于授权资料开展投资讨论。

| 功能域 | 职责 |
| --- | --- |
| 用户 | 账号、持仓与真实成交、现金与估值、观察列表、研究资料、方法论与复盘 |
| 市场 | 公共证券信息、价格、汇率、分红，以及数据采集、缓存与来源 |
| AI | 授权上下文、分析与评议、历史与记忆、模型连接与执行 |

当前技术栈为 React Web、Rust API 和 PostgreSQL。业务规则、代码边界与已采纳但尚未实现的行为，通过[文档索引](docs/README.md)查询。

## 本地启动

准备 Node.js 与 npm、Rust/Cargo，以及本机 PostgreSQL 工具（包括 `initdb`、`pg_ctl`）。确保 `pg_config` 可从 PATH 调用，或设置 `MARIO_TEST_PG_BIN` 为 PostgreSQL 的 bin 目录。工具查找与开发数据库逻辑见[PostgreSQL 启动器](scripts/postgres-cluster.mjs)。

在仓库根目录执行：

```bash
npm run install:all
npm run dev
```

启动后打开终端显示的 Vite 地址（默认端口 1420），在浏览器注册开发账号。开发脚本启动 PostgreSQL、Rust API 与前端，数据和开发专用凭据保存在 `.runtime/dev`，重启后保留。本地分析在 API 进程内执行。启动行为见[开发脚本](scripts/dev.mjs)。

## 部署入口

服务器部署通过根目录 [deploy.sh](deploy.sh) 手动执行。先查看脚本顶部配置和帮助，再按自己的服务器填写参数：

```bash
bash deploy.sh --help
sudo bash deploy.sh --ip '<公网IP>' --domain '<域名>'
```

当前脚本面向 Linux/systemd，Ubuntu/Debian 可自动安装缺少的依赖。显式同时传入 IP 和域名会配置 Nginx 与证书；后续更新源码后执行 `sudo bash deploy.sh`，复用已有配置部署应用。具体选项、服务管理和访问前缀以脚本帮助与顶部配置为准。提交或推送代码不会自动部署网站。

## 开发导航

- [文档索引](docs/README.md)：查询三域 spec、架构、现有 API 与实现差异。
- [项目协作规范](AGENTS.md)：修改、文档维护、验证及 Git 交付要求。
- [lint 范围与验证入口](docs/lint.md)：查询适用检查、执行命令和当前覆盖边界。
