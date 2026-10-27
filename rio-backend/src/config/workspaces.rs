//! Declarative launch recipes. Deserialization never touches the filesystem or spawns.
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root: String,
    pub panels: Vec<WorkspacePanel>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct WorkspacePanel {
    pub working_dir: Option<String>,
    pub command: Option<WorkspaceCommand>,
    /// Split the previously created panel down instead of right.
    pub split_down: bool,
    /// Zero-based index of an earlier panel to split. Absent means previous.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_from: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCommand {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

pub fn deserialize_workspaces<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<Workspace>, D::Error> {
    let workspaces = Vec::<Workspace>::deserialize(d)?;
    let mut ids = HashSet::new();
    for workspace in &workspaces {
        workspace.validate().map_err(serde::de::Error::custom)?;
        if !ids.insert(&workspace.id) {
            return Err(serde::de::Error::custom(format!(
                "duplicate workspace id: {}",
                workspace.id
            )));
        }
    }
    Ok(workspaces)
}

impl Workspace {
    pub fn validate(&self) -> Result<(), String> {
        let invalid = |message: &str| format!("workspace '{}': {message}", self.id);
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
        {
            return Err(invalid(
                "id must contain only ASCII letters, digits, '-' or '_'",
            ));
        }
        if self.name.trim().is_empty() {
            return Err(invalid("name must not be empty"));
        }
        if self.root.contains('\0')
            || !(Path::new(&self.root).is_absolute()
                || self.root == "~"
                || self.root.starts_with("~/"))
        {
            return Err(invalid("root must be an absolute path or start with ~/"));
        }
        if self.panels.is_empty() {
            return Err(invalid("at least one panel is required"));
        }
        for (index, panel) in self.panels.iter().enumerate() {
            if panel.split_from.is_some_and(|target| target >= index) {
                return Err(invalid(&format!(
                    "panel {}: split-from must refer to an earlier panel",
                    index + 1
                )));
            }
            if let Some(dir) = &panel.working_dir {
                if dir.contains('\0')
                    || Path::new(dir)
                        .components()
                        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
                {
                    return Err(invalid(&format!(
                        "panel {}: working-dir must be relative without '..'",
                        index + 1
                    )));
                }
            }
            if let Some(command) = &panel.command {
                if command.program.trim().is_empty()
                    || command.program.contains('\0')
                    || command.args.iter().any(|arg| arg.contains('\0'))
                {
                    return Err(invalid(&format!("panel {}: command program must not be empty and command must not contain NUL", index + 1)));
                }
            }
        }
        Ok(())
    }

    /// Checked only on explicit open. A temporarily unmounted project is valid config.
    pub fn root_directory(&self) -> Result<PathBuf, String> {
        self.validate()?;
        let path = if self.root == "~" || self.root.starts_with("~/") {
            dirs::home_dir()
                .ok_or("cannot find home directory")?
                .join(self.root.strip_prefix("~/").unwrap_or(""))
        } else {
            PathBuf::from(&self.root)
        };
        checked_directory(&path)
    }
}

pub fn checked_directory(path: &Path) -> Result<PathBuf, String> {
    let directory = path
        .canonicalize()
        .map_err(|e| format!("directory '{}': {e}", path.display()))?;
    if !directory.is_dir() {
        return Err(format!("'{}' is not a directory", path.display()));
    }
    Ok(directory)
}

pub mod store;

#[cfg(test)]
mod tests {
    use crate::config::Config;
    fn decode(body: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(body)
    }
    const VALID: &str = r#"
[[workspaces]]
id = "rio"
name = "Rio Terminal"
root = "~/src/rio"
[[workspaces.panels]]
[[workspaces.panels]]
working-dir = "frontends"
split-down = true
command = { program = "cargo", args = ["test", "--locked"] }
"#;
    #[test]
    fn workspace_config_roundtrips_and_preserves_panel_order() {
        let config = decode(VALID).unwrap();
        let workspace = &config.workspaces[0];
        assert!(workspace.panels[0].command.is_none());
        assert_eq!(
            workspace.panels[1].command.as_ref().unwrap().args,
            ["test", "--locked"]
        );
        // Rio's full Config serialization has unrelated read-only formats
        // (e.g. margin); roundtrip the workspace section through real Config.
        #[derive(serde::Serialize)]
        struct Recipes<'a> {
            workspaces: &'a [super::Workspace],
        }
        let encoded = toml::to_string(&Recipes {
            workspaces: &config.workspaces,
        })
        .unwrap();
        let reloaded = decode(&encoded).unwrap();
        assert_eq!(config.workspaces, reloaded.workspaces);
        assert!(decode("").unwrap().workspaces.is_empty());
    }
    #[test]
    fn rejects_invalid_workspace_config() {
        for (from, to) in [
            ("id = \"rio\"", "id = \"\""),
            ("name = \"Rio Terminal\"", "name = \" \""),
            ("~/src/rio", "relative"),
            (
                "working-dir = \"frontends\"",
                "working-dir = \"../outside\"",
            ),
            ("working-dir = \"frontends\"", "working-dir = \"/absolute\""),
            ("program = \"cargo\"", "program = \"\""),
            ("split-down", "split-dwon"),
        ] {
            assert!(decode(&VALID.replace(from, to)).is_err(), "{from} => {to}");
        }
        assert!(decode(&format!("{VALID}\n{VALID}")).is_err());
        assert!(decode("[[workspaces]]\nid='a'\nname='A'\nroot='/'\npanels=[]").is_err());
        assert!(decode(
            &VALID.replace("args = [\"test\", \"--locked\"]", "args = 'test'")
        )
        .is_err());
    }
    #[test]
    fn workspace_example_parses_without_opening_projects() {
        let config =
            decode(include_str!("../../../extra/workspaces/config.toml")).unwrap();
        assert_eq!(config.workspaces.len(), 2);
        assert_eq!(config.workspaces[0].panels.len(), 3);
    }

    #[test]
    fn split_source_must_precede_its_panel() {
        let valid =
            VALID.replace("split-down = true", "split-down = true\nsplit-from = 0");
        assert_eq!(
            decode(&valid).unwrap().workspaces[0].panels[1].split_from,
            Some(0)
        );
        assert!(decode(&valid.replace("split-from = 0", "split-from = 1")).is_err());
        assert!(decode(&valid.replace("split-from = 0", "split-from = -1")).is_err());
    }

    #[test]
    fn nonexistent_directory_is_deferred_until_open() {
        let config =
            decode(&VALID.replace("~/src/rio", "/no-such-rio-workspace-directory"))
                .unwrap();
        assert!(config.workspaces[0]
            .root_directory()
            .unwrap_err()
            .contains("directory"));
    }
}
