use std::time::Duration;

use chrono::NaiveDate;
use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use tc_core::task::Priority;

use crate::cli_parsers::{DateArg, DurationArg, parse_duration, parse_naive_date};

#[derive(Parser)]
#[command(
    name = "tc",
    version,
    about = "Task Commander -- task tracker + agent orchestrator"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize .tc/ in the current directory
    Init,

    /// Add a new task
    Add(AddArgs),

    /// List tasks
    List(ListArgs),

    /// Show task details
    Show {
        /// Task ID (e.g. T-001)
        id: String,
    },

    /// Edit task: with no flags opens $EDITOR; with flags applies them in place
    Edit(EditArgs),

    /// Delete a task
    Delete(DeleteArgs),

    /// Mark task as done
    Done {
        /// Task ID
        id: String,
    },

    /// Block a task
    Block {
        /// Task ID
        id: String,
        /// Reason for blocking
        #[arg(long)]
        reason: String,
    },

    /// Set task status
    Status {
        /// Task ID
        id: String,
        /// New status
        status: String,
    },

    /// Show next ready task
    Next,

    /// Smart view: tasks due or scheduled today
    Today,

    /// Smart view: tasks due or scheduled within the next N days
    Upcoming(UpcomingArgs),

    /// Smart view: open tasks with no due/scheduled date
    Inbox,

    /// Smart view: tasks past their due date
    Overdue,

    /// Fuzzy search by ID, title, or tag
    Find(FindArgs),

    /// Validate DAG integrity
    Validate,

    /// Show progress stats
    Stats,

    /// Show DAG graph
    Graph(GraphArgs),

    /// Pack codebase files
    Pack(PackArgs),

    /// Generate implementation plan for a task
    Plan(PlanArgs),

    /// Implement a task (run agent)
    #[command(name = "impl")]
    Impl(ImplArgs),

    /// Spawn parallel agents for ready tasks
    Spawn(SpawnArgs),

    /// List active workers
    Workers(WorkersArgs),

    /// View agent logs
    Logs(LogsArgs),

    /// Kill a worker
    Kill(KillArgs),

    /// Attach to a worker's tmux session
    Attach(AttachArgs),

    /// Review agent work
    Review(ReviewArgs),

    /// Merge worktree back to main
    Merge(MergeArgs),

    /// Run test agent
    Test(TestArgs),

    /// Manage epics
    Epic(EpicArgs),

    /// Manage project configuration
    Config(ConfigArgs),

    /// Import tasks from external sources (GitHub Issues, Linear)
    Import(ImportArgs),

    /// Export tasks to JSON or kairo-style markdown (round-trips with `tc import`)
    Export(ExportArgs),

    /// Generate changelog from done tasks
    Changelog(ChangelogArgs),

    /// Normalize .tc/tasks.yaml to the current schema (fills defaults for
    /// any newly-introduced fields). Safe to re-run.
    Migrate(MigrateArgs),

    /// Launch TUI
    Tui,

    /// Manage TUI appearance (themes, etc.)
    Ui(UiArgs),

    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        shell: Shell,
    },
}

#[derive(clap::Args)]
pub struct UiArgs {
    #[command(subcommand)]
    pub action: UiAction,
}

#[derive(Subcommand)]
pub enum UiAction {
    /// Manage TUI color theme
    Theme(UiThemeArgs),
}

#[derive(clap::Args)]
pub struct UiThemeArgs {
    /// Theme name to activate. Omit to print the current theme. Use `list` to
    /// list available presets.
    pub name: Option<String>,
}

#[derive(clap::Args)]
pub struct EpicArgs {
    #[command(subcommand)]
    pub command: EpicCommands,
}

#[derive(Subcommand)]
pub enum EpicCommands {
    /// List all epics with progress
    List,
    /// Show epic details (tasks, progress)
    Show {
        /// Epic name
        name: String,
    },
    /// Rename an epic across all tasks
    Rename {
        /// Current epic name
        old: String,
        /// New epic name
        new: String,
    },
}

