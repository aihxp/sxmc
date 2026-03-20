# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability in sxmc, please report it responsibly.

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, use one of the following:

1. **GitHub Private Vulnerability Reporting**: Go to the [Security Advisories](https://github.com/aihxp/sxmc/security/advisories) page and click "Report a vulnerability"
2. **Email**: Send details to the repository maintainers via the contact information on their GitHub profiles

### What to include

- Description of the vulnerability
- Steps to reproduce
- sxmc version and OS/architecture
- Potential impact assessment

### Response timeline

- **Acknowledgment**: Within 72 hours
- **Initial assessment**: Within 1 week
- **Fix or mitigation**: Depends on severity, but we aim for critical issues within 2 weeks

## Scope

### In scope

- Vulnerabilities in the sxmc binary itself
- Security scanner bypasses (patterns that should be caught but aren't)
- MCP transport authentication bypasses
- Secret leakage from `env:` or `file:` resolution
- Command injection via skill arguments or API parameters

### Out of scope

- Vulnerabilities in user-authored skills (sxmc provides scanning, but users are responsible for their own skill content)
- Upstream MCP protocol design issues (report these to the [MCP specification](https://github.com/modelcontextprotocol/specification))
- Vulnerabilities in third-party MCP servers connected via `sxmc stdio` or `sxmc http`
