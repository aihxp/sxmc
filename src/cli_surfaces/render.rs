use std::path::Path;

use serde_json::json;

use crate::cli_surfaces::model::{
    host_profile_spec, AiClientProfile, CliSurfaceProfile, CLI_AI_HOSTS_LAST_VERIFIED,
};

pub(crate) fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            previous_dash = false;
            ch.to_ascii_lowercase()
        } else {
            if previous_dash {
                continue;
            }
            previous_dash = true;
            '-'
        };
        slug.push(mapped);
    }
    slug.trim_matches('-').to_string()
}

pub(crate) fn render_agent_doc(profile: &CliSurfaceProfile, client: AiClientProfile) -> String {
    let spec = host_profile_spec(client);
    let mut lines = vec![
        format!("## sxmc CLI Surface: `{}`", profile.command),
        String::new(),
        format!(
            "Use `{}` as a first-class terminal workflow in this repo for {}.",
            profile.command, spec.label
        ),
        String::new(),
        format!("Summary: {}", profile.summary),
    ];

    if let Some(description) = &profile.description {
        lines.push(String::new());
        lines.push(description.clone());
    }

    if !profile.examples.is_empty() {
        lines.push(String::new());
        lines.push("Preferred flow:".into());
        for (index, example) in profile.examples.iter().take(3).enumerate() {
            lines.push(format!("{}. `{}`", index + 1, example.command));
        }
    } else {
        lines.push(String::new());
        lines.push("Preferred flow:".into());
        lines.push(format!("1. `{} --help`", profile.command));
        if let Some(subcommand) = profile.subcommands.first() {
            lines.push(format!(
                "2. `{} {}` --help",
                profile.command, subcommand.name
            ));
        }
    }

    push_subcommand_block(&mut lines, profile, 8, "Key subcommands");

    if !profile.environment.is_empty() {
        lines.push(String::new());
        lines.push("Environment/auth notes:".into());
        for env in &profile.environment {
            lines.push(format!(
                "- `{}`{}",
                env.name,
                env.summary
                    .as_ref()
                    .map(|summary| format!(": {}", summary))
                    .unwrap_or_default()
            ));
        }
    }

    lines.push(String::new());
    lines.push("Guidance:".into());
    lines.push(
        "- When the exact CLI surface is unclear, start with `sxmc inspect cli <tool> --depth 1 --format json-pretty` instead of pasting raw help output into chat."
            .into(),
    );
    lines.push(
        "- If the CLI has nested command groups, re-run with `--depth 2` to inspect deeper help without manually drilling into each branch."
            .into(),
    );
    lines.push(
        "- When the MCP surface is unknown, start with `sxmc stdio \"<cmd>\" --list` or `sxmc mcp grep <pattern>` before guessing tool calls."
            .into(),
    );
    lines.push(
        "- When the API surface is unknown, start with `sxmc api <url-or-spec> --list` before constructing requests by hand."
            .into(),
    );
    lines.push("- Keep bulky output in files or pipes when possible.".into());
    lines.push("- Prefer machine-friendly flags like `--json` when the CLI supports them.".into());
    lines.push("- Re-check `--help` before using low-confidence flows.".into());
    lines.push(format!(
        "- Startup file convention last verified against official docs on {}.",
        CLI_AI_HOSTS_LAST_VERIFIED
    ));
    lines.push(format!("- Reference: {}", spec.official_reference_url));

    lines.join("\n")
}

