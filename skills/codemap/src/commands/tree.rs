use crate::cli::TreeArgs;
use crate::output::print_json;
use crate::walk::default_walker;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct Node {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<Node>,
    is_dir: bool,
}

pub fn run(args: TreeArgs) -> anyhow::Result<()> {
    let root = args.path.canonicalize().unwrap_or(args.path.clone());
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in default_walker(&root).build() {
        let entry = entry?;
        let path = entry.into_path();
        if path == root {
            continue;
        }
        paths.push(path);
    }
    paths.sort();

    if args.json {
        let tree = build_tree(&root, &paths);
        print_json(&tree)?;
    } else {
        print_human(&root, &paths);
    }
    Ok(())
}

fn build_tree(root: &Path, paths: &[PathBuf]) -> Node {
    let mut root_node = Node {
        name: root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into()),
        children: Vec::new(),
        is_dir: true,
    };
    for path in paths {
        let rel = path.strip_prefix(root).unwrap();
        let parts: Vec<&str> = rel.iter().filter_map(|c| c.to_str()).collect();
        insert(&mut root_node, &parts, path.is_dir());
    }
    root_node
}

fn insert(node: &mut Node, parts: &[&str], is_dir: bool) {
    if parts.is_empty() {
        return;
    }
    let head = parts[0];
    let existing = node.children.iter_mut().position(|c| c.name == head);
    let idx = match existing {
        Some(i) => i,
        None => {
            node.children.push(Node {
                name: head.to_string(),
                children: Vec::new(),
                is_dir: parts.len() > 1 || is_dir,
            });
            node.children.len() - 1
        }
    };
    insert(&mut node.children[idx], &parts[1..], is_dir);
}

fn print_human(root: &Path, paths: &[PathBuf]) {
    let display_root = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".into());
    println!("{}", display_root);
    let mut last_parts: Vec<String> = Vec::new();
    for path in paths {
        let rel = path.strip_prefix(root).unwrap();
        let parts: Vec<String> = rel
            .iter()
            .map(|c| c.to_string_lossy().into_owned())
            .collect();
        for (i, part) in parts.iter().enumerate() {
            if last_parts.get(i) == Some(part) {
                continue;
            }
            let indent = "  ".repeat(i + 1);
            println!("{indent}{part}");
        }
        last_parts = parts;
    }
}
