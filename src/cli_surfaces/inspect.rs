use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::{json, Value};

use crate::cli_surfaces::model::{
    AuthRequirement, CliSurfaceProfile, ConfidenceLevel, ConfidenceNote, EnvironmentRequirement,
    OutputBehavior, ProfileExample, ProfileOption, ProfilePositional, ProfileSource,
    ProfileSubcommand, Provenance, Workflow, PROFILE_SCHEMA,
};
use crate::error::{Result, SxmcError};

pub fn parse_command_spec(command: &str) -> Result<Vec<String>> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if trimmed.starts_with('[') {
        return serde_json::from_str::<Vec<String>>(trimmed).map_err(|e| {
            SxmcError::Other(format!(
                "Invalid command JSON array. Expected [\"cmd\", \"arg1\", ...]: {}",
                e
            ))
        });
    }

    #[cfg(windows)]
    {
        if let Some(parts) = parse_windows_command_spec(trimmed) {
            return Ok(parts);
        }
        return Ok(trimmed.split_whitespace().map(str::to_string).collect());
    }

    #[cfg(not(windows))]
    shlex::split(trimmed).ok_or_else(|| {
        SxmcError::Other(
            "Invalid command string. Use shell-style quoting or a JSON array command spec.".into(),
        )
    })
}

#[cfg(windows)]
fn parse_windows_command_spec(command: &str) -> Option<Vec<String>> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }

    if let Some(rest) = trimmed.strip_prefix('"') {
        let quote_end = rest.find('"')?;
        let executable = &rest[..quote_end];
        let args = rest[quote_end + 1..].trim();
        let mut parts = vec![executable.to_string()];
        parts.extend(args.split_whitespace().map(str::to_string));
        return Some(parts);
    }

    let executable_pattern = Regex::new(r"(?i)^(.+?\.(exe|cmd|bat|ps1))(?:\s+(.*))?$").ok()?;
    let captures = executable_pattern.captures(trimmed)?;
    let executable = captures.get(1)?.as_str();
    let mut parts = vec![executable.to_string()];
    if let Some(args) = captures.get(3) {
        parts.extend(args.as_str().split_whitespace().map(str::to_string));
    }
    Some(parts)
}

pub fn inspect_cli(command_spec: &str, allow_self: bool) -> Result<CliSurfaceProfile> {
    inspect_cli_with_depth(command_spec, allow_self, 0)
}

pub fn inspect_cli_with_depth(
    command_spec: &str,
    allow_self: bool,
    depth: usize,
) -> Result<CliSurfaceProfile> {
    let parts = parse_command_spec(command_spec)?;
    if parts.is_empty() {
        return Err(SxmcError::Other("Empty command spec".into()));
    }

    let executable = &parts[0];
    let command_name = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(executable)
        .to_string();

    if !allow_self && is_self_command(&command_name) {
        return Err(SxmcError::Other(
            "Refusing to inspect sxmc itself without --allow-self".into(),
        ));
    }

    inspect_parts(&parts, &command_name, executable, allow_self, depth, 0)
}

pub fn load_profile(path: &Path) -> Result<CliSurfaceProfile> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn profile_value(profile: &CliSurfaceProfile) -> Value {
    serde_json::to_value(profile).unwrap_or_else(|_| json!({}))
}

fn is_self_command(command_name: &str) -> bool {
    let lowered = command_name.to_ascii_lowercase();
    lowered == "sxmc" || lowered == "sxmc.exe"
}

fn inspect_parts(
    parts: &[String],
    command_name: &str,
    source_identifier: &str,
    allow_self: bool,
    remaining_depth: usize,
    generation_depth: u32,
) -> Result<CliSurfaceProfile> {
    let help_text = read_help_text(parts, command_name)?;
    let mut profile = parse_help_text(command_name, source_identifier, &help_text);
    profile.provenance.generation_depth = generation_depth;

    if remaining_depth > 0 {
        let mut subcommand_profiles = Vec::new();
        for subcommand in profile
            .subcommands
            .iter()
            .filter(|subcommand| subcommand.confidence != ConfidenceLevel::Low)
        {
            if subcommand.name == command_name {
                continue;
            }

            let mut child_parts = parts.to_vec();
            child_parts.push(subcommand.name.clone());
            let child_source = format!("{source_identifier} {}", subcommand.name);
            let child_name = subcommand.name.clone();

            if let Ok(child_profile) = inspect_parts(
                &child_parts,
                &child_name,
                &child_source,
                allow_self,
                remaining_depth.saturating_sub(1),
                generation_depth + 1,
            ) {
                subcommand_profiles.push(child_profile);
            }
        }

        if !subcommand_profiles.is_empty() {
            profile.subcommand_profiles = subcommand_profiles;
        }
    }

    if remaining_depth > 0
        && profile.subcommand_profiles.is_empty()
        && !profile.subcommands.is_empty()
    {
        profile.confidence_notes.push(ConfidenceNote {
            level: ConfidenceLevel::Low,
            summary: "Recursive inspection was requested, but nested subcommand help could not be collected for this CLI.".into(),
        });
    }

    if looks_generic_summary(&profile.summary, command_name)
        && looks_like_man_fallback_candidate(&help_text)
    {
        profile.confidence_notes.push(ConfidenceNote {
            level: ConfidenceLevel::Low,
            summary: "Help output stayed generic even after inspection; review generated startup docs before applying them.".into(),
        });
    }

    if !allow_self && is_self_command(command_name) {
        return Err(SxmcError::Other(
            "Refusing to inspect sxmc itself without --allow-self".into(),
        ));
    }

    Ok(profile)
}

