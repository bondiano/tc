# Task Commander

Rust CLI + TUI: lightweight task tracker and AI agent orchestrator (Claude, Opencode).

Manage tasks in YAML, run parallel agents in git worktrees, review and merge
results -- all from a single CLI.

## Features

- **Task tracking**: tasks + epics + dependency DAG in `.tc/tasks.yaml`
- **Packer**: gitignore-aware file collection into a single markdown/xml for context
- **Impl**: run Claude/Opencode on a specific task with automatic verification
- **Spawn**: parallel execution of multiple agents in isolated git worktrees
- **Review/Merge**: review agent's diff and merge worktree back into main
- **Recovery**: detect orphaned workers after a crash and clean up
- **Sandbox**: integration with `sbx` (Docker AI Sandboxes) and `nono` (Landlock)

## Platform support

tc is **Unix-only** (macOS and Linux). Windows is not supported -- the worker
supervision layer relies on POSIX signals and PID semantics. PRs adding Windows
support (via `windows-sys` for `OpenProcess` / `TerminateProcess`) are welcome.

## Installation

```bash
git clone <repo>
cd tc
cargo install --path crates/tc
```

Or run from source:

```bash
cargo run -- <command>
```

## Quick Start

```bash
# Initialize in the current directory
tc init

# Add tasks
tc add "Setup database" --epic backend
tc add "Build API" --epic backend --after T-001 --files src/api/
tc add "Build UI" --epic frontend --ac "responsive" --ac "dark mode"

# See what's ready to work on
tc list --ready
tc next

# Run an agent on a single task (interactive)
tc impl T-001

# Dry-run (print context without running the agent)
tc impl T-001 --dry-run

# Parallel execution of multiple tasks in worktrees
tc spawn T-001 T-003

# Monitor workers
tc workers
tc logs T-001

# Review + merge
tc review T-001
tc merge T-001
```

## Commands

| Command | Description |
|---------|-------------|
| `tc init` | Create `.tc/` in the current directory |
| `tc add <title> --epic <epic>` | Add a task |
| `tc list [--ready] [--blocked] [--epic X]` | List tasks |
| `tc show <id>` | Show task card |
| `tc next` | Next ready task |
| `tc done <id>` | Mark as done |
| `tc block <id> --reason <text>` | Block with a reason |
| `tc validate` | Validate DAG (cycles, orphans) |
| `tc stats` | Progress by epics |
| `tc graph [--dot]` | DAG visualization (ASCII or Graphviz) |
| `tc pack [<id>] [--estimate]` | Collect relevant files for context |
| `tc impl <id> [--dry-run] [--yolo] [--no-verify]` | Run an agent on a task |
| `tc spawn [<ids>...] [--epic X] [--max N]` | Parallel execution in worktrees |
| `tc workers [--cleanup]` | List active workers |
| `tc logs <id> [--follow]` | Worker log |
| `tc kill <id>` / `tc kill --all` | Stop worker(s) |
| `tc review <id> [--reject "feedback"]` | Diff in pager or reject into notes |
| `tc merge <id>` / `tc merge --all` | Merge worktree back into main |

## Shell Completions

Generate static completions for your shell:

```sh
tc completion bash >> ~/.bash_completion
tc completion zsh  > ~/.zfunc/_tc
tc completion fish > ~/.config/fish/completions/tc.fish
```

### Dynamic task ID completion (bash)

`tc list --ids-only` prints one task ID per line. Wire it into bash completion
so `tc impl <TAB>`, `tc show <TAB>`, `tc test <TAB>`, etc. auto-complete real IDs:

```bash
_tc_task_ids() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    COMPREPLY=( $(compgen -W "$(tc list --ids-only 2>/dev/null)" -- "$cur") )
}
complete -F _tc_task_ids tc
```

Drop this in `~/.bashrc` after the line that sources `tc completion bash`.

## Architecture

```text
crates/
├── tc-core/      # types, DAG, status machine, config -- NO I/O
├── tc-storage/   # YAML persist (Store, init, tasks, config)
├── tc-packer/    # gitignore walker, markdown/xml formats, secret detection
├── tc-executor/  # Executor trait, ClaudeExecutor, OpencodeExecutor, sandbox, verify
├── tc-spawn/     # worktree, scheduler, recovery, merge -- parallel agents
├── tc-tui/       # ratatui-based interface (WIP -- Phase 4)
└── tc/           # CLI binary, commands, output helpers
```

**Rules:**
- `core/` -- pure Rust, no I/O
- `packer/` -- does not know about executor
- `executor/` -- does not know about spawn
- `spawn/` -- uses executor as a trait
- No `unwrap()` outside `#[cfg(test)]`
- `thiserror` in libraries, `anyhow`/`miette` in the `tc` binary

## Running Tests

The easiest way is via [Taskfile](https://taskfile.dev):

```bash
task test           # all tests
task test:spawn     # tc-spawn only
task lint           # clippy + fmt
task ci             # lint + test (same as CI)
```

Or directly via cargo:

```bash
# All tests (unit + integration)
cargo test

# Tests for a specific crate
cargo test -p tc-spawn
cargo test -p tc-core

# Integration tests only
cargo test -p tc-spawn --test integration
cargo test -p tc --test integration

# A specific test
cargo test -p tc-spawn full_spawn_poll_merge_flow -- --nocapture

# Lint
cargo clippy -- -D warnings
cargo fmt --check
```

### Manual Spawn-Flow Testing

Testing with real `claude`/`opencode` requires them to be in PATH:

```bash
# In a test directory
mkdir /tmp/tc-demo && cd /tmp/tc-demo
git init && git checkout -b main
echo "# demo" > README.md && git add . && git commit -m init

tc init
tc add "Add hello world" --epic demo --files src/

# Dry-run -- see context without launching
tc impl T-001 --dry-run

# Real run (needs claude in PATH)
tc impl T-001

# Parallel (needs claude + git)
tc spawn T-001
tc workers
tc logs T-001 --follow
```
