use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};

use crate::executor;
use crate::skills::models::Skill;
use crate::skills::parser::parse_argument_hint;

#[derive(Clone)]
pub struct SkillsServer {
    skills: Vec<Skill>,
    skill_index: HashMap<String, usize>,
    tool_index: HashMap<String, (usize, usize)>,
    resource_index: HashMap<String, (usize, usize)>,
}

impl SkillsServer {
    pub fn new(skills: Vec<Skill>) -> Self {
        let mut skill_index = HashMap::new();
        let mut tool_index = HashMap::new();
        let mut resource_index = HashMap::new();

        for (si, skill) in skills.iter().enumerate() {
            skill_index.insert(skill.name.clone(), si);

            for (sci, script) in skill.scripts.iter().enumerate() {
                let tool_name = Self::make_tool_name(&skill.name, &script.name);
                tool_index.insert(tool_name, (si, sci));
            }

            for (ri, reference) in skill.references.iter().enumerate() {
                resource_index.insert(reference.uri.clone(), (si, ri));
            }
        }

        Self {
            skills,
            skill_index,
            tool_index,
            resource_index,
        }
    }

    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    fn make_tool_name(skill_name: &str, script_name: &str) -> String {
        let stem = script_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(script_name);
        format!(
            "{}__{}",
            skill_name.replace('-', "_"),
            stem.replace('-', "_"),
        )
    }

    fn build_input_schema() -> Arc<JsonObject> {
        let mut props = serde_json::Map::new();
        let mut args_obj = serde_json::Map::new();
        args_obj.insert("type".into(), "string".into());
        args_obj.insert(
            "description".into(),
            "Arguments to pass to the script".into(),
        );
        props.insert("args".into(), serde_json::Value::Object(args_obj));

        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), "object".into());
        schema.insert("properties".into(), serde_json::Value::Object(props));
        Arc::new(schema)
    }
}

impl ServerHandler for SkillsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            let schema = Self::build_input_schema();
            let mut tools = Vec::new();

            for skill in &self.skills {
                for script in &skill.scripts {
                    let tool_name = Self::make_tool_name(&skill.name, &script.name);
                    let tool = Tool::new(
                        tool_name,
                        format!("Run {} from skill '{}'", script.name, skill.name),
                        schema.clone(),
                    );
                    tools.push(tool);
                }
            }

            Ok(ListToolsResult {
                tools,
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let tool_name: &str = request.name.as_ref();

            let (si, sci) = self.tool_index.get(tool_name).ok_or_else(|| {
                McpError::invalid_params(format!("Unknown tool: {}", tool_name), None)
            })?;

            let skill = &self.skills[*si];
            let script = &skill.scripts[*sci];

            let args_str = request
                .arguments
                .as_ref()
                .and_then(|args| args.get("args"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let args: Vec<&str> = if args_str.is_empty() {
                vec![]
            } else {
                args_str.split_whitespace().collect()
            };

            match executor::execute_script(&script.path, &args, &skill.base_dir, 30).await {
                Ok(result) => {
                    let mut output = result.stdout;
                    if result.exit_code != 0 {
                        output.push_str(&format!(
                            "\nSTDERR: {}\nExit code: {}",
                            result.stderr, result.exit_code
                        ));
                        return Ok(CallToolResult::error(vec![Content::text(output)]));
                    }
                    Ok(CallToolResult::success(vec![Content::text(output)]))
                }
                Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
            }
        }
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + Send + '_ {
        async move {
            let prompts: Vec<Prompt> = self
                .skills
                .iter()
                .map(|skill| {
                    let args = skill
                        .frontmatter
                        .argument_hint
                        .as_deref()
                        .map(parse_argument_hint)
                        .unwrap_or_default();

                    let prompt_args: Vec<PromptArgument> = args
                        .iter()
                        .map(|a| {
                            PromptArgument::new(a.name.clone())
                                .with_description(a.description.clone())
                                .with_required(a.required)
                        })
                        .collect();

                    Prompt::new(
                        skill.name.clone(),
                        Some(skill.frontmatter.description.clone()),
                        Some(prompt_args),
                    )
                })
                .collect();

            Ok(ListPromptsResult {
                prompts,
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, McpError>> + Send + '_ {
        async move {
            let skill_idx = self.skill_index.get(&request.name).ok_or_else(|| {
                McpError::invalid_params(format!("Unknown prompt: {}", request.name), None)
            })?;

            let skill = &self.skills[*skill_idx];
            let mut body = skill.body.clone();

            if let Some(ref args) = request.arguments {
                if let Some(full_args) = args.get("arguments").and_then(|v| v.as_str()) {
                    body = body.replace("$ARGUMENTS", full_args);
                }
                for (key, value) in args {
                    if let Some(val_str) = value.as_str() {
                        let placeholder = format!("${}", key.to_uppercase().replace('-', "_"));
                        body = body.replace(&placeholder, val_str);
                    }
                }
            }

            body = body.replace("$ARGUMENTS", "");

            let messages = vec![PromptMessage::new_text(PromptMessageRole::User, body)];

            Ok(GetPromptResult::new(messages)
                .with_description(skill.frontmatter.description.clone()))
        }
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        async move {
            let resources: Vec<Resource> = self
                .skills
                .iter()
                .flat_map(|skill| {
                    skill.references.iter().map(|r| {
                        let raw = RawResource::new(r.uri.clone(), r.name.clone())
                            .with_description(format!("Reference from skill '{}'", skill.name))
                            .with_mime_type(mime_from_name(&r.name));
                        Annotated::new(raw, None)
                    })
                })
                .collect();

            Ok(ListResourcesResult {
                resources,
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        async move {
            let uri_str = request.uri.as_str();

            let (si, ri) = self.resource_index.get(uri_str).ok_or_else(|| {
                McpError::invalid_params(format!("Unknown resource: {}", uri_str), None)
            })?;

            let reference = &self.skills[*si].references[*ri];
            let content = std::fs::read_to_string(&reference.path).map_err(|e| {
                McpError::internal_error(
                    format!("Failed to read {}: {}", reference.path.display(), e),
                    None,
                )
            })?;

            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                content, uri_str,
            )]))
        }
    }
}

fn mime_from_name(name: &str) -> String {
    match name.rsplit('.').next() {
        Some("md") => "text/markdown".to_string(),
        Some("json") => "application/json".to_string(),
        Some("yaml" | "yml") => "text/yaml".to_string(),
        Some("txt") => "text/plain".to_string(),
        Some("sh") => "text/x-shellscript".to_string(),
        Some("py") => "text/x-python".to_string(),
        _ => "text/plain".to_string(),
    }
}