fn read_help_text(parts: &[String], command_name: &str) -> Result<String> {
    let mut candidates = Vec::new();

    if let Ok(primary) = run_help_variant(parts, &["--help"]) {
        let lowered = primary.to_ascii_lowercase();
        candidates.push(primary.clone());

        if lowered.contains("--help-all") || lowered.contains("complete help information") {
            if let Ok(text) = run_help_variant(parts, &["--help-all"]) {
                if !text.trim().is_empty() {
                    candidates.push(text);
                }
            }
        }
        if lowered.contains("--help all") || lowered.contains("help all") {
            if let Ok(text) = run_help_variant(parts, &["--help", "all"]) {
                if !text.trim().is_empty() {
                    candidates.push(text);
                }
            }
        }
    }

    if let Ok(text) = read_man_page_text(command_name) {
        if !text.trim().is_empty() {
            candidates.push(text);
        }
    }

    candidates
        .into_iter()
        .max_by_key(|text| score_help_text(command_name, &parts[0], text))
        .ok_or_else(|| {
            SxmcError::Other(format!(
                "Command '{}' did not return readable help output",
                parts[0]
            ))
        })
}

#[cfg(not(windows))]
fn read_man_page_text(command_name: &str) -> Result<String> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg("MANPAGER=cat man \"$SXMC_MAN_TARGET\" 2>/dev/null | col -b")
        .env("SXMC_MAN_TARGET", command_name)
        .output()
        .map_err(|e| {
            SxmcError::Other(format!(
                "Failed to query man page for '{}': {}",
                command_name, e
            ))
        })?;

    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        return Err(SxmcError::Other(format!(
            "No readable man page output for '{}'",
            command_name
        )));
    }

    Ok(text)
}

#[cfg(windows)]
fn read_man_page_text(_command_name: &str) -> Result<String> {
    Err(SxmcError::Other(
        "man-page fallback is not available on Windows".into(),
    ))
}

fn run_help_variant(parts: &[String], extra_args: &[&str]) -> Result<String> {
    let mut command = Command::new(&parts[0]);
    if parts.len() > 1 {
        command.args(&parts[1..]);
    }
    command.args(extra_args);
    let output = command.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            SxmcError::Other(format!(
                "Could not find command '{}' on PATH while probing help. Install it first or pass a full executable path.",
                parts[0]
            ))
        } else {
            SxmcError::Other(format!(
                "Failed to run '{} {}': {}",
                parts[0],
                extra_args.join(" "),
                e
            ))
        }
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let text = if !stdout.trim().is_empty() {
        stdout
    } else {
        stderr
    };

    if !output.status.success() && text.trim().is_empty() {
        return Err(SxmcError::Other(format!(
            "Command '{}' did not return readable help output",
            parts[0]
        )));
    }

    Ok(text)
}

fn score_help_text(command_name: &str, source_identifier: &str, help: &str) -> i32 {
    let profile = parse_help_text(command_name, source_identifier, help);
    let mut score = 0;

    if profile.summary != format!("{} command-line interface", command_name) {
        score += 10;
    }
    if !profile.summary.to_ascii_lowercase().starts_with("usage:") {
        score += 5;
    }
    score += (profile.subcommands.len() as i32) * 4;
    score += (profile.options.len() as i32) * 2;
    score += (profile.examples.len() as i32) * 3;
    if profile.description.is_some() {
        score += 2;
    }

    score
}

