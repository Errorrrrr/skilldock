# 应用发布与更新

## 使用方式

设置 → 应用更新显示实际安装版本。正式构建默认读取：

`https://github.com/Errorrrrr/skilldock/releases/latest/download/latest.json`

点击检查后展示新版本与发布说明；确认安装后显示下载、签名验证、安装、重启阶段。失败可以重试安装，也可重新检查。重复点击、旧预览、来源或代理配置变更、预览超过 30 分钟都会受到保护。失败检查会撤销旧的安装预览。

安装使用本次检查得到的更新包和签名，不会再次检查后悄悄安装另一版本。下载前占用文件事务锁并暂停来源调度，存在运行任务或待恢复事务时拒绝安装；失败恢复原暂停状态。成功安装允许应用正常重启。Skill 库与分发配置位于独立数据目录，更新流程不迁移或删除这些数据。

应用更新与 Skill 更新独立。本版应用更新由用户手动检查、确认安装，没有启动自动检查或静默自动安装。请求使用设置中的网络代理，检查限时 30 秒、下载限时 15 分钟。

高级设置支持自定义 HTTPS 地址与公钥，必须成对填写或同时清空。恢复官方源后需要保存设置。正式公钥通过发布时配置注入，开发构建没有公钥时禁止在线升级；浏览器演示明确不提供真实更新。

## 首次配置发布签名

维护者在自己的机器上生成一次长期使用的 Tauri 更新签名密钥：

```sh
pnpm tauri signer generate -w ~/.tauri/skilldock-release.key
```

妥善备份私钥与密码，不提交私钥，不发送到聊天中。该公钥对应的已安装客户端只能接受匹配私钥签名的后续更新，因此不要每次发布重新生成密钥。

在 GitHub 仓库 Settings → Secrets and variables → Actions 配置：

| 类型 | 名称 | 内容 |
|---|---|---|
| Secret | `TAURI_SIGNING_PRIVATE_KEY` | `skilldock-release.key` 文件完整内容 |
| Secret | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 生成时的密码；无密码可留空 |
| Variable | `SKILLDOCK_UPDATER_PUBLIC_KEY` | `skilldock-release.key.pub` 文件完整内容 |

流水线仅将公钥写入构建配置，私钥通过构建环境传入。构建结束后用内置公钥对真实更新包进行签名验证，公私钥不匹配不会发布。

macOS 默认使用 ad-hoc 包签名，CI 使用 `codesign --verify --deep --strict` 校验完整应用包；它不等于 Apple Developer 身份签名或公证，首次打开时可能需要在系统“隐私与安全性”中确认允许打开。如需正式签名与公证，另行配置 `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_SIGNING_IDENTITY`、`APPLE_ID`、`APPLE_PASSWORD`、`APPLE_TEAM_ID`。更新包签名与系统代码签名是两套不同用途的凭据。Windows 当前提供 NSIS 更新包，系统代码签名尚未配置。

## 发布新版本

1. 将 `package.json`、`src-tauri/tauri.conf.json`、根目录 `Cargo.toml` 的应用版本同步修改（例如下一个补丁版本 `0.2.2`）。执行 `cargo check -p skilldock-desktop` 更新 `Cargo.lock` 中的工作区包版本。
2. 运行 `node scripts/release.mjs check`、`node --test tests/release.test.mjs`、`pnpm build` 和相关 Rust 测试。
3. 提交并推送代码，再创建并推送同名版本标签：

```sh
git tag v0.2.2
git push origin v0.2.2
```

4. 查看 Actions 中 `Release SkillDock`。当前仅接受 `v主版本.次版本.补丁版本` 正式标签；预发布标签会被拒绝。
5. 首次安装下载对应安装包。包含正式公钥的已安装版本可直接在应用内更新。

普通推送 main 不触发发布。仓库已配置长期更新签名 Secret 与公钥 Variable；后续发布复用现有凭据，不要重新生成密钥。所有平台构建和校验通过后，流水线自动公开 Release。

## 流水线约束

- 同一时间只处理一个版本发布；同一版本的构建串行合并 `latest.json`，避免平台条目互相覆盖。
- 每个系统/架构生成安装包、更新包和 `.sig`。macOS 使用 `.app.tar.gz`，Windows 使用 NSIS `.exe`，Linux 使用 AppImage/deb。
- Tauri Action 生成的 GitHub API 附件地址在发布前转换为当前版本标签的公开下载地址。草稿 API 中的 `untagged-*` 地址不可直接用于正式清单；更新说明取自 Release 正文。
- 公开前要求四个平台均完整、版本一致、更新包和签名附件存在。所有平台签名验证通过后才发布。
- 任一构建/验证失败保留草稿，可重跑失败任务；草稿通过 Release 列表恢复，避免按标签查询漏掉草稿后重复创建。已公开的同版本 Release 禁止覆盖，应发布新版本。
- 默认更新渠道需要公开 Release，私有仓库不能直接使用此无需认证的地址；应先另行配置公开发布仓库或认证转发服务，不在客户端内置 GitHub 私有令牌。
- 首个签名版本需手动安装。当前开发版没有官方公钥，不会自动信任未来发布的公钥。

## 验证与边界

2026-09-15 已发布 [v0.2.1](https://github.com/Errorrrrr/skilldock/releases/tag/v0.2.1)，仓库为公开状态。[GitHub Actions](https://github.com/Errorrrrr/skilldock/actions/runs/34932230309) 的准备、四个平台构建与最终发布全部通过。

- 本地测试：桌面 Rust 测试 18 项、系统代理定向测试 3 项、发布脚本测试 4 项通过；使用临时 Tauri 密钥验证有效签名与篡改拒绝。前端类型检查与生产构建通过。
- 交互验证：浏览器模拟接口验证版本显示、更新说明转义、取消、下载进度、切页保留、失败重试、配置校验和固定窗口框架。此验证不代表原生安装升级验收。
- CI 构建：macOS Apple Silicon、macOS Intel、Windows x64 NSIS、Linux x64 AppImage/deb 全部生成成功；各平台执行 4 项更新保护测试、4 项发布脚本测试，并用内置公钥验证实际更新包。两个 macOS 应用包额外通过完整包签名校验。
- 公开下载：无需登录成功获取官方 `latest.json`，其 9 个平台/安装格式条目对应 5 个更新包（两个 Mac 包、Windows NSIS、Linux AppImage 与 deb）；全部公开下载成功，清单签名与 `.sig` 附件一致，且通过长期公钥验签。
- 原生包复核：下载的 Apple Silicon `v0.2.1` 成品版本正确，更新签名及 `codesign --verify --deep --strict` 均通过。
- 尚未验证：各系统安装器实际安装、已安装旧版到新版的原生升级与重启；Windows/Linux 桌面真实操作。未配置 Apple Developer 公证及 Windows 系统代码签名。

首次 `v0.2.0` 草稿检查发现两个发布问题：GitHub 草稿附件使用临时 `untagged-*` 下载路径；macOS 二进制的 linker 签名不能代替完整应用包签名。已修复草稿地址规范化和草稿重试，增加 ad-hoc 包签名及 CI 校验，并发布 `v0.2.1`。旧标签未重写，`v0.2.0` 保持未公开草稿。
