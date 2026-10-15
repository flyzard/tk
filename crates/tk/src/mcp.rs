//! `tk mcp`: an MCP server over stdio, a thin layer over the same `Command`
//! and `execute` the CLI, `tk apply` and the TUI use.

use std::sync::{Arc, Mutex, MutexGuard};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::transport::stdio;
use rmcp::{ErrorData, ServerHandler, ServiceExt as _, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use tk_app::{AppError, Command, Ctx, ListFilter, Scope, execute};
use tk_core::{Label, ProjectKey, StateName, TicketId, Timestamp};

#[derive(Clone)]
pub struct TkServer {
    ctx: Arc<Mutex<Ctx>>,
    clock: fn() -> Timestamp,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RunArgs {
    /// The command, exactly as one line of `tk apply` takes it.
    command: Command,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NextArgs {
    /// Defaults to the server's project, else all projects.
    #[serde(default)]
    project: Option<ProjectKey>,
    /// Also start it, taking its lease.
    #[serde(default)]
    claim: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListArgs {
    /// Include done tickets.
    #[serde(default)]
    all: bool,
    #[serde(default)]
    states: Vec<StateName>,
    /// Tickets must carry every one of these.
    #[serde(default)]
    labels: Vec<Label>,
    /// Every project in the workspace, not just the server's.
    #[serde(default)]
    workspace: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ShowArgs {
    id: TicketId,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LintArgs {
    #[serde(default)]
    workspace: bool,
}

#[tool_router]
impl TkServer {
    #[tool(
        description = "Run one tk command: new, edit, start, block, unblock, release, finish, reopen, log, dep_add, dep_remove or next. Pass if_version from `show` to refuse the write if someone changed the ticket since."
    )]
    async fn run(
        &self,
        Parameters(RunArgs { command }): Parameters<RunArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut ctx = self.ctx()?;
        Ok(reply(run(command, &mut ctx)))
    }

    #[tool(
        description = "The top ready ticket nobody else holds; with claim, start it and take its lease."
    )]
    async fn next(
        &self,
        Parameters(NextArgs { project, claim }): Parameters<NextArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut ctx = self.ctx()?;
        Ok(reply(run(Command::Next { project, claim }, &mut ctx)))
    }

    #[tool(description = "List tickets, open ones by default, with what each is still waiting on.")]
    async fn list(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let ListArgs {
            all,
            states,
            labels,
            workspace,
        } = args;
        let filter = ListFilter {
            scope: Scope::from_workspace_flag(workspace),
            all,
            states,
            labels,
        };
        let ctx = self.ctx()?;
        Ok(reply(tk_app::list(&ctx, &filter)))
    }

    #[tool(
        description = "One ticket with its body, version and the dependencies it is waiting on."
    )]
    async fn show(
        &self,
        Parameters(ShowArgs { id }): Parameters<ShowArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = self.ctx()?;
        Ok(reply(tk_app::show(&ctx, &id)))
    }

    #[tool(description = "Every ticket file that doesn't parse and every dependency problem.")]
    async fn lint(
        &self,
        Parameters(LintArgs { workspace }): Parameters<LintArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let ctx = self.ctx()?;
        Ok(reply(
            tk_app::lint(&ctx, Scope::from_workspace_flag(workspace))
                .map(|report| serde_json::json!({ "findings": report.findings() })),
        ))
    }

    #[tool(
        description = "How to work with tk: the status rules, versions, leases and the working loop."
    )]
    async fn guide(&self) -> String {
        include_str!("guide.md").to_owned()
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "rmcp's tool_handler macro generates async trait methods"
)]
#[tool_handler(
    name = "tk",
    instructions = "Tickets as Markdown files in git. Call `guide` once for the rules. Usual loop: `next` with claim, `show`, `run` with op log as you work, then `run` with op finish."
)]
impl ServerHandler for TkServer {}

impl TkServer {
    /// The context, with the clock read now: every call gets its own time.
    fn ctx(&self) -> Result<MutexGuard<'_, Ctx>, ErrorData> {
        let mut ctx = self
            .ctx
            .lock()
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        ctx.now = (self.clock)();
        Ok(ctx)
    }
}

/// Runs a command; warnings that don't undo it go to stderr, which MCP clients log.
fn run(command: Command, ctx: &mut Ctx) -> Result<tk_app::Outcome, AppError> {
    let outcome = execute(command, ctx);
    for warning in ctx.warnings.drain(..) {
        eprintln!("warning: {warning}");
    }
    outcome
}

/// Success as structured JSON (always an object, as MCP requires); failure as the same `{code, message, hint}` the CLI prints.
fn reply<T: Serialize>(result: Result<T, AppError>) -> CallToolResult {
    let value =
        result.and_then(|v| serde_json::to_value(v).map_err(|e| AppError::Internal(e.to_string())));
    match value {
        Ok(value) => CallToolResult::structured(value),
        Err(e) => CallToolResult::structured_error(
            serde_json::to_value(e.to_json())
                .unwrap_or_else(|_| serde_json::Value::String(e.to_string())),
        ),
    }
}

/// Serves until the client closes stdin.
///
/// # Errors
/// Starting the runtime or the transport failed.
pub fn serve(ctx: Ctx, clock: fn() -> Timestamp) -> Result<(), AppError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let server = TkServer {
            ctx: Arc::new(Mutex::new(ctx)),
            clock,
        };
        let running = server
            .serve(stdio())
            .await
            .map_err(|e| AppError::Internal(format!("mcp: {e}")))?;
        running
            .waiting()
            .await
            .map_err(|e| AppError::Internal(format!("mcp: {e}")))?;
        Ok(())
    })
}
