# tc -- Roadmap

> Текущее состояние: Фазы 1-4 завершены. M-5.2 (edit + delete) готов. Фаза 5 в процессе.
> 273 теста (58 core, 40 packer, 32 executor, 24 storage, 48 spawn, 19 cli unit, 49 integration, 3 tui).
> Следующий шаг: M-5.1 (tc test -- тестировщик-агент) или M-5.3 (completions + config validation).

---

## Легенда

- **Файлы** -- что имплементировать/модифицировать
- **AC** -- acceptance criteria (что должно работать после)
- **Зависит от** -- номера milestone-ов, которые нужны до старта
- [x] -- сделано
- [ ] -- нужно имплементировать

---

## Фаза 1 -- Core CLI (MVP) [DONE]

Все milestone-ы (M-1.1 .. M-1.8) завершены.

**Что реализовано:**
- [x] **M-1.1** Output helpers -- цвета (NO_COLOR support), таблицы, detail formatter, print_success/warning/error
- [x] **M-1.2** `tc init` -- создание `.tc/` структуры, `.gitignore`, обработка AlreadyInitialized
- [x] **M-1.3** `tc add` -- все поля (title, epic, --after, --files, --ac), auto-increment ID
- [x] **M-1.4** `tc list` -- группировка по epic, фильтры --ready/--blocked/--epic
- [x] **M-1.5** `tc show/next/done/block/status` -- полная карточка, DAG-aware done, StatusMachine валидация
- [x] **M-1.6** `tc validate + stats` -- DAG валидация (cycles, orphans), прогресс-бары по epic
- [x] **M-1.7** `tc graph` -- ASCII + Graphviz DOT
- [x] **M-1.8** Тесты -- tc-core 100% unit, storage roundtrip, 36 integration tests

---

## Definition of Ready (DoR) -- для всех фаз

Milestone готов к работе, когда:
1. Все milestone-ы из **Зависит от** имеют статус DONE
2. Файлы из секции **Файлы** существуют (хотя бы как stubs/scaffolding)
3. Типы и traits, от которых зависит milestone, скомпилированы (`cargo check` проходит)
4. AC сформулированы и однозначны

## Definition of Done (DoD) -- для всех фаз

Milestone считается завершённым, когда:
1. Все пункты из **Что сделать** реализованы
2. Все **AC** выполнены и проверяемы
3. `cargo check` -- компилируется без ошибок
4. `cargo fmt --check` -- форматирование ок
5. `cargo clippy -- -D warnings` -- 0 warnings на изменённых файлах
6. `cargo test` -- все существующие + новые тесты проходят
7. Нет `todo!()` в коде milestone-а
8. Нет `unwrap()`/`expect()` вне `#[cfg(test)]`
9. Error handling через `thiserror` (lib) / `anyhow` (bin)
10. Код соответствует архитектуре: core/ без I/O, packer/ не знает про executor, и т.д.

---

## Фаза 2 -- Packer + Impl [DONE]

Цель: `tc pack T-001`, `tc impl T-001 --dry-run`, `tc impl T-001 --yolo` работают.

### M-2.1: packer/collect.rs (gitignore-aware walker)

**Файлы:**
- `crates/tc-packer/src/collect.rs`

**Что сделать:**
- [x] Использовать `ignore` crate для gitignore-aware обхода
- [x] Фильтрация по `include_paths` (из task.files) -- если пусто, весь проект
- [x] Фильтрация по `exclude_patterns` (из config + task.pack_exclude) через `globset`
- [x] Чтение содержимого файлов, skip binary (detect по первым байтам)
- [x] Возврат `Vec<CollectedFile>` с относительными путями

**AC:**
- Collect в tempdir с .gitignore -- правильно фильтрует
- include_paths ограничивает до указанных директорий
- Binary файлы пропускаются
- Тесты: 5+ с разными комбинациями include/exclude

**Зависит от:** ничего (независимый модуль)

---

### M-2.2: packer/format.rs (Markdown + XML)

**Файлы:**
- `crates/tc-packer/src/format.rs`

**Что сделать:**
- [x] `PackStyle::Markdown`: `### \`path/to/file\`\n\`\`\`ext\n{content}\n\`\`\`\n`
- [x] `PackStyle::Xml`: `<file path="path/to/file">\n{content}\n</file>`
- [x] Определение языка по расширению для code fence

