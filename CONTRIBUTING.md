# Contributing to Agent Router MCP

Thank you for your interest in contributing! This document provides guidelines for contributing to the project.

## Code of Conduct

- Be respectful and inclusive
- Welcome newcomers and help them get started
- Focus on constructive feedback
- Assume good intentions

## How to Contribute

### Reporting Bugs

1. Check if the bug has already been reported in [Issues](https://github.com/yourusername/agent-router-mcp/issues)
2. If not, create a new issue using the Bug Report template
3. Include as much detail as possible:
   - Operating system and version
   - Ollama version and model
   - Steps to reproduce
   - Expected vs actual behavior
   - Relevant configuration files

### Suggesting Features

1. Check [existing feature requests](https://github.com/yourusername/agent-router-mcp/issues?q=label%3Aenhancement)
2. Create a new issue using the Feature Request template
3. Clearly describe the problem and proposed solution
4. Consider how it fits with the project's goals

### Submitting Code Changes

#### Before You Start

1. **Check existing work**: Look for related issues or PRs
2. **Discuss major changes**: Open an issue first for significant features
3. **One change per PR**: Keep pull requests focused on a single improvement

#### Development Setup

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/agent-router-mcp.git
cd agent-router-mcp

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Ollama
# macOS: brew install ollama
# Linux: curl -fsSL https://ollama.com/install.sh | sh
# Windows: Download from https://ollama.com

# Build and test
cargo build
cargo test
```

#### Development Workflow

1. **Create a branch**
   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/bug-description
   ```

2. **Make your changes**
   - Write clear, self-documenting code
   - Follow Rust conventions and idioms
   - Keep commits focused and atomic

3. **Test your changes**
   ```bash
   # Run tests
   cargo test

   # Check formatting
   cargo fmt --check

   # Run linter
   cargo clippy -- -D warnings

   # Test on your platform
   cargo build --release
   ```

4. **Format your code**
   ```bash
   cargo fmt
   ```

5. **Commit your changes**
   ```bash
   git add .
   git commit -m "feat: add new routing condition type"
   ```

   Use conventional commit prefixes:
   - `feat:` - New features
   - `fix:` - Bug fixes
   - `docs:` - Documentation changes
   - `test:` - Test additions or fixes
   - `refactor:` - Code refactoring
   - `perf:` - Performance improvements
   - `ci:` - CI/CD changes

6. **Push and create PR**
   ```bash
   git push origin feature/your-feature-name
   ```
   Then open a PR on GitHub using the PR template

#### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Pass all clippy lints (`cargo clippy`)
- Write clear variable and function names
- Add comments for non-obvious logic
- Keep functions focused and small

#### Testing

- Add tests for new functionality
- Ensure all tests pass before submitting
- Test on multiple platforms if possible (Linux, macOS, Windows)

### Configuration Changes

If you're modifying the default config files:

- **agents.json**: Keep agent descriptions clear and concise
- **rules.json**: Add comments explaining complex rule logic
- **llm-tags.json**: Provide good examples for each tag

### Documentation

- Update README.md for user-facing changes
- Update code comments for implementation details
- Add examples for new features
- Keep documentation clear and concise

## Pull Request Process

1. Fill out the PR template completely
2. Link any related issues
3. Ensure CI checks pass
4. Wait for review from maintainers
5. Address any requested changes
6. Once approved, a maintainer will merge your PR

## Review Process

- PRs require at least one approving review
- Maintainers may request changes or ask questions
- Reviews typically happen within a few days
- Be patient and responsive to feedback

## Project Structure

```
agent-router-mcp/
├── src/
│   ├── main.rs           # Entry point and MCP protocol handling
│   ├── types.rs          # Type definitions and config structures
│   ├── rules.rs          # Rule matching engine
│   ├── classifier.rs     # LLM classification logic
│   └── model_manager.rs  # Ollama integration
├── config/               # Default configuration files
├── scripts/              # Build scripts
└── .github/              # GitHub-specific files
```

## Getting Help

- **Questions**: Use [GitHub Discussions](https://github.com/yourusername/agent-router-mcp/discussions)
- **Bugs**: Use [GitHub Issues](https://github.com/yourusername/agent-router-mcp/issues)
- **Real-time**: Check if there's a community chat (Discord, etc.)

## Recognition

Contributors will be recognized in:
- Release notes for their contributions
- GitHub's contributor graph
- Special thanks in major releases

Thank you for contributing! 🎉
