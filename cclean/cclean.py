#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
cclean - Claude Code 环境一键备份 / 清理 / 重装工具 (Windows / macOS / Linux)

仅依赖 Python 3.8+ 标准库。用法:
  python cclean.py                     交互式 TUI
  python cclean.py run [选项]          非交互执行 (见 --help)
  python cclean.py post-login          OAuth 登录后追加 Bedrock/Vertex 参数
  python cclean.py export-template     从本机 settings.json 提取模板 (隐私/更新/子Agent 相关键)
  python cclean.py restore --from DIR  从备份目录恢复配置文件
"""
from __future__ import annotations

import argparse
import ctypes
import glob
import hashlib
import json
import os
import platform
import re
import shutil
import stat
import subprocess
import sys
import time
import unicodedata
import urllib.error
import urllib.request
from typing import Dict, List, Optional, Tuple

WIN = sys.platform == "win32"
MAC = sys.platform == "darwin"

DEFAULT_CLAUDE_VERSION = "2.1.267"
DEFAULT_PROXY = "http://127.0.0.1:7891"
# ccline 的 npm 包 (可带 @版本); 安装前会先卸载 CCLINE_KNOWN_PKGS 中的其他版本
DEFAULT_CCLINE_PKG = "@yatotm/ccline"

CLAUDE_RELEASES = "https://downloads.claude.ai/claude-code-releases"
CLAUDE_NPM_PKG = "@anthropic-ai/claude-code"
CCLINE_KNOWN_PKGS = ("@cometix/ccline", "@yatotm/ccline")  # 清理时一并卸载

ENV_PREFIXES = ("ANTHROPIC_", "CLAUDE_")  # 需清理的用户级环境变量前缀
TEMPLATE_ENV_PREFIXES = (
    "DISABLE_", "CLAUDE_CODE_DISABLE_", "CLAUDE_CODE_MAX_SUBAGENT",
    "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY",
)
TEMPLATE_TOP_KEYS = ("includeCoAuthoredBy", "crossSessionInbound")
BULKY_DIRS = (
    "projects", "file-history", "shell-snapshots", "debug", "paste-cache",
    "session-env", "sessions", "cache", "todos", "statsig", "downloads", "daemon",
)

# ---- 模板: 从本机 settings.json 提取, 仅保留隐私 / 自动更新 / 子Agent 相关配置 ----
SETTINGS_TEMPLATE: Dict = {
    "$schema": "https://json.schemastore.org/claude-code-settings.json",
    "env": {
        "DISABLE_AUTOUPDATER": "1",
        "DISABLE_TELEMETRY": "1",
        "DISABLE_ERROR_REPORTING": "1",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
        "CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH": "1",
    },
    "includeCoAuthoredBy": False,
    "permissions": {"deny": ["ListAgents"]},
    "crossSessionInbound": "refuse",
}

# ---- 本机 ~/.claude/ccline/config.toml 原样内嵌 (usage 段 cache_duration=180 即自动刷新) ----
CCLINE_CONFIG_TOML = '''theme = "cometix"

[style]
mode = "nerd_font"
separator = " | "

[[segments]]
id = "model"
enabled = true

[segments.icon]
plain = "\U0001f916"
nerd_font = "\ue26d"

[segments.colors.icon]
c16 = 14

[segments.colors.text]
c16 = 14

[segments.styles]
text_bold = true

[segments.options]

[[segments]]
id = "directory"
enabled = true

[segments.icon]
plain = "\U0001f4c1"
nerd_font = "\U000f024b"

[segments.colors.icon]
c16 = 11

[segments.colors.text]
c16 = 10

[segments.styles]
text_bold = true

[segments.options]

[[segments]]
id = "git"
enabled = true

[segments.icon]
plain = "\U0001f33f"
nerd_font = "\U000f02a2"

[segments.colors.icon]
c16 = 12

[segments.colors.text]
c16 = 12

[segments.styles]
text_bold = true

[segments.options]
show_sha = false

[[segments]]
id = "context_window"
enabled = true

[segments.icon]
plain = "\u26a1\ufe0f"
nerd_font = "\uf49b"

[segments.colors.icon]
c16 = 13

[segments.colors.text]
c16 = 13

[segments.styles]
text_bold = true

[segments.options]

[[segments]]
id = "usage"
enabled = true

[segments.icon]
plain = "\U0001f4ca"
nerd_font = "\U000f0a9e"

[segments.colors.icon]
c16 = 14

[segments.colors.text]
c16 = 14

[segments.styles]
text_bold = false

[segments.options]
api_base_url = "https://api.anthropic.com"
cache_duration = 180
timeout = 2

[[segments]]
id = "cost"
enabled = true

[segments.icon]
plain = "\U0001f4b0"
nerd_font = "\ueec1"

[segments.colors.icon]
c16 = 3

[segments.colors.text]
c16 = 3

[segments.styles]
text_bold = true

[segments.options]

[[segments]]
id = "session"
enabled = true

[segments.icon]
plain = "\u23f1\ufe0f"
nerd_font = "\U000f19bb"

[segments.colors.icon]
c16 = 2

[segments.colors.text]
c16 = 2

[segments.styles]
text_bold = true

[segments.options]

[[segments]]
id = "output_style"
enabled = true

[segments.icon]
plain = "\U0001f3af"
nerd_font = "\U000f12f5"

[segments.colors.icon]
c16 = 6

[segments.colors.text]
c16 = 6

[segments.styles]
text_bold = true

[segments.options]

'''


# =============================================================================
# 基础工具
# =============================================================================
class C:
    """ANSI 颜色; Windows 老控制台不支持时自动置空."""
    RESET = "\x1b[0m"; BOLD = "\x1b[1m"; DIM = "\x1b[2m"; REV = "\x1b[7m"
    RED = "\x1b[31m"; GREEN = "\x1b[32m"; YELLOW = "\x1b[33m"; CYAN = "\x1b[36m"

    @classmethod
    def disable(cls):
        for k in ("RESET", "BOLD", "DIM", "REV", "RED", "GREEN", "YELLOW", "CYAN"):
            setattr(cls, k, "")


def enable_vt() -> bool:
    if not WIN:
        return True
    try:
        k32 = ctypes.windll.kernel32
        h = k32.GetStdHandle(-11)
        mode = ctypes.c_uint32()
        if not k32.GetConsoleMode(h, ctypes.byref(mode)):
            return False
        return bool(k32.SetConsoleMode(h, mode.value | 0x0004))
    except Exception:
        return False


LOG_LINES: List[str] = []


def log(msg: str, level: str = "info"):
    color = {"ok": C.GREEN, "warn": C.YELLOW, "err": C.RED, "step": C.CYAN + C.BOLD}.get(level, "")
    prefix = {"ok": "  [OK] ", "warn": "  [!!] ", "err": "  [XX] ", "step": "\n==> ", "info": "     "}.get(level, "")
    print(f"{color}{prefix}{msg}{C.RESET}", flush=True)
    LOG_LINES.append(f"{time.strftime('%H:%M:%S')} {level.upper():5} {msg}")


def home() -> str:
    return os.path.expanduser("~")


def config_dir() -> str:
    return os.environ.get("CLAUDE_CONFIG_DIR") or os.path.join(home(), ".claude")


def lp(p: str) -> str:
    """Windows 长路径前缀, 避免 MAX_PATH 限制."""
    if WIN and not p.startswith("\\\\?\\") and os.path.isabs(p):
        return "\\\\?\\" + os.path.abspath(p)
    return p


def tilde(p: str) -> str:
    h = home()
    return "~" + p[len(h):] if p.startswith(h) else p


def dw(s: str) -> int:
    return sum(2 if unicodedata.east_asian_width(ch) in "WF" else 1 for ch in s)


def pad(s: str, width: int) -> str:
    return s + " " * max(0, width - dw(s))


def human(n: int) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if n < 1024 or unit == "GB":
            return f"{n:.1f} {unit}" if unit != "B" else f"{n} B"
        n /= 1024
    return f"{n:.1f} GB"


def dir_size(path: str) -> int:
    total = 0
    for root, _dirs, files in os.walk(lp(path)):
        for f in files:
            try:
                total += os.lstat(os.path.join(root, f)).st_size
            except OSError:
                pass
    return total


def child_env(proxy: str = "") -> Dict[str, str]:
    """子进程环境: 去掉 ANTHROPIC_*/CLAUDE_* (避免干扰登录), 注入代理."""
    env = {k: v for k, v in os.environ.items()
           if k == "CLAUDE_CONFIG_DIR" or (not k.upper().startswith(ENV_PREFIXES) and k != "CLAUDECODE")}
    if proxy:
        for k in ("HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"):
            env[k] = proxy
        env.setdefault("NO_PROXY", "localhost,127.0.0.1,::1")
    return env


def run(cmd: List[str], capture=True, check=False, timeout=None, env=None, cwd=None) -> subprocess.CompletedProcess:
    kw = dict(env=env or child_env(), cwd=cwd, timeout=timeout)
    if capture:
        kw.update(stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                  encoding="utf-8", errors="replace")
    cp = subprocess.run(cmd, **kw)
    if check and cp.returncode != 0:
        raise RuntimeError(f"命令失败 ({cp.returncode}): {' '.join(cmd)}\n{(cp.stderr or '').strip()}")
    return cp


def which(name: str) -> Optional[str]:
    return shutil.which(name)


def load_json(path: str) -> Optional[Dict]:
    try:
        with open(lp(path), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def save_json(path: str, data: Dict):
    os.makedirs(lp(os.path.dirname(path)), exist_ok=True)
    tmp = path + ".tmp"
    with open(lp(tmp), "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        f.write("\n")
    os.replace(lp(tmp), lp(path))


def rm_force(path: str) -> bool:
    """删除文件或目录; 处理只读文件 (Windows 下 .git 对象常见)."""
    p = lp(path)
    if not os.path.lexists(p):
        return False

    def _fix(func, target, _exc):
        try:
            os.chmod(target, stat.S_IWRITE | stat.S_IREAD)
            func(target)
        except OSError:
            pass

    if os.path.isdir(p) and not os.path.islink(p):
        if sys.version_info >= (3, 12):
            shutil.rmtree(p, onexc=lambda f, t, e: _fix(f, t, e))
        else:
            shutil.rmtree(p, onerror=_fix)
    else:
        try:
            os.remove(p)
        except PermissionError:
            os.chmod(p, stat.S_IWRITE)
            os.remove(p)
    return not os.path.lexists(p)


def confirm(prompt: str) -> bool:
    try:
        return input(f"{C.YELLOW}{prompt} [y/N] {C.RESET}").strip().lower() in ("y", "yes")
    except (EOFError, KeyboardInterrupt):
        return False


# =============================================================================
# 网络下载
# =============================================================================
def http_get(url: str, proxy: str = "", dest: Optional[str] = None, timeout: int = 60, label: str = "") -> bytes:
    handlers = [urllib.request.ProxyHandler({"http": proxy, "https": proxy})] if proxy else []
    opener = urllib.request.build_opener(*handlers)
    req = urllib.request.Request(url, headers={"User-Agent": "cclean/1.0"})
    with opener.open(req, timeout=timeout) as r:
        if dest is None:
            return r.read()
        total = int(r.headers.get("Content-Length") or 0)
        done = 0
        os.makedirs(lp(os.path.dirname(dest)), exist_ok=True)
        with open(lp(dest), "wb") as f:
            while True:
                chunk = r.read(1 << 20)
                if not chunk:
                    break
                f.write(chunk)
                done += len(chunk)
                if total:
                    print(f"\r     下载 {label or os.path.basename(dest)}: {done * 100 // total:3d}% ({human(done)}/{human(total)})", end="", flush=True)
        print()
    return b""


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(lp(path), "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# =============================================================================
# 环境检测
# =============================================================================
def claude_platform_key() -> str:
    if WIN:
        arch = os.environ.get("PROCESSOR_ARCHITEW6432") or os.environ.get("PROCESSOR_ARCHITECTURE", "")
        return "win32-arm64" if arch.upper() == "ARM64" else "win32-x64"
    m = platform.machine().lower()
    arch = "arm64" if m in ("arm64", "aarch64") else "x64"
    if MAC:
        if arch == "x64":
            cp = run(["sysctl", "-n", "sysctl.proc_translated"])
            if cp.returncode == 0 and cp.stdout.strip() == "1":
                arch = "arm64"
        return f"darwin-{arch}"
    musl = any(os.path.exists(f) for f in ("/lib/libc.musl-x86_64.so.1", "/lib/libc.musl-aarch64.so.1"))
    if not musl:
        cp = run(["sh", "-c", "ldd /bin/ls 2>&1"])
        musl = "musl" in (cp.stdout or "")
    return f"linux-{arch}-musl" if musl else f"linux-{arch}"


def pkg_name(spec: str) -> str:
    """'@scope/name@1.2.3' -> '@scope/name'."""
    spec = spec.strip()
    at = spec.rfind("@")
    return spec[:at] if at > 0 else spec


def native_claude_path() -> str:
    return os.path.join(home(), ".local", "bin", "claude.exe" if WIN else "claude")


def ccline_dir() -> str:
    return os.path.join(home(), ".claude", "ccline")


def ccline_bin() -> str:
    return os.path.join(ccline_dir(), "ccline.exe" if WIN else "ccline")


def npm_cmd() -> Optional[str]:
    return which("npm.cmd") or which("npm") if WIN else which("npm")


def npm_paths() -> Tuple[str, str]:
    """(npm prefix -g, npm root -g); 任一失败返回空串."""
    npm = npm_cmd()
    if not npm:
        return "", ""
    try:
        prefix = run([npm, "prefix", "-g"], timeout=30).stdout.strip()
        root = run([npm, "root", "-g"], timeout=30).stdout.strip()
        return prefix, root
    except Exception:
        return "", ""


def npm_installed_globals() -> set:
    npm = npm_cmd()
    if not npm:
        return set()
    try:
        cp = run([npm, "ls", "-g", "--depth=0", "--json"], timeout=90)
        return set((json.loads(cp.stdout or "{}").get("dependencies") or {}).keys())
    except Exception:
        return set()


def npm_uninstall(o: "Opts", pkg: str):
    log(f"npm uninstall -g {pkg}")
    if o.dry_run:
        return
    cp = run([npm_cmd(), "uninstall", "-g", pkg], timeout=300)
    if cp.returncode != 0:
        log(f"npm 卸载 {pkg} 失败: {(cp.stderr or '').strip()[:200]}", "warn")


def find_claude() -> Optional[str]:
    cands = [native_claude_path()]
    w = which("claude")
    if w and "windowsapps" not in w.lower():  # 排除 Claude Desktop 的 Claude.exe
        cands.append(w)
    prefix, _ = npm_paths()
    if prefix:
        cands += [os.path.join(prefix, "claude.cmd"), os.path.join(prefix, "bin", "claude")]
    for c in cands:
        if os.path.isfile(c):
            return c
    return None


def cmd_version(path: str) -> str:
    try:
        cp = run([path, "--version"], timeout=20)
        return (cp.stdout or cp.stderr).strip().split("\n")[0]
    except Exception:
        return "?"


def running_claude_pids() -> List[str]:
    try:
        if WIN:
            cp = run(["tasklist", "/FO", "CSV", "/NH", "/FI", "IMAGENAME eq claude.exe"], timeout=20)
            return [ln.split('","')[1] for ln in cp.stdout.splitlines() if ln.startswith('"claude.exe"')]
        cp = run(["pgrep", "-x", "claude"], timeout=20)
        return [p for p in cp.stdout.split() if p.strip()]
    except Exception:
        return []


# ---- 环境变量: Windows 注册表 / Unix rc 文件 / PowerShell profile ----
def win_user_env() -> Dict[str, Tuple[str, int]]:
    import winreg
    out = {}
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment") as k:
        i = 0
        while True:
            try:
                name, val, typ = winreg.EnumValue(k, i)
            except OSError:
                break
            out[name] = (val, typ)
            i += 1
    return out


def win_broadcast_env():
    try:
        HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG = 0xFFFF, 0x001A, 0x0002
        res = ctypes.c_ulong()
        ctypes.windll.user32.SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, "Environment",
                                                 SMTO_ABORTIFHUNG, 5000, ctypes.byref(res))
    except Exception:
        pass


def win_matching_env() -> Dict[str, Tuple[str, int]]:
    """HKCU\\Environment 中匹配前缀的变量: name -> (value, reg_type)."""
    try:
        return {k: vt for k, vt in win_user_env().items() if k.upper().startswith(ENV_PREFIXES)}
    except Exception:
        return {}


def win_system_env_hits() -> List[str]:
    try:
        import winreg
        hits = []
        path = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, path) as k:
            i = 0
            while True:
                try:
                    name, _v, _t = winreg.EnumValue(k, i)
                except OSError:
                    break
                if name.upper().startswith(ENV_PREFIXES):
                    hits.append(name)
                i += 1
        return hits
    except Exception:
        return []


RC_LINE_RE = re.compile(r"^\s*(export\s+|set\s+-gx\s+|setenv\s+)?(ANTHROPIC_|CLAUDE_)[A-Za-z0-9_]*\s*[= ]")
PS_LINE_RE = re.compile(r"^\s*(\$env:(ANTHROPIC_|CLAUDE_)\w+\s*=|\[Environment\]::SetEnvironmentVariable\(\s*['\"](ANTHROPIC_|CLAUDE_))", re.I)


def shell_rc_files() -> List[str]:
    if WIN:
        files = []
        for shell in ("powershell", "pwsh"):
            if not which(shell):
                continue
            try:
                cp = run([shell, "-NoProfile", "-Command", "$PROFILE.CurrentUserAllHosts; $PROFILE.CurrentUserCurrentHost"], timeout=30)
                files += [ln.strip() for ln in cp.stdout.splitlines() if ln.strip()]
            except Exception:
                pass
        return sorted({f for f in files if os.path.isfile(f)})
    names = [".zshenv", ".zprofile", ".zshrc", ".bash_profile", ".bash_login", ".profile", ".bashrc",
             os.path.join(".config", "fish", "config.fish")]
    zdot = os.environ.get("ZDOTDIR")
    files = [os.path.join(home(), n) for n in names]
    if zdot:
        files.append(os.path.join(zdot, ".zshrc"))
    return [f for f in files if os.path.isfile(f)]


def rc_hits(path: str) -> List[Tuple[int, str]]:
    rx = PS_LINE_RE if WIN else RC_LINE_RE
    hits = []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for i, line in enumerate(f, 1):
                if rx.match(line) and not line.lstrip().startswith("#"):
                    hits.append((i, line.rstrip("\n")))
    except OSError:
        pass
    return hits


def detect_state() -> Dict:
    st: Dict = {"platform": claude_platform_key(), "config_dir": config_dir()}
    st["claude_path"] = find_claude()
    st["claude_version"] = cmd_version(st["claude_path"]) if st["claude_path"] else ""
    st["claude_kind"] = ""
    if st["claude_path"]:
        p = st["claude_path"].lower().replace("\\", "/")
        if p.startswith(native_claude_path().lower().replace("\\", "/")):
            st["claude_kind"] = "native"
        elif "node_modules" in p or "/npm/" in p or p.endswith("claude.cmd"):
            st["claude_kind"] = "npm"
        elif "caskroom" in p or "homebrew" in p:
            st["claude_kind"] = "brew/npm"
        else:
            st["claude_kind"] = "other"
    st["ccline_path"] = ccline_bin() if os.path.isfile(ccline_bin()) else ""
    st["ccline_version"] = cmd_version(st["ccline_path"]) if st["ccline_path"] else ""
    st["config_exists"] = os.path.isdir(st["config_dir"])
    st["config_size"] = dir_size(st["config_dir"]) if st["config_exists"] else 0
    st["claude_json"] = os.path.isfile(os.path.join(home(), ".claude.json"))
    st["win_env"] = win_matching_env() if WIN else {}
    st["win_sys_env"] = win_system_env_hits() if WIN else []
    st["rc_hits"] = {f: rc_hits(f) for f in shell_rc_files()}
    st["rc_hits"] = {f: h for f, h in st["rc_hits"].items() if h}
    st["session_env"] = sorted(k for k in os.environ if k.upper().startswith(ENV_PREFIXES) and k != "CLAUDECODE")
    st["node"] = cmd_version(which("node")) if which("node") else ""
    st["npm"] = bool(npm_cmd())
    st["pids"] = running_claude_pids()
    st["inside_claude"] = bool(os.environ.get("CLAUDECODE"))
    st["ide_warn"] = []
    if glob.glob(os.path.join(home(), ".vscode", "extensions", "anthropic.claude-code-*")):
        st["ide_warn"].append("VS Code Claude Code 扩展")
    if (WIN and os.path.isdir(os.path.join(os.environ.get("APPDATA", ""), "Claude"))) or (MAC and os.path.isdir("/Applications/Claude.app")):
        st["ide_warn"].append("Claude Desktop")
    return st


def state_summary(st: Dict) -> List[str]:
    lines = []
    if st["claude_path"]:
        lines.append(f"Claude Code: {st['claude_version']}  [{st['claude_kind']}]  {tilde(st['claude_path'])}")
    else:
        lines.append("Claude Code: 未安装")
    lines.append(f"ccline: {st['ccline_version'] or '未安装'}    配置目录: {tilde(st['config_dir'])} "
                 f"({human(st['config_size']) if st['config_exists'] else '不存在'})"
                 f"    ~/.claude.json: {'有' if st['claude_json'] else '无'}")
    envn = len(st["win_env"]) + sum(len(v) for v in st["rc_hits"].values())
    lines.append(f"待清理环境变量: {envn} 项    Node: {st['node'] or '无'}    npm: {'有' if st['npm'] else '无'}"
                 f"    运行中的 claude 进程: {len(st['pids'])}")
    if st["win_sys_env"]:
        lines.append(f"{C.YELLOW}系统级(HKLM)环境变量需手动处理: {', '.join(st['win_sys_env'])}{C.RESET}")
    if st["session_env"]:
        lines.append(f"{C.YELLOW}当前终端会话已设置: {', '.join(st['session_env'][:4])}{'...' if len(st['session_env']) > 4 else ''} (子进程中会被剔除){C.RESET}")
    if st["ide_warn"]:
        lines.append(f"{C.YELLOW}检测到 {' / '.join(st['ide_warn'])}, 它们会重建 ~/.claude, 如需彻底清理请先卸载{C.RESET}")
    if st["inside_claude"]:
        lines.append(f"{C.RED}当前正运行在 Claude Code 会话内部, 清理/安装步骤已禁用 (仅可预览){C.RESET}")
    return lines


# =============================================================================
# 选项
# =============================================================================
class Opts:
    def __init__(self):
        ts = time.strftime("%Y%m%d-%H%M%S")
        self.backup = True
        self.backup_dir = os.path.join(home(), f"claude-backup-{ts}")
        self.exclude_bulky = False
        self.clean = True
        self.clean_env = True
        self.install_claude = True
        self.claude_version = DEFAULT_CLAUDE_VERSION
        self.install_method = "native"  # native | npm
        self.install_ccline = True
        self.ccline_pkg = DEFAULT_CCLINE_PKG
        self.ccline_icons = "nerd_font"  # nerd_font | plain
        self.proxy = DEFAULT_PROXY
        self.seed_onboarding = True
        self.login = True
        self.bedrock_vertex = False
        self.dry_run = False
        self.force = False
        self.template: Optional[str] = None


# =============================================================================
# 备份
# =============================================================================
def backup_all(o: Opts, st: Dict) -> str:
    dest = o.backup_dir
    log(f"备份到 {dest}", "step")
    manifest = {"time": time.strftime("%Y-%m-%d %H:%M:%S"), "platform": st["platform"], "items": []}
    ignore = shutil.ignore_patterns(*BULKY_DIRS) if o.exclude_bulky else None

    def _copy_tree(src, name):
        if not os.path.isdir(src):
            return
        log(f"复制目录 {tilde(src)}" + (" (排除会话/缓存)" if o.exclude_bulky else ""))
        if not o.dry_run:
            shutil.copytree(lp(src), lp(os.path.join(dest, name)), symlinks=True, ignore=ignore,
                            ignore_dangling_symlinks=True, dirs_exist_ok=True)
        manifest["items"].append({"type": "dir", "src": src, "name": name})

    def _copy_file(src, name):
        if not os.path.isfile(src):
            return
        log(f"复制文件 {tilde(src)}")
        if not o.dry_run:
            os.makedirs(lp(os.path.dirname(os.path.join(dest, name))), exist_ok=True)
            shutil.copy2(lp(src), lp(os.path.join(dest, name)))
        manifest["items"].append({"type": "file", "src": src, "name": name})

    if not o.dry_run:
        os.makedirs(lp(dest), exist_ok=True)
    _copy_tree(st["config_dir"], ".claude")
    if st["config_dir"] != os.path.join(home(), ".claude"):
        _copy_tree(os.path.join(home(), ".claude"), ".claude-default")
    for f in glob.glob(os.path.join(home(), ".claude.json*")):
        _copy_file(f, os.path.basename(f))
    for f in st["rc_hits"]:
        _copy_file(f, os.path.join("shell-rc", os.path.basename(f)))
    if st["win_env"]:
        log(f"记录 Windows 用户环境变量 {len(st['win_env'])} 项 -> env-vars.json")
        if not o.dry_run:
            save_json(os.path.join(dest, "env-vars.json"), st["win_env"])
    if MAC and not o.dry_run:
        cp = run(["security", "find-generic-password", "-s", "Claude Code-credentials", "-w"])
        if cp.returncode == 0 and cp.stdout.strip():
            with open(os.path.join(dest, "keychain-credentials.json"), "w", encoding="utf-8") as f:
                f.write(cp.stdout)
            os.chmod(os.path.join(dest, "keychain-credentials.json"), 0o600)
            log("导出 macOS Keychain 中的 OAuth 凭据 -> keychain-credentials.json (权限 600)")
    if not o.dry_run:
        save_json(os.path.join(dest, "manifest.json"), manifest)
    log("备份完成", "ok")
    return dest


# =============================================================================
# 清理
# =============================================================================
def stop_processes(o: Opts, st: Dict):
    pids = st["pids"]
    if not pids:
        return
    log(f"结束运行中的 claude 进程: {', '.join(pids)}", "step")
    if o.dry_run:
        return
    if WIN:
        run(["taskkill", "/F", "/T", "/IM", "claude.exe"])
    else:
        run(["pkill", "-x", "claude"])
    time.sleep(1)


def _rm(o: Opts, path: str, what: str = ""):
    if not os.path.lexists(lp(path)):
        return
    log(f"删除 {what + ' ' if what else ''}{tilde(path)}")
    if not o.dry_run:
        try:
            rm_force(path)
        except Exception as e:  # noqa: BLE001
            log(f"删除失败 {tilde(path)}: {e}", "err")


def clean_npm_packages(o: Opts):
    npm = npm_cmd()
    if not npm:
        return
    prefix, root = npm_paths()
    installed = npm_installed_globals()
    ccline_pkgs = tuple(dict.fromkeys(CCLINE_KNOWN_PKGS + (pkg_name(o.ccline_pkg),)))
    for pkg in (CLAUDE_NPM_PKG,) + ccline_pkgs:
        if pkg in installed:
            npm_uninstall(o, pkg)
    if root:
        for rel in (CLAUDE_NPM_PKG,) + ccline_pkgs:
            _rm(o, os.path.join(root, *rel.split("/")), "npm 残留")
        for leftover in glob.glob(os.path.join(root, "@anthropic-ai", ".claude-code-*")):
            _rm(o, leftover, "npm 残留")
    if prefix:
        shims = ["claude", "claude.cmd", "claude.ps1", "ccline", "ccline.cmd", "ccline.ps1"] if WIN else []
        for s in shims:
            _rm(o, os.path.join(prefix, s), "npm shim")
        if not WIN:
            for s in ("claude", "ccline"):
                p = os.path.join(prefix, "bin", s)
                if os.path.islink(p) and not os.path.exists(p):
                    _rm(o, p, "失效链接")


def clean_package_managers(o: Opts):
    if MAC and which("brew"):
        cp = run(["brew", "list", "--cask"], timeout=60)
        for cask in ("claude-code", "claude-code@latest"):
            if cask in (cp.stdout or "").split():
                log(f"brew uninstall --cask {cask}")
                if not o.dry_run:
                    run(["brew", "uninstall", "--cask", cask], timeout=300)
    if WIN and which("winget"):
        common = ["--id", "Anthropic.ClaudeCode", "-e", "--disable-interactivity", "--accept-source-agreements"]
        cp = run(["winget", "list"] + common, timeout=90)
        if cp.returncode == 0 and "Anthropic.ClaudeCode" in (cp.stdout or ""):
            log("winget uninstall Anthropic.ClaudeCode")
            if not o.dry_run:
                run(["winget", "uninstall", "--silent"] + common, timeout=600)


def clean_install(o: Opts, st: Dict):
    log("清理 Claude Code / ccline 安装", "step")
    clean_npm_packages(o)
    clean_package_managers(o)
    _rm(o, native_claude_path(), "原生安装")
    _rm(o, os.path.join(home(), ".local", "share", "claude"), "原生版本目录")
    if st["claude_path"] and os.path.lexists(st["claude_path"]) and st["claude_kind"] == "other":
        log(f"检测到非标准位置的 claude: {st['claude_path']}, 不自动删除, 请自行处理", "warn")

    log("清理配置与缓存", "step")
    for d in dict.fromkeys([st["config_dir"], os.path.join(home(), ".claude")]):
        _rm(o, d, "配置目录")
    for f in glob.glob(os.path.join(home(), ".claude.json*")):
        _rm(o, f, "全局配置")
    if MAC:
        log("删除 macOS Keychain 条目 'Claude Code-credentials'")
        if not o.dry_run:
            for _ in range(5):
                if run(["security", "delete-generic-password", "-s", "Claude Code-credentials"]).returncode != 0:
                    break
    log("安装与配置清理完成", "ok")


def clean_env_vars(o: Opts, st: Dict):
    log("清理 ANTHROPIC_* / CLAUDE_* 环境变量", "step")
    if WIN:
        if st["win_env"]:
            import winreg
            for name, (val, _typ) in st["win_env"].items():
                log(f"删除用户环境变量 {name}={str(val)[:40]}")
            if not o.dry_run:
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment", 0, winreg.KEY_SET_VALUE) as k:
                    for name in st["win_env"]:
                        try:
                            winreg.DeleteValue(k, name)
                        except OSError as e:
                            log(f"删除 {name} 失败: {e}", "err")
                win_broadcast_env()
        if st["win_sys_env"]:
            log(f"系统级环境变量需以管理员手动删除: {', '.join(st['win_sys_env'])}", "warn")
    for f, hits in st["rc_hits"].items():
        log(f"注释 {tilde(f)} 中 {len(hits)} 行: " + "; ".join(h[1].strip()[:50] for h in hits[:3]))
        if o.dry_run:
            continue
        with open(f, encoding="utf-8", errors="replace") as fh:
            lines = fh.readlines()
        idx = {i for i, _ in hits}
        with open(f, "w", encoding="utf-8") as fh:
            for i, line in enumerate(lines, 1):
                fh.write(("# cclean-disabled: " + line) if i in idx else line)
    if not st["win_env"] and not st["rc_hits"]:
        log("未发现需要清理的环境变量")
    log("环境变量清理完成", "ok")


# =============================================================================
# 安装 Claude Code
# =============================================================================
def win_ensure_user_path(entry: str):
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment", 0, winreg.KEY_READ | winreg.KEY_SET_VALUE) as k:
        try:
            cur, typ = winreg.QueryValueEx(k, "Path")
        except OSError:
            cur, typ = "", winreg.REG_EXPAND_SZ
        parts = [p for p in cur.split(";") if p]
        expanded = {os.path.normcase(os.path.expandvars(p).rstrip("\\")) for p in parts}
        if os.path.normcase(os.path.expandvars(entry).rstrip("\\")) in expanded:
            return False
        winreg.SetValueEx(k, "Path", 0, typ or winreg.REG_EXPAND_SZ, ";".join(parts + [entry]))
    win_broadcast_env()
    return True


def install_claude_native(o: Opts):
    key = claude_platform_key()
    target = o.claude_version.strip() or "latest"
    log(f"原生安装 Claude Code {target} ({key})", "step")
    if o.dry_run:
        log(f"将下载 {CLAUDE_RELEASES}/<ver>/{key}/claude 并执行 `claude install {target}`")
        return
    ver = target
    if target in ("latest", "stable"):
        ver = http_get(f"{CLAUDE_RELEASES}/latest", o.proxy, timeout=30).decode().strip()
        if not re.match(r"^\d+\.\d+\.\d+", ver):
            raise RuntimeError(f"获取版本号失败, 返回内容异常: {ver[:80]}")
    try:
        manifest = json.loads(http_get(f"{CLAUDE_RELEASES}/{ver}/manifest.json", o.proxy, timeout=30))
    except urllib.error.HTTPError as e:
        if e.code == 404:
            raise RuntimeError(f"版本 {ver} 不存在于发布服务器 (manifest 404)") from e
        raise
    checksum = (manifest.get("platforms", {}).get(key) or {}).get("checksum", "")
    if not re.fullmatch(r"[a-f0-9]{64}", checksum):
        raise RuntimeError(f"manifest 中没有平台 {key} 的校验值 (版本 {ver} 是否存在?)")
    # 直接下载到官方版本目录: 二进制已在此处时 `claude install <ver>` 不再重新下载
    # (否则它会自行再下一遍 190MB, 经代理常被掐断), 只做启动器/PATH/shell 集成.
    versions_dir = os.path.join(home(), ".local", "share", "claude", "versions")
    binary = os.path.join(versions_dir, ver + (".exe" if WIN else ""))
    if os.path.isfile(binary) and sha256_file(binary) == checksum:
        log(f"已存在校验一致的 {tilde(binary)}, 跳过下载", "ok")
    else:
        http_get(f"{CLAUDE_RELEASES}/{ver}/{key}/claude{'.exe' if WIN else ''}", o.proxy, dest=binary,
                 timeout=120, label=f"claude {ver}")
        actual = sha256_file(binary)
        if actual != checksum:
            rm_force(binary)
            raise RuntimeError(f"SHA256 校验失败: {actual} != {checksum}")
        log("SHA256 校验通过", "ok")
    if not WIN:
        os.chmod(binary, 0o755)
    log(f"执行 claude install {target} ...")
    cp = run([binary, "install", target], capture=False, env=child_env(o.proxy), timeout=900)
    if cp.returncode != 0:
        raise RuntimeError(f"claude install 退出码 {cp.returncode}")
    if WIN:
        if win_ensure_user_path(r"%USERPROFILE%\.local\bin"):
            log(r"已把 %USERPROFILE%\.local\bin 加入用户 PATH (新开终端后生效)", "ok")
    else:
        unix_ensure_path()


def unix_ensure_path():
    """安装器只提示不写入; 这里把 ~/.local/bin 追加到当前 shell 的 rc 文件."""
    local_bin = os.path.join(home(), ".local", "bin")
    if local_bin in os.environ.get("PATH", "").split(":"):
        return
    shell = os.path.basename(os.environ.get("SHELL", "bash"))
    if shell == "fish":
        rc, line = os.path.join(home(), ".config", "fish", "config.fish"), "fish_add_path -g $HOME/.local/bin"
    elif shell == "zsh":
        rc, line = os.path.join(os.environ.get("ZDOTDIR") or home(), ".zshrc"), 'export PATH="$HOME/.local/bin:$PATH"'
    else:
        rc = os.path.join(home(), ".bash_profile" if MAC else ".bashrc")
        line = 'export PATH="$HOME/.local/bin:$PATH"'
    try:
        existing = open(rc, encoding="utf-8", errors="replace").read() if os.path.isfile(rc) else ""
        if ".local/bin" in existing:
            return
        os.makedirs(os.path.dirname(rc), exist_ok=True)
        with open(rc, "a", encoding="utf-8") as f:
            f.write(f"\n# added by cclean\n{line}\n")
        log(f"已把 ~/.local/bin 写入 {tilde(rc)} (新开终端后生效)", "ok")
    except OSError as e:
        log(f"无法写入 {tilde(rc)}: {e}; 请手动执行: {line}", "warn")


def install_claude_npm(o: Opts):
    log(f"npm 安装 Claude Code {o.claude_version}", "step")
    npm = npm_cmd()
    if not npm:
        raise RuntimeError("未找到 npm; 请安装 Node.js 22+ 或改用 native 方式")
    node_v = cmd_version(which("node") or "node")
    m = re.match(r"v?(\d+)", node_v)
    if m and int(m.group(1)) < 22:
        log(f"Node {node_v} 低于 22, npm 包 2.1.198+ 要求 Node 22 (安装可能仍完成)", "warn")
    spec = f"{CLAUDE_NPM_PKG}@{o.claude_version.strip() or 'latest'}"
    if o.dry_run:
        log(f"将执行 npm install -g {spec}")
        return
    cp = run([npm, "install", "-g", spec], capture=False, env=child_env(o.proxy), timeout=900)
    if cp.returncode != 0:
        raise RuntimeError(f"npm install 退出码 {cp.returncode}")


def install_claude(o: Opts):
    if o.install_method == "npm":
        install_claude_npm(o)
    else:
        install_claude_native(o)
    if o.dry_run:
        return
    path = find_claude()
    if not path:
        raise RuntimeError("安装后未找到 claude 可执行文件")
    log(f"claude -> {path}  {cmd_version(path)}", "ok")


# =============================================================================
# 安装 ccline (npm)
# =============================================================================
def install_ccline(o: Opts):
    spec = o.ccline_pkg.strip()
    log(f"安装 ccline 状态栏: npm install -g {spec}", "step")
    npm = npm_cmd()
    if not npm:
        raise RuntimeError("未找到 npm, 无法安装 ccline; 请先安装 Node.js")
    dest = ccline_bin()
    # 先卸载其他版本的 ccline (如原版 @cometix/ccline), 避免 bin 冲突与旧二进制残留
    installed = npm_installed_globals()
    for pkg in CCLINE_KNOWN_PKGS:
        if pkg != pkg_name(spec) and pkg in installed:
            npm_uninstall(o, pkg)
    if os.path.isfile(dest):
        _rm(o, dest, "旧 ccline 二进制")
    if not o.dry_run:
        os.makedirs(lp(ccline_dir()), exist_ok=True)
        cp = run([npm, "install", "-g", spec], capture=False, env=child_env(o.proxy), timeout=600)
        if cp.returncode != 0:
            raise RuntimeError(f"npm install {spec} 退出码 {cp.returncode}")
        if not os.path.isfile(dest):  # postinstall 未复制到 ~/.claude/ccline 时, 从平台子包手动取
            _, root = npm_paths()
            scope, name = (pkg_name(spec).split("/") + [""])[:2]
            found = glob.glob(os.path.join(root, scope, f"{name}-*", os.path.basename(dest)))
            if not found:
                raise RuntimeError("npm 安装后未在 ~/.claude/ccline 找到 ccline 二进制")
            shutil.copy2(found[0], lp(dest))
        if not WIN:
            os.chmod(dest, 0o755)

    cfg = CCLINE_CONFIG_TOML.replace('mode = "nerd_font"', f'mode = "{o.ccline_icons}"', 1)
    cfg_path = os.path.join(ccline_dir(), "config.toml")
    log(f"写入 {tilde(cfg_path)} (图标模式 {o.ccline_icons}, usage 段 180s 自动刷新)")
    if not o.dry_run:
        with open(lp(cfg_path), "w", encoding="utf-8") as f:
            f.write(cfg)
        log(f"ccline -> {dest}  {cmd_version(dest)}", "ok")


# =============================================================================
# 写配置
# =============================================================================
def settings_path() -> str:
    return os.path.join(config_dir(), "settings.json")


def build_settings(o: Opts) -> Dict:
    base = json.loads(json.dumps(SETTINGS_TEMPLATE))
    if o.template:
        custom = load_json(o.template)
        if custom is None:
            raise RuntimeError(f"模板文件无法读取: {o.template}")
        base = custom
    env = base.setdefault("env", {})
    for k in ("HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY"):
        env.pop(k, None)
    if o.proxy.strip():
        env["HTTP_PROXY"] = o.proxy.strip()
        env["HTTPS_PROXY"] = o.proxy.strip()
        env["NO_PROXY"] = "localhost,127.0.0.1,::1"
    if o.install_ccline:
        base["statusLine"] = {"type": "command",
                              "command": "~/.claude/ccline/ccline" + (".exe" if WIN else ""),
                              "padding": 0}
    else:
        base.pop("statusLine", None)
    return base


def write_settings(o: Opts):
    log("写入标准化 settings.json", "step")
    path = settings_path()
    data = build_settings(o)
    if os.path.isfile(path) and not o.clean:
        bak = path + ".cclean-" + time.strftime("%Y%m%d%H%M%S")
        log(f"已有 settings.json, 先备份为 {tilde(bak)}")
        if not o.dry_run:
            shutil.copy2(lp(path), lp(bak))
    for line in json.dumps(data, indent=2, ensure_ascii=False).splitlines():
        log(line)
    if not o.dry_run:
        save_json(path, data)
    log(f"settings.json -> {tilde(path)}", "ok")


def seed_claude_json(o: Opts):
    path = os.path.join(home(), ".claude.json")
    log("预置 ~/.claude.json: hasCompletedOnboarding=true (跳过首启连通性检查, 规避 ERR_BAD_REQUEST)", "step")
    if o.dry_run:
        return
    data = load_json(path) or {}
    data["hasCompletedOnboarding"] = True
    save_json(path, data)
    log("已写入", "ok")


# =============================================================================
# 登录 / 登录后配置
# =============================================================================
def auth_status(claude: str, proxy: str = "") -> Dict:
    try:
        cp = run([claude, "auth", "status", "--json"], timeout=60, env=child_env(proxy))
        return json.loads(cp.stdout.strip() or "{}")
    except Exception:
        return {}


def do_login(o: Opts) -> bool:
    log("OAuth 登录 (claude auth login --claudeai)", "step")
    if o.dry_run:
        return False
    claude = find_claude()
    if not claude:
        raise RuntimeError("未找到 claude, 无法登录")
    st = auth_status(claude, o.proxy)
    if st.get("loggedIn"):
        log(f"已登录: {st.get('email', '')} ({st.get('subscriptionType', '')})", "ok")
        return True
    log("即将打开浏览器完成授权; 结束后自动返回")
    run([claude, "auth", "login", "--claudeai"], capture=False, env=child_env(o.proxy), timeout=900)
    st = auth_status(claude, o.proxy)
    ok = bool(st.get("loggedIn"))
    log(f"登录{'成功' if ok else '未完成'}: {st.get('email', '')}", "ok" if ok else "warn")
    return ok


def apply_bedrock_vertex(o: Opts) -> bool:
    log("登录后追加 CLAUDE_CODE_USE_BEDROCK=1 / CLAUDE_CODE_USE_VERTEX=1", "step")
    claude = find_claude()
    if not claude:
        log("未找到 claude, 跳过", "err")
        return False
    st = auth_status(claude, o.proxy)
    if not st.get("loggedIn"):
        log("尚未完成 OAuth 登录, 为避免影响认证路由, 本步骤跳过. 登录后运行: python cclean.py post-login", "warn")
        return False
    if o.dry_run:
        return True
    data = load_json(settings_path()) or {}
    data.setdefault("env", {}).update({"CLAUDE_CODE_USE_BEDROCK": "1", "CLAUDE_CODE_USE_VERTEX": "1"})
    save_json(settings_path(), data)
    log(f"已写入 {tilde(settings_path())}", "ok")
    return True


# =============================================================================
# 恢复 / 模板导出
# =============================================================================
def restore_backup(src: str):
    if not os.path.isfile(os.path.join(src, "manifest.json")):
        raise RuntimeError(f"{src} 不是 cclean 备份目录 (缺少 manifest.json)")
    log(f"从 {src} 恢复", "step")
    d = os.path.join(src, ".claude")
    if os.path.isdir(d):
        rm_force(config_dir())
        shutil.copytree(lp(d), lp(config_dir()), symlinks=True)
        log(f"恢复 {tilde(config_dir())}", "ok")
    for f in glob.glob(os.path.join(src, ".claude.json*")):
        shutil.copy2(f, os.path.join(home(), os.path.basename(f)))
        log(f"恢复 ~/{os.path.basename(f)}", "ok")
    envf = os.path.join(src, "env-vars.json")
    if WIN and os.path.isfile(envf):
        import winreg
        data = load_json(envf) or {}
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment", 0, winreg.KEY_SET_VALUE) as k:
            for name, (val, typ) in data.items():
                winreg.SetValueEx(k, name, 0, int(typ), val)
        win_broadcast_env()
        log(f"恢复用户环境变量 {len(data)} 项", "ok")
    if glob.glob(os.path.join(src, "shell-rc", "*")):
        log("shell 配置文件原件位于 backup/shell-rc/, 如需恢复请手动比对", "warn")


def export_template(out: Optional[str]):
    s = load_json(settings_path())
    if s is None:
        raise RuntimeError(f"读取失败: {settings_path()}")
    tpl: Dict = {"$schema": s.get("$schema", SETTINGS_TEMPLATE["$schema"]), "env": {}}
    for k, v in (s.get("env") or {}).items():
        if k.startswith(TEMPLATE_ENV_PREFIXES):
            tpl["env"][k] = v
    for k in TEMPLATE_TOP_KEYS:
        if k in s:
            tpl[k] = s[k]
    deny = (s.get("permissions") or {}).get("deny")
    if deny:
        tpl["permissions"] = {"deny": deny}
    if s.get("statusLine"):
        tpl["statusLine"] = s["statusLine"]
    text = json.dumps(tpl, indent=2, ensure_ascii=False)
    if out:
        with open(out, "w", encoding="utf-8") as f:
            f.write(text + "\n")
        print(f"模板已写入 {out}")
    else:
        print(text)


# =============================================================================
# 执行流程
# =============================================================================
def plan_lines(o: Opts) -> List[str]:
    L = []
    if o.backup:
        L.append(f"备份 -> {tilde(o.backup_dir)}" + (" (排除会话/缓存)" if o.exclude_bulky else ""))
    if o.clean:
        L.append("清理: 结束 claude 进程, 卸载 npm/brew/winget/原生安装, 删除 ~/.claude 与 ~/.claude.json*" + (", Keychain 凭据" if MAC else ""))
    if o.clean_env:
        L.append("清理 ANTHROPIC_*/CLAUDE_* 用户环境变量" + (" (注册表 + PowerShell profile)" if WIN else " (shell rc 文件注释)"))
    if o.install_claude:
        L.append(f"安装 Claude Code {o.claude_version} [{o.install_method}]")
    if o.install_ccline:
        L.append(f"npm 安装 ccline ({o.ccline_pkg}), 图标 {o.ccline_icons}, 写入本机 config.toml")
    if o.install_claude or o.install_ccline:
        L.append(f"写入 settings.json (隐私/禁更新/子Agent 深度=1/拒绝 ListAgents/跨会话 refuse" + (f", 代理 {o.proxy}" if o.proxy.strip() else "") + ")")
    if o.seed_onboarding:
        L.append("预置 hasCompletedOnboarding=true")
    if o.login:
        L.append("运行 claude auth login --claudeai")
    if o.bedrock_vertex:
        L.append("登录成功后追加 CLAUDE_CODE_USE_BEDROCK/VERTEX=1")
    if o.dry_run:
        L.append(f"{C.YELLOW}** 预览模式, 不做任何更改 **{C.RESET}")
    return L


def execute(o: Opts, st: Dict, ask: bool = True) -> int:
    print()
    log("执行计划", "step")
    for ln in plan_lines(o):
        log("- " + ln)
    dangerous = (o.clean or o.clean_env or o.install_claude) and not o.dry_run
    if dangerous and st["inside_claude"] and not o.force:
        log("检测到在 Claude Code 会话内运行 (CLAUDECODE=1), 清理会杀掉当前会话. 请在普通终端运行, 或加 --force", "err")
        return 2
    if ask and not o.dry_run and not confirm("确认执行?"):
        log("已取消")
        return 1
    try:
        if o.backup:
            backup_all(o, st)
        if o.clean:
            stop_processes(o, st)
            clean_install(o, st)
        if o.clean_env:
            clean_env_vars(o, st)
        if o.install_claude:
            install_claude(o)
        if o.install_ccline:
            install_ccline(o)
        if o.install_claude or o.install_ccline:
            write_settings(o)
        if o.seed_onboarding:
            seed_claude_json(o)
        logged_in = do_login(o) if o.login else False
        if o.bedrock_vertex:
            if logged_in or not o.login:
                apply_bedrock_vertex(o)
            else:
                log("未登录, Bedrock/Vertex 参数未写入; 登录后运行: python cclean.py post-login", "warn")
    except KeyboardInterrupt:
        log("用户中断", "err")
        return 130
    except Exception as e:  # noqa: BLE001
        log(f"失败: {e}", "err")
        return 1
    finally:
        _write_log(o)
    log("全部完成. 请新开一个终端运行 `claude` 验证" + ("; Windows 下如提示找不到命令, 重新打开终端即可" if WIN else ""), "ok")
    return 0


def _write_log(o: Opts):
    target_dir = o.backup_dir if (o.backup and os.path.isdir(o.backup_dir)) else home()
    path = os.path.join(target_dir, "cclean.log")
    try:
        with open(lp(path), "a", encoding="utf-8") as f:
            f.write("\n".join(LOG_LINES) + "\n")
        print(f"{C.DIM}日志: {path}{C.RESET}")
    except OSError:
        pass


# =============================================================================
# TUI
# =============================================================================
FIELDS: List[Tuple] = [
    ("backup", "bool", "备份现有配置"),
    ("backup_dir", "text", "  备份目录"),
    ("exclude_bulky", "bool", "  备份时排除会话记录/缓存 (projects, file-history 等)"),
    ("clean", "bool", "清理现有 Claude Code / ccline 安装与全部配置"),
    ("clean_env", "bool", "  清理 ANTHROPIC_* / CLAUDE_* 用户级环境变量"),
    ("install_claude", "bool", "安装 Claude Code"),
    ("claude_version", "text", "  版本 (x.y.z / latest / stable)"),
    ("install_method", "choice", "  安装方式", ["native", "npm"]),
    ("install_ccline", "bool", "安装 ccline 状态栏 (含本机 config.toml, 用量自动刷新)"),
    ("ccline_pkg", "text", "  npm 包名[@版本]"),
    ("ccline_icons", "choice", "  图标模式", ["nerd_font", "plain"]),
    ("proxy", "text", "代理地址:端口 (留空=不设置)"),
    ("seed_onboarding", "bool", "预置 hasCompletedOnboarding (规避 Windows ERR_BAD_REQUEST)"),
    ("login", "bool", "安装后立即运行 claude auth login (OAuth)"),
    ("bedrock_vertex", "bool", "OAuth 登录成功后追加 CLAUDE_CODE_USE_BEDROCK/VERTEX=1"),
    ("dry_run", "bool", "仅预览, 不做任何更改"),
]
BUTTONS = ["开始执行", "仅执行登录后配置 (Bedrock/Vertex)", "退出"]


def read_key() -> str:
    """返回 'UP'/'DOWN'/'LEFT'/'RIGHT'/'ENTER'/'ESC'/'BACKSPACE'/'TAB' 或单个字符."""
    if WIN:
        import msvcrt
        ch = msvcrt.getwch()
        if ch in ("\x00", "\xe0"):
            return {"H": "UP", "P": "DOWN", "K": "LEFT", "M": "RIGHT"}.get(msvcrt.getwch(), "")
        return {"\r": "ENTER", "\n": "ENTER", "\x08": "BACKSPACE", "\x1b": "ESC", "\t": "TAB", "\x03": "CTRL_C"}.get(ch, ch)
    import select
    import termios
    import tty
    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        b = os.read(fd, 1)
        if b == b"\x1b":
            if select.select([fd], [], [], 0.05)[0]:
                seq = os.read(fd, 2)
                return {b"[A": "UP", b"[B": "DOWN", b"[D": "LEFT", b"[C": "RIGHT"}.get(seq, "")
            return "ESC"
        if b[0] >= 0xC0:  # UTF-8 多字节
            n = 1 if b[0] < 0xE0 else 2 if b[0] < 0xF0 else 3
            b += os.read(fd, n)
        ch = b.decode("utf-8", "replace")
        return {"\r": "ENTER", "\n": "ENTER", "\x7f": "BACKSPACE", "\x08": "BACKSPACE", "\t": "TAB", "\x03": "CTRL_C"}.get(ch, ch)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)


class Tui:
    def __init__(self, o: Opts, st: Dict, vt: bool):
        self.o, self.st, self.vt = o, st, vt
        self.cur = 0
        self.editing = False
        self.edit_buf = ""
        self.msg = ""

    # ---- 渲染 ----
    def clear(self):
        if self.vt:
            sys.stdout.write("\x1b[2J\x1b[H")
        else:
            os.system("cls" if WIN else "clear")

    def field_text(self, i: int) -> str:
        name, kind, label = FIELDS[i][:3]
        indent = " " * (len(label) - len(label.lstrip()))
        label = label.strip()
        val = getattr(self.o, name)
        if kind == "bool":
            return f"{indent}[{'x' if val else ' '}] {label}"
        if kind == "choice":
            return f"{indent}    {label}: < {val} >"
        shown = self.edit_buf + "_" if (self.editing and self.cur == i) else (tilde(str(val)) if val else "(空)")
        return f"{indent}    {label}: {shown}"

    def render(self):
        self.clear()
        w = min(shutil.get_terminal_size((100, 40)).columns, 110)
        out = [f"{C.BOLD}{C.CYAN} cclean - Claude Code 环境备份 / 清理 / 重装工具   [{self.st['platform']}]{C.RESET}", "-" * (w - 1)]
        out += [" " + ln for ln in state_summary(self.st)]
        out.append("-" * (w - 1))
        for i in range(len(FIELDS)):
            line = pad(" " + self.field_text(i), w - 2)
            out.append(f"{C.REV}{line}{C.RESET}" if i == self.cur else line)
        out.append("")
        btn_line = "   "
        for j, b in enumerate(BUTTONS):
            k = len(FIELDS) + j
            btn_line += (f"{C.REV}[ {b} ]{C.RESET}" if k == self.cur else f"[ {b} ]") + "   "
        out.append(btn_line)
        out.append("")
        help_ = "回车 确认/编辑" if self.editing else "上下/jk 移动   空格/回车 切换或编辑   左右 切换选项   q 退出"
        out.append(f"{C.DIM} {help_}   {self.msg}{C.RESET}")
        sys.stdout.write("\n".join(out) + "\n")
        sys.stdout.flush()

    # ---- 输入 ----
    def handle_edit(self, key: str):
        name = FIELDS[self.cur][0]
        if key == "ENTER":
            setattr(self.o, name, self.edit_buf.strip())
            self.editing = False
        elif key == "ESC":
            self.editing = False
        elif key == "BACKSPACE":
            self.edit_buf = self.edit_buf[:-1]
        elif len(key) == 1 and key.isprintable():
            self.edit_buf += key

    def activate(self) -> Optional[str]:
        if self.cur >= len(FIELDS):
            return BUTTONS[self.cur - len(FIELDS)]
        name, kind = FIELDS[self.cur][:2]
        if kind == "bool":
            setattr(self.o, name, not getattr(self.o, name))
        elif kind == "choice":
            self.cycle(1)
        else:
            self.editing, self.edit_buf = True, str(getattr(self.o, name))
        return None

    def cycle(self, d: int):
        name, kind = FIELDS[self.cur][:2]
        if kind != "choice":
            return
        opts = FIELDS[self.cur][3]
        setattr(self.o, name, opts[(opts.index(getattr(self.o, name)) + d) % len(opts)])

    def loop(self) -> Optional[str]:
        total = len(FIELDS) + len(BUTTONS)
        while True:
            self.render()
            key = read_key()
            if key == "CTRL_C":
                return "退出"
            if self.editing:
                self.handle_edit(key)
                continue
            if key in ("UP", "k"):
                self.cur = (self.cur - 1) % total
            elif key in ("DOWN", "j", "TAB"):
                self.cur = (self.cur + 1) % total
            elif key == "LEFT":
                self.cycle(-1) if self.cur < len(FIELDS) else setattr(self, "cur", max(len(FIELDS), self.cur - 1))
            elif key == "RIGHT":
                self.cycle(1) if self.cur < len(FIELDS) else setattr(self, "cur", min(total - 1, self.cur + 1))
            elif key in ("ENTER", " "):
                act = self.activate()
                if act:
                    return act
            elif key in ("q", "Q", "ESC"):
                return "退出"


def run_tui(o: Opts) -> int:
    vt = enable_vt()
    if not vt:
        C.disable()
    print("检测当前环境...", flush=True)
    st = detect_state()
    if vt:
        sys.stdout.write("\x1b[?25l")
    try:
        while True:
            action = Tui(o, st, vt).loop()
            if vt:
                sys.stdout.write("\x1b[?25h\x1b[2J\x1b[H")
                sys.stdout.flush()
            else:
                os.system("cls" if WIN else "clear")
            if action == "退出":
                return 0
            if action == BUTTONS[1]:
                rc = 0 if apply_bedrock_vertex(o) else 1
            else:
                rc = execute(o, st, ask=True)
            try:
                input(f"\n{C.DIM}按回车返回菜单, Ctrl+C 退出...{C.RESET}")
            except (EOFError, KeyboardInterrupt):
                return rc
            st = detect_state()
            if vt:
                sys.stdout.write("\x1b[?25l")
    finally:
        if vt:
            sys.stdout.write("\x1b[?25h")
            sys.stdout.flush()


# =============================================================================
# CLI
# =============================================================================
def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="cclean", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd")

    r = sub.add_parser("run", help="非交互执行 (默认全部步骤, 用 --no-xxx 关闭)")
    for name, kind, label, *rest in FIELDS:
        flag = name.replace("_", "-")
        if kind == "bool":
            r.add_argument(f"--{flag}", dest=name, action="store_true", default=None, help=label.strip())
            r.add_argument(f"--no-{flag}", dest=name, action="store_false", help=argparse.SUPPRESS)
        elif kind == "choice":
            r.add_argument(f"--{flag}", dest=name, choices=rest[0], default=None, help=label.strip())
        else:
            r.add_argument(f"--{flag}", dest=name, default=None, help=label.strip())
    r.add_argument("--template", help="自定义 settings.json 模板文件 (代替内置模板)")
    r.add_argument("-y", "--yes", action="store_true", help="不再确认, 直接执行")
    r.add_argument("--force", action="store_true", help="即使在 Claude Code 会话内也执行")

    pl = sub.add_parser("post-login", help="OAuth 登录后追加 CLAUDE_CODE_USE_BEDROCK/VERTEX=1")
    pl.add_argument("--proxy", default="", help="调用 claude auth status 时使用的代理")
    pl.add_argument("--dry-run", action="store_true")

    e = sub.add_parser("export-template", help="从本机 settings.json 提取模板")
    e.add_argument("-o", "--out", help="写入文件而非打印")

    rs = sub.add_parser("restore", help="从备份目录恢复配置")
    rs.add_argument("--from", dest="src", required=True, help="cclean 备份目录")
    return p


def main(argv: Optional[List[str]] = None) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        try:
            sys.stdout.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[attr-defined]
        except Exception:
            pass
    args = build_parser().parse_args(argv)
    o = Opts()
    try:
        if args.cmd is None:
            if not (sys.stdin.isatty() and sys.stdout.isatty()):
                print("非交互终端, 请使用: python cclean.py run --help", file=sys.stderr)
                return 2
            return run_tui(o)
        if not enable_vt():
            C.disable()
        if args.cmd == "run":
            for name, *_ in FIELDS:
                v = getattr(args, name)
                if v is not None:
                    setattr(o, name, v)
            o.template, o.force = args.template, args.force
            return execute(o, detect_state(), ask=not args.yes)
        if args.cmd == "post-login":
            o.proxy, o.dry_run = args.proxy, args.dry_run
            return 0 if apply_bedrock_vertex(o) else 1
        if args.cmd == "export-template":
            export_template(args.out)
            return 0
        if args.cmd == "restore":
            restore_backup(os.path.abspath(os.path.expanduser(args.src)))
            return 0
    except KeyboardInterrupt:
        print("\n已中断")
        return 130
    except Exception as e:  # noqa: BLE001
        print(f"{C.RED}错误: {e}{C.RESET}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