fn parse_help_text(command_name: &str, source_identifier: &str, help: &str) -> CliSurfaceProfile {
    let lines: Vec<&str> = help.lines().collect();
    let summary = select_summary(&lines, command_name);
    let description = parse_description(&lines, command_name, &summary);
    let subcommands = parse_subcommands(&lines, command_name);
    let options = parse_options(&lines);
    let positionals = parse_positionals(&lines, command_name);
    let examples = parse_examples(&lines, command_name);
    let (auth, environment) = infer_requirements(help);
    let workflows = infer_workflows(&subcommands);
    let output_behavior = infer_output_behavior(help);
    let mut confidence_notes = vec![ConfidenceNote {
        level: ConfidenceLevel::Medium,
        summary: "This profile was inferred from help output and may omit dynamic or plugin-provided behavior.".into(),
    }];
    if examples.is_empty() {
        confidence_notes.push(ConfidenceNote {
            level: ConfidenceLevel::Low,
            summary: "No examples were detected in help output; generated agent guidance may need manual examples.".into(),
        });
    }

    CliSurfaceProfile {
        profile_schema: PROFILE_SCHEMA.into(),
        command: command_name.into(),
        summary,
        description,
        source: ProfileSource {
            kind: "cli".into(),
            identifier: source_identifier.into(),
        },
        subcommands,
        subcommand_profiles: Vec::new(),
        options,
        positionals,
        examples,
        auth,
        environment,
        output_behavior,
        workflows,
        confidence_notes,
        provenance: Provenance {
            generated_by: "sxmc".into(),
            generator_version: env!("CARGO_PKG_VERSION").into(),
            source_kind: "cli".into(),
            source_identifier: source_identifier.into(),
            profile_schema: PROFILE_SCHEMA.into(),
            generation_depth: 0,
            generated_at: now_string(),
        },
    }
}

fn select_summary(lines: &[&str], command_name: &str) -> String {
    if let Some(summary) = parse_man_name_summary(lines, command_name) {
        return summary;
    }

    let first_non_empty = lines
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .unwrap_or(command_name);

    let mut skipping_usage_block = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            skipping_usage_block = false;
            continue;
        }
        if skipping_usage_block {
            if line.starts_with(char::is_whitespace)
                || trimmed.starts_with('[')
                || trimmed.starts_with('<')
            {
                continue;
            }
            skipping_usage_block = false;
        }
        if is_unhelpful_summary_line(trimmed, command_name) {
            if looks_like_usage_line(trimmed, command_name) {
                skipping_usage_block = true;
            }
            continue;
        }
        return sanitize_for_profile(trimmed, command_name);
    }

    if !is_unhelpful_summary_line(first_non_empty, command_name) {
        sanitize_for_profile(first_non_empty, command_name)
    } else {
        format!("{} command-line interface", command_name)
    }
}

fn parse_description(lines: &[&str], command_name: &str, summary: &str) -> Option<String> {
    if let Some(description) = parse_man_description(lines, command_name) {
        return Some(description);
    }

    let mut description = Vec::new();
    let mut started = false;
    let mut skipped_summary = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if started {
                break;
            }
            continue;
        }
        let sanitized = sanitize_for_profile(trimmed, command_name);
        if !skipped_summary && sanitized == summary {
            skipped_summary = true;
            continue;
        }
        if is_major_section_heading(trimmed) || looks_like_usage_line(trimmed, command_name) {
            break;
        }
        started = true;
        description.push(sanitized);
    }
    if description.is_empty() {
        None
    } else {
        Some(description.join(" "))
    }
}

fn parse_subcommands(lines: &[&str], command_name: &str) -> Vec<ProfileSubcommand> {
    let mut subcommands = Vec::new();
    let mut in_command_section = false;

    for line in lines {
        let trimmed = line.trim_end();
        let stripped = trimmed.trim();

        if stripped.is_empty() {
            continue;
        }

        if is_command_section_heading(stripped)
            || stripped.starts_with("These are common ")
            || stripped.starts_with("These are available ")
        {
            in_command_section = true;
            continue;
        }

        if !in_command_section {
            continue;
        }

        if is_major_section_heading(stripped) && !is_command_section_heading(stripped) {
            if !subcommands.is_empty() {
                break;
            }
            continue;
        }

        if let Some((name, summary, confidence)) = parse_subcommand_row(stripped, command_name) {
            push_subcommand(
                &mut subcommands,
                ProfileSubcommand {
                    name,
                    summary,
                    confidence,
                },
            );
            continue;
        }

        for name in parse_subcommand_list(stripped) {
            push_subcommand(
                &mut subcommands,
                ProfileSubcommand {
                    name,
                    summary: "Listed in CLI help output.".into(),
                    confidence: ConfidenceLevel::Medium,
                },
            );
        }
    }

    for inferred in parse_usage_subcommands(lines, command_name) {
        push_subcommand(&mut subcommands, inferred);
    }

    subcommands
}

