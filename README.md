# Agent Router MCP

![Alpha](https://img.shields.io/badge/status-beta-yellow)
![Build Status](https://github.com/forge18/agent-router-mcp/actions/workflows/ci.yml/badge.svg)
![License](https://img.shields.io/badge/license-MIT-blue)

> ⚠️ **Alpha Software** - This project is in active development. It is not advised to use the MCP server for more than basic testing at this time.

A **stateless, config-driven** Model Context Protocol (MCP) server that intelligently routes requests to specialized AI subagents using a hybrid rule-based + LLM approach.

## Table of Contents

**Getting Started**
- [Key Features](#key-features)
- [Requirements](#requirements)
- [Quick Start Installation](#quick-start-installation)
  - [1. Download Binary](#1-download-binary)
  - [2. Download Config Files](#2-download-config-files)
  - [3. Configure MCP Client](#3-configure-mcp-client)

**Using the Server**
- [MCP Tools Reference](#mcp-tools-reference)
  - [get_routing](#get_routing)
  - [start_ollama](#start_ollama)
  - [pull_model](#pull_model)
  - [load_model](#load_model)
- [How It Works](#how-it-works)

**Configuration**
- [Configuration Files](#configuration-files)
  - [agents.json](#configagentsjson)
  - [rules.json](#configrulesjson)
  - [llm-tags.json](#configllm-tagsjson)
- [Customization Examples](#customization-examples)
- [Model Switching](#model-switching)

**Advanced Topics**
- [Architecture](#architecture)
- [Creating Agents/Subagents](#creating-agentssubagents)
- [Cross-Platform Support](#cross-platform-support)
- [Compiling from Source](#compiling-from-source)
- [Development](#development)

---

## Key Features

- 🔧 **Fully Config-Driven**: All routing logic defined in JSON - no code changes needed
- 🚀 **Stateless Architecture**: No state between requests, configs loaded once on startup
- ⚡ **Fast Routing**: Rule-based matching handles 90%+ of cases locally
- 🧠 **LLM Semantic Tagging**: Uses any Ollama model for edge cases and semantic understanding
- 🔄 **Flexible Rules**: Boolean logic (any_of, all_of) with nesting support
- 📝 **User Customizable**: Define your own agents, tags, and routing rules

## Requirements

### Software
- Rust 1.70+
- Ollama installed and in PATH

### Hardware
- **RAM**: 8GB minimum (for 3B models like `smollm3:3b`)
  - 16GB recommended for better performance and multitasking
  - Larger models require more RAM (7B models need 16GB, 13B models need 32GB)
- **Disk Space**: ~2GB for default model
  - Varies by model size and quantization

## Quick Start Installation

> **Prerequisites:** Make sure [Ollama](https://ollama.com) is installed and running. See [Requirements](#requirements) above.

### 1. Download Binary

Download the latest release from [GitHub Releases](https://github.com/yourusername/agent-router-mcp/releases) for your platform:

**Choose Your Binary:**
- **Windows Intel/AMD**: `agent-router-mcp-windows-amd64.exe` (Most Windows PCs)
- **Windows ARM**: `agent-router-mcp-windows-arm64.exe` (Surface Pro X, Windows Dev Kit 2023)
- **macOS Intel**: `agent-router-mcp-macos-intel` (Intel Macs)
- **macOS Apple Silicon**: `agent-router-mcp-macos-silicon` (M1/M2/M3 Macs)
- **Linux Intel/AMD**: `agent-router-mcp-linux-amd64` (Most PCs/servers)
- **Linux ARM**: `agent-router-mcp-linux-arm64` (Raspberry Pi 4+, AWS Graviton)

**Not sure which binary?** On the command line:
- **Linux/macOS**: Run `uname -m`
  - Output `x86_64` → use `amd64`
  - Output `aarch64` or `arm64` → use `arm64`
- **Windows**: Run `echo %PROCESSOR_ARCHITECTURE%`
  - Output `AMD64` → use `windows-amd64.exe`
  - Output `ARM64` → use `windows-arm64.exe`

**macOS/Linux**: Make it executable
```bash
chmod +x agent-router-mcp-*
```

### 2. Download Config Files

Download the config archive from [GitHub Releases](https://github.com/yourusername/agent-router-mcp/releases):
- **Windows**: Download `agent-router-mcp-config.zip` and extract
- **macOS/Linux**: Download `agent-router-mcp-config.tar.gz` and extract with `tar -xzf agent-router-mcp-config.tar.gz`

Place the extracted files in a folder (e.g., `C:\agent-configs\` on Windows or `~/agent-configs/` on macOS/Linux).

Alternatively, download individual files:
- [agents.json](https://raw.githubusercontent.com/yourusername/agent-router-mcp/main/config/agents.json)
- [rules.json](https://raw.githubusercontent.com/yourusername/agent-router-mcp/main/config/rules.json)
- [llm-tags.json](https://raw.githubusercontent.com/yourusername/agent-router-mcp/main/config/llm-tags.json)

### 3. Configure MCP Client

Add to your MCP client's configuration file (location varies by client - check your client's documentation).

**Example (Windows)**:
```json
{
  "mcpServers": {
    "agent-router": {
      "type": "stdio",
      "command": "C:\\path\\to\\agent-router-mcp.exe",
      "env": {
        "OLLAMA_URL": "http://localhost:11434",
        "MODEL_NAME": "smollm3:3b",
        "AGENTS_CONFIG_PATH": "C:\\agent-configs\\agents.json",
        "LLM_TAGS_CONFIG_PATH": "C:\\agent-configs\\llm-tags.json",
        "RULES_CONFIG_PATH": "C:\\agent-configs\\rules.json"
      }
    }
  }
}
```

**Example (macOS/Linux)**:
```json
{
  "mcpServers": {
    "agent-router": {
      "type": "stdio",
      "command": "/path/to/agent-router-mcp",
      "env": {
        "OLLAMA_URL": "http://localhost:11434",
        "MODEL_NAME": "smollm3:3b",
        "AGENTS_CONFIG_PATH": "/Users/me/agent-configs/agents.json",
        "LLM_TAGS_CONFIG_PATH": "/Users/me/agent-configs/llm-tags.json",
        "RULES_CONFIG_PATH": "/Users/me/agent-configs/rules.json"
      }
    }
  }
}
```

Replace the paths with your actual file locations.

---

## MCP Tools Reference

The server exposes 4 tools for managing Ollama and getting routing instructions:

### `get_routing`

Get routing instructions for a user request. **This is the main tool** that performs intelligent routing.

**Input:**
```json
{
  "user_prompt": "Fix the authentication bug",
  "trigger": "user_request",
  "git_context": {
    "branch": "main",
    "changed_files": ["src/auth.ts"],
    "staged_files": []
  },
  "agent_config_path": null,
  "rules_config_path": null,
  "llm_tags_path": null
}
```

**Output (Success):**
```json
{
  "agents": [
    {
      "name": "security-auditor",
      "reason": "Matched file pattern or trigger"
    },
    {
      "name": "language-reviewer-typescript",
      "reason": "Matched file pattern or trigger"
    }
  ],
  "reasoning": "Clear rule-based matches",
  "method": "rules",
  "llm_tags": null
}
```

**Output (Prerequisites Not Met):**

The tool performs automatic prerequisite checks and returns helpful error messages:

```json
{"error": "Ollama is not started. Ask user if Ollama should be started."}
```
```json
{"error": "Model has not been downloaded. Ask user if the model should be pulled."}
```
```json
{"error": "Model is not loaded. Ask the user if the model should be loaded in Ollama."}
```

When you receive these errors, call the appropriate tool (`start_ollama`, `pull_model`, or `load_model`).

### `start_ollama`

Start the Ollama service (useful if it's not already running).

**Output:**
```json
{
  "success": true,
  "message": "Ollama started successfully"
}
```

### `pull_model`

Download a model from the Ollama registry.

**Input:**
```json
{
  "model_name": "smollm3:3b"
}
```

**Output:**
```json
{
  "success": true,
  "message": "Model smollm3:3b pulled successfully"
}
```

### `load_model`

Pre-load the configured model into memory for faster first request.

**Output:**
```json
{
  "success": true,
  "message": "Model smollm3:3b loaded successfully"
}
```

---

## How It Works

1. **Stateless**: No state maintained between requests
2. **Config Loading**: Loads `agents.json`, `rules.json`, `llm-tags.json` fresh each request
3. **Rule Matching**: Evaluates all rules, returns matched agents
4. **High Confidence Check**: If clear matches (files + lifecycle), returns immediately
5. **LLM Tagging**: If ambiguous, calls Ollama to identify semantic tags
6. **Tag Rules**: Applies tag-based rules to get additional agents
7. **LLM Fallback**: If still no matches, asks LLM to directly classify
8. **Return**: JSON result with agent names and routing reasoning

---

## Configuration Files

All routing logic lives in `config/*.json` - edit these to customize behavior:

### `config/agents.json`

Define available subagents (name + description only):

```json
{
  "agents": [
    {
      "name": "security-auditor",
      "description": "Reviews code for security vulnerabilities, secrets, supply chain attacks"
    },
    {
      "name": "language-reviewer-typescript",
      "description": "TypeScript-specific patterns and best practices"
    }
  ]
}
```

### `config/rules.json`

Define routing rules with boolean logic:

```json
{
  "rules": [
    {
      "description": "Route TypeScript files to TS reviewer",
      "conditions": {
        "any_of": [
          {"file_pattern": "*.ts"},
          {"file_pattern": "*.tsx"}
        ]
      },
      "route_to_subagents": ["language-reviewer-typescript"]
    },
    {
      "description": "Security files AND security tag → security auditor",
      "conditions": {
        "all_of": [
          {"file_pattern": "*auth*"},
          {"llm_tag": "security-concern"}
        ]
      },
      "route_to_subagents": ["security-auditor", "code-reviewer"]
    },
    {
      "description": "Commit hooks always trigger code review",
      "conditions": {
        "git_lifecycle": "commit"
      },
      "route_to_subagents": ["code-reviewer"]
    }
  ]
}
```

**Supported Conditions:**
- `file_pattern` - Glob match on file paths (e.g., `*.ts`, `*auth*`)
- `file_regex` - Regex match on file paths
- `prompt_regex` - Regex match on user prompt (e.g., `(?i)test` for case-insensitive)
- `branch_regex` - Regex match on git branch name
- `git_lifecycle` - Match git trigger (`commit`, `pre-commit`, `pull_request`)
- `llm_tag` - Match LLM-identified semantic tags

**Boolean Logic:**
- `any_of` - OR logic (match if ANY condition is true)
- `all_of` - AND logic (match if ALL conditions are true)
- Supports nesting for complex rules

### `config/llm-tags.json`

Define semantic tags for LLM to identify:

```json
{
  "tags": [
    {
      "name": "security-concern",
      "description": "Code that handles authentication, authorization, encryption, secrets...",
      "examples": [
        "JWT token generation",
        "Password hashing",
        "API key handling"
      ]
    }
  ]
}
```

## Customization Examples

### Add a New Agent

Edit `config/agents.json`:
```json
{
  "agents": [
    {
      "name": "my-custom-agent",
      "description": "Does something special"
    }
  ]
}
```

### Add a Routing Rule

Edit `config/rules.json`:
```json
{
  "rules": [
    {
      "description": "Route GraphQL files to API specialist",
      "conditions": {
        "file_pattern": "*.graphql"
      },
      "route_to_subagents": ["api-specialist"]
    },
    {
      "description": "Performance-critical code on hotfix branch",
      "conditions": {
        "all_of": [
          {"llm_tag": "performance-critical"},
          {"branch_regex": "^hotfix/.*"}
        ]
      },
      "route_to_subagents": ["performance-optimizer", "code-reviewer"]
    }
  ]
}
```

### Add a Custom LLM Tag

Edit `config/llm-tags.json`:
```json
{
  "tags": [
    {
      "name": "error-handling",
      "description": "Code that handles errors, exceptions, or error states",
      "examples": [
        "try-catch blocks",
        "error boundaries",
        "Result types"
      ]
    }
  ]
}
```

## Model Switching

**Any Ollama-compatible model works** - just set the `MODEL_NAME` environment variable:

```bash
# Try different models
ollama pull granite4-h-micro:3b
export MODEL_NAME="granite4-h-micro:3b"

ollama pull llama3.2:3b
export MODEL_NAME="llama3.2:3b"

ollama pull qwen2.5:3b
export MODEL_NAME="qwen2.5:3b"
```

**Popular options:**

| Model | Size | Best For |
|-------|------|----------|
| `smollm3:3b` | 3B | Balanced, fast (default) |
| `granite4-h-micro:3b` | 3B | Instruction following |
| `llama3.2:3b` | 3B | General purpose |
| `qwen2.5:3b` | 3B | Code understanding |
| `phi3:3.8b` | 3.8B | Reasoning |

Browse all models: https://ollama.com/library

---

## Architecture

This MCP is a **pure router** - it doesn't execute agents, it just determines which subagents should handle a request.

### Flow Diagram

```
User Request
  ├─ Prompt: "Fix auth bug"
  ├─ Files: src/auth.ts, src/db/users.ts
  └─ Trigger: user_request
       ↓
┌──────────────────────────────────────────────────┐
│  Agent Router MCP (Stateless Router)         │
│                                                  │
│  1. Load Configs                                 │
│     • agents.json (agent definitions)            │
│     • rules.json (routing rules)                 │
│     • llm-tags.json (semantic tags)              │
│                                                  │
│  2. Apply Rule-Based Matching                    │
│     • *.ts → language-reviewer-typescript        │
│     • *auth* → security-auditor                  │
│                                                  │
│  3. LLM Semantic Tagging (if needed)             │
│     • Ollama analyzes code                       │
│     • Returns: ["security-concern"]              │
│                                                  │
│  4. Apply Tag-Based Rules                        │
│     • security-concern → security-auditor        │
│                                                  │
│  5. LLM Fallback (if no matches)                 │
│     • Direct agent classification                │
└──────────────────────────────────────────────────┘
       ↓
Routing Result
  ├─ Agents: [language-reviewer-typescript, security-auditor]
  ├─ Method: "rules"
  └─ Reasoning: "Clear rule-based matches"
```

**Example Agent Names (included in default config):**
- **Language Reviewers**: `language-reviewer-typescript`, `language-reviewer-rust`, `language-reviewer-python`, `language-reviewer-javascript`, `language-reviewer-csharp`, `language-reviewer-lua`, `language-reviewer-zig`, `language-reviewer-gdscript`
- **Security & Quality**: `security-auditor`, `code-reviewer`
- **Testing**: `test-engineer-junior`, `test-engineer-midlevel`, `test-engineer-senior`
- **DevOps**: `devops-engineer-junior`, `devops-engineer-midlevel`, `devops-engineer-senior`
- **Specialized**: `planning-architect`, `documentation-writer`, `performance-optimizer`, `accessibility-specialist`

> **Note:** These are just names for routing. Your actual subagents live elsewhere (e.g., as separate MCP tools, CLI commands, or API endpoints).

## Creating Agents/Subagents

This router determines which agents should handle requests. You need to create the actual agent implementations in your IDE.

### Natively Supported

These platforms have built-in agent support:

#### Claude Code (Sub-Agents)
- Agents are markdown files with YAML frontmatter in `.claude/agents/`
- Supports per-agent model selection (`model: sonnet`, `opus`, `haiku`)
- Full documentation: [Claude Code Agents](https://github.com/anthropics/claude-code)

#### GitHub Copilot (Custom Agents)
- Agents are markdown files in `.github/agents/` or `{org}/.github/agents/`
- Supports custom prompts, tool selection, and MCP servers
- Full documentation: [GitHub Copilot Custom Agents](https://github.blog/changelog/2025-10-28-custom-agents-for-github-copilot/)

#### OpenCode (Agents/Subagents)
- Agents are markdown files in `~/.config/opencode/agent/` or `.opencode/agent/`
- Supports per-agent model selection and tool permissions
- Full documentation: [OpenCode Agents](https://opencode.ai/docs/agents/)

### Workarounds Available

These platforms can use this router via MCP integration or workarounds:

- **Cursor** - Use via MCP integration (agent functionality in development)
- **Windsurf** - Use via MCP integration
- **Cline** - Use via MCP integration
- **Roo** - Use via MCP integration

## Cross-Platform Support

This MCP server works on all major platforms. Ollama installation varies by platform:

| Platform | Installation | Service Behavior |
|----------|-------------|------------------|
| **macOS** | `brew install ollama` | Runs as background service automatically |
| **Linux** | `curl -fsSL https://ollama.com/install.sh \| sh` | Start with `ollama serve` |
| **Windows** | Download from [ollama.com](https://ollama.com) | Runs as Windows service automatically |

## Compiling from Source

If you prefer to build from source instead of downloading pre-built binaries:

### Prerequisites

- **Rust 1.70+**: Install from [rustup.rs](https://rustup.rs)
- **For cross-compilation (optional)**:
  - Zig: `brew install zig` (macOS) or [download from zig.dev](https://ziglang.org/download/)
  - MinGW-w64 (for Windows builds): `brew install mingw-w64` (macOS)

### Quick Build (Current Platform)

```bash
# Clone the repository
git clone https://github.com/yourusername/agent-router-mcp.git
cd agent-router-mcp

# Build using the build script
./scripts/build-all.sh

# Binaries will be created in dist/
```

### Cross-Platform Build (All Targets)

To build binaries for all platforms (Linux, macOS, Windows):

```bash
# Install prerequisites (macOS)
brew install zig mingw-w64

# Install cargo-zigbuild
cargo install cargo-zigbuild

# Build for all platforms
./scripts/build-all.sh

# Binaries will be created in dist/:
# - agent-router-mcp-linux-amd64
# - agent-router-mcp-linux-arm64
# - agent-router-mcp-macos-intel
# - agent-router-mcp-macos-silicon
# - agent-router-mcp-windows-amd64.exe
# - agent-router-mcp-windows-arm64.exe
```

### Manual Build

```bash
# Build for current platform
cargo build --release

# Binary will be at: target/release/agent-router-mcp
# (or target/release/agent-router-mcp.exe on Windows)
```

## Development

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run

# Format code
cargo fmt

# Lint
cargo clippy

# Build for development
cargo build
```

## How It Works

1. **Stateless**: No state maintained between requests
2. **Config Loading**: Loads `agents.json`, `rules.json`, `llm-tags.json` fresh each request
3. **Rule Matching**: Evaluates all rules, returns matched agents
4. **High Confidence Check**: If clear matches (files + lifecycle), returns immediately
5. **LLM Tagging**: If ambiguous, calls Ollama to identify semantic tags
6. **Tag Rules**: Applies tag-based rules to get additional agents
7. **LLM Fallback**: If still no matches, asks LLM to directly classify
8. **Return**: JSON result with agent names and routing reasoning

## License

MIT
