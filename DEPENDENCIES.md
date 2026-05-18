# Netherize Editor - Runtime Dependencies

This document lists all external tools and LSP servers that Netherize Editor uses **at runtime** (when the app is running).

> **Note:** These are NOT build dependencies. The editor binary itself has zero external dependencies and runs standalone. These tools enhance functionality when available.

## Core Runtime (Zero Dependencies)

The editor binary runs completely standalone with:
- ✅ Text editing (Vim keybindings)
- ✅ Syntax highlighting (16 languages via built-in tree-sitter)
- ✅ File picker (built-in fuzzy search)
- ✅ Git status display
- ✅ Terminal emulator
- ✅ Multi-cursor editing
- ✅ Theme system

**No external tools required for basic usage.**

## Optional Runtime Dependencies

These tools enhance the editor experience but are not required for basic functionality.

### System CLI Tools

| Tool | Purpose | Install (macOS) | Install (Linux) |
|------|---------|-----------------|-----------------|
| **fzf** | Fuzzy finder for file picker & live grep | `brew install fzf` | `sudo apt install fzf` |
| **rg** (ripgrep) | Fast text search for live grep | `brew install ripgrep` | `sudo apt install ripgrep` |
| **lazygit** | Git TUI integration | `brew install lazygit` | `sudo apt install lazygit` |
| **lazydocker** | Docker TUI integration | `brew install lazydocker` | `sudo apt install lazydocker` |
| **fd** | Fast file finder (alternative to find) | `brew install fd` | `sudo apt install fd-find` |
| **bat** | Syntax-highlighted file previews | `brew install bat` | `sudo apt install bat` |
| **delta** | Git diff viewer | `brew install git-delta` | `sudo apt install git-delta` |

**Detection:** The editor checks for these tools at runtime via `CheckSystemDeps` worker request (see `src/async_runtime/scheduler/syntax_jobs.rs:234`).

### AI Integration

| Tool | Purpose | Install |
|------|---------|---------|
| **opencode** | AI code assistant | `curl -fsSL https://opencode.ai/install \| sh` |

**Location:** `src/async_runtime/scheduler/ai_jobs.rs`

## Language Server Protocol (LSP) Servers

The editor supports LSP for the following languages. Each LSP server is **optional** and only needed if you work with that language.

### Supported Languages

| Language | LSP Binary | Install Command | Extensions |
|----------|------------|-----------------|------------|
| **Rust** | `rust-analyzer` | `rustup component add rust-analyzer` | `.rs` |
| **JavaScript** | `typescript-language-server` | `npm install -g typescript typescript-language-server` | `.js`, `.mjs`, `.cjs` |
| **JSX** | `typescript-language-server` | `npm install -g typescript typescript-language-server` | `.jsx` |
| **TypeScript** | `typescript-language-server` | `npm install -g typescript typescript-language-server` | `.ts` |
| **TSX** | `typescript-language-server` | `npm install -g typescript typescript-language-server` | `.tsx` |
| **Go** | `gopls` | `go install golang.org/x/tools/gopls@latest` | `.go` |
| **Python** | `pylsp` | `pip install python-lsp-server` | `.py` |
| **Java** | `jdtls` | `brew install jdtls` (macOS) | `.java` |
| **SQL** | `sqls` | `go install github.com/sqls-server/sqls@latest` | `.sql` |
| **YAML** | `yaml-language-server` | `npm install -g yaml-language-server` | `.yaml`, `.yml` |
| **Dockerfile** | `docker-langserver` | `npm install -g dockerfile-language-server-nodejs` | `Dockerfile*` |
| **JSON** | `vscode-json-language-server` | `npm install -g vscode-langservers-extracted` | `.json` |
| **Bash** | `bash-language-server` | `npm install -g bash-language-server` | `.sh` |

**Registry Location:** `src/lsp/registry.rs`

### Languages with Syntax Highlighting Only (No LSP)

These languages have tree-sitter syntax highlighting but no LSP integration:

