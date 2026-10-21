use super::*;
use crate::event::VoidListener;
use rio_backend::config::workspaces::{Workspace, WorkspaceCommand, WorkspacePanel};

fn recipe(id: &str) -> Workspace {
    Workspace {
        id: id.into(),
        name: format!("Project {id}"),
        root: std::env::temp_dir().to_string_lossy().into_owned(),
        panels: vec![WorkspacePanel::default()],
    }
}

fn manager() -> ContextManager<VoidListener> {
    ContextManager::start_with_capacity(8, VoidListener {}, WindowId::from(0)).unwrap()
}

#[test]
fn workspace_repeat_focus_and_reorder_preserve_contexts() {
    let mut cm = manager();
    let a = recipe("a");
    cm.open_workspace(&a).unwrap();
    let route = cm.current_route();
    let terminal = Arc::clone(&cm.current().terminal);
    cm.open_workspace(&recipe("b")).unwrap();
    let b_route = cm.current_route();
    assert_ne!(b_route, route);
    let mut edited = a.clone();
    edited.root = "/missing-even-on-repeat-selection".into();
    let launch = cm.open_workspace(&edited).unwrap();
    assert!(launch.panels.is_empty());
    assert_eq!(cm.len(), 3);
    assert_eq!(cm.current_route(), route);
    assert!(Arc::ptr_eq(&terminal, &cm.current().terminal));
    cm.move_current_tab_to(0);
    assert_eq!(cm.workspace_index("a"), Some(0));
    cm.open_workspace(&recipe("b")).unwrap();
    assert_eq!(cm.current_route(), b_route);
    cm.open_workspace(&a).unwrap();
    assert_eq!(cm.current_route(), route);
    // Model the exact container removal used by the close/child-exit paths.
    cm.contexts.remove(0);
    cm.set_current(0);
    assert_eq!(cm.workspace_index("a"), None);
    cm.open_workspace(&a).unwrap();
    assert_ne!(cm.current_route(), route);
}

#[test]
fn workspace_bad_root_and_capacity_leave_existing_tab_untouched() {
    let mut cm = manager();
    let original = cm.current_route();
    let mut missing = recipe("missing");
    missing.root = "/rio-no-such-workspace-root".into();
    assert!(cm
        .open_workspace(&missing)
        .err()
        .unwrap()
        .contains("directory"));
    assert_eq!(cm.len(), 1);
    assert_eq!(cm.current_route(), original);
    cm.capacity = 1;
    assert!(cm
        .open_workspace(&recipe("full"))
        .err()
        .unwrap()
        .contains("capacity"));
    cm.config.is_native = true;
    assert!(cm
        .open_workspace(&recipe("native"))
        .err()
        .unwrap()
        .contains("NativeTab"));
}

#[cfg(unix)]
#[test]
fn workspace_real_ptys_survive_switching_and_report_partial_spawn_errors() {
    let mut cm = manager();
    cm.config.dead_pty = false;
    cm.config.spawn_performer = true;
    cm.config.shell_integration = false;
    let mut a = recipe("live-a");
    a.panels[0].command = Some(WorkspaceCommand {
        program: "/bin/cat".into(),
        args: vec![],
    });
    let mut bad = a.panels[0].clone();
    bad.command.as_mut().unwrap().program = "/rio-no-such-workspace-command".into();
    let mut bad_dir = a.panels[0].clone();
    bad_dir.working_dir = Some("rio-no-such-panel-directory".into());
    // Failure before and after successes must keep all successfully spawned PTYs.
    a.panels = vec![
        bad.clone(),
        a.panels[0].clone(),
        bad_dir,
        a.panels[0].clone(),
    ];
    let launch = cm.open_workspace(&a).unwrap();
    assert_eq!(launch.errors.len(), 2);
    assert!(launch.errors[0].contains("panel 1"));
    assert!(launch.errors[0].contains("rio-no-such-workspace-command"));
    assert!(launch.errors[1].contains("panel 3"));
    assert_eq!(launch.panels.len(), 1);
    let extra_pid = launch.panels[0].0.shell_pid;
    let pid = cm.current().shell_pid;
    let route = cm.current_route();
    let terminal = Arc::clone(&cm.current().terminal);
    let mut b = recipe("live-b");
    b.panels[0].command = a.panels[1].command.clone();
    cm.open_workspace(&b).unwrap();
    let b_pid = cm.current().shell_pid;
    for _ in 0..5 {
        assert!(cm.open_workspace(&a).unwrap().panels.is_empty());
        assert_eq!(cm.current().shell_pid, pid);
        assert_eq!(cm.current_route(), route);
        assert!(Arc::ptr_eq(&terminal, &cm.current().terminal));
        cm.open_workspace(&b).unwrap();
        assert_eq!(cm.current().shell_pid, b_pid);
    }
    for child in [pid, extra_pid, b_pid] {
        assert_eq!(
            unsafe { libc::kill(child as i32, 0) },
            0,
            "PTY child died on switch"
        );
    }
    let count = cm.len();
    let mut all_bad = recipe("all-bad");
    all_bad.panels = vec![bad];
    assert!(cm.open_workspace(&all_bad).is_err());
    assert_eq!(cm.len(), count);
    assert_eq!(cm.workspace_index("all-bad"), None);
    // Dropping the manager and unattached test panels invokes normal Rio cleanup.
}

#[cfg(unix)]
#[test]
fn workspace_spawn_uses_root_relative_cwd_and_exact_argv() {
    let temp =
        std::env::temp_dir().join(format!("rio workspace {}", next_rich_text_id()));
    let panel_dir = temp.join("panel with spaces");
    std::fs::create_dir_all(&panel_dir).unwrap();
    let mut workspace = recipe("argv");
    workspace.root = temp.to_string_lossy().into_owned();
    workspace.panels[0] = WorkspacePanel {
        working_dir: Some("panel with spaces".into()),
        command: Some(WorkspaceCommand {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf '%s\\n' \"$PWD\" \"$1\" > result; exec /bin/cat".into(),
                "workspace".into(),
                "one argument; $(not expanded)".into(),
            ],
        }),
        ..WorkspacePanel::default()
    };
    let mut cm = manager();
    cm.config.dead_pty = false;
    cm.config.spawn_performer = true;
    cm.open_workspace(&workspace).unwrap();
    let output = panel_dir.join("result");
    let expected = format!(
        "{}\none argument; $(not expanded)\n",
        panel_dir.canonicalize().unwrap().display()
    );
    let mut actual = String::new();
    for _ in 0..100 {
        actual = std::fs::read_to_string(&output).unwrap_or_default();
        if actual == expected {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(actual, expected);
    drop(cm);
    std::fs::remove_dir_all(temp).unwrap();
}
