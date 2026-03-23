mod inspect;
mod materialize;
mod model;
mod render;

pub use inspect::{
    cache_stats_value, clear_profile_cache_value, compact_profile_value, diff_profile_value,
    inspect_cli, inspect_cli_batch, inspect_cli_batch_with_callback, inspect_cli_with_depth,
    invalidate_profile_cache_value, load_batch_requests, load_profile, parse_batch_since_filter,
    parse_command_spec, profile_value, warm_profile_cache, BatchInspectRequest, BatchSinceFilter,
};
pub use materialize::{
    generate_agent_doc_artifact, generate_ci_workflow_artifact, generate_client_config_artifact,
    generate_full_coverage_init_artifacts, generate_host_native_agent_doc_artifacts,
    generate_llms_txt_artifact, generate_mcp_wrapper_artifacts,
    generate_portable_agent_doc_artifact, generate_profile_artifact, generate_skill_artifacts,
    materialize_artifacts, materialize_artifacts_with_apply_selection, preview_artifacts,
    preview_artifacts_with_apply_selection, remove_artifacts,
    remove_artifacts_with_apply_selection,
};
pub use model::{
    host_profile_spec, AiClientProfile, AiCoverage, ApplyStrategy, ArtifactAudience, ArtifactMode,
    CliSurfaceProfile, ConfidenceLevel, ConfigShape, GeneratedArtifact, HostProfileSpec,
    ProfileOption, ProfileQualityReport, WriteOutcome, WriteStatus, AI_HOST_SPECS,
    CLI_AI_HOSTS_LAST_VERIFIED, PROFILE_SCHEMA,
};
