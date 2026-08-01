use lm_app::{AppState, Command};
use lm_rom::{Mapper, Region, SupportedGame};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum BuiltInRuntime {
    #[default]
    ExpandedSettings,
    CompleteLayer3,
    Lfix3Core,
    Map16Runtime,
    Layer2Runtime,
    Sprite19Fix,
    ExpandedSharedPalettes,
}

impl BuiltInRuntime {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::ExpandedSettings => "Expanded level settings",
            Self::CompleteLayer3 => "Complete Layer 3 family (includes expanded settings)",
            Self::Lfix3Core => "Lfix3 core runtime and shared tables",
            Self::Map16Runtime => "Complete Map16 runtime and auxiliary table",
            Self::Layer2Runtime => "Layer 2 object-data runtime format $103",
            Self::Sprite19Fix => "Sprite 19 ASM fix",
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
            Self::Layer2Runtime => {
                "Migrate an authenticated format-$102 Layer 2 pointer/descriptor table and runtime hook to format $103."
            }
            Self::Sprite19Fix => {
                "Install the recovered shared helper and branch patch that make sprite $19 safe on any level."
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
    lfix3_generation: lm_profile::SmwUsV1Lfix3Generation,
    map16_generation: lm_profile::SmwUsV1Map16RuntimeGeneration,
    layer2_generation: lm_profile::SmwUsV1Layer2RuntimeGeneration,
    sprite19_fix_installed: bool,
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
        let project = app
            .project()
            .ok_or("built-in runtime installation requires an open project")?;
        let lfix3_generation =
            lm_profile::probe_smw_us_v1_lfix3_generation(project.rom.logical_bytes())
                .map_err(|error| error.to_string())?;
        let map16_generation =
            lm_profile::probe_smw_us_v1_map16_runtime_generation(project.rom.logical_bytes())
                .map_err(|error| error.to_string())?;
        let layer2_generation = lm_profile::probe_smw_us_v1_layer2_runtime_generation(&project.rom)
            .map_err(|error| error.to_string())?;
        let sprite19_fix_installed =
            lm_profile::detect_smw_us_v1_sprite19_fix(project.rom.logical_bytes())
                .map_err(|error| error.to_string())?
                == lm_profile::SmwUsV1Sprite19FixState::Installed;
        Ok(Self {
            revision: snapshot.revision,
            runtime: BuiltInRuntime::default(),
            lfix3_generation,
            map16_generation,
            layer2_generation,
            sprite19_fix_installed,
        })
    }

    pub(super) const fn is_stale(&self, project_revision: u64) -> bool {
        self.revision != project_revision
    }

    pub(super) fn selection_is_installed(&self) -> bool {
        match self.runtime {
            BuiltInRuntime::Lfix3Core => {
                self.lfix3_generation == lm_profile::SmwUsV1Lfix3Generation::Generation3Current
            }
            BuiltInRuntime::Map16Runtime => {
                self.map16_generation == lm_profile::SmwUsV1Map16RuntimeGeneration::StageFourCurrent
            }
            BuiltInRuntime::Layer2Runtime => {
                self.layer2_generation
                    == lm_profile::SmwUsV1Layer2RuntimeGeneration::Format103Current
            }
            BuiltInRuntime::Sprite19Fix => self.sprite19_fix_installed,
            _ => false,
        }
    }

    pub(super) fn selection_migrates_legacy_runtime(&self) -> bool {
        match self.runtime {
            BuiltInRuntime::Lfix3Core => matches!(
                self.lfix3_generation,
                lm_profile::SmwUsV1Lfix3Generation::Generation1
                    | lm_profile::SmwUsV1Lfix3Generation::Generation2
            ),
            BuiltInRuntime::Map16Runtime => matches!(
                self.map16_generation,
                lm_profile::SmwUsV1Map16RuntimeGeneration::StageOneLegacy
                    | lm_profile::SmwUsV1Map16RuntimeGeneration::StageTwoLegacy
                    | lm_profile::SmwUsV1Map16RuntimeGeneration::StageThreeLegacy
            ),
            BuiltInRuntime::Layer2Runtime => {
                matches!(
                    self.layer2_generation,
                    lm_profile::SmwUsV1Layer2RuntimeGeneration::Format101Legacy
                        | lm_profile::SmwUsV1Layer2RuntimeGeneration::Format102Legacy
                )
            }
            _ => false,
        }
    }

