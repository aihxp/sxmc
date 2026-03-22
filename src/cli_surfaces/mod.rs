mod inspect;
mod materialize;
mod model;
mod render;

pub use inspect::{
    cache_stats_value, compact_profile_value, inspect_cli, inspect_cli_batch,
    inspect_cli_with_depth, load_profile, parse_command_spec, profile_value,
};
pub use materialize::{
    generate_agent_doc_artifact, generate_client_config_artifact,
    generate_full_coverage_init_artifacts, generate_host_native_agent_doc_artifacts,
    generate_llms_txt_artifact, generate_mcp_wrapper_artifacts,
    generate_portable_agent_doc_artifact, generate_profile_artifact, generate_skill_artifacts,
    materialize_artifacts, materialize_artifacts_with_apply_selection, remove_artifacts,
    remove_artifacts_with_apply_selection,
};
pub use model::{
    host_profile_spec, AiClientProfile, AiCoverage, ApplyStrategy, ArtifactAudience, ArtifactMode,
    CliSurfaceProfile, ConfigShape, GeneratedArtifact, HostProfileSpec, ProfileQualityReport,
    WriteOutcome, AI_HOST_SPECS, CLI_AI_HOSTS_LAST_VERIFIED, PROFILE_SCHEMA,
};