**AC:**
- Markdown output -- валидный markdown с code fences
- XML output -- валидный XML
- Тесты: оба формата, edge cases (пустой файл, без расширения)

**Зависит от:** M-2.1

---

### M-2.3: packer/security.rs (secret detection)

**Файлы:**
- `crates/tc-packer/src/security.rs`

**Что сделать:**
- [x] Regex patterns: AWS keys, GitHub tokens, private keys, generic API keys, passwords в env files
- [x] `scan_for_secrets(content) -> Vec<String>` -- описания найденных секретов
- [x] Вызывается из collect -- warning или skip

**AC:**
- Обнаруживает `AKIA...`, `ghp_...`, `sk-...`, `-----BEGIN RSA PRIVATE KEY-----`
- False positives минимальны
- Тесты: 10+ patterns

**Зависит от:** ничего

---

### M-2.4: tc pack command + public API

**Файлы:**
- `crates/tc-packer/src/lib.rs` (pub fn pack())
- `crates/tc/src/commands/pack.rs`

**Что сделать:**
- [x] `pack(options: PackOptions) -> PackResult` -- orchestrate collect -> security scan -> format -> token check
- [x] Token budget: если exceeded, усечь файлы (по приоритету: task.files сначала)
- [x] CLI: `tc pack` (весь проект), `tc pack T-001` (файлы таска), `tc pack --epic` (файлы epic)
- [x] `--estimate` -- только token count, без контента

**AC:**
- `tc pack` -- packed codebase на stdout
- `tc pack T-001` -- только файлы из task.files
- `tc pack T-001 --estimate` -- "~12,500 tokens (42 files)"
- Warning при обнаружении секретов
- Тест: end-to-end с tempdir проектом

**Зависит от:** M-2.1, M-2.2, M-2.3

---

### M-2.5: executor/claude.rs + sandbox.rs

**Файлы:**
- `crates/tc-executor/src/claude.rs`
- `crates/tc-executor/src/sandbox.rs`

**Что сделать:**
- [x] `ClaudeExecutor::build_command`: собрать `claude` CLI command с флагами
  - Interactive: `claude` (bare)
  - Accept: `claude --permission-mode acceptEdits`
  - Yolo: `claude --permission-mode bypassPermissions --print "{context}"`
- [x] `ClaudeExecutor::execute`: spawn child, pipe stdout/stderr в log_sink, wait
- [x] `sandbox.rs` -- sandbox provider chain с двумя провайдерами:
  - `SandboxProvider::Sbx` -- Docker AI Sandboxes (`sbx run claude -- {flags}`)
    - MicroVM isolation: отдельное ядро, Docker daemon, deny-by-default network
    - API keys через host-side proxy (не попадают в VM)
    - Auto-configure network policy: `sbx policy allow network "{domains}"`
    - Workspace монтируется read-write, остальная FS недоступна
  - `SandboxProvider::Nono` -- Landlock sandbox (`nono run --allow {working_dir} -- {cmd}`)
    - Linux-only fallback
    - extra_allow из конфига
  - `SandboxProvider::None` -- graceful degradation с warning
- [x] `detect_provider(config)`: `auto` -> sbx -> nono -> none; или explicit из config
- [x] Config: `sandbox.provider: auto | sbx | nono | never`
- [x] Config: `sandbox.network_allow: [domains]` -- для sbx policy

**AC:**
- `build_command` в Interactive mode -- просто `claude`
- `build_command` в Yolo mode -- `claude --permission-mode bypassPermissions --print "..."`
- С sbx -- команда обёрнута в `sbx run claude -- ...`
- С nono (fallback) -- команда обёрнута в `nono run ...`
- Graceful degradation без обоих -- warning + запуск без sandbox
- `detect_provider` корректно определяет приоритет: sbx > nono > none
- Тесты: build_command для всех mode-ов (unit, без spawn), detect_provider mock

**Зависит от:** ничего

---

### M-2.6: executor/opencode.rs

**Файлы:**
- `crates/tc-executor/src/opencode.rs`

**Что сделать:**
- [x] `OpencodeExecutor::build_command`: аналогично claude, но `opencode` CLI
- [x] `OpencodeExecutor::execute`: spawn + pipe + wait
- [x] Проверка наличия в PATH через `which::which("opencode")`

