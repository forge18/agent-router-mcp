# Agent Router MCP

![Alpha](https://img.shields.io/badge/status-alpha-orange)
![Test Coverage](https://img.shields.io/badge/coverage-62%25-yellow)
![License](https://img.shields.io/badge/license-MIT-blue)

> ⚠️ **Alpha Software** - This project is in early development. APIs and configuration formats may change. Not recommended for production use.

A **stateless, config-driven** Model Context Protocol (MCP) server that intelligently routes requests to specialized AI subagents using a hybrid rule-based + LLM approach.

## Key Features

- 🔧 **Fully Config-Driven**: All routing logic defined in JSON - no code changes needed
- 🚀 **Stateless Architecture**: No state between requests, loads configs fresh each time
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

## Installation

### 1. Install Ollama

```bash
# macOS
brew install ollama

# Linux
curl -fsSL https://ollama.com/install.sh | sh

# Windows
# Download from https://ollama.com
```

Start Ollama (Linux only - macOS/Windows run as service):
```bash
ollama serve
```

**Note:** The model (~2GB) will be automatically downloaded on first run.

### 2. Download the Binary

Download the latest release from [GitHub Releases](https://github.com/yourusername/agent-router-mcp/releases) for your platform and place it in a convenient location.

**macOS/Linux**: Make it executable
```bash
chmod +x agent-router-mcp
```

### 3. Download the Config Files

Download the example config files from the repository:
- [agents.json](https://raw.githubusercontent.com/yourusername/agent-router-mcp/main/config/agents.json)
- [rules.json](https://raw.githubusercontent.com/yourusername/agent-router-mcp/main/config/rules.json)
- [llm-tags.json](https://raw.githubusercontent.com/yourusername/agent-router-mcp/main/config/llm-tags.json)

Place them in a folder (e.g., `C:\agent-configs\` on Windows or `~/agent-configs/` on macOS/Linux).

### 4. Configure Your MCP Client

Add to your MCP client's configuration file (location varies by client - check your client's documentation).

**Example (Windows)**:
```json
{
  "mcpServers": {
    "agent-organizer": {
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
    "agent-organizer": {
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

## Example Agent Names (included in default config)

The default [agents.json](config/agents.json) includes 20 example agent names that you can route to. **You define your own agents** - these are just examples:

### Language Reviewers
- `language-reviewer-typescript`, `language-reviewer-rust`, `language-reviewer-python`, `language-reviewer-javascript`, `language-reviewer-csharp`, `language-reviewer-lua`, `language-reviewer-zig`, `language-reviewer-gdscript`

### Security & Quality
- `security-auditor`, `code-reviewer`

### Testing
- `test-engineer-junior`, `test-engineer-midlevel`, `test-engineer-senior`

### DevOps
- `devops-engineer-junior`, `devops-engineer-midlevel`, `devops-engineer-senior`

### Specialized
- `planning-architect`, `documentation-writer`, `performance-optimizer`, `accessibility-specialist`

**Note:** These are just names for routing. Your actual subagents live elsewhere (e.g., as separate MCP tools, CLI commands, or API endpoints).

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

### ✅ macOS
- Install: `brew install ollama`
- Runs as background service automatically

### ✅ Linux
- Install: `curl -fsSL https://ollama.com/install.sh | sh`
- Start: `ollama serve`

### ✅ Windows
- Download from https://ollama.com
- Runs as Windows service automatically

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
# - agent-router-mcp-linux-x86_64
# - agent-router-mcp-linux-aarch64
# - agent-router-mcp-macos-intel
# - agent-router-mcp-macos-silicon
# - agent-router-mcp-windows-x86_64.exe
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
