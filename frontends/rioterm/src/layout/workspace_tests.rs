use super::*;
use crate::{
    context::{create_mock_context, next_rich_text_id, ContextManager},
    event::VoidListener,
};
use rio_backend::{
    config::workspaces::{store, WorkspaceCommand},
    event::WindowId,
};

fn add(
    grid: &mut ContextGrid<VoidListener>,
    source: NodeId,
    down: bool,
    directory: &std::path::Path,
) -> NodeId {
    grid.current = source;
    let node = if down {
        grid.try_split_down()
    } else {
        grid.try_split_right()
    }
    .unwrap();
    let mut context = create_mock_context(
        VoidListener {},
        WindowId::from(0),
        next_rich_text_id(),
        grid.grid_dimension(),
    );
    context.initial_directory = Some(directory.into());
    grid.inner.insert(node, ContextGridItem::new(context));
    grid.current = node;
    node
}

#[test]
fn workspace_capture_save_reload_preserves_nested_branches_cwd_and_argv() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let sub = root.join("sub dir");
    std::fs::create_dir(&sub).unwrap();
    let mut manager =
        ContextManager::start_with_capacity(8, VoidListener {}, WindowId::from(0))
            .unwrap();
    let grid = manager.current_grid_mut();
    let a = grid.current;
    grid.inner.get_mut(&a).unwrap().val.initial_directory = Some(root.clone());
    let b = add(grid, a, false, &root);
    let c = add(grid, a, true, &sub);
    let d = add(grid, b, true, &root);
    // Original shell integration CWD takes priority over the startup directory
    // when an OS query isn't possible (these are headless mock contexts).
    grid.inner
        .get_mut(&b)
        .unwrap()
        .val
        .terminal
        .lock()
        .current_directory = Some(sub.clone());
    let command = WorkspaceCommand {
        program: "cargo".into(),
        args: vec!["watch".into(), "-x".into(), "test --locked".into()],
    };
    grid.inner.get_mut(&c).unwrap().val.launch_command = Some(command.clone());
    let original_routes: Vec<_> = grid
        .workspace_splits()
        .unwrap()
        .iter()
        .map(|step| grid.inner[&step.node].val.route_id)
        .collect();
    let captured = manager.capture_workspace("Nested Project").unwrap();
    assert_eq!(captured.root, root.to_str().unwrap());
    assert_eq!(captured.panels.len(), 4);
    assert_eq!(captured.panels[1].split_from, Some(0));
    assert!(!captured.panels[1].split_down);
    assert_eq!(captured.panels[2].split_from, Some(0));
    assert!(captured.panels[2].split_down);
    assert_eq!(captured.panels[2].command, Some(command));
    assert_eq!(captured.panels[1].working_dir.as_deref(), Some("sub dir"));
    assert_eq!(captured.panels[3].split_from, Some(1));
    let path = temp.path().join("config.toml");
    let saved = store::save_new(&path, captured, &[]).unwrap();
    let config: rio_backend::config::Config =
        toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(config.workspaces.as_slice(), std::slice::from_ref(&saved));
    // Replay the saved recipe into a new tree without a GPU or spawning processes.
    let mut other =
        ContextManager::start_with_capacity(8, VoidListener {}, WindowId::from(1))
            .unwrap();
    let restored = other.current_grid_mut();
    let first = restored.current;
    let mut nodes = vec![first];
    for panel in saved.panels.iter().skip(1) {
        let source = nodes[panel.split_from.unwrap()];
        nodes.push(add(restored, source, panel.split_down, &root));
    }
    let replay = restored.workspace_splits().unwrap();
    let plan: Vec<_> = replay
        .iter()
        .map(|s| (s.split_from, s.split_down))
        .collect();
    assert_eq!(
        plan,
        saved
            .panels
            .iter()
            .map(|p| (p.split_from, p.split_down))
            .collect::<Vec<_>>()
    );
    let grid = manager.current_grid();
    assert_eq!(grid.len(), 4);
    assert_eq!(grid.current, d);
    assert_eq!(
        original_routes,
        grid.workspace_splits()
            .unwrap()
            .iter()
            .map(|step| grid.inner[&step.node].val.route_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn workspace_capture_reports_unknown_directory_and_blank_name() {
    let manager =
        ContextManager::start_with_capacity(8, VoidListener {}, WindowId::from(0))
            .unwrap();
    assert!(manager.capture_workspace(" ").unwrap_err().contains("name"));
    assert!(manager
        .capture_workspace("Project")
        .unwrap_err()
        .contains("directory"));
}