**AC:**
- `build_command` -- корректная opencode команда
- NotFound ошибка если opencode не в PATH
- Тесты: build_command unit тесты

**Зависит от:** M-2.5 (share logic pattern)

---

### M-2.7: tc impl (полный flow + verification)

**Файлы:**
- `crates/tc/src/commands/impl_.rs`
- `crates/tc-executor/src/verify.rs` (NEW)

**Что сделать:**
- [x] Load task, validate: not terminal, deps resolved
- [x] Set status -> in_progress, save
- [x] IF task.files -> pack (через tc-packer)
- [x] Render TASK_CONTEXT.md через ContextRenderer (включая acceptance_criteria)
- [x] IF --dry-run: вывести context и выйти
- [x] IF --yolo: wrap в sandbox, spawn headless
- [x] ELSE: записать TASK_CONTEXT.md, exec interactive
- [x] Verification loop (если config.verification.commands):
  - [x] Run each command sequentially после завершения агента
  - [x] ALL PASS -> status: config.verification.on_pass (default: review)
  - [x] ANY FAIL -> status: config.verification.on_fail (default: blocked)
- [x] Cleanup: удалить TASK_CONTEXT.md
- [x] Prompt: "Mark as done? [y/n/review]" (skip если verification уже выставил статус)

**AC:**
- `tc impl T-001 --dry-run` -- выводит TASK_CONTEXT.md (с AC секцией)
- `tc impl T-001 --dry-run --pack` -- + packed files inline
- `tc impl T-001 --yolo` -- запускает claude headless с sandbox
- `tc impl T-001` -- interactive mode, inject в CLAUDE.md, cleanup после выхода
- После агента: verification commands запускаются, на pass -> review, на fail -> retry/blocked
- `tc impl T-001 --no-verify` -- skip verification loop
- Error если task done или deps не resolved

**Зависит от:** M-2.4, M-2.5, M-1.5

---

## Фаза 3 -- Spawn (параллельные агенты) [DONE]

Цель: `tc spawn T-001 T-002` запускает 2 агента в worktree, `tc workers`, `tc kill`, `tc review`, `tc merge`.

### M-3.1: spawn/worktree.rs (git worktree lifecycle)

**Файлы:**
- `crates/tc-spawn/src/worktree.rs`

**Что сделать:**
- [x] `create(task_id)`: `git worktree add .tc-worktrees/{id} -b {prefix}{id}` от base_branch
- [x] `remove(task_id)`: `git worktree remove .tc-worktrees/{id}` + `git branch -d {prefix}{id}`
- [x] `list()`: `git worktree list --porcelain` -> parse -> Vec<WorktreeInfo>
- [x] Copy `.tc/` в worktree после создания
- [x] Все git операции через `std::process::Command`

**AC:**
- create + list -- worktree видна
- create + remove -- cleanup полный
- Повторный create -- ошибка WorktreeExists
- Тесты: с реальным git repo в tempdir

**Зависит от:** ничего

---

### M-3.2: spawn/scheduler.rs (spawn + monitor + recovery)

**Файлы:**
- `crates/tc-spawn/src/scheduler.rs`
- `crates/tc-spawn/src/process.rs`
- `crates/tc-spawn/src/recovery.rs` (NEW)

**Что сделать:**
- [x] `spawn_tasks`: для каждого task_id -- create worktree -> generate context -> build_command -> spawn -> WorkerHandle
- [x] Respect max_parallel: если workers >= max, queue остальные
- [x] Write worker state file: `.tc/workers/{task_id}.json` с `{ pid, started_at, worktree_path, status }`
- [x] Monitor loop (async): poll child processes
  - exit 0 -> run verification (через executor verify) -> pass: status review, fail: retry/blocked
  - exit != 0 -> status blocked
  - Update worker state file на каждом transition
- [x] File conflict detection: проверить task.files на пересечение между задачами
- [x] Pipe stdout/stderr в log file
- [x] **recovery.rs**: crash recovery
  - [x] `scan_workers()` -- scan `.tc/workers/*.json`, check PID liveness (`kill(pid, 0)`)
  - [x] `cleanup_orphans()` -- dead PID: reset task status -> todo, cleanup worktree (optional), remove state file
  - [x] Вызывается из `tc workers` (автоматически) и `tc workers --cleanup` (принудительно)

