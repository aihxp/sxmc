# Demo Notes

If you want a short terminal demo or GIF, use the scripted path instead of typing commands live.

## Scripted Demo

```bash
bash scripts/demo.sh target/debug/sxmc tests/fixtures
```

That sequence covers:
- startup-discovery / next-step guidance
- skill discovery
- `skills -> MCP -> CLI`
- API listing
- `CLI -> AI` inspection

## Recording Suggestions

- terminal-first screencast tools like `asciinema` work well for `sxmc`
- keep the recording under 20 seconds
- show one command per surface instead of narrating every subcommand
- prefer the baked or fixture-based flows so the demo is deterministic

## Suggested Short Sequence

1. `sxmc skills list --paths tests/fixtures`
2. `sxmc doctor`
3. `sxmc stdio "sxmc serve --paths tests/fixtures" --list-tools --limit 5`
4. `sxmc api https://petstore3.swagger.io/api/v3/openapi.json --list`
5. `sxmc inspect cli gh --format toon`