fn parse_options(lines: &[&str]) -> Vec<ProfileOption> {
    let mut options = Vec::new();
    let mut in_options = false;

    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        if is_named_section(trimmed, &["options", "flags"]) {
            in_options = true;
            continue;
        }
        if !in_options {
            continue;
        }
        if is_major_section_heading(trimmed.trim())
            && !is_named_section(trimmed, &["options", "flags"])
        {
            break;
        }
        if let Some(option) = parse_option_entry(trimmed) {
            options.push(option);
        } else if let Some(last) = options.last_mut() {
            let continuation = trimmed.trim();
            if !continuation.starts_with('-') {
                let merged = match &last.summary {
                    Some(existing) => format!("{existing} {continuation}"),
                    None => continuation.to_string(),
                };
                last.summary = Some(merged.trim().to_string());
            }
        }
    }
    if options.is_empty() && looks_like_man_page(lines) {
        return parse_man_options(lines);
    }

    options
}

fn parse_positionals(lines: &[&str], command_name: &str) -> Vec<ProfilePositional> {
    let usage_line = lines
        .iter()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("Usage:") {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    if usage_line.is_empty() {
        return Vec::new();
    }

    usage_line
        .split_whitespace()
        .skip_while(|token| *token != command_name && !token.ends_with(command_name))
        .skip(1)
        .filter_map(|token| {
            if token.starts_with('-') || token.starts_with('[') || token == "[COMMAND]" {
                return None;
            }
            if !(token.starts_with('<') && token.ends_with('>')
                || token
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c == '-'))
            {
                return None;
            }
            let name = token.trim_matches(&['<', '>'][..]).trim_matches('.');
            if name.is_empty() {
                return None;
            }
            Some(ProfilePositional {
                name: name.to_ascii_lowercase(),
                required: true,
                summary: None,
                confidence: ConfidenceLevel::Medium,
            })
        })
        .collect()
}

fn parse_examples(lines: &[&str], command_name: &str) -> Vec<ProfileExample> {
    let mut examples = Vec::new();
    let mut in_examples = false;
    for line in lines {
        let trimmed = line.trim_end();
        let stripped = trimmed.trim();
        if stripped.is_empty() {
            if in_examples && !examples.is_empty() {
                break;
            }
            continue;
        }
        if is_named_section(stripped, &["examples", "example", "example usage"]) {
            in_examples = true;
            continue;
        }
        if !in_examples {
            continue;
        }
        if is_major_section_heading(stripped) {
            break;
        }
        if stripped.starts_with(command_name) || stripped.starts_with('$') {
            examples.push(ProfileExample {
                command: sanitize_for_profile(stripped.trim_start_matches("$ "), command_name),
                summary: None,
                confidence: ConfidenceLevel::High,
            });
        }
    }
    examples
}

fn infer_requirements(help: &str) -> (Vec<AuthRequirement>, Vec<EnvironmentRequirement>) {
    let mut auth = Vec::new();
    let mut environment = Vec::new();
    let mut seen_env = std::collections::BTreeSet::new();
    let lowered = help.to_ascii_lowercase();
    let auth_regex = Regex::new(r"\b(login|authenticate|authentication|auth)\b").unwrap();

    if auth_regex.is_match(&lowered) {
        auth.push(AuthRequirement {
            kind: "interactive".into(),
            summary:
                "Help output mentions login/authentication, so interactive setup may be required."
                    .into(),
        });
    }

    let env_regex = Regex::new(r"\b([A-Z][A-Z0-9_]{2,})\b").unwrap();
    for capture in env_regex.captures_iter(help) {
        let name = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
        if (name.ends_with("_TOKEN")
            || name.ends_with("_KEY")
            || name.ends_with("_SECRET")
            || name == "TOKEN")
            && seen_env.insert(name.to_string())
        {
            environment.push(EnvironmentRequirement {
                name: name.into(),
                summary: Some(
                    "Detected in help output; likely needed for auth or configuration.".into(),
                ),
                required: true,
            });
            auth.push(AuthRequirement {
                kind: "env_var".into(),
                summary: format!("Help output mentions environment variable `{}`.", name),
            });
        }
    }

    (auth, environment)
}

