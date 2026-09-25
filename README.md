# SkillDock

统一管理 Agent Skill 的跨平台桌面应用与独立 CLI。当前版本 `0.3.2`。项目已在 macOS Apple Silicon 验证原生界面和真实文件操作；Windows/Linux 的安装与真实桌面操作仍需平台验收。

## 单份当前内容（0.3.0）

新建库默认使用单份当前内容：每个 Skill 一个名称入口，所有已管理工具及预设使用当前内容。工具目录使用软链，不按工具复制内容，也不提供固定旧版本或版本列表。

```text
SkillDock/
├── code-review/       # 当前内容入口（软链）
├── git-commit/        # 当前内容入口（软链）
└── .skilldock/        # 内部数据、完整依赖包和恢复备份
```

- 同名且完整内容、依赖包及成员路径相同的导入合并，保留收录来源；不同内容独立保留并加名称后缀。详情「更新」中可明确替换当前内容。
- 新添加的本地来源也复制到统一库，原路径只作后续手动同步来源。旧本地引用保持兼容，需在来源页面明确选择「迁入统一库」；不会在升级时自动移动用户文件。
- 名称后缀只区分库中记录。分发还检查 `SKILL.md` 声明名，同一工具默认只启用一个同名实现；切换需经过来源切换预览与引用检查。
- 每个来源包最多保留一次更新恢复备份；连续更新会回收更早且无引用、未修改的对象。「撤销上次更新」恢复整个来源包及工具链接，消费备份并暂停自动应用。
- 新库默认不长期保留归集原目录：先复制、校验并完成软链事务，提交成功后清理临时原件；失败、中断或检测到外部修改时保留恢复材料。旧库沿用原有留存设置。
- 共享依赖按完整包保存。历史归集原目录、已配置或历史路径中的外部引用、修改过的文件及未完成事务仍受保护，因此不承诺磁盘上永远只有两个目录。
- 旧库升级后保持原规则，在 Skill 库或「设置 → 统一目录」预览并确认简化。同名不同内容必须明确选用或分别保留；确认后统一工具内容并整理目录。迁移不会覆盖未知文件或已被外部修改的链接。
- 状态格式为 schemaVersion 3，兼容读取旧库；旧客户端不能读取新模式。以下历史版本、锁定与选择来源分发说明仅适用于尚未迁移的旧库。

实体仍保存在 `.skilldock/objects/<内容摘要>/tree/`，外层名称入口用于浏览；本轮没有改成按可读包名存放实体。实现及接口见 [单份内容方案](docs/plans/single-current-content/specs/技术方案.md) 与 [统一管理优化说明（2026-09-25）](docs/management-optimization-2026-09-25.md)。

## 下载与更新

