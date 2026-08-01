use lm_app::{AppState, Command};
use lm_rom::{Mapper, Region, SupportedGame};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum BuiltInRuntime {
    #[default]
    ExpandedSettings,
    CompleteLayer3,
    Lfix3Core,
    Map16Runtime,
    ExpandedSharedPalettes,
}

impl BuiltInRuntime {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::ExpandedSettings => "Expanded level settings",
            Self::CompleteLayer3 => "Complete Layer 3 family (includes expanded settings)",
            Self::Lfix3Core => "Lfix3 core runtime and shared tables",
            Self::Map16Runtime => "Complete Map16 runtime and auxiliary table",
            Self::ExpandedSharedPalettes => "Expanded shared/custom palettes",
        }
    }

    pub(super) const fn description(self) -> &'static str {
        match self {
            Self::ExpandedSettings => {
                "Install the recovered 512-record settings table and its exact runtime hooks."
            }
            Self::CompleteLayer3 => {
                "Install all recovered Layer 3 runtime allocations, hooks, compatibility code, \
                 and expanded settings as one transaction."
            }
            Self::Lfix3Core => {
                "Install the recovered Lfix3 runtime, three initialized 512-entry tables, and all \
                 fixed entry hooks."
            }
            Self::Map16Runtime => {
                "Install the recovered fixed Map16 hooks and the relocated 32-KiB auxiliary table."
            }
            Self::ExpandedSharedPalettes => {
                "Install the recovered shared-palette hooks, helpers, expanded table, and the \
                 512-entry per-level custom-palette pointer table."
            }
        }
    }
}

pub(super) struct BuiltInRuntimeWorkspace {
    revision: u64,
    pub runtime: BuiltInRuntime,
}

impl BuiltInRuntimeWorkspace {
    pub(super) fn load(app: &AppState) -> Result<Self, String> {
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        let identity = snapshot.identity;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(
                "built-in runtime installation requires SMW US revision 0 with LoROM mapping"
                    .to_owned(),
            );
        }
        Ok(Self {
            revision: snapshot.revision,
            runtime: BuiltInRuntime::default(),
        })
    }

    pub(super) const fn is_stale(&self, project_revision: u64) -> bool {
        self.revision != project_revision
    }

    pub(super) fn prepare(&self, project_revision: u64) -> Result<Command, String> {
        if self.is_stale(project_revision) {
            return Err("the ROM changed after the runtime installer was opened; reopen it".into());
        }
        Ok(match self.runtime {
            BuiltInRuntime::ExpandedSettings => Command::InstallSettings { rev: self.revision },
            BuiltInRuntime::CompleteLayer3 => Command::InstallLayer3 { rev: self.revision },
            BuiltInRuntime::Lfix3Core => Command::InstallLfix3 { rev: self.revision },
            BuiltInRuntime::Map16Runtime => Command::InstallMap16Runtime { rev: self.revision },
            BuiltInRuntime::ExpandedSharedPalettes => {
                Command::InstallExpandedSharedPalettes { rev: self.revision }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pristine_app() -> AppState {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

    #[test]
    fn commands_are_bound_to_the_open_revision_and_selected_family() {
        let app = pristine_app();
        let mut workspace = BuiltInRuntimeWorkspace::load(&app).unwrap();
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallSettings { rev: 0 }
        ));
        workspace.runtime = BuiltInRuntime::CompleteLayer3;
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallLayer3 { rev: 0 }
        ));
        workspace.runtime = BuiltInRuntime::ExpandedSharedPalettes;
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallExpandedSharedPalettes { rev: 0 }
        ));
        workspace.runtime = BuiltInRuntime::Lfix3Core;
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallLfix3 { rev: 0 }
        ));
        workspace.runtime = BuiltInRuntime::Map16Runtime;
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallMap16Runtime { rev: 0 }
        ));
        assert!(workspace.prepare(app.project_revision() + 1).is_err());
    }
}