fn infer_workflows(subcommands: &[ProfileSubcommand]) -> Vec<Workflow> {
    if subcommands.is_empty() {
        return Vec::new();
    }
    let steps = subcommands
        .iter()
        .take(3)
        .map(|subcommand| format!("Use `{}` for {}", subcommand.name, subcommand.summary))
        .collect();
    vec![Workflow {
        name: "Common command flow".into(),
        steps,
        confidence: ConfidenceLevel::Medium,
    }]
}

fn infer_output_behavior(help: &str) -> OutputBehavior {
    let lowered = help.to_ascii_lowercase();
    OutputBehavior {
        stdout_style: if lowered.contains("--json") || lowered.contains(" json ") {
            "mixed".into()
        } else {
            "plain_text".into()
        },
        stderr_usage: "Unknown; inspect live behavior before piping stderr into machine parsers."
            .into(),
        machine_friendly: lowered.contains("--json") || lowered.contains("json output"),
    }
}

fn is_named_section(line: &str, headings: &[&str]) -> bool {
    let normalized = line
        .trim()
        .trim_end_matches(':')
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    headings.iter().any(|heading| {
        normalized == *heading
            || normalized.contains(heading)
            || normalized.contains(&format!("{heading} "))
            || normalized.ends_with(&format!(" {heading}"))
    })
}

fn is_major_section_heading(line: &str) -> bool {
    if line.ends_with(':') {
        return true;
    }

    let has_alpha = line.chars().any(|c| c.is_ascii_alphabetic());
    let is_upperish = has_alpha
        && line.chars().all(|c| {
            c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || c.is_ascii_whitespace()
                || matches!(c, '&' | '/' | '-' | '_' | '(' | ')')
        });

    is_upperish
}

fn looks_like_man_page(lines: &[&str]) -> bool {
    lines.iter().any(|line| {
        let trimmed = line.trim();
        trimmed == "NAME"
            || trimmed == "SYNOPSIS"
            || trimmed == "DESCRIPTION"
            || trimmed == "OPTIONS"
    })
}

fn is_command_section_heading(line: &str) -> bool {
    let trimmed = line.trim();
    let lowered = trimmed.trim_end_matches(':').to_ascii_lowercase();
    let has_keyword = lowered.contains("command") || lowered.contains("subcommand");
    let all_caps = trimmed.chars().any(|c| c.is_ascii_alphabetic())
        && trimmed.chars().all(|c| {
            c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || c.is_ascii_whitespace()
                || matches!(c, '&' | '/' | '-' | '_' | '(' | ')' | ':')
        });

    has_keyword && (trimmed.ends_with(':') || all_caps || lowered == "usage")
}

fn looks_like_usage_line(line: &str, command_name: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.starts_with("usage:")
        || lowered == "usage"
        || lowered.starts_with(&format!("{command_name} "))
}

fn looks_like_man_fallback_candidate(help: &str) -> bool {
    help.lines()
        .any(|line| matches!(line.trim(), "NAME" | "SYNOPSIS" | "DESCRIPTION"))
}

fn is_unhelpful_summary_line(line: &str, command_name: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    let trimmed = line.trim();
    let descriptive_intro =
        trimmed.starts_with("These are common ") || trimmed.starts_with("These are available ");

    trimmed.is_empty()
        || (!descriptive_intro && is_major_section_heading(trimmed))
        || looks_like_usage_line(trimmed, command_name)
        || looks_like_option_line(trimmed)
        || looks_like_cli_example_line(trimmed, command_name)
        || parse_subcommand_row(trimmed, command_name).is_some()
        || !parse_subcommand_list(trimmed).is_empty()
        || lowered.starts_with("error:")
        || lowered.contains("unrecognized option")
        || lowered.contains("unknown option")
        || lowered.contains("invalid option")
        || lowered.starts_with("try '")
        || lowered.starts_with("see ")
        || trimmed == command_name
        || trimmed == format!("{command_name} <command>")
        || is_version_banner(trimmed)
}

fn looks_generic_summary(summary: &str, command_name: &str) -> bool {
    let trimmed = summary.trim();
    trimmed.eq_ignore_ascii_case(&format!("{command_name} command-line interface"))
        || trimmed.eq_ignore_ascii_case(command_name)
        || trimmed.starts_with("usage:")
}

fn is_version_banner(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.contains("version")
        && !lowered.contains("command")
        && line.chars().any(|c| c.is_ascii_digit())
}

fn looks_like_option_line(line: &str) -> bool {
    line.trim_start().starts_with('-')
}

fn looks_like_cli_example_line(line: &str, command_name: &str) -> bool {
    let trimmed = line.trim_start_matches("$ ").trim();
    trimmed.starts_with(&format!("{command_name} "))
}