**AC:**
- 2 tasks spawn -> 2 worktrees + 2 processes + 2 state files
- max_parallel=1 -> второй ждёт
- File conflict -> ошибка до spawn
- Exit 0 + verification pass -> review, exit 0 + verification fail -> retry/blocked, exit 1 -> blocked
- После kill -9 scheduler: `tc workers` показывает orphaned workers, `tc workers --cleanup` чистит
- State files удаляются при merge/cleanup

**Зависит от:** M-3.1, M-2.5 (+ M-2.7 для verification)

---

### M-3.3: spawn/merge.rs

**Файлы:**
- `crates/tc-spawn/src/merge.rs`

**Что сделать:**
- [x] `merge_worktree`: `git merge {prefix}{id}` в main, cleanup worktree
- [x] Detect conflicts -> MergeResult::Conflict с details
- [x] Auto-abort merge при конфликте (`git merge --abort`)
- [x] Success -> remove worktree + delete branch

**AC:**
- Clean merge -> worktree удалён, branch удалён
- Conflict -> abort, worktree сохранён, описание конфликта

**Зависит от:** M-3.1

---

### M-3.4: tc spawn + tc workers + tc logs + tc kill

**Файлы:**
- `crates/tc/src/commands/spawn.rs`

**Что сделать:**
- [x] `run(SpawnArgs)`: discover ready tasks (или из args), create Scheduler, spawn_tasks
- [x] `run_workers`: list active workers (task_id, status, worktree path, +/- lines diff) + PID liveness check
- [x] `run_workers_cleanup`: scan orphaned workers, reset statuses, cleanup worktrees
- [x] `run_logs`: читать log file, `--follow` через tail -f аналог
- [x] `run_kill`: найти worker, вызвать WorkerHandle::kill, cleanup

**AC:**
- `tc spawn` -- запускает все ready
- `tc spawn T-001 T-002` -- конкретные
- `tc spawn --epic backend` -- фильтр по epic
- `tc workers` -- таблица workers + PID liveness
- `tc workers --cleanup` -- cleanup orphaned workers
- `tc logs T-001` -- вывод лога
- `tc kill T-001` -- worker убит
- `tc kill --all` -- все workers убиты

**Зависит от:** M-3.2

---

### M-3.5: tc review + tc merge

**Файлы:**
- `crates/tc/src/commands/review.rs`

**Что сделать:**
- [x] `run(ReviewArgs)`: `git diff main...{prefix}{id}` -> открыть в $PAGER
- [x] `--reject "feedback"`: append feedback в task.notes, status -> todo, опционально respawn
- [x] `run_merge(MergeArgs)`: вызвать merge_worktree, обработать конфликт
- [x] `--all`: merge все worktree в статусе done/review

**AC:**
- `tc review T-001` -- diff в pager
- `tc review T-001 --reject "Переделай"` -- feedback в notes, status todo
- `tc merge T-001` -- merge + cleanup
- `tc merge --all` -- batch merge
- Конфликт -- понятное сообщение

**Зависит от:** M-3.3, M-3.4

---

### M-3.6: Тесты фазы 3

**Файлы:**
- `crates/tc-spawn/` (тесты во всех модулях)
- `crates/tc-executor/src/traits.rs` (MockExecutor)

**Что сделать:**
- [x] MockExecutor: реализация Executor trait для тестов (configurable exit code, delay)
- [x] Worktree integration tests с реальным git repo
- [x] Scheduler tests с mock executor
- [x] Merge tests: clean merge + conflict scenarios
- [x] CLI integration: spawn -> workers -> kill flow

**AC:**
- `cargo test -p tc-spawn` -- все проходят
- Минимум 20 новых тестов

**Зависит от:** M-3.5

---

## Фаза 4 -- TUI [DONE]

Цель: `tc` (без аргументов) запускает полноценный TUI с task list, detail panel, DAG view.

### M-4.1: App state + event loop + terminal [DONE]

**Файлы:**
- `crates/tc-tui/src/app.rs`
- `crates/tc-tui/src/event.rs`
- `crates/tc-tui/src/ui.rs`
- `crates/tc-tui/src/runtime.rs`
- `crates/tc/src/commands/tui.rs`