- **Markdown** (`.md`, `.markdown`, `.mdx`)
- **Protobuf** (`.proto`)
- **Dotenv** (`.env`, `.env*`)
- **XML** (`.xml`)
- **HTML** (`.html`)
- **CSS** (`.css`)
- **Plain Text** (`.txt`)

## Tree-sitter Parsers (Built-in)

These are compiled into the binary via `Cargo.toml` dependencies:

- `tree-sitter-rust`
- `tree-sitter-javascript`
- `tree-sitter-typescript`
- `tree-sitter-go`
- `tree-sitter-python`
- `tree-sitter-java`
- `tree-sitter-bash`
- `tree-sitter-json`
- `tree-sitter-yaml`
- `tree-sitter-md` (Markdown)
- `tree-sitter-sequel` (SQL)
- `tree-sitter-containerfile` (Dockerfile)
- `tree-sitter-html`
- `tree-sitter-css`
- `tree-sitter-proto` (Protobuf)
- `tree-sitter-xml`

## Graceful Degradation

The editor is designed to work without optional dependencies:

1. **Missing fzf/rg**: Falls back to built-in fuzzy picker (slower but functional)
2. **Missing LSP servers**: Syntax highlighting still works via tree-sitter
3. **Missing lazygit/lazydocker**: Git/Docker features disabled but editor remains functional
4. **Missing opencode**: AI features disabled

## Dependency Checking

The editor checks for missing system dependencies at runtime:

```rust
// src/async_runtime/scheduler/syntax_jobs.rs:232
WorkerRequestPayload::CheckSystemDeps => {
    let tools = ["fzf", "lazygit", "lazydocker", "rg", "fd", "bat", "delta"];
    // ... checks which tools are missing
}
```

LSP servers are checked per-file when opened:

```rust
// src/async_runtime/scheduler/syntax_jobs.rs:206
WorkerRequestPayload::CheckLspForPath { path } => {
    // Returns binary name, install command, and installation status
}
```

## Installation Scripts

See `scripts/` directory for automated installation helpers:

- `scripts/install.sh` - Main installer
- `scripts/bundle_macos.sh` - macOS app bundle creator
- `scripts/bundle_windows.sh` - Windows installer creator

## Runtime Dependency Summary

### What End Users Need to Run the App

**Minimum (works out of the box):**
```bash
./netherize_editor
# Zero dependencies - just run the binary
```

**Recommended (enhanced experience):**
```bash
# Install these for better search/navigation:
brew install fzf ripgrep        # macOS
sudo apt install fzf ripgrep    # Linux
```

**Full Experience (all features):**
```bash
# Add LSP servers for languages you use:
rustup component add rust-analyzer              # Rust
npm install -g typescript-language-server       # JS/TS
go install golang.org/x/tools/gopls@latest      # Go
pip install python-lsp-server                   # Python

# Add optional tools:
brew install lazygit lazydocker fd bat delta    # macOS
```

## For Distribution

### Option 1: Minimal Binary (Recommended)
Ship just the `netherize_editor` binary. Users install tools as needed.

**Pros:**
- Small download (~15-20MB)
- Works immediately
- Users only install what they need

**Cons:**
- Users must manually install LSP servers for their languages

### Option 2: Bundled Package
Include common LSP servers in the package.

**Pros:**
- Better out-of-box experience for common languages
- No manual setup for Rust/JS/TS/Python

**Cons:**
- Larger download (~100-200MB)
- Includes tools users may not need

### Option 3: Smart Installer
Detect missing tools and offer to install them on first run.

**Pros:**
- Best UX - guided setup
- Only installs what user needs

**Cons:**
- More complex installer logic
- Requires network access

### Recommended Distribution Strategy

1. **Ship minimal binary** (15-20MB)
2. **On first run:** Show welcome screen with dependency checker
3. **Let users choose:** "Install recommended tools?" (fzf, rg, rust-analyzer)
4. **Provide install commands** for their platform

This gives users control while making setup easy.
