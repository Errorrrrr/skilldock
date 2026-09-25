# SkillDock 功能检查（2026-09-25）

> 本文保留优化前的历史检查基线。后续已实施的修复与当前边界见[统一管理优化说明](management-optimization-2026-09-25.md)，不要把下述问题全部视为当前仍未修复。

检查基线：当前工作区提交 `8798224`，应用版本 0.3.0。检查对象是源码与现有功能流程，不代表已安装程序或远端发布状态。未修改业务代码、真实资料库或分发目录。

主要结论：导入、分发、预设、更新、事务恢复已形成完整框架；当前应优先修复跨入口行为不一致、部分失败反馈和异常状态退出路径，再补更新审阅、任务控制与长期存储管理。

## 已确认的流程缺陷

### 1. 网站安装发生内容合并时，后续操作静默跳过（高优先级）

- 场景：库中已有相同名称、完整内容及成员路径的 Skill，再从网站安装该内容，同时勾选加入预设或分发目标。
- 原因：单份内容模式会复用已有记录，并把新来源写入 `skillOrigins`；发现页只按新来源的 `sourceId` 查找 Skill。主来源仍为旧来源时找不到记录，后续分支全部跳过，弹窗仍关闭。
- 依据：[安装后查找记录](/Users/zxx/dev/skilldock/src/pages/DiscoveryPage.vue:116)、[来源合并规则](/Users/zxx/dev/skilldock/crates/skilldock-core/src/single_content.rs:46)。
- 验证：提取当前 `install()` 函数、注入符合合并规则的模拟 API 返回值，确认仅调用 install，未调用 savePreset、plan 或 distribute，且无错误提示。属于函数级复现，未做原生端到端验证。
- 建议：安装接口返回最终解析后的 Skill ID 列表，前端依据明确结果执行后续操作；无法解析时保留弹窗并说明未完成项。

### 2. 加入预设失败会被后续分发成功掩盖（高优先级）

- 场景：安装成功，加入预设因并发写入等原因失败，随后分发到工具成功。
- 原因：发现页未检查循环中 `app.mutate()` 的布尔返回值；下一次 mutate 会清空 `error`，最终显示“安装并分发已完成”。多个预设之间也存在同类问题。
- 依据：[安装流程](/Users/zxx/dev/skilldock/src/pages/DiscoveryPage.vue:127)、[错误状态重置](/Users/zxx/dev/skilldock/src/stores/app.ts:64)。
- 验证：对当前 install 函数模拟 savePreset 失败、distribute 成功，最终 error 为空、弹窗关闭，确认失败信息丢失。
- 建议：分别记录入库、各预设和各目标结果，明确显示部分成功，保留失败项重试入口；不要求回滚已经成功入库的内容。

### 3. “加入预设”是否同步目标，取决于操作入口（中优先级）

- 场景：预设已应用到工具且跟随更新，从 Skill 库或发现页加入一个成员。
- 实际行为：预设定义已更新，但这两个入口没有传 `syncApplied: true`，不会立即创建新增成员的目标链接。预设编辑器保存则会同步；用户可能看到预设内已有成员，实际工具中仍没有。
- 依据：[库页面](/Users/zxx/dev/skilldock/src/pages/LibraryPage.vue:297)、[预设编辑器](/Users/zxx/dev/skilldock/src/components/PresetEditorDialog.vue:339)、[后端同步条件](/Users/zxx/dev/skilldock/crates/skilldock-core/src/lib.rs:619)。
- 验证方式：前后端调用链静态确认。后续显式重试、重新应用或其他触发 reconcile 的操作可能补齐，本问题不是永久无法分发。
- 建议：统一各入口的同步策略；明确区分“预设已保存”和“目标已同步”，冲突按目标显示。

### 4. 本地来源成员失配后，取消分发和移除配置也被阻止（中优先级）

- 场景：来源的成员 ID 在库中不存在，或成员已不再是 externalPath 类型，但已有绑定仍记录该来源的 claim。
- 原因：apply、revoke、remove 三个动作都先调用 `local_source_skills` 校验全部成员；失败后无法进入按 claim 撤销已有绑定的逻辑。
- 依据：[共同前置校验](/Users/zxx/dev/skilldock/crates/skilldock-core/src/local_sources.rs:198)、[成员解析](/Users/zxx/dev/skilldock/crates/skilldock-core/src/local_sources.rs:5)。
- 验证方式：静态确认具体错误分支；未重新运行 Rust 复现。历史项目笔记记录过修复，但当前检查基线仍包含该逻辑，不能把历史记录当作当前已修复证据。
- 建议：新增分发继续严格验证成员；取消和移除按已有绑定撤销，保留版本、实际链接与所有权校验。

## 功能完整性不足

### 5. 更新前缺少足够的内容审阅