pub(crate) fn render_portable_agent_doc(profile: &CliSurfaceProfile) -> String {
    let mut lines = vec![
        format!("## sxmc CLI Surface: `{}`", profile.command),
        String::new(),
        format!(
            "Use `{}` as a portable terminal workflow across AI tools in this repo.",
            profile.command
        ),
        String::new(),
        format!("Summary: {}", profile.summary),
    ];

    if let Some(description) = &profile.description {
        lines.push(String::new());
        lines.push(description.clone());
    }

    lines.push(String::new());
    lines.push("Recommended startup guidance:".into());
    lines.push(
        "- When the exact CLI surface is unclear, start with `sxmc inspect cli <tool> --depth 1 --format json-pretty`."
            .into(),
    );
    lines.push(
        "- For multi-layer CLIs, bump inspection to `--depth 2` before writing detailed agent guidance."
            .into(),
    );
    lines.push(format!(
        "- For this CLI specifically, `{}` `--help` is still a good follow-up once you know you are on the right command.",
        profile.command
    ));
    lines.push(
        "- When the MCP surface is unknown, start with `sxmc stdio \"<cmd>\" --list` or `sxmc mcp grep <pattern>`."
            .into(),
    );
    lines.push(
        "- When the API surface is unknown, start with `sxmc api <url-or-spec> --list`.".into(),
    );
    lines.push("- Prefer machine-friendly flags like `--json` when available.".into());
    lines.push(
        "- Keep bulky output in files or pipes instead of pasting it into chat context.".into(),
    );
    lines.push("- Re-check auth or environment requirements before write actions.".into());
    lines.push(format!(
        "- Host profile conventions in this repo were last verified on {}.",
        CLI_AI_HOSTS_LAST_VERIFIED
    ));

    if !profile.examples.is_empty() {
        lines.push(String::new());
        lines.push("Preferred commands:".into());
        for example in profile.examples.iter().take(4) {
            lines.push(format!("- `{}`", example.command));
        }
    }

    push_subcommand_block(&mut lines, profile, 8, "Key subcommands");

    lines.join("\n")
}

pub(crate) fn render_llms_txt(profile: &CliSurfaceProfile) -> String {
    let mut lines = vec![
        format!("# {}", profile.command),
        String::new(),
        profile.summary.clone(),
    ];

    if let Some(description) = &profile.description {
        lines.push(String::new());
        lines.push(description.clone());
    }

    if !profile.examples.is_empty() {
        lines.push(String::new());
        lines.push("## Recommended Commands".into());
        for example in profile.examples.iter().take(5) {
            lines.push(format!("- `{}`", example.command));
        }
    }

    push_subcommand_block(&mut lines, profile, 12, "## Key Subcommands");

    if !profile.environment.is_empty() {
        lines.push(String::new());
        lines.push("## Environment".into());
        for env in &profile.environment {
            lines.push(format!("- `{}`", env.name));
        }
    }

    lines.push(String::new());
    lines.push("## Notes".into());
    lines.push("- Generated by `sxmc scaffold llms-txt` from a CLI surface profile.".into());
    lines.push("- Review before publishing as project-facing LLM guidance.".into());
    lines.push(format!(
        "- Host profile conventions referenced by this repo were last verified on {}.",
        CLI_AI_HOSTS_LAST_VERIFIED
    ));

    lines.join("\n")
}

pub(crate) fn render_ci_workflow(profile: &CliSurfaceProfile) -> String {
    let slug = slugify(&profile.command);
    let profile_path = format!(".sxmc/ai/profiles/{slug}.json");
    format!(
        r#"name: sxmc drift ({command})

on:
  pull_request:
  workflow_dispatch:

jobs:
  cli-drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install sxmc
        run: cargo install --locked sxmc

      - name: Summarize CLI drift
        run: |
          sxmc inspect diff {command} \
            --before {profile_path} \
            --format markdown | tee -a "$GITHUB_STEP_SUMMARY"

      - name: Fail on unexpected CLI drift
        run: |
          sxmc inspect diff {command} \
            --before {profile_path} \
            --exit-code \
            --format json-pretty >/dev/null

      - name: Snapshot status
        run: sxmc status --format json-pretty
"#,
        command = profile.command,
        profile_path = profile_path,
    )
}

