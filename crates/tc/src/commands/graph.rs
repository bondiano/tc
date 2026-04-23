use std::collections::HashMap;

use tc_core::dag::TaskDag;
use tc_core::task::Task;

use crate::cli::GraphArgs;
use crate::error::CliError;
use crate::output;

pub fn run(args: GraphArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;

    if tasks.is_empty() {
        println!("No tasks.");
        return Ok(());
    }

    let dag = TaskDag::from_tasks(&tasks)?;
    let topo = dag.topological_order()?;

    if args.dot {
        println!("{}", render_dot(&tasks, &topo));
    } else {
        println!("{}", render_ascii(&tasks, &dag, &topo));
    }

    Ok(())
}

fn render_ascii(tasks: &[Task], dag: &TaskDag, topo: &[tc_core::task::TaskId]) -> String {
    let task_map: HashMap<_, _> = tasks.iter().map(|t| (&t.id, t)).collect();
    let mut lines = Vec::new();

    for id in topo {
        let task = task_map[id];
        let status = output::colored_status(&task.status);
        let deps = dag.dependencies(id);

        if deps.is_empty() {
            lines.push(format!("[{status}] {} -- {}", id, task.title));
        } else {
            let dep_strs: Vec<String> = deps.iter().map(|d| d.0.clone()).collect();
            lines.push(format!(
                "{} --> [{status}] {} -- {}",
                dep_strs.join(", "),
                id,
                task.title
            ));
        }
    }

    lines.join("\n")
}

fn render_dot(tasks: &[Task], topo: &[tc_core::task::TaskId]) -> String {
    let task_map: HashMap<_, _> = tasks.iter().map(|t| (&t.id, t)).collect();
    let mut lines = vec!["digraph tc {".to_string(), "  rankdir=LR;".to_string()];

    for id in topo {
        let task = task_map[id];
        let color = output::status_dot_color(&task.status.0);
        let label = format!("{}\\n{}", id, task.title);
        lines.push(format!(
            "  \"{}\" [label=\"{label}\", style=filled, fillcolor={color}];",
            id
        ));
    }

    for task in tasks {
        for dep in &task.depends_on {
            lines.push(format!("  \"{}\" -> \"{}\";", dep, task.id));
        }
    }

    lines.push("}".to_string());
    lines.join("\n")
}
