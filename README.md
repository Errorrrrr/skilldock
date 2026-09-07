# SkillDock

统一管理 Agent Skill 的跨平台桌面应用与独立 CLI。当前版本 `0.1.0`，已在 macOS Apple Silicon 完成编译、原生界面及真实文件操作验证。

## 启动

需要 Node.js 22.12+、pnpm、稳定版 Rust 和 Git。桌面编译还需要对应系统的 [Tauri 2 前置依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
pnpm install --frozen-lockfile
pnpm desktop
```

只预览前端：`pnpm dev`。浏览器明确使用本地演示数据；真实文件管理仅在桌面应用和 CLI 中可用。两种开发启动方式使用同一个 1420 端口，请勿同时启动。

```sh
cargo build -p skilldock-cli
cargo run -p skilldock-cli -- --help
pnpm tauri build --debug --bundles app  # macOS 本地验证包
pnpm desktop:build                    # 正式优化构建，尚需自行配置发布签名
```

macOS 本地验证包：`target/debug/bundle/macos/SkillDock.app`。CLI：`target/debug/skilldock`。

## 已实现

- 自动发现常见工具的现有用户级目录；添加任意用户级或项目级分发目标。
- 扫描文件夹、审阅可归集内容、导入统一库；可选择备份原实体目录后以软链接管。
- 已登记工具目录中的已有安装软链与断链只展示并保留，不自动解引用或覆盖。
- Skill 库筛选、Markdown 预览、批量分发、从库卸载；同名目标预检与版本变化检测。
- 预设从库或文件夹选取成员、整体分发与取消；手动分发和多个预设分别记录引用，最后一个引用取消后才移除链接。
- 预设保存时锁定成员内容；支持含内容快照的 ZIP 导入导出，重复导入相同预设复用已有定义。
- Git 仓库/子目录、单 Skill 与整个集合导入；完整快照保留集合共享资源。
- ClawHub、SkillHub、Skills.sh 搜索安装；ClawHub 保留作者限定标识；可添加 ClawHub API 兼容 HTTPS 站点。
- 按来源定时检查或更新，按分发关系选择固定版本/跟随，保留历史快照供回滚。
- 统一目录可恢复迁移、文件事务日志、中断恢复、跨进程文件锁、内容摘要与路径边界检查。
- macOS 状态栏/系统托盘、关闭窗口后常驻、完全退出、开机启动、主题、来源变化通知。
- 独立应用更新入口，只有配置 HTTPS 升级地址与签名公钥后启用。

## 默认数据位置

| 平台 | 配置目录 | 统一库 |
| --- | --- | --- |
| macOS | `~/.config/skilldock` | `~/.local/share/skilldock` |
| Linux | `$XDG_CONFIG_HOME/skilldock`，未设置时同上 | `$XDG_DATA_HOME/skilldock`，未设置时同上 |
| Windows | `%LOCALAPPDATA%/SkillDock/config` | `%LOCALAPPDATA%/SkillDock/data` |

首次启动可自定义统一库。更改位置使用“设置 → 存储 → 修改位置”，迁移后保留旧库。已配置目录不可访问时会停止操作，不创建第二份库。

```text
统一库/
  state.json
  objects/<内容摘要>/tree/       # 完整来源快照
  transactions/<任务ID>.json    # 文件事务与恢复记录
  cache/                       # 隔离下载和 Git 缓存
  backups/
  trash/
```

接管现有安装时，原内容暂存为原目录旁的 `.skilldock-backup-<ID>`，使跨卷归集也能通过同目录重命名保留原件。只有受管软链会被取消操作删除，外部文件不会被覆盖。

## CLI 示例

下面的路径均为示例，请替换为自己的目录。

```sh
skilldock init /absolute/path/to/skilldock-data
skilldock scan /absolute/path/to/existing-skills
skilldock import /absolute/path/to/existing-skills --adopt
skilldock git https://github.com/vercel-labs/skills --subdir skills
skilldock search vue --site clawhub --site skillhub --site skills.sh
skilldock install find-skill-skillhub --site skillhub
skilldock target 'My Agent' /absolute/path/to/agent/skills
skilldock list --json
skilldock plan --skill SKILL_ID --target TARGET_ID
skilldock distribute --skill SKILL_ID --target TARGET_ID --revision REVISION
skilldock diagnose
```

`--config-dir /absolute/test/config` 可创建隔离实例。`--json` 输出机器可读结果，错误返回非零退出码。GUI 和 CLI 复用同一个 Rust 核心；完整操作也可通过 `skilldock exec '{"action":"..."}'` 调用。

```sh
skilldock exec '{"action":"save_preset","name":"开发工具","skillIds":["SKILL_ID"]}'
skilldock exec '{"action":"apply_preset","presetId":"PRESET_ID","targetIds":["TARGET_ID"],"expectedRevision":5}'
skilldock exec '{"action":"revoke_preset","presetId":"PRESET_ID","targetIds":["TARGET_ID"]}'
```

## 开发结构与检查

- `src/`：Vue 3 / TypeScript / Pinia / Vue Router；Tailwind 4、Reka UI、本地 UI 组件和 Lucide 图标。
- `crates/skilldock-core/`：存储、分发、预设、迁移、更新与网站适配。
- `crates/skilldock-cli/`：Clap 独立 CLI。
- `src-tauri/`：Tauri Commands、托盘、窗口生命周期与应用升级。
- `src/services/types.ts`：从 Rust 模型经 ts-rs 生成，勿手工修改。

```sh
pnpm typecheck
pnpm build
pnpm types:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

修改 Rust DTO 后执行 `pnpm types:generate`。调试构建支持 `SKILLDOCK_TEST_CONFIG` 环境变量隔离原生验收；正式构建忽略它。

## 当前边界

- 已验证 macOS arm64；Windows 和 Linux 仍需各自打包及实机验收。Windows 创建真实目录软链需要系统允许该能力，失败时不会降级为复制或 junction。
- 归集后的本地来源若原位置已替换为软链，会暂停更新；重新绑定来源的专用界面尚未实现，可从原始 Git/网站重新导入为来源。
- 已有外部软链本身不接管；需选择其原始实体文件夹。工具内置、插件托管与 `.system` 内容不自动迁移。
- Git 子模块与未解析 LFS 内容会明确拒绝；HTTPS 私有仓库认证界面、OS Keychain 凭据管理尚未接入。Git SSH 使用本机已有的 Git/SSH 环境。
- 当前更新策略以来源/集合为单位。上游新增成员提示审阅，不自动分发；删除与改名保留旧安装。单成员独立覆盖策略、自动合并本地修改和批量任务取消尚未实现。
- 快照、缓存与原安装备份保留供恢复；卸载移除库记录，不自动回收全部历史磁盘内容。自动垃圾回收尚未实现。
- 便携预设包暂不支持内部资源软链；跨操作系统执行权限与内容摘要的转换仍需完善，当前便携包按同平台使用。
- 不执行 Skill 脚本；软链创建成功不代表对应 Agent 已加载或执行成功。
- 当前 macOS 包用于本地开发验证，尚未配置签名、公证与线上升级服务；未发布。

## 网站接口依据

- [ClawHub HTTP API](https://docs.openclaw.ai/clawhub/http-api)
- [腾讯 SkillHub 列表、详情与下载](https://github.com/Tencent/skillhub/blob/main/docs/api/skills.md)
- [Skills.sh 官方 CLI 搜索实现](https://github.com/vercel-labs/skills/blob/main/src/find.ts)

站点字段、服务策略和限流可能变化；单站失败不会伪造成功结果或覆盖现有安装。
