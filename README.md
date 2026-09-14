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

## 恢复与永久清理

在“任务与记录 → 归集备份”中恢复原目录。恢复的是归集时的版本；已被其他分发或预设引用的库记录会保留。

- 恢复弹窗列出统一库实体及各历史版本，只有无引用、内容未修改且能完成校验的项目可选择。点击“恢复并永久清理”后先恢复原目录，再清理选中项；“仅恢复”保留统一库实体。
- 已经恢复或卸载后留下的实体，可从“清理统一库残留”重新预览并确认清理。永久删除无法撤销。
- 清理失败会保留剩余文件并显示原因，可在同一区域“重试实体清理”；清理失败不会撤销已成功的原目录恢复。
- 管理记录、预设锁定版本、可回滚的历史版本、备份及未完成任务都参与引用保护。软链检查覆盖已配置目录及日志中的历史来源、分发和原目录所在文件夹，不能保证发现其他位置的手动软链。
- 统一库内 `.object-cleanup` 是清理暂存目录。正常完成后移除；失败或检测到修改时保留内容，清理记录展示实际路径。
- “保留最近 N 批次”只控制原目录备份，与统一库实体清理独立。

CLI 与桌面复用相同校验。先通过 `preview_restore` 或 `preview_object_cleanup` 获取 `revision` 和候选，再将选中项目的 `digest`、`fingerprint` 原样提交；空选择不会隐式清理全部内容。

```sh
skilldock exec '{"action":"preview_object_cleanup"}'
skilldock exec '{"action":"cleanup_objects","expectedRevision":REVISION,"items":[{"digest":"DIGEST","fingerprint":"FINGERPRINT"}]}'
skilldock exec '{"action":"list_object_cleanups"}'
skilldock exec '{"action":"retry_object_cleanup","taskId":"TASK_ID"}'
```

`restore_backup` 可传 `cleanupItems`（同上述 `items` 格式）和 `expectedRevision`；省略 `cleanupItems` 时仅恢复。重试只处理原任务中尚未完成的项目，执行前会重新检查引用和内容。

## 默认数据位置

| 平台 | 配置目录 | 统一库 |
| --- | --- | --- |
| macOS | `~/.config/skilldock` | `~/.local/share/skilldock` |
| Linux | `$XDG_CONFIG_HOME/skilldock`，未设置时同上 | `$XDG_DATA_HOME/skilldock`，未设置时同上 |
| Windows | `%LOCALAPPDATA%/SkillDock/config` | `%LOCALAPPDATA%/SkillDock/data` |

首次启动可自定义统一库。更改位置使用“设置 → 存储 → 修改位置”，迁移完成后清理旧库；清理失败时保留残余内容并支持重试。已配置目录不可访问时会停止操作，不创建第二份库。

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

## Git 集合导入

在「Skill 库 → 添加 Skill → 从 Git 仓库导入」或「预设 → 从 Git 集合」粘贴仓库 / GitHub、GitLab 子目录链接，预览后同步。整仓共享资源保留在统一库，各 Skill 独立登记和软链分发；仅所选成员入库；可仅入库、创建预设或加入已有预设。定时更新默认关闭，需在来源中自行配置。

预设可自动加入后续新增成员，来源删除默认保留，确认移除只影响当前预设引用。详情见 [Git 集合导入](docs/git-collection-import.md)。

## 网络代理

