# Security Policy

## Supported Versions

We release security updates for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security issue, please follow these steps:

### 1. **Do Not** Open a Public Issue

Please do not report security vulnerabilities through public GitHub issues.

### 2. Report Privately

Instead, please report security vulnerabilities by emailing:
- Create a private security advisory at: https://github.com/yourusername/agent-router-mcp/security/advisories/new
- Or email the maintainers directly (if contact info is available)

### 3. Include Details

Please include as much information as possible:
- Type of vulnerability
- Steps to reproduce
- Affected versions
- Potential impact
- Suggested fix (if any)

### 4. Response Timeline

- **Initial Response**: We aim to respond within 48 hours
- **Status Update**: We'll provide a status update within 7 days
- **Fix Timeline**: We'll work to release a fix as quickly as possible, typically within 30 days for critical issues

## Security Considerations

### Configuration Files

- **Never commit sensitive data** to `agents.json`, `rules.json`, or `llm-tags.json`
- Be cautious with regex patterns that could cause ReDoS (Regular Expression Denial of Service)
- Validate all file paths to prevent path traversal attacks

### Ollama Integration

- Ensure Ollama is running on localhost or a trusted network
- Use HTTPS for remote Ollama connections
- Be aware that LLM responses are non-deterministic and should not be used for security-critical decisions alone

### Agent Routing

- This tool routes requests to agents but does not execute them
- Ensure your actual agent implementations have proper security controls
- Validate and sanitize all inputs before passing to subagents

## Disclosure Policy

When a security vulnerability is reported:
1. We'll confirm the vulnerability and determine its severity
2. We'll develop and test a fix
3. We'll release a security advisory and patched version
4. We'll credit the reporter (if desired) in the security advisory

## Safe Usage

- Keep Ollama and all dependencies up to date
- Review configuration files before deployment
- Use the principle of least privilege for file system access
- Monitor for unusual routing patterns or behavior