在 [GitHub Releases](https://github.com/Errorrrrr/skilldock/releases/latest) 下载对应系统的安装包。首次使用请手动安装；后续发现新版本时会在侧栏提示，点击进入「设置 → 应用更新」确认安装，也可手动检查。设置编辑完成后自动保存。

正式安装包内置更新公钥，并校验每次更新的签名。macOS 使用 ad-hoc 包签名，尚未进行 Apple 公证；Windows 尚未配置系统代码签名。开发构建没有正式更新公钥，不能直接在线升级。发布配置见 [应用发布与更新](docs/app-release.md)。

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
- 本地来源扫描并复制完整内容入库；同目录再次扫描后手动同步，可整体分发或取消。
- 归集向导区分「保留整包结构」与「各 Skill 相互独立」，审阅与已有库及本批扫描的同名关系后导入；可选择以软链接管原位置，提交前重新校验资料库修订与来源内容。
- 已登记工具目录中的已有安装软链与断链只展示并保留，不自动解引用或覆盖。
- Skill 库筛选、Markdown 预览、批量分发、从库卸载；目标路径和 Skill 声明名冲突预检。
- 预设从库或文件夹选取成员、整体分发与取消；手动分发和多个预设分别记录引用，最后一个引用取消后才移除链接。
- 从 Skill 库和网站安装入口加入预设时同步跟随目标；成员已保存但目标同步失败会单独显示并支持重试。支持含内容快照的 ZIP 导入导出，重复导入相同预设复用已有定义；旧库保留锁定版本兼容。
- Git 仓库/子目录、单 Skill 与整个集合导入；完整快照保留集合共享资源。
- ClawHub、SkillHub、Skills.sh 搜索安装；ClawHub 保留作者限定标识；可添加 ClawHub API 兼容 HTTPS 站点。
- 网站安装分别记录入库、成员解析、各预设和各目标结果；识别合并后的 Skill 及全部来源成员，部分失败重试时跳过已完成步骤。
- 按来源定时检查或更新，所有受管工具同步当前内容，按来源包保留一次更新恢复。
- 统一目录可恢复迁移、文件事务日志、中断恢复、跨进程文件锁、内容摘要与路径边界检查。
- macOS 状态栏/系统托盘、关闭窗口后常驻、完全退出、开机启动、主题、来源变化通知。
- 独立应用更新入口，正式发布构建内置 GitHub 更新源与签名公钥，支持版本说明、下载进度和失败重试。

## 恢复与永久清理

在「任务与记录 → 恢复与清理」管理归集原目录留存和统一库残留。新库 `backupRetention=0`，成功归集并校验后清理临时原件，不提供该次归集原目录的长期撤销；失败或中断仍保留事务恢复。每个来源的「撤销上次更新」独立保留，不受此设置影响。

旧库保留原有设置。可将原目录留存设为 1–100 批；仍有可用原件时，可以恢复归集时的版本。已被其他分发或预设引用的库记录会保留。

- 恢复弹窗列出统一库实体及各历史版本，只有无引用、内容未修改且能完成校验的项目可选择。点击“恢复并永久清理”后先恢复原目录，再清理选中项；“仅恢复”保留统一库实体。
- 已经恢复或卸载后留下的实体，可从“清理统一库残留”重新预览并确认清理。永久删除无法撤销。
- 清理失败会保留剩余文件并显示原因，可在同一区域“重试实体清理”；清理失败不会撤销已成功的原目录恢复。
- 管理记录、预设锁定版本、可回滚的历史版本、备份及未完成任务都参与引用保护。软链检查覆盖已配置目录及日志中的历史来源、分发和原目录所在文件夹，不能保证发现其他位置的手动软链。
- 统一库内 `.object-cleanup` 是清理暂存目录。正常完成后移除；失败或检测到修改时保留内容，清理记录展示实际路径。
- 「原目录保留最近 N 批次」只控制归集原件，与统一库实体清理和更新撤销独立。保存设置会立即清理超额留存；设为 0 会清理全部可安全删除的旧原件，异常或已修改的内容继续保留并说明原因。

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

新库的内部管理目录如下；未简化的旧库可能直接把这些内容放在统一库根目录。

```text
统一库/.skilldock/
  state.json
  objects/<内容摘要>/tree/       # 完整来源快照
  transactions/<任务ID>.json    # 文件事务与恢复记录
  cache/                       # 隔离下载和 Git 缓存
  backups/
  trash/
```

接管现有安装时，原内容暂存为原目录旁的 `.skilldock-backup-<ID>`，使跨卷归集也能通过同目录重命名保留原件。留存为 0 时，事务提交并校验后清理这份临时原件；恢复材料异常时保留并提示。只有受管软链会被取消操作删除，外部文件不会被覆盖。

## 归集工具目录中的已有 Skill

「归集已有 Skill」先选择目录和内容组织方式，再审阅内容关系：

- 「保留整包结构」保留扫描根的共享资源与相对路径，适合工作流包，是默认选项。
- 「各 Skill 相互独立」以各成员完整目录比较和去重，适合确认没有包根或兄弟目录依赖的散装 Skill。父 Skill 与嵌套成员仍一起保存。
- 同名且内容、包边界和成员路径一致时复用；同名但内容或依赖包不同，必须明确选择独立保留或跳过。独立保留不意味着可以在同一个工具同时启用。

执行会复核预览时的资料库修订、扫描范围、接管选项和内容指纹；发生变化需重新扫描。选择接管父 Skill 时，不能跳过其中被连带接管的嵌套成员。已有外部软链、工具内部目录及受保护内容不由此流程自动覆盖。

## Git 集合导入

在「Skill 库 → 添加 Skill → 从 Git 仓库导入」或「预设 → 从 Git 集合」粘贴仓库 / GitHub、GitLab 子目录链接，预览后同步。整仓共享资源保留在统一库，各 Skill 独立登记和软链分发；仅所选成员入库；可仅入库、创建预设或加入已有预设。定时更新默认关闭，需在来源中自行配置。

预设可自动加入后续新增成员，来源删除默认保留，确认移除只影响当前预设引用。详情见 [Git 集合导入](docs/git-collection-import.md)。

## 网络代理

在「设置 → 网络代理」选择跟随系统代理（默认）、直连或自定义 HTTP/HTTPS 代理。例如 `http://127.0.0.1:7897`。测试连接分别检查 GitHub HTTP 与 Git，不保存草稿；编辑完成并自动保存后，新发起的 Git、网站搜索、下载和 Skill 来源更新使用该配置。不会修改全局 Git 或系统代理，应用自身升级也使用该配置。暂不支持代理账号密码和 SOCKS 地址。系统模式读取 macOS 网络代理、Windows 当前用户 Internet Settings 或 Linux GNOME gsettings 的固定代理及绕过列表；PAC/自动发现会提示改用自定义。旧 inherit 配置按 system 处理，不再读取终端代理环境。Windows/Linux 尚未进行真机验证。

## CLI 示例

下面的路径均为示例，请替换为自己的目录。

```sh
skilldock init /absolute/path/to/skilldock-data
skilldock scan /absolute/path/to/existing-skills
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

新归集流程先调用 `preview_collection`，再把返回的 `revision`、`fingerprint` 与审阅选择交给 `collect_skills`，见[接口说明](docs/management-optimization-2026-09-25.md#接口)。原 `skilldock import` / `import_folder` 保留兼容，不提供新的同名审阅计划；需要新规则时使用上述两阶段接口。

```sh
skilldock exec '{"action":"save_preset","name":"开发工具","skillIds":["SKILL_ID"],"syncApplied":true}'
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
- 更新任务以来源为单位；订阅整包的跟随预设可自动加入、分发新增成员，旧库的固定目标保持原版本；删除与改名保留旧安装并持续提示审阅。单成员独立覆盖策略、自动合并本地修改和批量任务取消尚未实现。
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

每个预设在每个目标上的应用独立记录已应用修订和失败原因。新库及单份内容模式下，所有受管目标使用当前内容；旧库保留跟随策略和固定版本兼容。预设变更按目标事务同步跟随的应用，未应用的目标不自动分发；冲突保留原安装并支持重试。

库页面通过实际软链识别“外部已安装”，不会因此获得删除权限。应用预设可勾选接管，复用原安装名称并保存原链接指向；取消所有引用时恢复原链接，原实体不可用时阻止恢复并保留当前安装。嵌套在父软链下的成员只识别，不把子目录自动注册成新的分发目标。

成员入口必须精确命名为 `SKILL.md`；普通 `skill.md` 不算入口。已有 Skill 内的 `references/`、`assets/`、`scripts/` 只作为内部资源保存，不递归发现成员；没有 Skill 入口的包目录仍可递归扫描多个成员。此规则只影响发现，不删除或排除同步文件。

包内容保留共享结构。Git 元数据和本地 Python 环境不复制；发现 .venv 且没有可识别的依赖重建入口时，标记依赖待处理并阻止分发/接管。解决依赖后再次同步会重新检查，即使内容摘要未变化。不会自动执行包内脚本或重建运行环境。

已登记包和旧本地引用禁止移动式归集。旧归集目录可从导入界面“检查旧归集迁移”查看实体、入口和待恢复状态；不自动还原文件、不覆盖库版本。需要使用现有备份恢复功能审阅恢复完整来源后再登记同步包。统一库不能迁入包或本地来源目录。旧模式使用 schemaVersion 2；新建库和显式简化后的库使用 schemaVersion 3，旧程序会拒绝新状态而不会静默丢失关系。

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

预设再次同步原包会识别新增、内容变更和来源移除。移除成员默认保留旧内容，勾选确认并保存后仅解除该预设引用，其他引用及旧库的固定目标保留。完整行为与验证见 [预设包差异同步](docs/package-diff-sync.md)。

## 移除分发目标

在目标详情底部选择「移除目标」，确认后移除目标配置、分发记录和该目标的预设应用关系，停止后续同步。实际目录、文件、软链、中央库和预设保留。操作校验确认时的资料库版本，避免在后台状态变化后继续执行旧确认。

已移除目录仍作为只读引用保护范围参与中央库清理检查；不会再显示为分发目标。若保留链接仍指向旧中央库，迁移会拒绝并提示处理依赖，防止移除旧库后链接失效。可重新添加该目录。


## 本地来源

侧栏「本地来源」统一处理本地添加，Skill 库和发现页的「本地文件夹（复制入库）」也进入此流程。扫描后选择成员，将完整包内容复制到统一库，按所选成员登记与软链分发；原始文件保留。原目录中的共享资源与相对结构保留，不对用户仓库执行 Git pull，也不建立远程定时更新任务。

同目录重复添加复用本地来源。原目录修改不会立即影响工具，需要重新扫描并确认同步；扫描后内容或资料库状态变化时拒绝旧确认。同名且内容或包结构不同需要明确选择独立保留；同一目标只能启用一个相同声明名的实现。来源分发使用独立引用，不创建隐藏预设，也不会取消其他手动或预设引用。

保存后按该来源的已有分发关系添加或取消成员，并把仍被手动分发或预设引用的同一成员同步到当前内容；冲突时回滚该次保存。原目录缺失的旧成员保留最后入库内容供检查。移除配置保留原文件和库记录；异常成员记录不再阻止按已有引用取消分发或移除配置，链接所有权与内容校验仍生效。

旧 `local_reference` 仍可读取和管理，迁入前实体仍在库外，修改直接生效。页面明确显示「迁入统一库」并要求确认复制及链接切换；迁入后为 `local_managed`，原目录保留，此后需手动同步。旧库需先启用单份内容模式；成员被多个旧引用共享、旧成员缺失或链接校验失败时会拒绝迁入，需先整理关系。升级不会自动迁移现有用户库。

### 切换已有分发的来源

同名 Skill 从 Git 或本地重新导入后，可以通过 Skill 库的「选择来源分发」，或对应预设／本地来源的分发入口，预览并确认切换。预览展示旧、新来源、实体路径、版本、内容差异状态与现有引用；确认选项默认关闭。

旧安装仍被其他预设或本地来源引用时会阻止切换，并列出需要先解除的引用。确认后只改变所选目标的链接，旧实体、归集备份和其他工具保持原样。详见 [分发来源切换](docs/specs/distribution-source-switch.md)。


## 应用版本发布与更新

应用默认检查本仓库 GitHub Releases 的 `latest.json`。正式发布时，流水线将签名公钥内置到安装包，用户无需填写更新配置；本地未配置公钥的开发构建会明确提示无法在线升级。浏览器演示不会模拟可安装的新版本。

推送正式版本标签（例如 `v0.2.0`）触发 `.github/workflows/release.yml`：版本校验 → 草稿 Release → macOS Apple Silicon/Intel、Windows x64、Linux x64 构建 → 实际更新包签名校验 → 完整清单校验 → 发布。全部构建成功前不会进入用户更新渠道。

首次使用需由仓库维护者配置更新签名密钥和公钥，详见 [应用发布与更新](docs/app-release.md)。私有仓库需单独提供可访问的发布渠道，此默认流水线面向公开仓库。macOS 签名与公证凭据独立于更新包签名。
