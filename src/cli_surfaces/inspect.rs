use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::{json, Value};

use crate::cache::Cache;
use crate::cli_surfaces::model::{
    AuthRequirement, CliSurfaceProfile, ConfidenceLevel, ConfidenceNote, EnvironmentRequirement,
    OutputBehavior, ProfileExample, ProfileOption, ProfilePositional, ProfileSource,
    ProfileSubcommand, Provenance, Workflow, PROFILE_SCHEMA,
};
use crate::error::{Result, SxmcError};

const CLI_PROFILE_CACHE_TTL_SECS: u64 = 60 * 60 * 24 * 14;
const CLI_PROFILE_CACHE_SCHEMA_VERSION: u32 = 3;
const COMPACT_SUBCOMMAND_LIMIT: usize = 12;
const COMPACT_OPTION_LIMIT: usize = 15;

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

pub fn inspect_cli_batch(command_specs: &[String], allow_self: bool, depth: usize) -> Value {
    let mut profiles = Vec::new();
    let mut failures = Vec::new();

    for command_spec in command_specs {
        match inspect_cli_with_depth(command_spec, allow_self, depth) {
            Ok(profile) => profiles.push(profile_value(&profile)),
            Err(error) => failures.push(json!({
                "command": command_spec,
                "error": error.to_string(),
            })),
        }
    }

    json!({
        "count": command_specs.len(),
        "success_count": profiles.len(),
        "failed_count": failures.len(),
        "profiles": profiles,
        "failures": failures,
    })
}

pub fn cache_stats_value() -> Result<Value> {
    let cache = Cache::new(CLI_PROFILE_CACHE_TTL_SECS)?;
    let stats = cache.stats()?;
    Ok(json!({
        "path": stats.path.display().to_string(),
        "entry_count": stats.entry_count,
        "total_bytes": stats.total_bytes,
        "default_ttl_secs": stats.default_ttl_secs,
    }))
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

    let cache_key = profile_cache_key(&parts, depth);
    if let Some(profile) = load_cached_profile(&cache_key) {
        maybe_print_progress(&format!("Using cached profile for `{}`", parts.join(" ")));
        return Ok(profile);
    }

    maybe_print_progress(&format!(
        "Inspecting `{}`{}",
        parts.join(" "),
        if depth > 0 {
            format!(" (depth {depth})")
        } else {
            String::new()
        }
    ));

    let profile = inspect_parts(&parts, &command_name, executable, allow_self, depth, 0)?;
    store_cached_profile(&cache_key, &profile);
    Ok(profile)
}

pub fn load_profile(path: &Path) -> Result<CliSurfaceProfile> {
    let raw = fs::read_to_string(path).map_err(|error| {
        SxmcError::Other(format!(
            "Failed to read CLI profile '{}': {}",
            path.display(),
            error
        ))
    })?;

    if raw.trim().is_empty() {
        return Err(SxmcError::Other(format!(
            "Profile file '{}' is empty. Expected a JSON CLI surface profile from `sxmc inspect cli <tool> --format json-pretty`.",
            path.display()
        )));
    }

    let value: Value = serde_json::from_str(&raw).map_err(|error| {
        SxmcError::Other(format!(
            "Profile file '{}' is not valid JSON: {}. Expected a CLI surface profile from `sxmc inspect cli <tool> --format json-pretty`.",
            path.display(),
            error
        ))
    })?;

    let schema = value
        .get("profile_schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema != PROFILE_SCHEMA {
        return Err(SxmcError::Other(format!(
            "Profile file '{}' is not a valid sxmc CLI surface profile. Expected `profile_schema: {}` from `sxmc inspect cli <tool> --format json-pretty`.",
            path.display(),
            PROFILE_SCHEMA
        )));
    }

    serde_json::from_value(value).map_err(|error| {
        SxmcError::Other(format!(
            "Profile file '{}' could not be decoded as an sxmc CLI surface profile: {}",
            path.display(),
            error
        ))
    })
}

pub fn profile_value(profile: &CliSurfaceProfile) -> Value {
    serde_json::to_value(profile).unwrap_or_else(|_| json!({}))
}

pub fn compact_profile_value(profile: &CliSurfaceProfile) -> Value {
    json!({
        "command": profile.command,
        "summary": profile.summary,
        "description": profile.description,
        "subcommand_count": profile.subcommands.len(),
        "option_count": profile.options.len(),
        "nested_profile_count": profile.subcommand_profiles.len(),
        "machine_friendly": profile.output_behavior.machine_friendly,
        "stdout_style": profile.output_behavior.stdout_style,
        "examples": profile.examples.iter().take(3).map(|example| {
            json!({
                "command": example.command,
                "summary": example.summary,
            })
        }).collect::<Vec<_>>(),
        "subcommands": profile.subcommands.iter().take(COMPACT_SUBCOMMAND_LIMIT).map(|subcommand| {
            json!({
                "name": subcommand.name,
                "summary": subcommand.summary,
                "confidence": subcommand.confidence,
            })
        }).collect::<Vec<_>>(),
        "options": profile.options.iter().take(COMPACT_OPTION_LIMIT).map(|option| {
            json!({
                "name": option.name,
                "short": option.short,
                "value_name": option.value_name,
                "required": option.required,
                "summary": option.summary,
            })
        }).collect::<Vec<_>>(),
        "environment": profile.environment.iter().map(|env| {
            json!({
                "name": env.name,
                "required": env.required,
                "summary": env.summary,
            })
        }).collect::<Vec<_>>(),
        "confidence_notes": profile.confidence_notes,
        "generation_depth": profile.provenance.generation_depth,
        "generator_version": profile.provenance.generator_version,
    })
}