fn sanitize_for_profile(text: &str, command_name: &str) -> String {
    let unix_path = Regex::new(r"(?P<prefix>^|[\s(])(?P<path>/[^\s:]+)").unwrap();
    let windows_path = Regex::new(r"(?P<prefix>^|[\s(])(?P<path>[A-Za-z]:\\[^\s:]+)").unwrap();

    let mut sanitized = unix_path
        .replace_all(text, |caps: &regex::Captures<'_>| {
            let prefix = caps.name("prefix").map(|m| m.as_str()).unwrap_or_default();
            let path = caps.name("path").map(|m| m.as_str()).unwrap_or_default();
            let replacement = Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or(command_name);
            format!("{prefix}{replacement}")
        })
        .into_owned();

    sanitized = windows_path
        .replace_all(&sanitized, |caps: &regex::Captures<'_>| {
            let prefix = caps.name("prefix").map(|m| m.as_str()).unwrap_or_default();
            let path = caps.name("path").map(|m| m.as_str()).unwrap_or_default();
            let replacement = Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or(command_name);
            format!("{prefix}{replacement}")
        })
        .into_owned();

    sanitized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_man_name_summary(lines: &[&str], command_name: &str) -> Option<String> {
    let mut in_name = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "NAME" {
            in_name = true;
            continue;
        }
        if !in_name {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if is_major_section_heading(trimmed) {
            break;
        }

        let summary = trimmed
            .split_once(" - ")
            .or_else(|| trimmed.split_once(" – "))
            .map(|(_, summary)| summary.trim())
            .unwrap_or(trimmed);
        let sanitized = sanitize_for_profile(summary, command_name);
        if !sanitized.is_empty() {
            return Some(sanitized);
        }
    }
    None
}

fn parse_man_description(lines: &[&str], command_name: &str) -> Option<String> {
    let mut in_description = false;
    let mut collected = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "DESCRIPTION" {
            in_description = true;
            continue;
        }
        if !in_description {
            continue;
        }
        if trimmed.is_empty() {
            if !collected.is_empty() {
                break;
            }
            continue;
        }
        if is_major_section_heading(trimmed) {
            break;
        }
        if trimmed.starts_with('-') {
            break;
        }
        collected.push(sanitize_for_profile(trimmed, command_name));
    }

    (!collected.is_empty()).then(|| collected.join(" "))
}

fn parse_man_options(lines: &[&str]) -> Vec<ProfileOption> {
    let mut options = Vec::new();
    let mut in_description = false;
    for line in lines {
        let trimmed = line.trim_end();
        let stripped = trimmed.trim();
        if stripped == "DESCRIPTION" || stripped == "OPTIONS" {
            in_description = true;
            continue;
        }
        if !in_description {
            continue;
        }
        if stripped.is_empty() {
            continue;
        }
        if is_major_section_heading(stripped) && stripped != "DESCRIPTION" && stripped != "OPTIONS"
        {
            break;
        }
        if let Some(option) = parse_option_entry(stripped) {
            options.push(option);
        } else if let Some(last) = options.last_mut() {
            if !stripped.starts_with('-') {
                let merged = match &last.summary {
                    Some(existing) => format!("{existing} {stripped}"),
                    None => stripped.to_string(),
                };
                last.summary = Some(merged);
            }
        }
    }
    options
}

fn parse_subcommand_row(
    line: &str,
    command_name: &str,
) -> Option<(String, String, ConfidenceLevel)> {
    let colon_match = Regex::new(
        r"^(?P<name>[A-Za-z0-9][A-Za-z0-9._-]*(?:,\s*[A-Za-z0-9._-]+)*)\s*:\s+(?P<summary>.+)$",
    )
    .unwrap();
    if let Some(caps) = colon_match.captures(line) {
        let raw_name = caps.name("name")?.as_str().trim();
        let summary = caps.name("summary")?.as_str().trim();
        return Some((
            canonical_subcommand_name(raw_name),
            sanitize_for_profile(summary, command_name),
            ConfidenceLevel::High,
        ));
    }

    let columns: Vec<&str> = line
        .split("  ")
        .filter(|chunk| !chunk.trim().is_empty())
        .collect();
    if columns.len() >= 2 {
        let raw_name = columns[0].trim();
        if raw_name.starts_with('-') {
            return None;
        }
        return Some((
            canonical_subcommand_name(raw_name),
            sanitize_for_profile(&columns[1..].join(" "), command_name),
            ConfidenceLevel::High,
        ));
    }

    None
}

