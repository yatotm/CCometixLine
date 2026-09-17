# cclean — Claude Code 环境一键备份 / 清理 / 重装

单文件 Python 脚本（仅标准库，Python 3.8+），支持 Windows (cmd / PowerShell)、macOS、Linux。

## 一键运行（无需下载仓库）

固定链接指向本仓库 `master` 分支下的 `cclean/` 目录，任何系统只要装了 Python 3 即可直接执行：

```powershell
# Windows（PowerShell，cmd 里先输入 powershell 再执行）
irm https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.ps1 | iex

# 带参数：
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.ps1))) run -y --proxy http://127.0.0.1:7891
```

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.sh | sh

# 带参数：
curl -fsSL https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.sh | sh -s -- run -y --proxy http://127.0.0.1:7891

# 非交互子命令也可以直接喂给 Python（TUI 模式需要终端，请用上面的 cclean.sh）
curl -fsSL https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.py | python3 - run --dry-run -y
```

启动器只做三件事：把 `cclean.py` 下载到临时目录、找到 Python 3、带着当前终端运行它，退出后删除临时文件。`CCLEAN_RAW` 环境变量可指向其他分支或提交，例如 `https://raw.githubusercontent.com/yatotm/CCometixLine/<commit>/cclean`。

## 用法

```bash
# 交互式 TUI（推荐）
python cclean.py            # Windows 也可直接双击 cclean.cmd

# 非交互一键执行（默认全部步骤开启，用 --no-xxx 关闭）
python cclean.py run -y --proxy http://127.0.0.1:7891
python cclean.py run -y --no-backup --claude-version 2.1.267 --install-method native

# 先预览会做什么，不做任何更改
python cclean.py run --dry-run -y

# OAuth 登录完成后，单独追加 CLAUDE_CODE_USE_BEDROCK=1 / CLAUDE_CODE_USE_VERTEX=1
python cclean.py post-login

# 从备份恢复配置文件（~/.claude、~/.claude.json*、Windows 用户环境变量）
python cclean.py restore --from ~/claude-backup-20260917-180000

# 在本机重新提取 settings.json 模板（只保留隐私 / 自动更新 / 子 Agent 相关键）
python cclean.py export-template -o template.json
python cclean.py run --template template.json
```

## TUI 选项

| 选项 | 默认 | 说明 |
| --- | --- | --- |
| 备份现有配置 | 开 | 复制 `~/.claude`、`~/.claude.json*`、含相关环境变量的 shell rc / PowerShell profile、Windows 注册表环境变量、macOS Keychain 凭据到 `~/claude-backup-<时间>` |
| 备份时排除会话记录/缓存 | 关 | 跳过 projects / file-history / shell-snapshots 等大目录 |
| 清理现有安装与全部配置 | 开 | 结束 claude 进程；`npm uninstall -g`、brew / winget 卸载、删除原生安装 (`~/.local/bin/claude`, `~/.local/share/claude`)、删除 `~/.claude` 与 `~/.claude.json*`、macOS Keychain 条目 |
| 清理 ANTHROPIC_* / CLAUDE_* 用户级环境变量 | 开 | Windows：删除 HKCU\Environment 中的变量并广播刷新，注释 PowerShell profile 中的相关行；Unix：注释 `.zshrc` / `.bashrc` 等中的 export 行（前缀 `# cclean-disabled:`）。系统级 (HKLM) 变量只提示不删 |
| 安装 Claude Code | 开 | 版本默认 `2.1.267`，可填 `latest` / `stable`。`native` 方式复刻官方安装器：下载指定版本二进制 → SHA256 校验 → `claude install <版本>`，下载走 TUI 里的代理；`npm` 方式执行 `npm install -g @anthropic-ai/claude-code@<版本>` |
| 安装 ccline | 开 | 默认包 `@yatotm/ccline`。安装前先 `npm uninstall -g` 其他版本（如原版 `@cometix/ccline`）并删除旧的 `~/.claude/ccline/ccline` 二进制，再 `npm install -g`，随后写入本机 `~/.claude/ccline/config.toml`（cometix 主题、usage 段 180 秒自动刷新）。图标模式可选 `nerd_font` / `plain` |
| 代理 | `http://127.0.0.1:7891` | 同时用于脚本自身下载、npm、以及写入 settings.json 的 `HTTP_PROXY` / `HTTPS_PROXY`；留空则不设置 |
| 预置 hasCompletedOnboarding | 开 | 在 `~/.claude.json` 写入 `hasCompletedOnboarding: true`，跳过首启对 api.anthropic.com 的连通性探测，规避 Windows 上的 `Unable to connect to Anthropic services / ERR_BAD_REQUEST` |
| 安装后立即登录 | 开 | 运行 `claude auth login --claudeai`，结束后用 `claude auth status --json` 确认 |
| 登录后追加 Bedrock/Vertex 参数 | 关 | 仅在 `claude auth status` 报告已登录时才写入，避免影响 OAuth 认证路由；未登录时提示稍后运行 `post-login` |
| 仅预览 | 关 | 打印每一步将做什么，不做任何更改 |

## 写入的 settings.json

```json
{
  "env": {
    "DISABLE_AUTOUPDATER": "1",
    "DISABLE_TELEMETRY": "1",
    "DISABLE_ERROR_REPORTING": "1",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
    "CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH": "1",
    "HTTP_PROXY": "http://127.0.0.1:7891",
    "HTTPS_PROXY": "http://127.0.0.1:7891",
    "NO_PROXY": "localhost,127.0.0.1,::1"
  },
  "includeCoAuthoredBy": false,
  "permissions": { "deny": ["ListAgents"] },
  "crossSessionInbound": "refuse",
  "statusLine": { "type": "command", "command": "~/.claude/ccline/ccline", "padding": 0 }
}
```

不包含 model / hooks / plugins / MCP 等本机个性化配置。

## 实现细节

- `native` 安装不直接调用官方 install.sh / install.ps1（它们用 curl / Invoke-WebRequest，Windows 下代理行为不可控），而是用 Python 复刻其流程：读取 `manifest.json` → 下载指定版本二进制到 `~/.local/share/claude/versions/<版本>` → SHA256 校验 → 从该位置运行 `claude install <版本>`。二进制已在版本目录时，`claude install` 不会再下载一次（否则它会自行重下 190 MB，经代理常被掐断），只生成启动器并写入 shell 集成。重复运行时校验一致即跳过下载。
- `claude install` 会自动卸载 npm 全局的 `@anthropic-ai/claude-code`，这是官方安装器的行为。
- ccline 的 npm 包名由 `DEFAULT_CCLINE_PKG` 常量决定（`@yatotm/ccline`），TUI 里也可临时改成其他包或指定版本。`CCLINE_KNOWN_PKGS` 中列出的包在清理和安装前都会被卸载。

## 注意

- 不要在 Claude Code 会话内部运行清理（会杀掉当前会话），脚本检测到 `CLAUDECODE=1` 时会拒绝，除非加 `--force`。
- Windows 安装完成后如提示找不到 `claude`，重新打开终端即可（脚本已把 `%USERPROFILE%\.local\bin` 加入用户 PATH）。
- VS Code 扩展 / JetBrains 插件 / Claude Desktop 会重建 `~/.claude`，如需彻底清理请先卸载它们。
- 日志写入备份目录下的 `cclean.log`（未备份时写入 `~/cclean.log`）。