fn load_cached_profile(cache_key: &str) -> Option<CliSurfaceProfile> {
    let cache = Cache::new(CLI_PROFILE_CACHE_TTL_SECS).ok()?;
    let raw = cache.get(cache_key)?;
    serde_json::from_str(&raw).ok()
}

fn store_cached_profile(cache_key: &str, profile: &CliSurfaceProfile) {
    if let Ok(cache) = Cache::new(CLI_PROFILE_CACHE_TTL_SECS) {
        if let Ok(raw) = serde_json::to_string(profile) {
            let _ = cache.set(cache_key, &raw);
        }
    }
}

fn profile_cache_key(parts: &[String], depth: usize) -> String {
    let executable = parts.first().map(String::as_str).unwrap_or_default();
    let fingerprint = executable_fingerprint(executable);
    format!(
        "cli-profile:v{}:{}:{}:{}:{}",
        CLI_PROFILE_CACHE_SCHEMA_VERSION,
        env!("CARGO_PKG_VERSION"),
        depth,
        fingerprint,
        parts.join("\u{1f}")
    )
}

fn executable_fingerprint(executable: &str) -> String {
    let resolved = resolve_executable_path(executable);
    if let Some(path) = resolved {
        let canonical = fs::canonicalize(&path).unwrap_or(path.clone());
        let metadata = fs::metadata(&canonical).ok();
        let modified = metadata
            .as_ref()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let len = metadata.map(|meta| meta.len()).unwrap_or(0);
        format!("{}:{}:{}", canonical.display(), modified, len)
    } else {
        executable.to_string()
    }
}