fn canonical_subcommand_name(raw: &str) -> String {
    raw.split(',')
        .next()
        .map(str::trim)
        .unwrap_or(raw)
        .to_string()
}

fn parse_subcommand_list(line: &str) -> Vec<String> {
    if !line.contains(',') {
        return Vec::new();
    }

    let items: Vec<&str> = line
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect();

    if items.len() < 2 {
        return Vec::new();
    }

    if items.iter().all(|item| {
        item.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    }) {
        return items.into_iter().map(str::to_string).collect();
    }

    Vec::new()
}

fn push_subcommand(subcommands: &mut Vec<ProfileSubcommand>, candidate: ProfileSubcommand) {
    if !subcommands
        .iter()
        .any(|existing| existing.name == candidate.name)
    {
        subcommands.push(candidate);
    }
}

fn parse_usage_subcommands(lines: &[&str], command_name: &str) -> Vec<ProfileSubcommand> {
    let mut inferred = Vec::new();
    let mut in_usage_block = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if in_usage_block {
                break;
            }
            continue;
        }

        let usage_line = if let Some(rest) = trimmed.strip_prefix("Usage:") {
            in_usage_block = true;
            rest.trim()
        } else if in_usage_block && trimmed.starts_with(command_name) {
            trimmed
        } else {
            continue;
        };

        let tokens: Vec<&str> = usage_line.split_whitespace().collect();
        let mut iter = tokens.into_iter().peekable();
        while let Some(token) = iter.next() {
            if token == command_name || token.ends_with(&format!("/{command_name}")) {
                if let Some(next) = iter.peek().copied() {
                    if is_literal_subcommand_token(next) {
                        inferred.push(ProfileSubcommand {
                            name: next.to_string(),
                            summary: "Inferred from usage examples in help output.".into(),
                            confidence: ConfidenceLevel::Medium,
                        });
                    }
                }
                break;
            }
        }
    }

    inferred
}

fn is_literal_subcommand_token(token: &str) -> bool {
    !token.is_empty()
        && !token.starts_with('-')
        && !token.starts_with('<')
        && !token.starts_with('[')
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || matches!(c, '-' | '_' | '.'))
}

fn parse_option_entry(line: &str) -> Option<ProfileOption> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('-') {
        return None;
    }

    let (signature, summary) = split_option_signature(trimmed);
    let short_regex = Regex::new(r"(^|[\s,])(-[A-Za-z0-9?])(?:$|[\s,=:\[])").unwrap();
    let long_regex = Regex::new(r"(--[A-Za-z0-9][A-Za-z0-9-]*)").unwrap();
    let value_regex = Regex::new(
        r"(?:(?:--[A-Za-z0-9][A-Za-z0-9-]*|-[A-Za-z0-9?]))(?:[ =]([A-Z<>\[\]\-_|.]+|\.\.\.))",
    )
    .unwrap();

    let short = short_regex
        .captures(signature)
        .and_then(|caps| caps.get(2).map(|m| m.as_str().to_string()));
    let long = long_regex
        .captures(signature)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()));
    let name = long.clone().or_else(|| short.clone())?;
    let value_name = value_regex.captures(signature).and_then(|caps| {
        caps.get(1)
            .map(|m| m.as_str().trim_matches(&['<', '>'][..]).to_string())
    });

    Some(ProfileOption {
        name,
        short,
        value_name,
        required: false,
        summary: summary.map(|value| value.to_string()),
        confidence: if long.is_some() {
            ConfidenceLevel::High
        } else {
            ConfidenceLevel::Medium
        },
    })
}

fn split_option_signature(line: &str) -> (&str, Option<&str>) {
    if let Some(index) = line.find("  ") {
        let (signature, rest) = line.split_at(index);
        let summary = rest.trim().trim_start_matches(':').trim();
        return (signature.trim(), (!summary.is_empty()).then_some(summary));
    }

    if let Some(index) = line.find(':') {
        let (signature, rest) = line.split_at(index);
        if signature.contains('-') {
            let summary = rest.trim_start_matches(':').trim();
            return (signature.trim(), (!summary.is_empty()).then_some(summary));
        }
    }

    (line.trim(), None)
}

fn now_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", seconds)
}

#[cfg(test)]
mod tests {
    use super::{parse_command_spec, parse_help_text};