**Что сделать:**
- [x] App: загрузить Store, tasks, config, построить DAG. State: selected_task, selected_epic, focus_panel
- [x] Event loop: crossterm events -> Message -> App::update() -> ui::render(). Tick каждые 250ms
- [x] Message enum: Tick, Quit, Key(KeyCode, KeyModifiers)
- [x] Terminal setup/teardown: enable_raw_mode, enter_alternate_screen, restore on panic (panic hook)
- [x] runtime.rs: run() -> discover store, install panic hook, setup terminal, run_loop, teardown
- [x] tui.rs command handler: вызывает runtime::run()

**AC:**
- `tc` -- запускается TUI, показывает layout, `q` выходит
- Корректный restore терминала при panic (panic hook)
- Event loop не блокирует (async tick)

**Зависит от:** M-1.8 (нужен рабочий Storage + DAG)

---

### M-4.2: task_table + epic_list [DONE]

**Файлы:**
- `crates/tc-tui/src/components/task_table.rs`
- `crates/tc-tui/src/components/epic_list.rs`

**Что сделать:**
- [x] epic_list: список epic-ов с count, "all" вверху, highlight selected. Tab для switch focus
- [x] task_table: таблица (ID, Title, Status), фильтр по selected epic, цветные статусы
- [x] j/k навигация, Enter -> select task, Tab -> cycle focus
- [x] Worker indicator через worker_for() lookup

**AC:**
- Epic list показывает правильный count
- Task table фильтруется при выборе epic
- j/k скроллит, Enter выбирает
- Цветные статусы

**Зависит от:** M-4.1

---

### M-4.3: detail panel + dag_view [DONE]

**Файлы:**
- `crates/tc-tui/src/components/detail.rs`
- `crates/tc-tui/src/components/dag_view.rs`

**Что сделать:**
- [x] detail: полная карточка выбранного task (title, epic, status, deps, notes, files)
- [x] dag_view: мини-DAG для выбранного task (deps + dependents)
- [x] Toggle: `g` toggle DAG view (split detail panel 60/40)

**AC:**
- Detail panel показывает все поля
- DAG view показывает связи выбранного task
- Toggle работает без мерцания

**Зависит от:** M-4.2

---

### M-4.4: input + keybindings (действия) [DONE]

**Файлы:**
- `crates/tc-tui/src/components/input.rs`
- `crates/tc-tui/src/app.rs` (расширить update)
- `crates/tc-tui/src/runtime.rs` (suspend/resume для impl и review)

**Что сделать:**
- [x] Input component: текстовое поле с cursor, backspace, Enter/Esc
- [x] `a` -- add task prompt (title inline, epic = selected или "default")
- [x] `d` -- done для selected task (action_done)
- [x] `/` -- filter tasks (live filtering по id/title)
- [x] `i` -- impl: suspend TUI, запустить claude interactive, resume TUI. Обновляет статус -> review/blocked
- [x] `y`/`s` -- spawn: запускает selected task в worktree (headless yolo). Создаёт Scheduler, spawn_tasks, обновляет workers
- [x] `K` -- kill worker: SIGTERM + обновление worker state -> killed
- [x] `r` -- review diff: suspend TUI, `git diff base...branch` в $PAGER, resume TUI
- [x] `R` -- reject: InputMode::Reject prompt, feedback в notes, status -> todo
- [x] `m` -- merge: merge_worktree, status -> done. При конфликте -- abort + toast

**AC:**
- Все keybindings из PRD секции 5 работают
- Input field с cursor, backspace, Enter/Esc
- Actions обновляют state немедленно
- TuiAction enum для suspend/resume (SuspendForImpl, SuspendForReview)
- Runtime обрабатывает pending_action: teardown terminal -> action -> setup terminal

**Зависит от:** M-4.3

---

### M-4.5: log_viewer + worker status [DONE]

**Файлы:**
- `crates/tc-tui/src/components/log_viewer.rs`
- `crates/tc-tui/src/components/detail.rs`
- `crates/tc-tui/src/app.rs` (worker polling)

