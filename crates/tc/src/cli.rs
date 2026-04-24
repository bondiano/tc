use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use tc_core::task::Priority;

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

    /// Edit task in $EDITOR
    Edit {
        /// Task ID
        id: String,
    },

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

    /// Generate changelog from done tasks
    Changelog(ChangelogArgs),

    /// Launch TUI
    Tui,

    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        shell: Shell,
    },
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
    /// Priority level
    #[arg(long, value_enum, default_value_t = CliPriority::Normal)]
    pub priority: CliPriority,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum CliPriority {
    Critical,
    High,
    Normal,
    Low,
}

impl From<CliPriority> for Priority {
    fn from(p: CliPriority) -> Self {
        match p {
            CliPriority::Critical => Priority::Critical,
            CliPriority::High => Priority::High,
            CliPriority::Normal => Priority::Normal,
            CliPriority::Low => Priority::Low,
        }
    }
}

#[derive(clap::Args)]
pub struct ListArgs {
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
    #[command(subcommand)]
    pub source: ImportSource,
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
