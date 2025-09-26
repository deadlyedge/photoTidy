优化建议

migrate.md:24 当前扫描→规划→执行是全量串行，可在保存的 origin.info.json 中缓存 fileHash/fileSize/mtime，增量扫描时先比较指纹，只对新增或变化文件做哈希与 EXIF 解析，并将这些步骤拆成 scan -> diff -> hash 的事件流，Tauri 后端可通过 app_handle.emit_all 实时推送进度以减少前端阻塞。
migrate.md:20 仍使用单线程 MD5，建议改为 BLAKE3/Rayon 并在读取层做 4–8MB 分片预取，既提升重复检测速度，也让 Rust 端能更好地调度 I/O；同时保留向后兼容字段（可将 BLAKE3 摘要落在新字段，旧字段继续写 MD5 直至完全迁移）。
migrate.md:31 前端每次启动都重写路径，可将路径派生逻辑集中在 Rust ConfigService，启动时一次性解析用户家目录、outputRootName 等，并持久化 schema version；React 端只读取拍扁后的对象，避免在多处手写路径拼接与斜杠处理。
migrate.md:29 里遗留 moveFiles/undoMoves/killUseless 的空实现，Rust 版本可以借助一个事务化执行器：对每个计划项写入操作日志（原路径、新路径、阶段），失败时按日志回滚，同时暴露“干跑”模式给前端，减少用户对真实文件操作的顾虑。
migrate.md:146 测试清单很好，但可以补充：a) 使用固定样本集对 Python 输出与 Rust 输出做 snapshot diff 测试，确保数据合同未破；b) 针对中文/emoji 路径与大视频文件各建一条集成测试；c) 为前端加上 Playwright 测试覆盖最关键的导入→规划→执行流程。

