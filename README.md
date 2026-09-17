# CCometixLine

[English](README.md) | [中文](README.zh.md)

A high-performance Claude Code statusline tool written in Rust with Git integration, usage tracking, interactive TUI configuration, and Claude Code enhancement utilities.

![Language:Rust](https://img.shields.io/static/v1?label=Language&message=Rust&color=orange&style=flat-square)
![License:MIT](https://img.shields.io/static/v1?label=License&message=MIT&color=blue&style=flat-square)

> **Fork notice** — this is a maintained fork of [Haleclipse/CCometixLine](https://github.com/Haleclipse/CCometixLine) (based on v1.1.2), published to npm as `@yatotm/ccline`. Changes on top of upstream:
> - **Usage segment** shows both 7-day and 5-hour utilization (`7d% 5h%`).
> - **Context window** uses the `context_window.context_window_size` that Claude Code (≥ 2.0.37) reports, so models running with 1M context are detected even without the `[1m]` suffix; older CLIs keep the suffix-based logic. Fable/Mythos default to 1M.
> - **Windows icons**: with the stock cmd/PowerShell fonts (no Nerd Font installed) icons fall back to the theme's emoji variants instead of showing `?`; with a Nerd Font installed, Material Design glyphs outside the BMP are swapped for BMP equivalents. Override detection with `CCLINE_NERD_FONT=1` / `0`. `ccline --install-font` installs a bundled Nerd Font symbols font for Windows Terminal.
> - **Default config** is the `cometix` theme with every segment enabled.
> - Accepts `model` as a bare string (upstream #118 crash on newer Claude Code).

## Screenshots

![CCometixLine](assets/img1.png)

The statusline shows: Model | Directory | Git Branch Status | Context Window Information

## Features

### Core Functionality
- **Git integration** with branch, status, and tracking info  
- **Model display** with simplified Claude model names
- **Usage tracking** based on transcript analysis
- **Directory display** showing current workspace
- **Minimal design** using Nerd Font icons

### Interactive TUI Features
- **Interactive main menu** when executed without input
- **TUI configuration interface** with real-time preview
- **Theme system** with multiple built-in presets
- **Segment customization** with granular control
- **Configuration management** (init, check, edit)

### Claude Code Enhancement
- **Context warning disabler** - Remove annoying "Context low" messages
- **Verbose mode enabler** - Enhanced output detail
- **Robust patcher** - Survives Claude Code version updates
- **Automatic backups** - Safe modification with easy recovery

## Installation

### Quick Install (Recommended)

Install via npm (works on all platforms):

```bash
# Install globally
npm install -g @yatotm/ccline

# Or using yarn
yarn global add @yatotm/ccline

# Or using pnpm
pnpm add -g @yatotm/ccline
```

Use npm mirror for faster download:
```bash
npm install -g @yatotm/ccline --registry https://registry.npmmirror.com
```

After installation:
- ✅ Global command `ccline` is available everywhere
- ⚙️ Follow the configuration steps below to integrate with Claude Code
- 🎨 Run `ccline -c` to open configuration panel for theme selection

### Claude Code Configuration

Add to your Claude Code `settings.json`:

**Cross-Platform (Recommended)**
```json
{
  "statusLine": {
    "type": "command",
    "command": "~/.claude/ccline/ccline",
    "padding": 0
  }
}
```

> **Note for Windows users:** Starting from Claude Code v2.1.47+, Unix-style path parsing is supported on Windows. The `~` symbol is automatically expanded to your user home directory. **Do not use `%USERPROFILE%`** - it no longer works reliably in v2.1.47+.
> - Recommended: `~/.claude/ccline/ccline` (works on all platforms)
> - Alternative: `"ccline"` (requires npm global installation)

**Fallback (npm installation):**
```json
{
  "statusLine": {
    "type": "command",
    "command": "ccline",
    "padding": 0
  }
}
```
*Use this if npm global installation is available in PATH*

### Windows: real Nerd Font icons (optional)

Stock Windows has no font with Nerd Font glyphs, so ccline shows emoji icons there by default. To get the real icons in Windows Terminal (Windows 11's default host for cmd/PowerShell), run once:

```powershell
ccline --install-font
```

This installs the bundled [Symbols Nerd Font Mono](https://github.com/ryanoasis/nerd-fonts) (MIT, see `assets/fonts/`) for the current user — no admin rights — and adds it as a fallback font to the Command Prompt / PowerShell profiles through a Windows Terminal fragment (Windows Terminal 1.20+). Restart Windows Terminal afterwards; ccline switches to Nerd Font icons automatically. For other terminals or profiles, set the font face to `Cascadia Mono, Symbols Nerd Font Mono` yourself.

### Update

```bash
npm update -g @yatotm/ccline
```

<details>
<summary>Manual Installation (Click to expand)</summary>

Alternatively, download from [Releases](https://github.com/yatotm/CCometixLine/releases):

#### Linux

#### Option 1: Dynamic Binary (Recommended)
```bash
mkdir -p ~/.claude/ccline
wget https://github.com/yatotm/CCometixLine/releases/latest/download/ccline-linux-x64.tar.gz
tar -xzf ccline-linux-x64.tar.gz
cp ccline ~/.claude/ccline/
chmod +x ~/.claude/ccline/ccline
```
*Requires: Ubuntu 22.04+, CentOS 9+, Debian 11+, RHEL 9+ (glibc 2.35+)*

#### Option 2: Static Binary (Universal Compatibility)
```bash
mkdir -p ~/.claude/ccline
wget https://github.com/yatotm/CCometixLine/releases/latest/download/ccline-linux-x64-static.tar.gz
tar -xzf ccline-linux-x64-static.tar.gz
cp ccline ~/.claude/ccline/
chmod +x ~/.claude/ccline/ccline
```
*Works on any Linux distribution (static, no dependencies)*

#### macOS (Intel)

```bash  
mkdir -p ~/.claude/ccline
wget https://github.com/yatotm/CCometixLine/releases/latest/download/ccline-macos-x64.tar.gz
tar -xzf ccline-macos-x64.tar.gz
cp ccline ~/.claude/ccline/
chmod +x ~/.claude/ccline/ccline
```

#### macOS (Apple Silicon)

```bash
mkdir -p ~/.claude/ccline  
wget https://github.com/yatotm/CCometixLine/releases/latest/download/ccline-macos-arm64.tar.gz
tar -xzf ccline-macos-arm64.tar.gz
cp ccline ~/.claude/ccline/
chmod +x ~/.claude/ccline/ccline
```

#### Windows

```powershell
# Create directory and download
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.claude\ccline"
Invoke-WebRequest -Uri "https://github.com/yatotm/CCometixLine/releases/latest/download/ccline-windows-x64.zip" -OutFile "ccline-windows-x64.zip"
Expand-Archive -Path "ccline-windows-x64.zip" -DestinationPath "."
Move-Item "ccline.exe" "$env:USERPROFILE\.claude\ccline\"
```

</details>

### Build from Source

```bash
git clone https://github.com/yatotm/CCometixLine.git
cd CCometixLine
cargo build --release

# Linux/macOS
mkdir -p ~/.claude/ccline
cp target/release/ccometixline ~/.claude/ccline/ccline
chmod +x ~/.claude/ccline/ccline

# Windows (PowerShell)
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.claude\ccline"
copy target\release\ccometixline.exe "$env:USERPROFILE\.claude\ccline\ccline.exe"
```

## Usage

### Theme Override

```bash
# Temporarily use specific theme (overrides config file)
ccline --theme cometix
ccline --theme minimal
ccline --theme gruvbox
ccline --theme nord
ccline --theme powerline-dark

# Or use custom theme files from ~/.claude/ccline/themes/
ccline --theme my-custom-theme
```

### Claude Code Enhancement

```bash
# Disable context warnings and enable verbose mode
ccline --patch /path/to/claude-code/cli.js

# Example for common installation
ccline --patch ~/.local/share/fnm/node-versions/v24.4.1/installation/lib/node_modules/@anthropic-ai/claude-code/cli.js
```

## Default Segments

Fresh installs use the `cometix` theme (Nerd Font icons) with every segment enabled:
`Model | Directory | Git | Context Window | Usage | Cost | Session | Output Style`

### Git Status Indicators

- Branch name with Nerd Font icon
- Status: `✓` Clean, `●` Dirty, `⚠` Conflicts  
- Remote tracking: `↑n` Ahead, `↓n` Behind

### Model Display

Shows simplified Claude model names:
- `claude-3-5-sonnet` → `Sonnet 3.5`
- `claude-4-sonnet` → `Sonnet 4`

### Context Window Display

Token usage percentage based on transcript analysis with context limit tracking.

## Configuration

CCometixLine supports full configuration via TOML files and interactive TUI:

- **Configuration file**: `~/.claude/ccline/config.toml`
- **Interactive TUI**: `ccline --config` for real-time editing with preview
- **Theme files**: `~/.claude/ccline/themes/*.toml` for custom themes
- **Automatic initialization**: `ccline --init` creates default configuration

### Available Segments

All segments are configurable with:
- Enable/disable toggle
- Custom separators and icons
- Color customization
- Format options

Supported segments: Directory, Git, Model, Usage, Time, Cost, OutputStyle

### Model Configuration (`models.toml`)

Location: `~/.claude/ccline/models.toml` (auto-created on first run)

This file configures how model IDs are displayed and their context window limits. Claude models (Sonnet, Opus, Haiku) are automatically recognized with version extraction — you only need this file for overrides or third-party models.

```toml
# Model entries: simple substring matching on the model ID
# These take priority over built-in Claude model recognition
[[models]]
pattern = "glm-4.5"
display_name = "GLM-4.5"
context_limit = 128000

[[models]]
pattern = "kimi-k2"
display_name = "Kimi K2"
context_limit = 128000

# Context modifiers: matched independently and composable with model entries
# Overrides context_limit and appends display_suffix to the display name
# e.g., model "Opus 4" + modifier " 1M" = "Opus 4 1M"
[[context_modifiers]]
pattern = "[1m]"
display_suffix = " 1M"
context_limit = 1000000
```


## Requirements

- **Git**: Version 1.5+ (Git 2.22+ recommended for better branch detection)
- **Terminal**: Must support Nerd Fonts for proper icon display
  - Install a [Nerd Font](https://www.nerdfonts.com/) (e.g., FiraCode Nerd Font, JetBrains Mono Nerd Font)
  - Configure your terminal to use the Nerd Font
  - Windows without a Nerd Font: icons automatically fall back to emoji; run `ccline --install-font` for real icons in Windows Terminal (`CCLINE_NERD_FONT=1` forces Nerd Font icons if yours isn't detected)
- **Claude Code**: For statusline integration

## cclean: reset a Claude Code environment

[`cclean/`](cclean/README.md) is a standalone Python 3 script (standard library only) that backs up, wipes and reinstalls Claude Code plus this statusline: kills running sessions, uninstalls npm / native / brew / winget installs, clears `~/.claude` and `ANTHROPIC_*` / `CLAUDE_*` user environment variables, then installs a pinned Claude Code version through a proxy, installs `@yatotm/ccline`, writes a privacy-oriented `settings.json` and logs in. Run it from anywhere without cloning:

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.ps1 | iex
```

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.sh | sh
```

Both launchers accept cclean arguments, e.g. `... | sh -s -- run --dry-run -y`. See the [cclean README](cclean/README.md) for every option.

## Development

```bash
# Build development version
cargo build

# Run tests
cargo test

# Build optimized release
cargo build --release
```

### Releasing (maintainers)

1. Bump `version` in `Cargo.toml`, commit, then `git tag vX.Y.Z && git push origin master vX.Y.Z`.
2. GitHub Actions (`release.yml`) builds all 7 targets and attaches them to a GitHub Release.
3. Publish to npm from your machine with your own `npm login` session (no CI token needed):
   ```bash
   node npm/scripts/publish-from-release.js X.Y.Z            # add --dry-run to rehearse
   ```

## Roadmap

- [x] TOML configuration file support
- [x] TUI configuration interface
- [x] Custom themes
- [x] Interactive main menu
- [x] Claude Code enhancement tools

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests.

## Related Projects

- [tweakcc](https://github.com/Piebald-AI/tweakcc) - Command-line tool to customize your Claude Code themes, thinking verbs, and more.

## License

This project is licensed under the [MIT License](LICENSE).

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=yatotm/CCometixLine&type=Date)](https://star-history.com/#yatotm/CCometixLine&Date)