在「设置 → 网络代理」选择跟随系统代理（默认）、直连或自定义 HTTP/HTTPS 代理。例如 `http://127.0.0.1:7897`。测试连接分别检查 GitHub HTTP 与 Git，不保存草稿；点击保存设置后，新发起的 Git、网站搜索、下载和 Skill 来源更新使用该配置。不会修改全局 Git 或系统代理，应用自身升级不在此配置范围内。暂不支持代理账号密码和 SOCKS 地址。系统模式读取 macOS 网络代理、Windows 当前用户 Internet Settings 或 Linux GNOME gsettings 的固定代理及绕过列表；PAC/自动发现会提示改用自定义。旧 inherit 配置按 system 处理，不再读取终端代理环境。Windows/Linux 尚未进行真机验证。

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
- 归集记录不作为更新来源。本地文件夹只手动扫描并确认同步；仅明确的远程 Git 或网站来源可配置定时更新。
- 已有外部软链会被识别；应用预设时可明确确认接管，取消最后一个引用时恢复原链接。工具内置、插件托管与 `.system` 内容不自动迁移。
- Git 子模块与未解析 LFS 内容会明确拒绝；HTTPS 私有仓库认证界面、OS Keychain 凭据管理尚未接入。Git SSH 使用本机已有的 Git/SSH 环境。
- 更新任务以来源为单位；订阅整包的跟随预设可自动加入、分发新增成员，固定目标保持原版本；删除与改名保留旧安装并持续提示审阅。单成员独立覆盖策略、自动合并本地修改和批量任务取消尚未实现。
- 卸载移除库记录，不自动清理实体；可使用统一库残留清理入口。缓存回收与定时垃圾回收尚未实现。
- 便携预设包暂不支持内部资源软链；跨操作系统执行权限与内容摘要的转换仍需完善，当前便携包按同平台使用。
- 不执行 Skill 脚本；软链创建成功不代表对应 Agent 已加载或执行成功。
- 当前 macOS 包用于本地开发验证，尚未配置签名、公证与线上升级服务；未发布。

## 网站接口依据

