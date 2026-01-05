# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial release of Agent Router MCP
- Config-driven routing with `agents.json`, `rules.json`, and `llm-tags.json`
- Rule-based matching with boolean logic (any_of, all_of)
- LLM semantic tagging via Ollama integration
- Support for multiple condition types:
  - file_pattern (glob matching)
  - file_regex
  - prompt_regex
  - branch_regex
  - git_lifecycle
  - llm_tag
- Cross-platform binaries (Linux x86_64/ARM64, macOS Intel/Silicon, Windows)
- GitHub Actions CI/CD pipeline
- Automated releases with checksums
- Example configurations for 20 agent types
- Comprehensive documentation

### Security
- Added SECURITY.md with vulnerability reporting guidelines
- Stateless architecture (no persistent state)
- Config files loaded fresh each request

## [0.1.0] - YYYY-MM-DD

### Added
- Initial public release

---

## How to Update This File

When making changes, add entries under `[Unreleased]` in these categories:

- **Added** - New features
- **Changed** - Changes to existing functionality
- **Deprecated** - Features that will be removed
- **Removed** - Removed features
- **Fixed** - Bug fixes
- **Security** - Security-related changes

When releasing a new version:
1. Change `[Unreleased]` to `[X.Y.Z] - YYYY-MM-DD`
2. Add a new `[Unreleased]` section at the top
3. Update the version in `Cargo.toml`
4. Tag the release: `git tag vX.Y.Z`