**Что сделать:**
- [x] log_viewer: панель с live tail лога worker-а (последние 200 строк). `l` toggle
- [x] Worker status bar: "Workers: N/M" в header (workers_summary)
- [x] Auto-refresh: poll worker state files каждый tick, обновить статусы
- [x] Live output: detail panel показывает "Worker: status (pid N)" и "Last output: ..." для задач с worker-ом
- [x] Тесты: snapshot тесты (ratatui TestBackend) -- 3 теста (basic layout, dag toggle, event translate)

**AC:**
- `l` -- показывает лог текущего worker-а, auto-scroll
- Header показывает active/max workers
- Смена статуса worker-а отражается в таблице
- Тесты: snapshot тесты для компонентов (ratatui TestBackend)

**Зависит от:** M-4.4

---

## Фаза 5 -- Testing + Polish

Цель: production quality. `tc test`, `tc edit`, `tc delete`, shell completions, полное покрытие тестами.

### M-5.1: executor/tester.rs + tc test

> Note: базовая verification loop (cargo test/clippy) уже в M-2.7.
> Этот milestone -- про отдельного тестировщика-агента с MCP (browser, playwright).

**Файлы:**
- `crates/tc-executor/src/tester.rs`
- `crates/tc/src/commands/test.rs`

**Что сделать:**
- [ ] TesterExecutor: build_command с MCP серверами (`--mcp-server {name} -- {command}`)
- [ ] System prompt из config.tester.system_prompt с template variables
- [ ] Context: task description + acceptance_criteria + impl diff (если worktree)
- [ ] tc test handler: load tester config, build context, spawn tester
- [ ] Parse результат: PASS -> done, FAIL -> block + создать fix task

**AC:**
- `tc test T-001` -- запускает тестировщика
- `tc test T-001 --browser` -- с browser MCP
- `tc test T-001 --no-mcp` -- без MCP
- PASS/FAIL правильно обрабатываются
- Тестировщик видит acceptance_criteria в контексте

**Зависит от:** M-2.5

---

### M-5.2: tc edit + tc delete

**Файлы:**
- `crates/tc/src/commands/edit.rs`
- `crates/tc/src/commands/delete.rs`

**Что сделать:**
- [x] `edit`: извлечь task как YAML fragment, открыть в $EDITOR (tempfile), parse обратно, save
- [x] `delete`: найти task, проверить что никто не зависит от него (или force), удалить, save
- [x] Delete с подтверждением (y/n prompt)

**AC:**
- `tc edit T-001` -- открывает YAML в $EDITOR, сохраняет изменения
- `tc delete T-001` -- удаляет после подтверждения
- `tc delete T-001` при наличии dependents -- ошибка (без --force)

**Зависит от:** M-1.5

---

### M-5.3: Config customization + shell completions

**Файлы:**
- `crates/tc/src/cli.rs` (shell completions)
- `crates/tc-core/src/config.rs` (validation)

**Что сделать:**
- [ ] Кастомные статусы: валидация при load config (terminal хотя бы один, id уникальны)
- [ ] Custom context template: валидация minijinja при load
- [ ] Shell completions: `tc completion bash/zsh/fish` через clap_complete
- [ ] Completion для task IDs (dynamic completions через shell script)

**AC:**
- Кастомные статусы работают во всех командах
- `tc completion bash >> ~/.bashrc` -- completions работают
- Невалидный config -- понятная ошибка при любой команде

**Зависит от:** M-1.8

---

### M-5.4: E2E тесты

**Файлы:**
- `tests/e2e/` (новая директория в workspace root)
- `tests/e2e/cli_workflow.rs` -- полный CLI workflow
- `tests/e2e/packer_workflow.rs` -- pack с реальным проектом
- `tests/e2e/helpers.rs` -- общие утилиты (tempdir, fixture creation, assert helpers)

**Что сделать:**
- [ ] Фреймворк: helper для создания tempdir с git init + `.tc/` + fixture tasks
- [ ] CLI workflow: `tc init` -> `tc add` (3 задачи с deps) -> `tc list` -> `tc list --ready` -> `tc show T-001` -> `tc done T-001` -> `tc next` -> `tc validate` -> `tc stats` -> `tc graph --dot`
- [ ] Packer workflow: создать fixture проект с .gitignore -> `tc pack` -> verify output содержит нужные файлы, не содержит excluded
- [ ] Impl dry-run: `tc add` -> `tc impl T-001 --dry-run` -> verify TASK_CONTEXT.md output
- [ ] Error paths: `tc done T-999` (not found), `tc done` на blocked task, circular deps -> `tc validate` fails
- [ ] Status transitions: todo -> in_progress -> review -> done, todo -> blocked -> todo -> done
- [ ] Concurrent safety: 2 процесса пишут в tasks.yaml одновременно (race condition check)