- [ClawHub HTTP API](https://docs.openclaw.ai/clawhub/http-api)
- [腾讯 SkillHub 列表、详情与下载](https://github.com/Tencent/skillhub/blob/main/docs/api/skills.md)
- [Skills.sh 官方 CLI 搜索实现](https://github.com/vercel-labs/skills/blob/main/src/find.ts)

站点字段、服务策略和限流可能变化；单站失败不会伪造成功结果或覆盖现有安装。


## 包同步与预设

“预设 → 从文件夹”保留完整原目录，将包同步到统一库。包在编辑器中独立分组，可以展开选择成员、保留排除项，并开启或关闭“自动加入新增成员”。普通导入不再提供直接引用原目录的模式；旧 local_reference 数据仍可读取。

同一本地目录重复导入复用包；不同本地副本按各自目录管理，不因 Git remote 相同而合并。文件夹中即使包含 Git 仓库，也只手动同步。通过 Git 入口导入的远程仓库在受管缓存中更新，按仓库、引用和扫描范围复用来源；不对用户原目录执行 pull。Git 子模块/未解析 LFS 延续底层拒绝规则。

每个预设在每个目标上的应用独立记录跟随策略、已应用修订和失败原因。更新先入库，再按目标事务同步跟随的预设；固定目标和未应用目标不变。同一目标的多个跟随预设共享相同成员版本时共同推进；版本冲突保留原安装并支持重试。

库页面通过实际软链识别“外部已安装”，不会因此获得删除权限。应用预设可勾选接管，复用原安装名称并保存原链接指向；取消所有引用时恢复原链接，原实体不可用时阻止恢复并保留当前安装。嵌套在父软链下的成员只识别，不把子目录自动注册成新的分发目标。

成员入口必须精确命名为 `SKILL.md`；普通 `skill.md` 不算入口。已有 Skill 内的 `references/`、`assets/`、`scripts/` 只作为内部资源保存，不递归发现成员；没有 Skill 入口的包目录仍可递归扫描多个成员。此规则只影响发现，不删除或排除同步文件。

包内容保留共享结构。Git 元数据和本地 Python 环境不复制；发现 .venv 且没有可识别的依赖重建入口时，标记依赖待处理并阻止分发/接管。解决依赖后再次同步会重新检查，即使内容摘要未变化。不会自动执行包内脚本或重建运行环境。

已登记包和旧本地引用禁止移动式归集。旧归集目录可从导入界面“检查旧归集迁移”查看实体、入口和待恢复状态；不自动还原文件、不覆盖库版本。需要使用现有备份恢复功能审阅恢复完整来源后再登记同步包。统一库不能迁入包或本地来源目录。写入新状态使用 schemaVersion 2，兼容读取版本 1，旧程序会拒绝版本 2 而不会静默丢失包关系。

### CLI API

所有操作通过 `skilldock exec` 与桌面共用核心；目录和 ID 由实际预览结果提供。

- `preview_preset_folder`：path，扫描成员。
- `import_package`：path、selectedPaths、revision，返回 snapshot、packageId、所选 skillIds。同步保存完整包内容，仅所选成员加入当前预设。
- `save_preset`：原字段外增加 packages、syncApplied（保存后同步跟随目标）；每项包含 presetId（保存时自动设置）、packageId、autoAdd、excludedIds、selectedIds。
- `apply_preset`：presetId、targetIds、expectedRevision；外部安装接管需要显式 takeover=true。
- `set_preset_follow`：presetId、targetId、follow，固定或跟随目标版本。
- `retry_preset_sync`：重新按目标检查并同步；失败原因保留在 presetApplications。
- `package_migration_preview`：path，只读展示旧归集入口与实体。

原 `import_preset_folder` API 保留以兼容旧客户端；不用于新的普通包导入。便携导出保留当前选定快照，不携带自动更新订阅和本机路径。

Skill 库按名称汇总展示安装记录，工具分发列合并各来源成员的目标状态；侧栏、标题和状态栏按汇总后的 Skill 数统计。每份来源、版本、预设引用和绑定仍独立保存，取消分发按实际绑定处理；来源记录提示可查看不同来源及版本。批量分发跳过已存在的外部安装。

归集、文件夹导入和来源更新使用忽略 Finder `.DS_Store` 的内容摘要；旧库重复导入只有在来源与所有既有成员快照的实际内容一致时才转换摘要。旧实体、安装软链和预设锁定版本保留；真实内容不同仍要求通过更新中心处理。

预设编辑中的“同步包并加入预设”先持久化包和库成员，“保存预设”才保存订阅关系。关闭未保存的编辑器不撤销入库；再次扫描通过包作用域的原目录、来源 ID 和成员相对路径识别 Git 包、嵌套仓库及同仓库不同检出目录，重复添加复用已有成员。真正不同来源的同名成员仍提示冲突。

预设再次同步原包会识别新增、内容变更和来源移除。移除成员默认保留旧内容，勾选确认并保存后仅解除该预设引用，其他引用及固定目标保留。完整行为与验证见 [预设包差异同步](docs/package-diff-sync.md)。

## 移除分发目标

在目标详情底部选择「移除目标」，确认后移除目标配置、分发记录和该目标的预设应用关系，停止后续同步。实际目录、文件、软链、中央库和预设保留。操作校验确认时的资料库版本，避免在后台状态变化后继续执行旧确认。

已移除目录仍作为只读引用保护范围参与中央库清理检查；不会再显示为分发目标。若保留链接仍指向旧中央库，迁移会拒绝并提示处理依赖，防止移除旧库后链接失效。可重新添加该目录。


## 本地来源

侧栏「本地来源」或「发现与安装 → 本地来源」支持名称与目录配置、成员扫描、整体分发、按目标取消及移除配置。与「复制文件夹入库」不同，本地来源直接引用原实体，不复制内容、不拉取 Git、不建立更新任务。

同目录重复添加复用配置，指向同一实体的成员复用身份；不同实体的同名成员不自动合并。已有同实体软链默认显示“复用已有软链”，取消后保留原链接。可在预览中勾选“接管已有软链的管理”：全部分发引用取消后删除软链，原实体目录保留；以前已复用的链接也可重新分发并选择接管。来源分发使用独立引用，不创建隐藏预设，也不会取消其他手动或预设引用。

重扫并确认成员选择后，只同步该来源已分发的目标；冲突时整个配置保存回滚。移除配置保留原文件和库记录。原目录内容修改直接生效，不提供快照式版本冻结；原目录需要保持可用。引用来源不能导出成锁定快照预设。

### 切换已有分发的来源

同名 Skill 从 Git 或本地重新导入后，可以通过 Skill 库的「选择来源分发」，或对应预设／本地来源的分发入口，预览并确认切换。预览展示旧、新来源、实体路径、版本、内容差异状态与现有引用；确认选项默认关闭。

旧安装仍被其他预设或本地来源引用时会阻止切换，并列出需要先解除的引用。确认后只改变所选目标的链接，旧实体、归集备份和其他工具保持原样。详见 [分发来源切换](docs/specs/distribution-source-switch.md)。
