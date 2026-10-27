//! Append a recipe without reserializing the user's configuration.
use super::Workspace;
use crate::config::Config;
use serde::Serialize;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

/// Always creates a new ID; never overwrites an existing recipe. The caller
/// updates tab identity only after this durable write succeeds.
pub fn save_new(
    path: &Path,
    mut workspace: Workspace,
    reserved_ids: &[String],
) -> Result<Workspace, String> {
    save(path, &mut workspace, reserved_ids)
        .map_err(|e| format!("Cannot save workspace to '{}': {e}", path.display()))?;
    Ok(workspace)
}

fn read_existing(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        result => result,
    }
}

fn save(
    path: &Path,
    workspace: &mut Workspace,
    reserved_ids: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    // Preserve symlink-based config setups, including relative targets.
    let target = if fs::symlink_metadata(path).is_ok() {
        path.canonicalize()?
    } else {
        path.to_path_buf()
    };
    let parent = target.parent().ok_or("configuration path has no parent")?;
    fs::create_dir_all(parent)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(target.with_extension("workspace-lock"))?;
    lock.try_lock()
        .map_err(|e| format!("another save is in progress or locking failed: {e}"))?;
    let original = read_existing(&target)?;
    let config: Config = toml::from_str(&original)
        .map_err(|e| format!("fix the existing configuration first: {e}"))?;
    let slug: String = workspace
        .name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let stem = if slug.is_empty() { "workspace" } else { slug };
    let mut id = stem.to_string();
    let mut suffix = 2;
    while config.workspaces.iter().any(|w| w.id == id) || reserved_ids.contains(&id) {
        id = format!("{stem}-{suffix}");
        suffix += 1;
    }
    workspace.id = id;
    workspace.name = workspace.name.trim().into();
    workspace.validate()?;
    #[derive(Serialize)]
    struct Recipe<'a> {
        workspaces: &'a [Workspace],
    }
    let addition = toml::to_string(&Recipe {
        workspaces: std::slice::from_ref(workspace),
    })?;
    let mut document: toml_edit::DocumentMut = original.parse()?;
    let added: toml_edit::DocumentMut = addition.parse()?;
    let mut table = added["workspaces"]
        .as_array_of_tables()
        .ok_or("invalid workspace serialization")?
        .iter()
        .next()
        .ok_or("empty workspace serialization")?
        .clone();
    // Positions belong to the tiny serialized document, not the user's file.
    // Clear them recursively or nested panel headers can move under another recipe.
    fn clear_positions(table: &mut toml_edit::Table) {
        table.set_position(None);
        for (_, item) in table.iter_mut() {
            if let Some(nested) = item.as_table_mut() {
                clear_positions(nested);
            }
            if let Some(array) = item.as_array_of_tables_mut() {
                for nested in array.iter_mut() {
                    clear_positions(nested);
                }
            }
        }
    }
    clear_positions(&mut table);
    if let Some(tables) = document
        .get_mut("workspaces")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
    {
        tables.push(table);
    } else {
        let mut tables = toml_edit::ArrayOfTables::new();
        if let Some(existing) = document
            .get("workspaces")
            .and_then(toml_edit::Item::as_array)
        {
            for item in existing {
                tables.push(
                    item.as_inline_table()
                        .ok_or("invalid workspace array")?
                        .clone()
                        .into_table(),
                );
            }
        }
        tables.push(table);
        document["workspaces"] = toml_edit::Item::ArrayOfTables(tables);
    }
    let updated = document.to_string();
    let _: Config = toml::from_str(&updated)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = fs::metadata(&target) {
        if metadata.permissions().readonly() {
            return Err("configuration is read-only".into());
        }
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.write_all(updated.as_bytes())?;
    temporary.as_file().sync_all()?;
    // Editors don't necessarily take our advisory lock. Avoid overwriting edits
    // made during serialization; an editor racing the rename still needs care.
    if read_existing(&target)? != original {
        return Err("configuration changed during save; retry".into());
    }
    temporary.persist(&target).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::workspaces::WorkspacePanel;
    fn recipe(name: &str) -> Workspace {
        Workspace {
            id: "draft".into(),
            name: name.into(),
            root: "/".into(),
            panels: vec![WorkspacePanel::default()],
        }
    }
    #[test]
    fn save_preserves_comments_settings_and_allocates_new_ids() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let original = "# keep this comment\n[window]\nwidth = 1234 # and this one\n";
        fs::write(&path, original).unwrap();
        let first =
            save_new(&path, recipe("My Project"), &["my-project".into()]).unwrap();
        let second = save_new(&path, recipe("My Project"), &[]).unwrap();
        assert_eq!(first.id, "my-project-2");
        assert_eq!(second.id, "my-project");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with(original));
        let loaded: Config = toml::from_str(&text).unwrap();
        assert_eq!(loaded.window.width, 1234);
        assert_eq!(loaded.workspaces, vec![first, second]);
    }
    #[test]
    fn invalid_config_or_name_does_not_change_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        for text in ["broken = [", "# valid\n"] {
            fs::write(&path, text).unwrap();
            let name = if text == "# valid\n" { "  " } else { "Project" };
            assert!(save_new(&path, recipe(name), &[]).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    }
    #[test]
    fn inline_array_is_supported() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            "# user settings\nworkspaces = []\n[window]\nwidth = 987 # keep\n",
        )
        .unwrap();
        save_new(&path, recipe("New"), &[]).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# keep"));
        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.window.width, 987);
        assert_eq!(config.workspaces.len(), 1);
    }

    #[test]
    fn new_file_unicode_name_and_competing_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/config.toml");
        let saved = save_new(&path, recipe("Проект"), &[]).unwrap();
        assert_eq!(saved.id, "workspace");
        assert_eq!(saved.name, "Проект");
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.with_extension("workspace-lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let before = fs::read_to_string(&path).unwrap();
        assert!(save_new(&path, recipe("Other"), &[]).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }
    #[cfg(unix)]
    #[test]
    fn save_preserves_symlinks_and_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("actual.toml");
        let link = directory.path().join("config.toml");
        fs::write(&target, "# settings\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        symlink("actual.toml", &link).unwrap();
        save_new(&link, recipe("Saved"), &[]).unwrap();
        assert!(link.is_symlink());
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        fs::set_permissions(&target, fs::Permissions::from_mode(0o440)).unwrap();
        let before = fs::read_to_string(&target).unwrap();
        assert!(save_new(&link, recipe("Read Only"), &[]).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), before);
    }
}
