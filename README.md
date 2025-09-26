# photoTidy

## 项目简介
photoTidy 是一个基于 Tauri 2 + React + Rust 的跨平台桌面应用，目标是帮助用户整理海量照片与视频资料。项目在保留原有工作流数据契约的前提下，采用 SQLite 持久化、结构化日志与现代前端栈，为后续的媒体扫描、规划与执行流程打下基础。

## 已实现功能
- **跨平台基础框架**：完成 Tauri 2.0 + React + TypeScript 脚手架，引入 Zustand 作为状态管理，并整合 Tailwind 风格化能力。
- **配置服务**：Rust `ConfigService` 在启动时解析 `config/config.json`，支持 HOME/DATA 目录的环境变量覆盖，统一输出 UI 所需的展平配置。
- **SQLite 初始化**：定义媒体清单、计划项与操作日志三大表结构，自动迁移并写入 schema 版本元数据。
- **核心工具库**：移植路径归一化、JSON 读写、哈希计算（MD5/BLAKE3）、时间戳格式化与目录遍历等工具函数，供后续扫描/规划逻辑复用。
- **事件与日志**：配置 `tracing` 日志订阅器，预留应用内事件常量，确保前后端间的可观测性。
- **前端配置总览**：UI 在启动与事件广播时拉取配置快照，展示数据库位置、输入/输出目录、重复文件目录及可扫描的扩展名。
- **工程化链路**：引入 ESLint v9 Flat Config、Prettier、Vitest（含 jest-dom 预设）与基础 Rust 单元测试，提供 `lint` / `test` / `cargo test` 三套校验流程。
- **开发文档**：新增 `docs/setup.md` 指南，并在 README 中集中链接项目文档以便新人快速上手。

## 计划中
- **媒体扫描管线**：实现 `scan_media` 工作线程，递归枚举媒体文件、增量更新 SQLite，并结合哈希缓存与 EXIF 元数据提取。
- **规划与执行引擎**：将 `makeNewPath` 迁移为 `plan_targets`，并构建复制/移动执行流程、操作日志与回滚能力。
- **前端工作流界面**：重建配置初始化、扫描进度、计划审查、执行结果等关键页面，实时呈现事件进度与重复文件提醒。
- **质量保障**：补充实用工具与端到端测试、对比旧版 Python 输出的快照测试，以及 Playwright E2E 流程。
- **打包发布**：完成 Windows/macOS/Linux 打包、权限校验、版本与签名流程，并筹备 Beta 发布。

## 相关文档
- [开发计划](docs/plan.md)
- [系统结构图](docs/structure.md)
- [本地开发环境搭建](docs/setup.md)