更新中心主要显示来源状态、当前版本、现有成员和概括提示，没有文件级增删改列表、SKILL.md 文本差异、共享资源变化及完整受影响目标预览。来源切换也主要显示“内容一致／有差异”，不能展开实际差异。网站详情明确要求安装后才能查看 SKILL.md。

依据：[更新摘要](/Users/zxx/dev/skilldock/src/pages/UpdatesPage.vue:136)、[切换来源比较](/Users/zxx/dev/skilldock/src/components/DistributionDialog.vue:153)、[网站文件预览](/Users/zxx/dev/skilldock/src/pages/DiscoveryPage.vue:300)。

建议：增加基于确定候选版本的只读预览，展示成员、文件差异与受影响工具；确认时校验候选版本，避免审阅后应用到另一份内容。

### 6. 长任务控制与失败续作不足

批量检查已有“停止后续检查”和失败项重试，但不能中止当前网络请求；Git clone 超时为 300 秒，任务页没有统一取消、下载进度或按阶段续作入口。来源调度逐项执行，慢来源会拖延后续来源。

依据：[停止边界](/Users/zxx/dev/skilldock/src/pages/UpdatesPage.vue:298)、[网络超时](/Users/zxx/dev/skilldock/crates/skilldock-core/src/network.rs:41)、[任务页](/Users/zxx/dev/skilldock/src/pages/TasksPage.vue:1)。

建议：为下载、扫描、入库、分发提供阶段状态；网络准备阶段支持取消，文件事务提交阶段保持受保护；失败后重试未完成阶段。

### 7. 缓存及历史记录缺少长期管理

每次远程 Git 准备都会建立独立缓存目录，成功路径不清理；检查更新也走此路径。现有“统一库实体清理”主要处理对象，不等于缓存清理。README 明确说明缓存回收尚未实现。

同时每次 snapshot 会遍历并解析事务日志，桌面每 5 秒刷新一次；事务日志含操作前后完整状态。随着任务历史增加，磁盘占用与读取成本会增长。此处是代码结构确认的规模风险，尚未做性能基准，不能据此断言当前已卡顿。

依据：[缓存创建](/Users/zxx/dev/skilldock/crates/skilldock-core/src/network.rs:236)、[日志扫描](/Users/zxx/dev/skilldock/crates/skilldock-core/src/lib.rs:189)、[轮询](/Users/zxx/dev/skilldock/src/stores/app.ts:35)。

建议：显示对象、缓存、备份、日志占用；增加带引用保护的缓存清理和保留策略；恢复日志按未完成事务建立索引，避免高频读取全部历史。清理不得破坏回滚或已有引用。

## 后续能力边界

以下是产品增强方向，不全是应修复的缺陷：

- 安装可用性：现有诊断主要检查文件、链接和快照；缺少统一的依赖、环境变量、外部工具及 Agent 加载状态检查。可增加只读就绪检查，不默认执行 Skill 脚本。
- 换机与团队共享：已有预设 ZIP，但本地引用、内部资源软链和跨操作系统内容转换有边界；未提供完整配置与来源关系的换机恢复向导。见 [导出限制](/Users/zxx/dev/skilldock/crates/skilldock-core/src/portable.rs:353)。
- 私有与自定义来源：HTTPS 私有仓库认证界面、Keychain、Git 子模块和未解析 LFS 尚不支持；自定义网站主要依赖 ClawHub 兼容接口。SSH 可使用本机环境，不应描述成“完全不能使用私有仓库”。
- 平台验收：Windows/Linux 真机可用性、Windows 软链权限仍是明确边界，构建成功不能代替真实安装与分发验收。
- 文档一致性：README 混有旧版锁定/跟随规则及“未发布”等历史描述，需按当前模式整理；本次未核验远端是否已发布。

不建议把旧版本固定、多级版本列表直接补回默认模式：0.3.0 的单份当前内容是明确设计选择。当前优先保证安装结果真实、入口行为一致、失败可恢复。

## 本次验证

- 静态检查：人工核对 Vue 页面、API 调用、Rust 核心、桌面调度与发布流程；前端 typecheck 未完成，缺失依赖且 npm registry 不可达。
- 自动测试：`node --test tests/release.test.mjs` 4/4 通过；另做上述安装函数的两种隔离场景复现。前端分发测试未运行。
- Rust 测试：绝对路径调用 Cargo 成功启动，但离线依赖缺少 `anstream v1.0.0`，未进入测试执行。不能引用历史测试通过数代替本次结果。
- 构建：未运行完整构建。
- 真实验证：未启动原生应用、未实际联网安装、未修改真实 Skill 文件或软链。
- Git／推送／发布：仅新增本报告，未改业务代码，未提交、推送或发布。验证中产生的临时 pnpm store 已清除。

建议执行顺序：先修复 1–4 并补针对性回归，再实现更新差异预览，随后完善任务控制、缓存管理和安装就绪检查。