pub(crate) fn render_client_config(
    client: AiClientProfile,
    server_name: &str,
    skills_path: &Path,
) -> String {
    let skills_display = skills_path.display().to_string();
    match client {
        AiClientProfile::OpenaiCodex => format!(
            "# sxmc CLI->AI startup scaffold\n[mcp_servers.{server_name}]\ncommand = \"sxmc\"\nargs = [\"serve\", \"--paths\", \"{skills_display}\"]\n"
        ),
        AiClientProfile::GenericHttpMcp => serde_json::to_string_pretty(&json!({
            "mcpServers": {
                server_name: {
                    "url": "http://127.0.0.1:8000/mcp"
                }
            }
        }))
        .unwrap(),
        AiClientProfile::Cursor => serde_json::to_string_pretty(&json!({
            "mcpServers": {
                server_name: {
                    "type": "stdio",
                    "command": "sxmc",
                    "args": ["serve", "--paths", skills_display]
                }
            }
        }))
        .unwrap(),
        AiClientProfile::GeminiCli => serde_json::to_string_pretty(&json!({
            "mcpServers": {
                server_name: {
                    "command": "sxmc",
                    "args": ["serve", "--paths", skills_display]
                }
            }
        }))
        .unwrap(),
        AiClientProfile::OpenCode => serde_json::to_string_pretty(&json!({
            "mcp": {
                server_name: {
                    "type": "local",
                    "command": ["sxmc", "serve", "--paths", skills_display]
                }
            }
        }))
        .unwrap(),
        _ => serde_json::to_string_pretty(&json!({
            "mcpServers": {
                server_name: {
                    "command": "sxmc",
                    "args": ["serve", "--paths", skills_display]
                }
            }
        }))
        .unwrap(),
    }
}

pub(crate) fn render_skill_markdown(profile: &CliSurfaceProfile) -> String {
    let name = format!("{}-cli", slugify(&profile.command));
    let description = profile
        .description
        .as_deref()
        .unwrap_or(&profile.summary)
        .replace('"', "'");
    let argument_hint = profile
        .positionals
        .iter()
        .map(|positional| format!("<{}>", positional.name))
        .chain(
            profile
                .options
                .iter()
                .take(3)
                .map(|option| option.name.clone()),
        )
        .collect::<Vec<_>>()
        .join(" ");

    let mut body = vec![
        "---".to_string(),
        format!("name: {}", name),
        format!("description: \"{}\"", description),
    ];
    if !argument_hint.trim().is_empty() {
        body.push(format!("argument-hint: \"{}\"", argument_hint));
    }
    body.push("---".to_string());
    body.push(String::new());
    body.push(format!("# {} CLI workflow", profile.command));
    body.push(String::new());
    body.push(profile.summary.clone());

    if let Some(description) = &profile.description {
        body.push(String::new());
        body.push(description.clone());
    }

    if !profile.examples.is_empty() {
        body.push(String::new());
        body.push("Recommended commands:".into());
        for example in profile.examples.iter().take(5) {
            body.push(format!("- `{}`", example.command));
        }
    }

    push_subcommand_block(&mut body, profile, 10, "Key subcommands");

    body.push(String::new());
    body.push("Execution guidance:".into());
    body.push(format!(
        "- Start with `{}` `--help` if the exact shape is unclear.",
        profile.command
    ));
    body.push("- Prefer machine-friendly flags like `--json` when available.".into());
    body.push("- Keep large output in files or pipes instead of pasting it into context.".into());
    body.push(
        "- Re-check auth or environment requirements before performing write actions.".into(),
    );
    body.push(String::new());
    body.push(
        "This file was generated by `sxmc scaffold skill` from a CLI profile and should be reviewed before wider use."
            .into(),
    );
    body.join("\n")
}

