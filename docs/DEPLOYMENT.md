# Deployment Guide

This guide covers the practical hosted story for `sxmc`.

## Recommended Modes

### Local development

Use stdio or local HTTP:

```bash
sxmc serve --watch
sxmc serve --transport http --host 127.0.0.1 --port 8000 --watch
```

### Team or hosted remote endpoint

Prefer bearer auth:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths /absolute/path/to/skills
```

### Strict header-based integration

Use an explicit required header when the caller already injects a custom key:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --require-header "X-API-Key: env:SXMC_MCP_KEY" \
  --paths /absolute/path/to/skills
```

## Operational Checks

### Health endpoint

```bash
curl http://127.0.0.1:8000/healthz
```

The response is useful for:
- load balancer health checks
- deployment monitoring
- confirming auth mode and current skill inventory

### Remote MCP inspection

```bash
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --list
```

## Reverse Proxy Notes

If you place `sxmc` behind a reverse proxy:

- keep `/mcp` and `/healthz` reachable without path rewriting surprises
- preserve request headers needed for auth
- prefer TLS termination at the proxy for internet-facing deployments
- keep body and idle timeout settings friendly to streamable HTTP MCP traffic

## Secret Handling

Prefer secret references over literals:

```bash
sxmc serve --transport http --bearer-token env:SXMC_MCP_TOKEN
sxmc serve --transport http --require-header "Authorization: file:/run/secrets/sxmc_header"
```

Supported secret forms:
- `env:VAR_NAME`
- `file:/absolute/path`

## Common Failure Modes

### Unauthorized remote requests

Symptoms:
- `401 Unauthorized`
- bearer-protected endpoint returns a `WWW-Authenticate` challenge

Check:
- the client is sending the expected header
- the token value matches the deployed config

### Skill changes not appearing

Use:

```bash
sxmc serve --watch
```

Without `--watch`, skills are loaded at process start.

### Resource or prompt not found

Check:
- the skill directory is inside the configured `--paths`
- the prompt/resource name matches the served skill inventory
- the remote server was restarted or reloaded after skill changes

## Recommended Production Pattern

1. Start `sxmc serve --transport http` with `--bearer-token env:...`
2. Put it behind TLS
3. Monitor `/healthz`
4. Use fixed absolute `--paths`
5. Reserve `--watch` for development or carefully managed local deployments
