---
name: skilldock
description: 通过 SkillDock CLI 安装、更新、分发和取消分发 Agent Skills，统一记录来源与工具目录。用户要求安装 Skill、从 Git 或网站添加 Skill、同步给 Codex 或 WorkBuddy 等工具时使用。插件市场的完整插件安装仍使用对应工具的插件入口。
---

# 通过 SkillDock 管理 Skill

用户要求安装 Skill 时，调用 `skilldock` 完成入库与分发。工具目录中的受管 Skill 是软链，内容在统一库；直接复制、删除或修改受管软链内文件会绕开管理记录。

## 确认入口与目标

先用 `skilldock --version` 和 `skilldock list --json` 确认 CLI 与统一库可用。CLI 不在 PATH 时使用用户配置的绝对路径；不能推断开发仓库所在位置。尚未初始化时依据用户指定的统一目录执行 `skilldock init /绝对路径`。不要创建第二份资料库来绕过现有配置错误。

目标只选用户指定的工具；用户在当前工具中说“安装”且上下文明确时，默认当前工具。不要默认分发到所有工具。用户级工具标识包括 `codex`、`workbuddy`、`claude`、`cursor`、`opencode`、`gemini`、`openclaw`、`trae`、`antigravity`、`agents`，实际以配置中的工具档案为准。项目级安装使用 `list` 中的目标 ID；未登记时用 `skilldock target '项目名称' /项目/技能目录 --tool 工具标识 --scope project` 登记。

## 安装

```sh
skilldock install '目录站点中的标识' --site clawhub --to codex --json
skilldock install 'https://github.com/owner/repo' --subdir skills --to workbuddy --json
skilldock install '/绝对路径/技能包' --to codex --to workbuddy --json
```

本地相对路径以 `./` 开头，或明确传 `--from local`。网站标识可含斜杠，用 `--site` 指定网站；HTTPS 来源默认按 Git 解析，不把普通网页当作网站标识。集合默认收录全部成员；只安装其中一个时用 `--select 包根下的成员相对路径`，可重复指定。Git 子目录筛选的成员路径仍相对于仓库根。保留完整依赖包，不单独复制 SKILL.md。

不传 `--to` 仅入库。普通目录中的本地来源会复制到库中，不接管或删除原目录。已有工具目录归集使用桌面端的预览确认流程。

重新安装同一来源可能同步来源内容及其他受管工具。用户只是要把已入库的 Skill 加到另一个工具时，直接使用其现有 ID 调用下面的 plan / distribute，避免重新导入来源。

退出码 0 且 `status=succeeded` 才表示本次所有步骤成功；`partial` 表示已入库、部分后续步骤失败。检查 `skillIds`、`targets`、`error`，向用户报告哪些目标成功。重试分发使用返回的 ID，避免重新下载或更新来源：

```sh
skilldock plan --skill SKILL_ID --target TARGET_ID --json
skilldock distribute --skill SKILL_ID --target TARGET_ID --revision PLAN_REVISION --json
```

plan 返回冲突或替换要求时，说明受影响的旧来源并由用户选择，使用桌面端处理；不要自动覆盖、删除旧目录或添加接管参数。修订过期可重新 plan 一次；仍失败就保留已完成结果并报告原因。不要因 CLI 失败而默默切换到其他安装器。

## 更新与取消分发

从 `list` 获取真实来源 ID，先 `skilldock update SOURCE_ID --json` 检查；用户要求应用更新时使用 `--apply`。同一来源可能包含多个成员，更新会同步其他已管理工具中的当前内容，执行前说明影响范围。

用户只要求从某工具卸载时，在 `list` 的 bindings 中找到对应绑定，执行 `skilldock revoke --binding BINDING_ID --json`，再核对结果。预设或本地来源仍有引用时可能保留链接，应说明原因。用户要求彻底从库中删除、归集现有目录或处理冲突时，使用桌面端相应预览确认入口。

接入规则仅引导 Agent 对话中的操作，不能拦截工具内置安装按钮、插件市场或用户自行运行的脚本。