fn resolve_executable_path(executable: &str) -> Option<PathBuf> {
    let candidate = Path::new(executable);
    if (candidate.components().count() > 1 || candidate.is_absolute()) && candidate.exists() {
        return Some(candidate.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let direct = dir.join(executable);
        if direct.is_file() {
            return Some(direct);
        }

        #[cfg(windows)]
        {
            let exts = std::env::var_os("PATHEXT")
                .unwrap_or_else(|| ".EXE;.CMD;.BAT;.COM".into())
                .to_string_lossy()
                .into_owned();
            for ext in exts.split(';').filter(|ext| !ext.is_empty()) {
                let with_ext = dir.join(format!("{}{}", executable, ext.to_ascii_lowercase()));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }

    None
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
    if let Ok(man_text) = read_man_page_text(command_name) {
        let man_profile = parse_help_text(command_name, source_identifier, &man_text);
        merge_man_page_profile(&mut profile, &man_profile, command_name);
        if command_name == "brew" {
            merge_profile_options(
                &mut profile.options,
                parse_specific_option_section(
                    &man_text.lines().collect::<Vec<_>>(),
                    &["global options"],
                ),
            );
        }
    }
    if let Ok(supplemental_text) = read_supplemental_help_text(parts, command_name) {
        let supplemental_profile =
            parse_help_text(command_name, source_identifier, &supplemental_text);
        merge_supplemental_profile(&mut profile, &supplemental_profile);
    }
    profile.provenance.generation_depth = generation_depth;

    if remaining_depth > 0 {
        let candidates: Vec<_> = profile
            .subcommands
            .iter()
            .filter(|subcommand| subcommand.confidence != ConfidenceLevel::Low)
            .collect();
        let mut subcommand_profiles = Vec::new();
        for (index, subcommand) in candidates.iter().enumerate() {
            if subcommand.name == command_name {
                continue;
            }

            let mut child_parts = parts.to_vec();
            child_parts.push(subcommand.name.clone());
            let child_source = format!("{source_identifier} {}", subcommand.name);
            let child_name = subcommand.name.clone();
            maybe_print_progress(&format!(
                "Inspecting nested subcommand {}/{}: `{}`",
                index + 1,
                candidates.len(),
                child_source
            ));

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

    if remaining_depth == 0
        && profile.subcommand_profiles.is_empty()
        && profile.subcommands.len() >= 8
    {
        profile.confidence_notes.push(ConfidenceNote {
            level: ConfidenceLevel::Medium,
            summary: format!(
                "This CLI exposes {} top-level subcommands. Re-run with `--depth 2` if you want nested help for multi-layer workflows.",
                profile.subcommands.len()
            ),
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

fn merge_man_page_profile(
    profile: &mut CliSurfaceProfile,
    man_profile: &CliSurfaceProfile,
    command_name: &str,
) {
    let force_man_summary = !man_profile.summary.trim().is_empty()
        && (is_version_banner(&profile.summary)
            || profile
                .summary
                .to_ascii_lowercase()
                .starts_with("please report bugs"));

    if force_man_summary
        || should_prefer_man_summary(&profile.summary, &man_profile.summary, command_name)
    {
        profile.summary = man_profile.summary.clone();
    }

    if profile
        .description
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
        && man_profile.description.is_some()
    {
        profile.description = man_profile.description.clone();
    }

    if command_name != "brew" && profile.options.len() < man_profile.options.len() {
        profile.options = man_profile.options.clone();
    }

    if man_profile.subcommands.len() >= 3
        && man_profile.subcommands.len() > profile.subcommands.len()
    {
        let mut merged = profile.subcommands.clone();
        for candidate in &man_profile.subcommands {
            if looks_like_plausible_subcommand_name(&candidate.name)
                && !merged
                    .iter()
                    .any(|existing| existing.name == candidate.name)
            {
                merged.push(candidate.clone());
            }
        }
        profile.subcommands = merged;
    }
}

fn merge_supplemental_profile(
    profile: &mut CliSurfaceProfile,
    supplemental_profile: &CliSurfaceProfile,
) {
    for candidate in &supplemental_profile.subcommands {
        if looks_like_plausible_subcommand_name(&candidate.name)
            && !profile
                .subcommands
                .iter()
                .any(|existing| existing.name == candidate.name)
        {
            profile.subcommands.push(candidate.clone());
        }
    }
}

fn merge_profile_options(options: &mut Vec<ProfileOption>, candidates: Vec<ProfileOption>) {
    for candidate in candidates {
        if let Some(existing) = options
            .iter_mut()
            .find(|option| option.name == candidate.name)
        {
            if existing.short.is_none() && candidate.short.is_some() {
                existing.short = candidate.short.clone();
            }
            if existing.value_name.is_none() && candidate.value_name.is_some() {
                existing.value_name = candidate.value_name.clone();
            }
            if existing.summary.is_none()
                || candidate.summary.as_ref().map(|s| s.len()).unwrap_or(0)
                    > existing.summary.as_ref().map(|s| s.len()).unwrap_or(0)
            {
                existing.summary = candidate.summary.clone();
            }
            if matches!(existing.confidence, ConfidenceLevel::Low)
                && !matches!(candidate.confidence, ConfidenceLevel::Low)
            {
                existing.confidence = candidate.confidence;
            }
        } else {
            options.push(candidate);
        }
    }
}

fn read_help_text(parts: &[String], command_name: &str) -> Result<String> {
    let mut help_candidates = Vec::new();

    if let Ok(primary) = run_help_variant(parts, &["--help"]) {
        let lowered = primary.to_ascii_lowercase();
        help_candidates.push(primary.clone());

        if lowered.contains("--help-all") || lowered.contains("complete help information") {
            if let Ok(text) = run_help_variant(parts, &["--help-all"]) {
                if !text.trim().is_empty() {
                    help_candidates.push(text);
                }
            }
        }
        if lowered.contains("--help all") || lowered.contains("help all") {
            if let Ok(text) = run_help_variant(parts, &["--help", "all"]) {
                if !text.trim().is_empty() {
                    help_candidates.push(text);
                }
            }
        }
    }

    let best_help = help_candidates
        .into_iter()
        .max_by_key(|text| score_help_text(command_name, &parts[0], text));

    if let Some(help) = best_help {
        if score_help_text(command_name, &parts[0], &help) >= 20 {
            return Ok(help);
        }

        if let Ok(text) = read_man_page_text(command_name) {
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }

        return Ok(help);
    }

    if let Ok(text) = read_man_page_text(command_name) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }

    Err(SxmcError::Other(format!(
        "Could not parse help output for '{}'. Try running '{} --help' manually and verify it prints readable text. If the CLI uses a non-standard layout, inspect a narrower subcommand or use --compact to reduce output size.",
        parts[0], parts[0]
    )))
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
            "Could not parse help output for '{}': the command exited without readable stdout/stderr for '{}'.",
            parts[0],
            extra_args.join(" ")
        )));
    }

    Ok(text)
}

fn read_supplemental_help_text(parts: &[String], command_name: &str) -> Result<String> {
    if command_name == "brew" && parts.len() == 1 {
        maybe_print_progress("Collecting supplemental Homebrew commands with `brew commands`");
        let output = run_help_variant(parts, &["commands"])?;
        let mut lines = vec!["COMMANDS".to_string()];

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("==>")
                || trimmed.starts_with("--")
                || trimmed.starts_with('#')
            {
                continue;
            }

            if looks_like_plausible_subcommand_name(trimmed) {
                lines.push(format!("{trimmed}  Listed by `brew commands`."));
            }
        }

        if lines.len() > 1 {
            return Ok(lines.join("\n"));
        }
    }

    Err(SxmcError::Other(format!(
        "No supplemental help source for '{}'",
        command_name
    )))
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
    let options = parse_options(&lines, command_name);
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
    let mut saw_major_section = false;
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
        if is_major_section_heading(trimmed) {
            saw_major_section = true;
        }
        if is_unhelpful_summary_line(trimmed, command_name) {
            if looks_like_usage_line(trimmed, command_name) {
                skipping_usage_block = true;
            }
            if saw_major_section {
                break;
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
    let man_page = looks_like_man_page(lines);
    let mut subcommands = Vec::new();
    let mut in_command_section = false;
    let mut pending_summary_idx: Option<usize> = None;

    for line in lines {
        let trimmed = line.trim_end();
        let stripped = trimmed.trim();

        if stripped.is_empty() {
            continue;
        }

        if is_command_section_heading(stripped, man_page)
            || stripped.starts_with("These are common ")
            || stripped.starts_with("These are available ")
        {
            in_command_section = true;
            pending_summary_idx = None;
            continue;
        }

        if !in_command_section {
            continue;
        }

        if is_major_section_heading(stripped) && !is_command_section_heading(stripped, man_page) {
            if !subcommands.is_empty() {
                break;
            }
            pending_summary_idx = None;
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
            pending_summary_idx = None;
            continue;
        }

        if let Some(name) = parse_indented_command_name(line, command_name) {
            if !subcommands.iter().any(|existing| existing.name == name) {
                subcommands.push(ProfileSubcommand {
                    name,
                    summary: "Listed in CLI help output.".into(),
                    confidence: ConfidenceLevel::Medium,
                });
                pending_summary_idx = Some(subcommands.len() - 1);
            }
            continue;
        }

        if let Some(idx) = pending_summary_idx {
            if let Some(summary) = parse_indented_command_summary(line, command_name) {
                if let Some(entry) = subcommands.get_mut(idx) {
                    if entry.summary == "Listed in CLI help output." {
                        entry.summary = summary;
                    }
                }
                pending_summary_idx = None;
                continue;
            }
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

    if !man_page {
        for inferred in parse_invocation_subcommands(lines, command_name) {
            push_subcommand(&mut subcommands, inferred);
        }
    }

    subcommands
}

fn parse_options(lines: &[&str], command_name: &str) -> Vec<ProfileOption> {
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
    if options.is_empty() {
        options.extend(parse_usage_options(lines, command_name));
    }
    if options.is_empty() {
        options.extend(parse_inline_options(lines));
    }
    if options.is_empty() && looks_like_man_page(lines) {
        options.extend(parse_man_synopsis_options(lines));
    }
    if looks_like_man_page(lines) {
        let man_options = parse_man_options(lines);
        if man_options.len() > options.len() {
            return man_options;
        }
    }

    if command_name == "brew" {
        merge_profile_options(
            &mut options,
            parse_specific_option_section(lines, &["global options"]),
        );
    }

    options
}

fn parse_specific_option_section(lines: &[&str], headings: &[&str]) -> Vec<ProfileOption> {
    let mut options = Vec::new();
    let mut in_options = false;

    for line in lines {
        let trimmed = line.trim_end();
        let stripped = trimmed.trim();
        if stripped.is_empty() {
            continue;
        }
        if is_named_section(stripped, headings) {
            in_options = true;
            continue;
        }
        if !in_options {
            continue;
        }
        if is_major_section_heading(stripped) && !is_named_section(stripped, headings) {
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
                last.summary = Some(merged.trim().to_string());
            }
        }
    }

    options
}

fn parse_inline_options(lines: &[&str]) -> Vec<ProfileOption> {
    let mut options = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for line in lines {
        if let Some(option) = parse_option_entry(line) {
            if seen.insert(option.name.clone()) {
                options.push(option);
            }
        }
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
    let mut count = 0;
    let mut has_name_like = false;

    for line in lines {
        let trimmed = line.trim();
        if matches_normalized_heading(trimmed, "name")
            || matches_normalized_heading(trimmed, "synopsis")
            || matches_normalized_heading(trimmed, "description")
        {
            has_name_like = true;
            count += 1;
        } else if matches_normalized_heading(trimmed, "options") {
            count += 1;
        }
    }

    has_name_like && count >= 2
}

fn is_command_section_heading(line: &str, man_page: bool) -> bool {
    if man_page {
        return false;
    }

    let trimmed = line.trim();
    let normalized = normalize_heading(trimmed);
    if normalized.starts_with("the command") || normalized.starts_with("this command") {
        return false;
    }

    normalized == "commands"
        || normalized == "subcommands"
        || normalized.ends_with(" commands")
        || normalized.ends_with(" subcommands")
}

fn looks_like_usage_line(line: &str, command_name: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    if lowered.starts_with("usage:") || lowered == "usage" {
        return true;
    }

    if let Some(rest) = line.strip_prefix(&format!("{command_name} ")) {
        let rest = rest.trim_start();
        return rest.starts_with('[')
            || rest.starts_with('<')
            || rest.starts_with('-')
            || rest.starts_with('{')
            || rest.starts_with('(');
    }

    false
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
        || lowered.starts_with("copyright")
        || lowered.starts_with("report bugs at:")
        || lowered.starts_with("latest revision:")
        || lowered.starts_with("latest faq:")
        || lowered.starts_with("latest man page:")
        || lowered.starts_with("please report bugs")
        || lowered.starts_with("project home page:")
        || lowered.starts_with("use -h ")
        || lowered.starts_with("defaults in parentheses")
        || lowered.starts_with("apple specific options")
        || lowered.starts_with("summary of ")
        || lowered.starts_with("this is free software")
        || lowered.starts_with("for details, use")
        || lowered.contains('@')
        || lowered.contains("complete list of options")
        || lowered.contains("unrecognized option")
        || lowered.contains("illegal option")
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

fn should_prefer_man_summary(summary: &str, man_summary: &str, command_name: &str) -> bool {
    summary_quality(man_summary, command_name) > summary_quality(summary, command_name)
}

fn is_version_banner(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    let versiony_number = Regex::new(r"\b\d+\.\d+(?:\.\d+)?\b").unwrap();
    let trailing_integer_version = Regex::new(r"\b\d{2,}\b$").unwrap();
    let leading_version_banner = Regex::new(r"^\S+\s+\d+(?:\.\d+)+(?:\s+of\b.*)?(?:,|$)").unwrap();
    (lowered.contains("version")
        || lowered.contains("(rev ")
        || leading_version_banner.is_match(line)
        || trailing_integer_version.is_match(line)
        || (versiony_number.is_match(line) && line.split_whitespace().count() <= 8))
        && !lowered.contains("command")
        && line.chars().any(|c| c.is_ascii_digit())
}

fn looks_like_option_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('-')
        || trimmed.starts_with("+|-")
        || trimmed.starts_with("-?|-")
        || trimmed.starts_with("-- ")
}

fn looks_like_cli_example_line(line: &str, command_name: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("$ ")
        && trimmed
            .trim_start_matches("$ ")
            .trim()
            .starts_with(&format!("{command_name} "))
}

fn sanitize_for_profile(text: &str, command_name: &str) -> String {
    let overstrike_stripped = strip_overstrike(text);
    let unix_path = Regex::new(r"(?P<prefix>^|[\s(])(?P<path>/[^\s:]+)").unwrap();
    let windows_path = Regex::new(r"(?P<prefix>^|[\s(])(?P<path>[A-Za-z]:\\[^\s:]+)").unwrap();

    let mut sanitized = unix_path
        .replace_all(&overstrike_stripped, |caps: &regex::Captures<'_>| {
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

fn strip_overstrike(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch == '\u{0008}' {
            out.pop();
        } else {
            out.push(ch);
        }
    }
    out
}

fn parse_man_name_summary(lines: &[&str], command_name: &str) -> Option<String> {
    let mut in_name = false;
    let mut collected = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if matches_normalized_heading(trimmed, "name") {
            in_name = true;
            continue;
        }
        if !in_name {
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

        let sanitized = sanitize_for_profile(trimmed, command_name);
        if !sanitized.is_empty() {
            collected.push(sanitized);
        }
    }

    if collected.is_empty() {
        return None;
    }

    let joined = collected.join(" ");
    let separator_regex = Regex::new(r"^.+?\s+[–—-]\s+(.+)$").unwrap();
    if let Some(caps) = separator_regex.captures(&joined) {
        return caps
            .get(1)
            .map(|m| sanitize_for_profile(m.as_str().trim(), command_name))
            .filter(|summary| !summary.is_empty());
    }

    Some(joined)
}

fn parse_man_description(lines: &[&str], command_name: &str) -> Option<String> {
    let mut in_description = false;
    let mut collected = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if matches_normalized_heading(trimmed, "description") {
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
        if matches_normalized_heading(stripped, "description")
            || matches_normalized_heading(stripped, "options")
        {
            in_description = true;
            continue;
        }
        if !in_description {
            continue;
        }
        if stripped.is_empty() {
            continue;
        }
        if is_major_section_heading(stripped)
            && !matches_normalized_heading(stripped, "description")
            && !matches_normalized_heading(stripped, "options")
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

fn parse_man_synopsis_options(lines: &[&str]) -> Vec<ProfileOption> {
    let mut options = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut in_synopsis = false;
    let short_regex =
        Regex::new(r"(^|[\[(\s|])(-[A-Za-z0-9?])(?:\s+([A-Za-z<>\[\]_.=-]+))?").unwrap();
    let long_regex =
        Regex::new(r"(^|[\[(\s|])(--[A-Za-z0-9][A-Za-z0-9-]*)(?:[ =]([A-Za-z<>\[\]_.=-]+))?")
            .unwrap();

    for line in lines {
        let trimmed = line.trim();
        if matches_normalized_heading(trimmed, "synopsis") {
            in_synopsis = true;
            continue;
        }
        if !in_synopsis {
            continue;
        }
        if trimmed.is_empty() {
            if !options.is_empty() {
                break;
            }
            continue;
        }
        if is_major_section_heading(trimmed) && !matches_normalized_heading(trimmed, "synopsis") {
            break;
        }

        for caps in short_regex.captures_iter(trimmed) {
            let short = caps
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if short.is_empty() || !seen.insert(short.clone()) {
                continue;
            }
            let value_name = caps
                .get(3)
                .map(|m| m.as_str().trim_matches(&['<', '>'][..]).to_string())
                .filter(|value| !value.starts_with('-'));
            options.push(ProfileOption {
                name: short.clone(),
                short: Some(short),
                value_name,
                required: false,
                summary: Some("Inferred from the CLI synopsis.".into()),
                confidence: ConfidenceLevel::Medium,
            });
        }

        for caps in long_regex.captures_iter(trimmed) {
            let name = caps
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if name.is_empty() || !seen.insert(name.clone()) {
                continue;
            }
            let value_name = caps
                .get(3)
                .map(|m| m.as_str().trim_matches(&['<', '>'][..]).to_string())
                .filter(|value| !value.starts_with('-'));
            options.push(ProfileOption {
                name,
                short: None,
                value_name,
                required: false,
                summary: Some("Inferred from the CLI synopsis.".into()),
                confidence: ConfidenceLevel::Medium,
            });
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
        if looks_like_env_symbol(raw_name) {
            return None;
        }
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
        if raw_name.starts_with('-') || looks_like_env_symbol(raw_name) {
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
        return items
            .into_iter()
            .filter(|item| !looks_like_env_symbol(item))
            .map(str::to_string)
            .collect();
    }

    Vec::new()
}

fn parse_indented_command_name(line: &str, command_name: &str) -> Option<String> {
    let leading_spaces = line.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed_start = line.trim_start();
    if trimmed_start == line
        || !(3..=5).contains(&leading_spaces)
        || trimmed_start.starts_with('-')
        || trimmed_start.starts_with(command_name)
    {
        return None;
    }

    if trimmed_start.split_whitespace().count() != 1 {
        return None;
    }

    let first = trimmed_start.split_whitespace().next()?;
    if !is_literal_subcommand_token(first) || looks_like_env_symbol(first) || first.ends_with('.') {
        return None;
    }

    Some(first.to_string())
}

fn parse_indented_command_summary(line: &str, command_name: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || is_major_section_heading(trimmed)
        || looks_like_option_line(trimmed)
        || trimmed.starts_with(command_name)
    {
        return None;
    }

    let sanitized = sanitize_for_profile(trimmed, command_name);
    (!sanitized.is_empty()).then_some(sanitized)
}

fn looks_like_env_symbol(value: &str) -> bool {
    value.len() > 3
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn push_subcommand(subcommands: &mut Vec<ProfileSubcommand>, candidate: ProfileSubcommand) {
    if !looks_like_plausible_subcommand_name(&candidate.name) {
        return;
    }

    if !subcommands
        .iter()
        .any(|existing| existing.name == candidate.name)
    {
        subcommands.push(candidate);
    }
}

fn looks_like_plausible_subcommand_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('.')
        && !value.chars().any(char::is_whitespace)
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

fn parse_usage_subcommands(lines: &[&str], command_name: &str) -> Vec<ProfileSubcommand> {
    let mut inferred = Vec::new();
    let mut in_usage_block = false;
    let mut saw_usage_content = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if in_usage_block && !inferred.is_empty() {
                break;
            }
            continue;
        }

        if in_usage_block
            && is_major_section_heading(trimmed)
            && !looks_like_usage_line(trimmed, command_name)
        {
            if saw_usage_content {
                break;
            }
            continue;
        }

        let usage_line = if let Some(rest) = trimmed.strip_prefix("Usage:") {
            in_usage_block = true;
            rest.trim()
        } else if in_usage_block && looks_like_usage_continuation(trimmed, command_name) {
            trimmed
        } else {
            continue;
        };

        saw_usage_content = true;

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

fn parse_invocation_subcommands(lines: &[&str], command_name: &str) -> Vec<ProfileSubcommand> {
    let mut inferred = Vec::new();
    let mut in_invocation_section = false;

    for line in lines {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }

        if is_major_section_heading(stripped) {
            in_invocation_section = is_named_section(
                stripped,
                &[
                    "example usage",
                    "troubleshooting",
                    "contributing",
                    "further help",
                ],
            );
            continue;
        }

        let trimmed = line.trim_start_matches("$ ").trim();
        if !in_invocation_section {
            continue;
        }

        if !trimmed.starts_with(command_name) {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let head = parts.next().unwrap_or_default();
        if head != command_name && !head.ends_with(&format!("/{command_name}")) {
            continue;
        }

        if let Some(next) = parts.next() {
            if is_literal_subcommand_token(next) {
                inferred.push(ProfileSubcommand {
                    name: next.to_string(),
                    summary: "Inferred from CLI usage examples.".into(),
                    confidence: ConfidenceLevel::Medium,
                });
            }
        }
    }

    inferred
}

fn parse_usage_options(lines: &[&str], command_name: &str) -> Vec<ProfileOption> {
    let mut inferred = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut in_usage_block = false;
    let mut saw_usage_content = false;
    let option_regex = Regex::new(r"(?P<opt>--[A-Za-z0-9][A-Za-z0-9-]*|-[A-Za-z0-9?])").unwrap();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if in_usage_block && !inferred.is_empty() {
                break;
            }
            continue;
        }

        if in_usage_block
            && is_major_section_heading(trimmed)
            && !looks_like_usage_line(trimmed, command_name)
        {
            if saw_usage_content {
                break;
            }
            continue;
        }

        let usage_line = if let Some(rest) = trimmed.strip_prefix("Usage:") {
            in_usage_block = true;
            rest.trim()
        } else if in_usage_block && looks_like_usage_continuation(trimmed, command_name) {
            trimmed
        } else {
            continue;
        };

        saw_usage_content = true;

        for capture in option_regex.captures_iter(usage_line) {
            let option = capture.name("opt").map(|m| m.as_str()).unwrap_or_default();
            if seen.insert(option.to_string()) {
                let short = (!option.starts_with("--")).then(|| option.to_string());
                inferred.push(ProfileOption {
                    name: option.to_string(),
                    short,
                    value_name: None,
                    required: false,
                    summary: Some("Inferred from the CLI usage line.".into()),
                    confidence: ConfidenceLevel::Medium,
                });
            }
        }
    }

    inferred
}

fn looks_like_usage_continuation(line: &str, command_name: &str) -> bool {
    line.starts_with(command_name)
        || line.starts_with('[')
        || line.starts_with('<')
        || line.starts_with('-')
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
    let trimmed = line.trim_start();
    if let Some(index) = trimmed.find("  ") {
        let (signature, rest) = trimmed.split_at(index);
        let summary = rest.trim().trim_start_matches(':').trim();
        return (signature.trim(), (!summary.is_empty()).then_some(summary));
    }

    if let Some(index) = trimmed.find(':') {
        let (signature, rest) = trimmed.split_at(index);
        if signature.contains('-') {
            let summary = rest.trim_start_matches(':').trim();
            return (signature.trim(), (!summary.is_empty()).then_some(summary));
        }
    }

    (trimmed.trim(), None)
}

fn summary_quality(summary: &str, command_name: &str) -> i32 {
    let trimmed = summary.trim();
    let lowered = trimmed.to_ascii_lowercase();
    let mut score = 0;

    if trimmed.is_empty() {
        return -100;
    }
    if looks_generic_summary(trimmed, command_name) {
        score -= 25;
    }
    if looks_like_usage_line(trimmed, command_name) {
        score -= 25;
    }
    if is_version_banner(trimmed) {
        score -= 20;
    }
    if looks_like_option_line(trimmed) {
        score -= 15;
    }
    if lowered.starts_with("please report bugs")
        || lowered.starts_with("copyright")
        || lowered.starts_with("report bugs at:")
        || lowered.starts_with("latest revision:")
        || lowered.starts_with("latest faq:")
        || lowered.starts_with("latest man page:")
        || lowered.starts_with("general options")
        || lowered.starts_with("options")
        || lowered.starts_with("project home page:")
        || lowered.starts_with("defaults in parentheses")
        || lowered.starts_with("summary of ")
        || lowered.starts_with("apple specific options")
        || lowered.starts_with("this is free software")
        || lowered.starts_with("use \"")
        || lowered.starts_with("for details, use")
        || trimmed.starts_with('.')
    {
        score -= 18;
    }
    if lowered.contains("complete list of options") {
        score -= 18;
    }
    if trimmed.contains(';') {
        score -= 10;
    }
    if trimmed.ends_with(':') {
        score -= 10;
    }
    if trimmed.ends_with('.') {
        score += 4;
    }
    if trimmed.split_whitespace().count() >= 3 {
        score += 4;
    }
    if trimmed
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
    {
        score += 2;
    }
    if lowered.contains("command-line interface") {
        score -= 5;
    }

    score
}

fn normalize_heading(line: &str) -> String {
    line.trim()
        .trim_end_matches(':')
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn matches_normalized_heading(line: &str, heading: &str) -> bool {
    normalize_heading(line) == heading
}

fn now_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", seconds)
}

fn maybe_print_progress(message: &str) {
    if std::io::stderr().is_terminal() && std::env::var_os("SXMC_NO_PROGRESS").is_none() {
        eprintln!("[sxmc] {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        merge_profile_options, parse_command_spec, parse_help_text, parse_specific_option_section,
        strip_overstrike, ConfidenceLevel, ProfileOption,
    };

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
    fn parse_gh_multi_section_command_headings() {
        let help = r#"Work seamlessly with GitHub from the command line.

GITHUB ACTIONS COMMANDS
  cache:         Manage GitHub Actions caches
  run:           View details about workflow runs

ALIAS COMMANDS
  co:            Alias for "pr checkout"

ADDITIONAL COMMANDS
  api:           Make an authenticated GitHub API request
  config:        Manage configuration for gh
"#;
        let profile = parse_help_text("gh", "gh", help);
        assert!(profile
            .subcommands
            .iter()
            .any(|entry| entry.name == "cache"));
        assert!(profile.subcommands.iter().any(|entry| entry.name == "run"));
        assert!(profile.subcommands.iter().any(|entry| entry.name == "co"));
        assert!(profile.subcommands.iter().any(|entry| entry.name == "api"));
        assert!(profile
            .subcommands
            .iter()
            .any(|entry| entry.name == "config"));
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

    #[test]
    fn parse_rg_help_skips_banners_and_fake_subcommands() {
        let help = r#"ripgrep 15.1.0 (rev af60c2de9d)
Andrew Gallant <jamslam@gmail.com>

ripgrep (rg) recursively searches the current directory for lines matching
a regex pattern.

USAGE:
    rg [OPTIONS] PATTERN [PATH ...]

INPUT OPTIONS:
    -z, --search-zip
        Search in compressed files.
"#;
        let profile = parse_help_text("rg", "rg", help);
        assert!(
            profile.summary.contains("recursively searches"),
            "{}",
            profile.summary
        );
        assert!(!profile.subcommands.iter().any(|entry| entry.name == "-z"));
        assert!(!profile
            .subcommands
            .iter()
            .any(|entry| entry.name == "--search-zip"));
    }

    #[test]
    fn parse_wrapped_man_name_prefers_description() {
        let help = r#"NAME
     grep, egrep, fgrep, rgrep, bzgrep, bzegrep, bzfgrep, zgrep, zegrep,
     zfgrep – file pattern searcher
"#;
        let profile = parse_help_text("grep", "grep", help);
        assert_eq!(profile.summary, "file pattern searcher");
    }

    #[test]
    fn parse_brew_supplemental_command_list_recovers_subcommands() {
        let help = r#"COMMANDS
alias  Listed by `brew commands`.
analytics  Listed by `brew commands`.
autoremove  Listed by `brew commands`.
"#;
        let profile = parse_help_text("brew", "brew", help);
        assert!(profile
            .subcommands
            .iter()
            .any(|entry| entry.name == "alias"));
        assert!(profile
            .subcommands
            .iter()
            .any(|entry| entry.name == "analytics"));
        assert!(profile
            .subcommands
            .iter()
            .any(|entry| entry.name == "autoremove"));
    }

    #[test]
    fn parse_man_synopsis_extracts_awk_flags() {
        let help = r#"NAME
       awk - pattern-directed scanning and processing language

SYNOPSIS
       awk [ -F fs ] [ -v var=value ] [ 'prog' | -f progfile ] [ file ... ]
"#;
        let profile = parse_help_text("awk", "awk", help);
        assert!(profile.options.iter().any(|option| option.name == "-F"));
        assert!(profile.options.iter().any(|option| option.name == "-v"));
        assert!(profile.options.iter().any(|option| option.name == "-f"));
    }

    #[test]
    fn parse_brew_global_options_section_recovers_real_options() {
        let help = r#"GLOBAL OPTIONS
       These options are applicable across multiple subcommands.

       -d, --debug
              Display any debugging information.

       -q, --quiet
              Make some output more quiet.

       -v, --verbose
              Make some output more verbose.

       -h, --help
              Show this message.
"#;
        let options =
            parse_specific_option_section(&help.lines().collect::<Vec<_>>(), &["global options"]);
        assert!(options.iter().any(|option| option.name == "--debug"));
        assert!(options.iter().any(|option| option.name == "--quiet"));
        assert!(options.iter().any(|option| option.name == "--verbose"));
        assert!(options.iter().any(|option| option.name == "--help"));
    }

    #[test]
    fn merge_profile_options_enriches_existing_entries() {
        let mut options = vec![ProfileOption {
            name: "--verbose".into(),
            short: None,
            value_name: None,
            required: false,
            summary: None,
            confidence: ConfidenceLevel::Medium,
        }];
        merge_profile_options(
            &mut options,
            vec![ProfileOption {
                name: "--verbose".into(),
                short: Some("-v".into()),
                value_name: None,
                required: false,
                summary: Some("Make some output more verbose.".into()),
                confidence: ConfidenceLevel::High,
            }],
        );
        assert_eq!(options[0].short.as_deref(), Some("-v"));
        assert_eq!(
            options[0].summary.as_deref(),
            Some("Make some output more verbose.")
        );
        assert_eq!(options[0].confidence, ConfidenceLevel::Medium);
    }

    #[test]
    fn parse_man_examples_do_not_create_cat_subcommands() {
        let help = r#"NAME
     cat – concatenate and print files

EXAMPLES
     The command:

           cat file1

     will print the contents of file1 to the standard output.
"#;
        let profile = parse_help_text("cat", "cat", help);
        assert_eq!(profile.summary, "concatenate and print files");
        assert!(profile.subcommands.is_empty());
    }

    #[test]
    fn parse_titlecase_man_name_works_for_dc() {
        let help = r#"Name
       dc - arbitrary-precision decimal reverse-Polish notation calculator
"#;
        let profile = parse_help_text("dc", "dc", help);
        assert_eq!(
            profile.summary,
            "arbitrary-precision decimal reverse-Polish notation calculator"
        );
    }

    #[test]
    fn parse_unzip_man_name_prefers_description_over_version_banner() {
        let help = r#"NAME
       unzip - list, test and extract compressed files in a ZIP archive
"#;
        let profile = parse_help_text("unzip", "unzip", help);
        assert_eq!(
            profile.summary,
            "list, test and extract compressed files in a ZIP archive"
        );
    }

    #[test]
    fn unzip_style_version_banner_is_treated_as_banner() {
        assert!(super::is_version_banner(
            "UnZip 6.00 of 20 April 2009, by Info-ZIP, with modifications by Apple Inc."
        ));
    }

    #[test]
    fn strip_overstrike_sequences_for_less_style_output() {
        assert_eq!(
            strip_overstrike(
                "S\u{0008}SU\u{0008}UM\u{0008}MM\u{0008}MA\u{0008}AR\u{0008}RY\u{0008}Y"
            ),
            "SUMMARY"
        );
    }

    #[test]
    fn descriptive_lines_starting_with_command_name_are_not_treated_as_usage() {
        let help = r#"bc 7.0.3
usage: bc [options] [file...]

bc is a command-line, arbitrary-precision calculator with a Turing-complete
language. For details, use `man bc`.
"#;
        let profile = parse_help_text("bc", "bc", help);
        assert!(profile.summary.starts_with("bc is a command-line"));
    }
}
