# Agent 统一安装入口

各工具通过 SkillDock CLI 入库并软链分发，桌面端和 CLI 共用同一个配置与核心。此接入针对 Agent 对话中的安装操作；不拦截工具内置市场或第三方安装器。

## 配置一次

在项目根目录安装 CLI 到 Cargo 的 bin 目录，并确保各工具启动的终端 PATH 包含该目录：

```sh
cargo install --path crates/skilldock-cli --locked
skilldock --version
skilldock list --json
```

未设置资料库时，先在桌面端设置统一目录，或执行 `skilldock init /绝对路径/统一库`。已有配置时不重复初始化。

用 CLI 自身安装仓库提供的接入 Skill：

```sh
skilldock install ./integrations/skilldock --to codex --to workbuddy --json
```

该 Skill 也由统一库管理。工具重新加载 Skill 后可自动选择它；不同工具的重新加载机制需分别验证。更多工具重复追加 `--to`，不要默认分发给所有工具。

在各工具支持的用户规则入口加入下面这段规则（Codex 可加入用户级 AGENTS.md；其他工具按其当前规则设置入口配置）。保留已有规则，只追加此段：

> 安装、更新或取消分发 Agent Skill 时，优先使用 skilldock Skill 与 SkillDock CLI，统一记录来源和工具分发关系。默认只安装到当前工具或用户指定的目标。CLI 不可用、目标不明确或发生同名冲突时说明具体原因，不自动改用其他安装器或覆盖目录。插件市场的完整插件使用工具自己的插件入口。用户明确要求其他安装方式时遵从用户指令。

CLI 不在工具 PATH 时，在规则中补充实际可执行文件的绝对路径；隔离配置实例还需记录相应 `--config-dir`。本仓库不覆盖用户规则文件。

## 命令约定

```sh
skilldock install ./my-skills --to codex --to workbuddy
skilldock install https://github.com/owner/repo --subdir skills --select skills/example --to codex
skilldock install owner/skill --site clawhub --to workbuddy
```

- 默认根据输入区分来源：绝对路径、`./`、`../`、`~/` 是本地目录；HTTPS、SSH、`user@host:path` 是 Git；其余为网站标识。可用 `--from local|git|catalog` 明确覆盖。`--site` 明确指定网站来源。HTTP 和 file URL 会交给 Git 校验并拒绝；本地 Git 仓库使用绝对路径加 `--from git`。
- `--to` 接受工具标识或已登记的目标 ID，可重复指定。工具标识只指用户级目录；项目级目录先用 `target` 登记，再使用目标 ID。
- 已登记的用户级目标优先。没有登记时读取设置中的工具目录：存在唯一目录则使用它，都不存在则创建第一候选目录。多个实际目录或多个用户级目标时拒绝猜测，要求明确目标 ID。WorkBuddy 的 `.workbuddy` 与 `.codebuddy` 同时存在时适用此规则。
- 不传 `--to` 仅入库。`--select` 适用于本地和 Git 集合；本地相对于来源根、Git 相对于仓库根。省略时选择全部可用成员。来源目录保持原样，完整依赖包保存在统一库。
- 沿用核心的修订检查、文件锁与冲突检查。安装不自动替换其他来源，不自动接管未知目录。

## 返回值与重试

统一安装命令返回 `{status, source, stage, imported, skillIds, targets, error}`。每个目标包含 ID、路径、状态、错误和分发计划。其他命令保持原输出格式。

`succeeded` 退出 0；`failed` 或 `partial` 退出 1。安装报告始终写到 stdout（`--json` 为单行），底层日志仍写 stderr。这是 `install` 相比旧版直接返回 Snapshot 的输出变化，已有脚本应改读报告，并使用 `list --json` 获取完整状态；旧 API `exec '{"action":"install_catalog",...}'` 不变。

入库及各目标分发分别提交，整次调用不是跨目标的单一事务。失败不会自动删除已入库内容或撤销成功目标。分发失败时，使用报告的 `skillIds` 与目标 ID 调用 `plan`，确认无错误后将最新 revision 传给 `distribute`；只重试失败目标。目标登记失败时，先修复路径或权限再登记目标。

来源重新导入可能同步该来源已有内容及受管工具。只重试分发时不要重复安装。安装前或入库阶段报错时，可用 `list`、`diagnose` 检查实际状态及事务记录。

## 本轮验证（2026-09-27）

- 静态检查：CLI 编译、改动文件 rustfmt 与 diff 检查通过。
- 自动测试：CLI 11 项（含真实子进程、隔离文件系统、Git 本地仓库和网站来源身份解析）；开发工作区核心回归 112 项通过，其中包含此前未提交的 3 项归集分类测试。
- 构建：本地调试 CLI 构建通过；未制作新的桌面安装包。
- 真实验证：隔离目录确认跨工具共享实体、依赖文件保留、重复安装复用绑定、部分失败保护外部目录、逐次选成员保留原分发引用。网站真实联网安装与各工具会话中的 Skill 自动触发尚未验收。
- Skill 校验：已通过核心真实导入与 YAML 元数据解析；skill-creator 自带 Python 校验器因本机缺少 PyYAML 未能完成，未为此安装全局 Python 依赖。
- Git 与发布：本次发布目标为 v0.3.6；用户已有归集相关改动保留，不纳入此版本。标签推送只触发既有发布流水线，不代表安装包已公开。
