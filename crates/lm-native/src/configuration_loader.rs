//! Non-blocking loading and decoding of replaceable frontend configuration.

use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader, LoadedDocument},
};
use eframe::egui;
use lm_app::{FrontendConfig, ToolConfig};

#[derive(Debug)]
pub(crate) enum LoadedConfiguration {
    Frontend(FrontendConfig),
    ExternalTools(ToolConfig),
}

#[derive(Clone, Copy)]
enum ConfigurationKind {
    Frontend,
    ExternalTools,
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
    fn rejects_ambiguous_groups() {
        let mut group = loaded(ToolConfig::default().encode().unwrap());
        group.files.push((PathBuf::from("extra"), Vec::new()));
        assert!(decode(ConfigurationKind::ExternalTools, group).is_err());
    }
}