    #[test]
    fn parse_json_array_command_spec() {
        let parsed = parse_command_spec(r#"["sxmc","serve","--paths","tests/fixtures"]"#).unwrap();
        assert_eq!(parsed, vec!["sxmc", "serve", "--paths", "tests/fixtures"]);
    }

    #[test]
    fn parse_gh_style_grouped_commands() {
        let help = r#"Work seamlessly with GitHub from the command line.

USAGE
  gh <command> <subcommand> [flags]

CORE COMMANDS
  auth:          Authenticate gh and git with GitHub
  repo:          Manage repositories

ADDITIONAL COMMANDS
  api:           Make an authenticated GitHub API request

EXAMPLES
  $ gh repo clone cli/cli
"#;
        let profile = parse_help_text("gh", "gh", help);
        assert_eq!(
            profile.summary,
            "Work seamlessly with GitHub from the command line."
        );
        assert_eq!(profile.subcommands.len(), 3);
        assert_eq!(profile.subcommands[0].name, "auth");
        assert_eq!(profile.subcommands[2].name, "api");
        assert_eq!(profile.examples[0].command, "gh repo clone cli/cli");
    }

    #[test]
    fn parse_git_style_common_commands() {
        let help = r#"usage: git [-v | --version] [-h | --help] [-C <path>] [-c <name>=<value>]
           [--exec-path[=<path>]] [--html-path] [--man-path] [--info-path]
           <command> [<args>]

These are common Git commands used in various situations:

start a working area (see also: git help tutorial)
   clone      Clone a repository into a new directory
   init       Create an empty Git repository or reinitialize an existing one

collaborate (see also: git help workflows)
   fetch      Download objects and refs from another repository
"#;
        let profile = parse_help_text("git", "git", help);
        assert_eq!(
            profile.summary,
            "These are common Git commands used in various situations:"
        );
        assert_eq!(profile.subcommands.len(), 3);
        assert_eq!(profile.subcommands[0].name, "clone");
        assert_eq!(profile.subcommands[2].name, "fetch");
    }

    #[test]
    fn parse_npm_style_command_lists() {
        let help = r#"npm <command>

Usage:

npm install        install all the dependencies in your project
npm test           run this project's tests

All commands:

    access, adduser, audit, bugs, cache, ci, completion,
    config, dedupe, doctor, exec
"#;
        let profile = parse_help_text("npm", "npm", help);
        assert_eq!(profile.summary, "npm command-line interface");
        assert!(profile
            .subcommands
            .iter()
            .any(|command| command.name == "access"));
        assert!(profile
            .subcommands
            .iter()
            .any(|command| command.name == "doctor"));
    }

    #[test]
    fn parse_python_help_sanitizes_paths_and_options() {
        let help = r#"usage: /opt/homebrew/Cellar/python@3.14/3.14.2_1/Frameworks/Python.framework/Versions/3.14/Resources/Python.app/Contents/MacOS/Python [option] ...
Options (and corresponding environment variables):
-h     : print this help message and exit
-X opt : set implementation-specific option

Arguments:
file   : program read from script file
"#;
        let profile = parse_help_text("python3", "python3", help);
        assert_eq!(profile.summary, "python3 command-line interface");
        assert!(profile.options.iter().any(|option| option.name == "-h"));
        assert!(profile.options.iter().any(|option| option.name == "-X"));
        let option = profile
            .options
            .iter()
            .find(|option| option.name == "-h")
            .unwrap();
        assert_eq!(
            option.summary.as_deref(),
            Some("print this help message and exit")
        );
        assert!(!profile.summary.contains("/opt/homebrew"));
    }

    #[test]
    fn parse_node_usage_subcommands_and_wrapped_options() {
        let help = r#"Usage: node [options] [ script.js ] [arguments]
       node inspect [options] [ script.js | host:port ] [arguments]

Options:
  --abort-on-uncaught-exception
                              aborting instead of exiting causes a
                              core file to be generated for analysis
  -c, --check                 syntax check script without executing
"#;
        let profile = parse_help_text("node", "node", help);
        assert!(profile.auth.is_empty());
        assert!(profile
            .subcommands
            .iter()
            .any(|command| command.name == "inspect"));
        assert!(!profile
            .subcommands
            .iter()
            .any(|command| command.name.starts_with("--")));
        let option = profile
            .options
            .iter()
            .find(|option| option.name == "--abort-on-uncaught-exception")
            .unwrap();
        assert!(option
            .summary
            .as_deref()
            .unwrap_or_default()
            .contains("core file"));
    }

    #[test]
    fn parse_cargo_aliases_uses_primary_name() {
        let help = r#"Rust's package manager

Commands:
    build, b    Compile the current package
    check, c    Analyze the current package and report errors
"#;
        let profile = parse_help_text("cargo", "cargo", help);
        assert_eq!(profile.subcommands[0].name, "build");
        assert_eq!(profile.subcommands[1].name, "check");
    }
}