#[derive(clap::Args)]
pub struct AddArgs {
    /// Task title
    pub title: String,
    /// Epic name
    #[arg(long)]
    pub epic: String,
    /// Dependencies (comma-separated task IDs)
    #[arg(long, value_delimiter = ',')]
    pub after: Option<Vec<String>>,
    /// Relevant file paths (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub files: Option<Vec<String>>,
    /// Acceptance criteria (repeatable)
    #[arg(long = "ac")]
    pub acceptance_criteria: Option<Vec<String>>,
    /// Priority level (p1..p5)
    #[arg(long, value_enum, default_value_t = CliPriority::P3)]
    pub priority: CliPriority,
    /// Tag (repeatable: --tag backend --tag perf)
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    /// Due date (YYYY-MM-DD)
    #[arg(long, value_parser = parse_naive_date)]
    pub due: Option<NaiveDate>,
    /// Scheduled start date (YYYY-MM-DD)
    #[arg(long, value_parser = parse_naive_date)]
    pub scheduled: Option<NaiveDate>,
    /// Time estimate (e.g. 2h, 45m, 1h30m)
    #[arg(long, value_parser = parse_duration)]
    pub estimate: Option<Duration>,
}

/// Inline-edit flags. With no flags set, `tc edit` opens $EDITOR (current
/// behavior). When *any* flag is set we skip the editor and apply the patch
/// in place via `Store::update_tasks`.
#[derive(clap::Args)]
pub struct EditArgs {
    /// Task ID (e.g. T-001)
    pub id: String,
    /// New title
    #[arg(long)]
    pub title: Option<String>,
    /// New status
    #[arg(long)]
    pub status: Option<String>,
    /// New epic
    #[arg(long)]
    pub epic: Option<String>,
    /// New priority (p1..p5)
    #[arg(long, value_enum)]
    pub priority: Option<CliPriority>,
    /// Replace all tags (repeatable: --tag a --tag b)
    #[arg(long = "tag")]
    pub tags: Option<Vec<String>>,
    /// Append a tag (repeatable; preserves existing)
    #[arg(long = "add-tag")]
    pub add_tags: Vec<String>,
    /// Remove a tag (repeatable; no-op if not present)
    #[arg(long = "rm-tag")]
    pub rm_tags: Vec<String>,
    /// Due date (YYYY-MM-DD or `clear`)
    #[arg(long)]
    pub due: Option<DateArg>,
    /// Scheduled date (YYYY-MM-DD or `clear`)
    #[arg(long)]
    pub scheduled: Option<DateArg>,
    /// Estimate (e.g. 2h, 45m, or `clear`)
    #[arg(long)]
    pub estimate: Option<DurationArg>,
    /// Append acceptance criterion (repeatable)
    #[arg(long = "add-ac")]
    pub add_acceptance_criteria: Vec<String>,
}