**AC:**
- `cargo test --test e2e` -- все проходят
- Каждый тест создаёт изолированный tempdir, не зависит от порядка запуска
- Минимум 15 e2e тестов
- Тесты проходят за < 30 секунд суммарно

**Зависит от:** M-2.7

---

### M-5.5: Full test suite + CI

**Файлы:**
- Все crate-ы (тесты)

**Что сделать:**
- [ ] tc-core: 100% unit coverage (dag, status, context, config)
- [ ] tc-storage: roundtrip, concurrent access, corrupt YAML handling
- [ ] tc-packer: integration tests с fixture проектами
- [ ] tc-executor: mock-based unit tests для всех executors
- [ ] tc-spawn: mock executor + real git в tempdir
- [ ] tc-tui: snapshot тесты (TestBackend)
- [ ] `cargo clippy -- -D warnings`, `cargo fmt --check`

**AC:**
- `cargo test` -- 100+ тестов (unit + integration + e2e), все зелёные
- `cargo clippy` -- 0 warnings
- No `todo!()` в кодовой базе

**Зависит от:** M-5.4 + все предыдущие milestone-ы

---

## Граф зависимостей

```
M-1.1 -> M-1.2 -> M-1.3 -> M-1.4 -> M-1.5 -> M-1.6 -> M-1.7 -> M-1.8
                                        |                           |
                                        v                           v
                                      M-2.7                       M-4.1 -> M-4.2 -> M-4.3 -> M-4.4 -> M-4.5
                                        ^
M-2.1 -> M-2.2 ----> M-2.4             |
M-2.3 -------------> M-2.4             |
M-2.5 -> M-2.6                         |
  |                                     |
  +---> M-2.7 <------------------------+
  |       |
  |       +---> M-3.2 (verification shared)
  |
  +---> M-3.2 -> M-3.4 -> M-3.5 -> M-3.6
  |
  +---> M-5.1
  
M-3.1 -> M-3.2
M-3.1 -> M-3.3 -> M-3.5

M-1.5 -> M-5.2
M-1.8 -> M-5.3
M-2.7 -> M-5.4 -> M-5.5

All -> M-5.5
```

### Параллелизм

Независимые ветки, которые можно делать одновременно:
- **Фаза 1** (M-1.1..M-1.8) -- последовательная цепочка
- **M-2.1..M-2.3** -- packer internals, параллельно с фазой 1
- **M-2.5, M-2.6** -- executor, параллельно с фазой 1
- **M-3.1** -- worktree, параллельно с фазой 1
- **M-5.1** -- tester, после M-2.5

---

## Текущий статус

| Crate | Types | Logic | Tests | Заметки |
|-------|-------|-------|-------|---------|
| tc-core | done | done | 58 | DAG, Status, Context, VerificationConfig -- полностью |
| tc-storage | done | done | 24 | load/save/init + roundtrip |
| tc (CLI) | done | done | 19 unit + 49 integration | Все Phase 1-3 команды + edit/delete; test -- todo!() stub |
| tc-packer | done | done | 40 | collect, format, security, tokens, pack() API |
| tc-executor | done | done | 32 | Claude, Opencode, Mock executors, sandbox, verify; tester.rs -- todo!() stub |
| tc-spawn | done | done | 31 unit + 17 integration | worktree, scheduler, merge, recovery, process -- всё реализовано |
| tc-tui | done | done | 3 | UI + все action-шорткаты подключены (impl, spawn, kill, review, reject, merge) |

### Оставшиеся todo!() в коде

| Файл | Что нужно |
|------|-----------|
| `crates/tc/src/commands/test.rs` | `tc test` -- запуск тестировщика-агента |
| `crates/tc-executor/src/tester.rs` | TesterExecutor -- build_command + execute с MCP |
