//! Non-blocking loading and decoding of replaceable frontend configuration.

use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader, LoadedDocument},
};
use eframe::egui;
use lm_app::{
    FrontendConfig, LocalizationCatalog, OriginalLanguageModuleMetadata, ToolConfig,
    decode_original_language_module_catalog,
};
use std::path::{Path, PathBuf};

const MAX_INSTALLED_LOCALIZATIONS: usize = 64;
const MAX_ORIGINAL_LANGUAGE_MODULE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstalledLocalization {
    pub(crate) locale: String,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstalledOriginalLocalization {
    pub(crate) metadata: OriginalLanguageModuleMetadata,
    pub(crate) catalog: LocalizationCatalog,
    pub(crate) path: PathBuf,
}

#[derive(Debug)]
pub(crate) enum LoadedConfiguration {
    Frontend(FrontendConfig),
    ExternalTools(ToolConfig),
    Localization(LocalizationCatalog),
}

#[derive(Clone, Copy)]
enum ConfigurationKind {
    Frontend,
    ExternalTools,
    Localization,
}

#[derive(Default)]
pub(crate) struct ConfigurationLoader {
    loader: DocumentLoader,
    kind: Option<ConfigurationKind>,
}

impl ConfigurationLoader {
    pub(crate) fn is_running(&self) -> bool {
        self.loader.is_running()
    }

    pub(crate) fn choose_frontend_and_start(&mut self) -> Result<bool, String> {
        let Some(path) = dialogs::choose_frontend_config() else {
            return Ok(false);
        };
        self.start(
            ConfigurationKind::Frontend,
            BoundedRead::new(
                path,
                FrontendConfig::MAX_ENCODED_LEN as u64,
                "frontend configuration",
            ),
        )?;
        Ok(true)
    }

    pub(crate) fn choose_external_tools_and_start(&mut self) -> Result<bool, String> {
        let Some(path) = dialogs::choose_tool_config() else {
            return Ok(false);
        };
        self.start(
            ConfigurationKind::ExternalTools,
            BoundedRead::new(
                path,
                ToolConfig::MAX_ENCODED_LEN as u64,
                "external-tool configuration",
            ),
        )?;
        Ok(true)
    }

    pub(crate) fn choose_localization_and_start(&mut self) -> Result<bool, String> {
        let Some(path) = dialogs::choose_localization_catalog() else {
            return Ok(false);
        };
        self.start(
            ConfigurationKind::Localization,
            BoundedRead::new(
                path,
                LocalizationCatalog::MAX_ENCODED_LEN as u64,
                "language catalog",
            ),
        )?;
        Ok(true)
    }

    pub(crate) fn start_localization_path(&mut self, path: PathBuf) -> Result<(), String> {
        self.start(
            ConfigurationKind::Localization,
            BoundedRead::new(
                path,
                LocalizationCatalog::MAX_ENCODED_LEN as u64,
                "installed language catalog",
            ),
        )
    }

    pub(crate) fn discover_installed_localizations(
        executable_directory: &Path,
    ) -> Result<Vec<InstalledLocalization>, String> {
        let directory = executable_directory.join("sysLMLanguage");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(format!(
                    "cannot enumerate installed language directory {}: {error}",
                    directory.display()
                ));
            }
        };
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "cannot enumerate installed language directory {}: {error}",
                    directory.display()
                )
            })?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("cannot inspect installed language entry: {error}"))?;
            if !file_type.is_file()
                || !entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lmlang"))
            {
                continue;
            }
            paths.push(entry.path());
            if paths.len() > MAX_INSTALLED_LOCALIZATIONS {
                return Err(format!(
                    "installed language directory exceeds {MAX_INSTALLED_LOCALIZATIONS} catalogs"
                ));
            }
        }
        paths.sort_by(|left, right| {
            left.file_name()
                .cmp(&right.file_name())
                .then_with(|| left.cmp(right))
        });
        let mut installed = Vec::with_capacity(paths.len());
        for path in paths {
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            let maximum = LocalizationCatalog::MAX_ENCODED_LEN as u64;
            if metadata.len() > maximum {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(catalog) = LocalizationCatalog::decode(&bytes) else {
                continue;
            };
            installed.push(InstalledLocalization {
                locale: catalog.locale().to_owned(),
                path,
            });
        }
        installed.sort_by(|left, right| {
            left.locale
                .to_ascii_lowercase()
                .cmp(&right.locale.to_ascii_lowercase())
                .then_with(|| left.locale.cmp(&right.locale))
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(installed)
    }

    pub(crate) fn discover_installed_original_localizations(
        executable_directory: &Path,
    ) -> Result<Vec<InstalledOriginalLocalization>, String> {
        discover_installed_original_localizations_with(executable_directory, |bytes| {
            decode_original_language_module_catalog(bytes).ok()
        })
    }

    fn start(&mut self, kind: ConfigurationKind, request: BoundedRead) -> Result<(), String> {
        self.loader.start(vec![request])?;
        self.kind = Some(kind);
        Ok(())
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
    ) -> Option<Result<LoadedConfiguration, String>> {
        self.loader.show(context).map(|result| {
            let kind = self
                .kind
                .take()
                .ok_or_else(|| "configuration loader lost the requested format".to_owned())?;
            decode(kind, result?)
        })
    }
}

fn discover_installed_original_localizations_with(
    executable_directory: &Path,
    decode: impl Fn(&[u8]) -> Option<(OriginalLanguageModuleMetadata, LocalizationCatalog)>,
) -> Result<Vec<InstalledOriginalLocalization>, String> {
    let directory = executable_directory.join("sysLMLanguage");
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "cannot enumerate installed language directory {}: {error}",
                directory.display()
            ));
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "cannot enumerate installed language directory {}: {error}",
                directory.display()
            )
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect installed language entry: {error}"))?;
        if !file_type.is_file()
            || !entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            continue;
        }
        paths.push(entry.path());
        if paths.len() > MAX_INSTALLED_LOCALIZATIONS {
            return Err(format!(
                "installed language directory exceeds {MAX_INSTALLED_LOCALIZATIONS} original modules"
            ));
        }
    }
    paths.sort_by(|left, right| {
        left.file_name()
            .cmp(&right.file_name())
            .then_with(|| left.cmp(right))
    });
    let mut installed = Vec::with_capacity(paths.len());
    for path in paths {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_ORIGINAL_LANGUAGE_MODULE_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Some((metadata, catalog)) = decode(&bytes) else {
            continue;
        };
        installed.push(InstalledOriginalLocalization {
            metadata,
            catalog,
            path,
        });
    }
    installed.sort_by(|left, right| {
        left.metadata
            .display_name
            .to_ascii_lowercase()
            .cmp(&right.metadata.display_name.to_ascii_lowercase())
            .then_with(|| left.metadata.display_name.cmp(&right.metadata.display_name))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(installed)
}

fn decode(kind: ConfigurationKind, loaded: LoadedDocument) -> Result<LoadedConfiguration, String> {
    let mut files = loaded.files.into_iter();
    let (_, bytes) = files
        .next()
        .ok_or_else(|| "configuration loader returned no file".to_owned())?;
    if files.next().is_some() {
        return Err("configuration loader returned more than one file".into());
    }
    match kind {
        ConfigurationKind::Frontend => FrontendConfig::decode(&bytes)
            .map(LoadedConfiguration::Frontend)
            .map_err(|error| error.to_string()),
        ConfigurationKind::ExternalTools => ToolConfig::decode(&bytes)
            .map(LoadedConfiguration::ExternalTools)
            .map_err(|error| error.to_string()),
        ConfigurationKind::Localization => LocalizationCatalog::decode(&bytes)
            .map(LoadedConfiguration::Localization)
            .map_err(|error| error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn loaded(bytes: Vec<u8>) -> LoadedDocument {
        LoadedDocument {
            files: vec![(PathBuf::from("configuration"), bytes)],
        }
    }

    #[test]
    fn decodes_external_tool_configuration() {
        let bytes = ToolConfig::default().encode().unwrap();
        let result = decode(ConfigurationKind::ExternalTools, loaded(bytes)).unwrap();
        assert!(
            matches!(result, LoadedConfiguration::ExternalTools(config) if config.tools.is_empty())
        );
    }

    #[test]
    fn rejects_malformed_configuration_without_a_replacement() {
        assert!(decode(ConfigurationKind::Frontend, loaded(vec![0; 8])).is_err());
    }

    #[test]
    fn decodes_standalone_localization_catalog() {
        let catalog = LocalizationCatalog::new(
            "test",
            lm_app::UiTextKey::ALL.map(|key| (key, format!("{key:?}"))),
        )
        .unwrap();
        let result = decode(
            ConfigurationKind::Localization,
            loaded(catalog.encode().unwrap()),
        )
        .unwrap();
        assert!(matches!(result, LoadedConfiguration::Localization(decoded) if decoded == catalog));
    }

    #[test]
    fn rejects_ambiguous_groups() {
        let mut group = loaded(ToolConfig::default().encode().unwrap());
        group.files.push((PathBuf::from("extra"), Vec::new()));
        assert!(decode(ConfigurationKind::ExternalTools, group).is_err());
    }

    fn catalog(locale: &str) -> LocalizationCatalog {
        LocalizationCatalog::new(
            locale,
            lm_app::UiTextKey::ALL.map(|key| (key, format!("{locale}-{key:?}"))),
        )
        .unwrap()
    }

    #[test]
    fn installed_catalog_discovery_is_bounded_filtered_and_locale_sorted() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("sysLMLanguage");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("zeta.LMLANG"),
            catalog("zh-Hant").encode().unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("alpha.lmlang"),
            catalog("de-DE").encode().unwrap(),
        )
        .unwrap();
        std::fs::write(directory.join("ignored.txt"), b"not a catalog").unwrap();
        std::fs::create_dir(directory.join("directory.lmlang")).unwrap();

        let installed = ConfigurationLoader::discover_installed_localizations(root.path()).unwrap();
        assert_eq!(
            installed
                .iter()
                .map(|catalog| catalog.locale.as_str())
                .collect::<Vec<_>>(),
            ["de-DE", "zh-Hant"]
        );
        assert_eq!(
            installed[0].path.file_name().unwrap(),
            std::ffi::OsStr::new("alpha.lmlang")
        );
    }

    #[test]
    fn installed_catalog_discovery_skips_invalid_and_retains_duplicate_locales() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("sysLMLanguage");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("bad.lmlang"), b"not a catalog").unwrap();
        assert!(
            ConfigurationLoader::discover_installed_localizations(root.path())
                .unwrap()
                .is_empty()
        );

        std::fs::remove_file(directory.join("bad.lmlang")).unwrap();
        let encoded = catalog("fr-FR").encode().unwrap();
        std::fs::write(directory.join("one.lmlang"), &encoded).unwrap();
        std::fs::write(directory.join("two.lmlang"), &encoded).unwrap();
        let installed = ConfigurationLoader::discover_installed_localizations(root.path()).unwrap();
        assert_eq!(installed.len(), 2);
        assert!(installed.iter().all(|catalog| catalog.locale == "fr-FR"));
        assert!(
            ConfigurationLoader::discover_installed_localizations(&root.path().join("absent"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn original_module_discovery_is_filtered_bounded_and_metadata_sorted() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("sysLMLanguage");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("zeta.DLL"), b"zeta").unwrap();
        std::fs::write(directory.join("alpha.dll"), b"alpha").unwrap();
        std::fs::write(directory.join("invalid.dll"), b"invalid").unwrap();
        std::fs::write(directory.join("ignored.txt"), b"alpha").unwrap();
        std::fs::create_dir(directory.join("directory.dll")).unwrap();
        std::fs::File::create(directory.join("oversized.dll"))
            .unwrap()
            .set_len(MAX_ORIGINAL_LANGUAGE_MODULE_BYTES + 1)
            .unwrap();
        let installed = discover_installed_original_localizations_with(root.path(), |bytes| {
            let name = match bytes {
                b"alpha" => "Deutsch",
                b"zeta" => "Français",
                _ => return None,
            };
            let metadata = OriginalLanguageModuleMetadata {
                display_name: name.into(),
                version: "3.63".into(),
                locale: if bytes == b"alpha" { "de-DE" } else { "fr-FR" }.into(),
                code_page: "1252".into(),
            };
            let catalog = catalog(&metadata.locale);
            Some((metadata, catalog))
        })
        .unwrap();
        assert_eq!(installed.len(), 2);
        assert_eq!(installed[0].metadata.display_name, "Deutsch");
        assert_eq!(installed[1].metadata.display_name, "Français");
        assert_eq!(installed[0].catalog.locale(), "de-DE");
        assert_eq!(
            installed[0].path.file_name().unwrap(),
            std::ffi::OsStr::new("alpha.dll")
        );
    }

    #[test]
    fn original_module_discovery_rejects_more_than_sixty_four_candidates() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("sysLMLanguage");
        std::fs::create_dir(&directory).unwrap();
        for index in 0..=MAX_INSTALLED_LOCALIZATIONS {
            std::fs::write(
                directory.join(format!("language-{index:02}.dll")),
                b"module",
            )
            .unwrap();
        }
        assert!(
            discover_installed_original_localizations_with(root.path(), |_| None)
                .unwrap_err()
                .contains("exceeds 64 original modules")
        );
    }
}