    pub(super) fn prepare(&self, project_revision: u64) -> Result<Command, String> {
        if self.is_stale(project_revision) {
            return Err("the ROM changed after the runtime installer was opened; reopen it".into());
        }
        if self.selection_is_installed() {
            return Err(
                "the selected current runtime is already installed and authenticated".into(),
            );
        }
        Ok(match self.runtime {
            BuiltInRuntime::ExpandedSettings => Command::InstallSettings { rev: self.revision },
            BuiltInRuntime::CompleteLayer3 => Command::InstallLayer3 { rev: self.revision },
            BuiltInRuntime::Lfix3Core => Command::InstallLfix3 { rev: self.revision },
            BuiltInRuntime::Map16Runtime => Command::InstallMap16Runtime { rev: self.revision },
            BuiltInRuntime::Layer2Runtime => {
                if !matches!(
                    self.layer2_generation,
                    lm_profile::SmwUsV1Layer2RuntimeGeneration::Format101Legacy
                        | lm_profile::SmwUsV1Layer2RuntimeGeneration::Format102Legacy
                ) {
                    return Err(format!(
                        "Layer 2 migration currently requires authenticated format $101 or $102; detected {:?}",
                        self.layer2_generation
                    ));
                }
                Command::InstallLayer2Runtime { rev: self.revision }
            }
            BuiltInRuntime::Sprite19Fix => Command::InstallSprite19Fix { rev: self.revision },
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
        workspace.runtime = BuiltInRuntime::Sprite19Fix;
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallSprite19Fix { rev: 0 }
        ));
        assert!(workspace.prepare(app.project_revision() + 1).is_err());

        workspace.runtime = BuiltInRuntime::Layer2Runtime;
        assert!(workspace.prepare(app.project_revision()).is_err());
        workspace.layer2_generation = lm_profile::SmwUsV1Layer2RuntimeGeneration::Format102Legacy;
        assert!(workspace.selection_migrates_legacy_runtime());
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallLayer2Runtime { rev: 0 }
        ));
        workspace.layer2_generation = lm_profile::SmwUsV1Layer2RuntimeGeneration::Format101Legacy;
        assert!(workspace.selection_migrates_legacy_runtime());
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallLayer2Runtime { rev: 0 }
        ));

        workspace.runtime = BuiltInRuntime::Lfix3Core;
        workspace.lfix3_generation = lm_profile::SmwUsV1Lfix3Generation::Generation2;
        assert!(workspace.selection_migrates_legacy_runtime());
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallLfix3 { rev: 0 }
        ));
        workspace.lfix3_generation = lm_profile::SmwUsV1Lfix3Generation::Generation1;
        assert!(workspace.selection_migrates_legacy_runtime());
        assert!(matches!(
            workspace.prepare(app.project_revision()).unwrap(),
            Command::InstallLfix3 { rev: 0 }
        ));
        workspace.runtime = BuiltInRuntime::Map16Runtime;
        for generation in [
            lm_profile::SmwUsV1Map16RuntimeGeneration::StageOneLegacy,
            lm_profile::SmwUsV1Map16RuntimeGeneration::StageTwoLegacy,
            lm_profile::SmwUsV1Map16RuntimeGeneration::StageThreeLegacy,
        ] {
            workspace.map16_generation = generation;
            assert!(workspace.selection_migrates_legacy_runtime());
            assert!(matches!(
                workspace.prepare(app.project_revision()).unwrap(),
                Command::InstallMap16Runtime { rev: 0 }
            ));
        }
    }
}