pub(crate) fn render_mcp_wrapper_readme(profile: &CliSurfaceProfile) -> String {
    let slug = slugify(&profile.command);
    let mut lines = vec![
        format!("# {} MCP wrapper scaffold", profile.command),
        String::new(),
        "This scaffold is a starting point for wrapping a CLI as a focused MCP server.".into(),
        String::new(),
        "Recommended approach:".into(),
        format!(
            "- Start from the `{}` CLI profile rather than mirroring the whole CLI.",
            slug
        ),
        "- Expose a few narrow tools first.".into(),
        "- Keep outputs machine-friendly and bounded.".into(),
        "- Treat prompts/resources as optional depending on the CLI.".into(),
    ];

    if !profile.subcommands.is_empty() {
        lines.push(String::new());
        lines.push("Candidate tool surfaces:".into());
        for subcommand in profile.subcommands.iter().take(5) {
            lines.push(format!("- `{}`: {}", subcommand.name, subcommand.summary));
        }
        if profile.subcommands.len() > 5 {
            lines.push(format!(
                "- ... plus {} more discovered subcommands in the inspected profile.",
                profile.subcommands.len() - 5
            ));
        }
    }

    if !profile.examples.is_empty() {
        lines.push(String::new());
        lines.push("Examples to preserve in wrapper behavior:".into());
        for example in profile.examples.iter().take(5) {
            lines.push(format!("- `{}`", example.command));
        }
    }

    lines.push(String::new());
    lines.push("Files:".into());
    lines.push(
        "- `manifest.json` captures the inspected profile details and suggested wrapper shape."
            .into(),
    );
    lines.push(
        "- Add server code, tests, and launch scripts beside this scaffold as needed.".into(),
    );
    lines.join("\n")
}

fn push_subcommand_block(
    lines: &mut Vec<String>,
    profile: &CliSurfaceProfile,
    limit: usize,
    heading: &str,
) {
    if profile.subcommands.is_empty() {
        return;
    }

    let total = profile.subcommands.len();
    let shown = total.min(limit);
    lines.push(String::new());
    lines.push(format!("{heading} (showing {shown} of {total}):"));
    for subcommand in profile.subcommands.iter().take(limit) {
        lines.push(format!("- `{}`: {}", subcommand.name, subcommand.summary));
    }
    if total > limit {
        lines.push(format!(
            "- ... plus {} more discovered subcommands in the inspected profile.",
            total - limit
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{push_subcommand_block, render_skill_markdown};
    use crate::cli_surfaces::model::{
        CliSurfaceProfile, ConfidenceLevel, OutputBehavior, ProfileSource, ProfileSubcommand,
        Provenance, PROFILE_SCHEMA,
    };

    fn demo_profile_with_subcommands(count: usize) -> CliSurfaceProfile {
        CliSurfaceProfile {
            profile_schema: PROFILE_SCHEMA.into(),
            command: "demo".into(),
            summary: "Demo CLI summary.".into(),
            description: Some("Demo CLI description.".into()),
            source: ProfileSource {
                kind: "cli".into(),
                identifier: "demo".into(),
            },
            subcommands: (0..count)
                .map(|index| ProfileSubcommand {
                    name: format!("cmd-{index}"),
                    summary: format!("Command {index}."),
                    confidence: ConfidenceLevel::Medium,
                })
                .collect(),
            subcommand_profiles: Vec::new(),
            options: Vec::new(),
            positionals: Vec::new(),
            examples: Vec::new(),
            auth: Vec::new(),
            environment: Vec::new(),
            output_behavior: OutputBehavior {
                stdout_style: "plain_text".into(),
                stderr_usage: "unknown".into(),
                machine_friendly: false,
            },
            workflows: Vec::new(),
            confidence_notes: Vec::new(),
            provenance: Provenance {
                generated_by: "sxmc".into(),
                generator_version: env!("CARGO_PKG_VERSION").into(),
                source_kind: "cli".into(),
                source_identifier: "demo".into(),
                profile_schema: PROFILE_SCHEMA.into(),
                generation_depth: 0,
                generated_at: "unix:0".into(),
            },
        }
    }

    #[test]
    fn push_subcommand_block_reports_total_and_overflow() {
        let profile = demo_profile_with_subcommands(12);
        let mut lines = Vec::new();
        push_subcommand_block(&mut lines, &profile, 5, "Key subcommands");
        let rendered = lines.join("\n");
        assert!(rendered.contains("Key subcommands (showing 5 of 12):"));
        assert!(rendered.contains("plus 7 more discovered subcommands"));
    }

    #[test]
    fn render_skill_markdown_surfaces_more_than_five_subcommands() {
        let profile = demo_profile_with_subcommands(14);
        let rendered = render_skill_markdown(&profile);
        assert!(rendered.contains("Key subcommands (showing 10 of 14):"));
        assert!(rendered.contains("plus 4 more discovered subcommands"));
    }
}