impl EditArgs {
    /// True when at least one mutating flag was supplied. Drives the
    /// dispatch in `commands::edit::run`.
    pub fn has_any_patch(&self) -> bool {
        self.title.is_some()
            || self.status.is_some()
            || self.epic.is_some()
            || self.priority.is_some()
            || self.tags.is_some()
            || !self.add_tags.is_empty()
            || !self.rm_tags.is_empty()
            || self.due.is_some()
            || self.scheduled.is_some()
            || self.estimate.is_some()
            || !self.add_acceptance_criteria.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum CliPriority {
    P1,
    P2,
    #[default]
    P3,
    P4,
    P5,
}

impl From<CliPriority> for Priority {
    fn from(p: CliPriority) -> Self {
        match p {
            CliPriority::P1 => Priority::P1,
            CliPriority::P2 => Priority::P2,
            CliPriority::P3 => Priority::P3,
            CliPriority::P4 => Priority::P4,
            CliPriority::P5 => Priority::P5,
        }
    }
}

#[derive(clap::Args)]
pub struct ListArgs {
    /// Filter DSL query (e.g. "priority:p1 tag:backend status:todo").
    /// Supported keys: priority, tag, status, epic, due, scheduled, id, title.
    /// Prefix a term with `!` to negate it.
    pub query: Vec<String>,
    /// Show only ready tasks
    #[arg(long)]
    pub ready: bool,
    /// Show only blocked tasks
    #[arg(long)]
    pub blocked: bool,
    /// Filter by epic
    #[arg(long)]
    pub epic: Option<String>,
    /// Print task IDs one per line (for shell completion)
    #[arg(long)]
    pub ids_only: bool,
}

#[derive(clap::Args)]
pub struct UpcomingArgs {
    /// Window in days (default: 7)
    #[arg(long, default_value_t = 7)]
    pub days: u32,
}

#[derive(clap::Args)]
pub struct FindArgs {
    /// Query string -- matched against task ID, title, and tags
    pub query: String,
    /// Maximum number of results (default: 20)
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    /// Print matched IDs only (one per line, for shell scripting)
    #[arg(long)]
    pub ids_only: bool,
}

#[derive(clap::Args)]
pub struct GraphArgs {
    /// Output Graphviz DOT format
    #[arg(long)]
    pub dot: bool,
}

#[derive(clap::Args)]
pub struct PackArgs {
    /// Task ID (pack only task-relevant files)
    pub task_id: Option<String>,
    /// Pack files for entire epic
    #[arg(long)]
    pub epic: Option<String>,
    /// Only show token estimate
    #[arg(long)]
    pub estimate: bool,
}

#[derive(clap::Args)]
pub struct PlanArgs {
    /// Task ID
    pub id: String,
    /// Use opencode instead of claude
    #[arg(long)]
    pub opencode: bool,
    /// Show prompt without running agent
    #[arg(long)]
    pub dry_run: bool,
    /// Include packed files in context
    #[arg(long)]
    pub pack: bool,
    /// Save plan to task notes
    #[arg(long)]
    pub save: bool,
}

#[derive(clap::Args)]
pub struct ImplArgs {
    /// Task ID
    pub id: String,
    /// Accept edits mode
    #[arg(long)]
    pub accept: bool,
    /// Use opencode instead of claude
    #[arg(long)]
    pub opencode: bool,
    /// Headless mode with sandbox
    #[arg(long)]
    pub yolo: bool,
    /// Disable sandbox in yolo mode
    #[arg(long)]
    pub no_sandbox: bool,
    /// Show context without running
    #[arg(long)]
    pub dry_run: bool,
    /// Include packed files in dry run
    #[arg(long)]
    pub pack: bool,
    /// Skip verification loop after agent completes
    #[arg(long)]
    pub no_verify: bool,
}

#[derive(clap::Args)]
pub struct SpawnArgs {
    /// Task IDs to spawn (empty = all ready)
    pub task_ids: Vec<String>,
    /// Spawn only from this epic
    #[arg(long)]
    pub epic: Option<String>,
    /// Override max parallel workers
    #[arg(long)]
    pub max: Option<usize>,
    /// Fire-and-forget: spawn workers and exit immediately (default: foreground drive)
    #[arg(long)]
    pub detach: bool,
    /// Disable tmux sessions for workers (use plain background processes)
    #[arg(long)]
    pub no_tmux: bool,
}

#[derive(clap::Args)]
pub struct WorkersArgs {
    /// Follow (live tail) worker output
    #[arg(long)]
    pub follow: bool,
    /// Cleanup orphaned workers
    #[arg(long)]
    pub cleanup: bool,
}

#[derive(clap::Args)]
pub struct LogsArgs {
    /// Task ID
    pub id: String,
    /// Follow (live tail)
    #[arg(long)]
    pub follow: bool,
}

#[derive(clap::Args)]
pub struct KillArgs {
    /// Task ID (or --all)
    pub id: Option<String>,
    /// Kill all workers
    #[arg(long)]
    pub all: bool,
}

#[derive(clap::Args)]
pub struct AttachArgs {
    /// Task ID
    pub id: String,
}

#[derive(clap::Args)]
pub struct ReviewArgs {
    /// Task ID
    pub id: String,
    /// Reject with feedback
    #[arg(long)]
    pub reject: Option<String>,
}

#[derive(clap::Args)]
pub struct MergeArgs {
    /// Task ID (or --all)
    pub id: Option<String>,
    /// Merge all done worktrees
    #[arg(long)]
    pub all: bool,
}

#[derive(clap::Args)]
pub struct DeleteArgs {
    /// Task ID
    pub id: String,
    /// Force delete even if other tasks depend on this one
    #[arg(long)]
    pub force: bool,
}

#[derive(clap::Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: Option<ConfigAction>,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show all configuration (default)
    List,
    /// Get a config value by dot-path (e.g. spawn.max_parallel)
    Get {
        /// Dot-separated key path
        key: String,
    },
    /// Set a config value by dot-path
    Set {
        /// Dot-separated key path
        key: String,
        /// New value
        value: String,
    },
    /// Open config in $EDITOR
    Edit,
    /// Print config file path
    Path,
    /// Reset config to defaults
    Reset,
}

#[derive(clap::Args)]
pub struct TestArgs {
    /// Task ID
    pub id: String,
    /// Enable browser MCP
    #[arg(long)]
    pub browser: bool,
    /// Disable all MCP servers
    #[arg(long)]
    pub no_mcp: bool,
}

#[derive(clap::Args)]
pub struct ImportArgs {
    /// File-based import format (alternative to GitHub/Linear API subcommands)
    #[arg(long, value_enum)]
    pub format: Option<ImportFormat>,
    /// Path to file (required with --format)
    #[arg(long)]
    pub file: Option<std::path::PathBuf>,
    /// Epic for imported tasks (required with --format)
    #[arg(long)]
    pub epic: Option<String>,
    /// Dry-run: print what would be imported without creating tasks
    #[arg(long)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub source: Option<ImportSource>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ImportFormat {
    /// JSON array of task records (see docs)
    Json,
    /// Kairo-style markdown checklist (`- [ ] Title +tag:foo !p1`)
    KairoMd,
    /// Linear CSV export
    LinearCsv,
}

#[derive(Subcommand)]
pub enum ImportSource {
    /// Import from GitHub Issues
    Github(GithubImportArgs),
    /// Import from Linear issues
    Linear(LinearImportArgs),
}

#[derive(clap::Args)]
pub struct GithubImportArgs {
    /// Repository (owner/repo)
    #[arg(long)]
    pub repo: String,
    /// Epic name for imported tasks
    #[arg(long)]
    pub epic: String,
    /// Filter by label
    #[arg(long)]
    pub label: Option<String>,
    /// Filter by milestone
    #[arg(long)]
    pub milestone: Option<String>,
    /// Filter by state (open, closed, all) [default: open]
    #[arg(long, default_value = "open")]
    pub state: String,
    /// Dry run -- show what would be imported without creating tasks
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(clap::Args)]
pub struct LinearImportArgs {
    /// Linear team key (e.g. ENG)
    #[arg(long)]
    pub team: String,
    /// Epic name for imported tasks
    #[arg(long)]
    pub epic: String,
    /// Filter by project name
    #[arg(long)]
    pub project: Option<String>,
    /// Filter by state (e.g. "In Progress", "Todo")
    #[arg(long)]
    pub state: Option<String>,
    /// Filter by label
    #[arg(long)]
    pub label: Option<String>,
    /// Dry run -- show what would be imported without creating tasks
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(clap::Args)]
pub struct ExportArgs {
    /// Output format (default: json)
    #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
    pub format: ExportFormat,
    /// Filter DSL query (same grammar as `tc list`).
    /// Example: `tc export --format json "priority:p1 tag:backend"`
    pub query: Vec<String>,
    /// Filter by epic (combined with query, AND)
    #[arg(long)]
    pub epic: Option<String>,
    /// Write to file instead of stdout
    #[arg(long, short)]
    pub output: Option<std::path::PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ExportFormat {
    /// JSON array, schema mirrors `tc import --format json`
    Json,
    /// Kairo-style markdown checklist, round-trips with `tc import --format kairo-md`
    Md,
}

#[derive(clap::Args)]
pub struct ChangelogArgs {
    /// Filter by epic
    #[arg(long)]
    pub epic: Option<String>,
    /// Output format (markdown or plain)
    #[arg(long, default_value = "markdown")]
    pub format: ChangelogFormat,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum ChangelogFormat {
    Markdown,
    Plain,
}

#[derive(clap::Args)]
pub struct MigrateArgs {
    /// Print the normalized YAML without writing.
    #[arg(long)]
    pub dry_run: bool,
    /// Exit non-zero if migration would change the file (CI use).
    /// Mutually exclusive with --dry-run.
    #[arg(long, conflicts_with = "dry_run")]
    pub check: bool,
}
