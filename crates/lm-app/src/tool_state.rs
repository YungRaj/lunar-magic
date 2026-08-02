use crate::{
    AppError, AppState, EditorMode, EmulatorTestRequest, ExternalTool, ExternalToolError,
    FrontendEffect, ToolContext, ToolEvent, validate_tools,
};

impl AppState {
    /// Returns the ordinary project context available to configured external tools.
    #[must_use]
    pub fn tool_context(&self) -> ToolContext<'_> {
        ToolContext {
            rom: self.document_path.as_deref(),
            level: match self.mode {
                EditorMode::Level(level) => Some(level),
                _ => None,
            },
            graphics: None,
        }
    }

    /// Replaces the external-tool collection after validating all stable identifiers.
    ///
    /// # Errors
    ///
    /// Returns [`ExternalToolError`] for malformed entries or duplicate identifiers. Existing
    /// configuration is preserved on failure.
    pub fn set_external_tools(
        &mut self,
        external_tools: Vec<ExternalTool>,
    ) -> Result<(), ExternalToolError> {
        validate_tools(&external_tools)?;
        self.external_tools = external_tools;
        Ok(())
    }

    /// Returns the currently configured external tools in frontend display order.
    #[must_use]
    pub fn external_tools(&self) -> &[ExternalTool] {
        &self.external_tools
    }

    /// Expands every tool subscribed to `event` without launching any process.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if a subscribed tool has an invalid template for current state.
    pub fn external_tool_event(&self, event: ToolEvent) -> Result<Vec<FrontendEffect>, AppError> {
        self.external_tools
            .iter()
            .filter(|tool| tool.subscriptions.contains(&event))
            .map(|tool| {
                tool.expand(self.tool_context())
                    .map(FrontendEffect::LaunchExternalTool)
                    .map_err(AppError::from)
            })
            .collect()
    }

    pub(crate) fn external_tool_event_effects(&self, event: ToolEvent) -> Vec<FrontendEffect> {
        self.external_tools
            .iter()
            .filter(|tool| tool.subscriptions.contains(&event))
            .map(|tool| match tool.expand(self.tool_context()) {
                Ok(invocation) => FrontendEffect::LaunchExternalTool(invocation),
                Err(error) => FrontendEffect::ExternalToolFailed {
                    tool_id: tool.id.clone(),
                    error,
                },
            })
            .collect()
    }

    pub(crate) fn run_external_tool(&self, id: &str) -> Result<Vec<FrontendEffect>, AppError> {
        let tool = self
            .external_tools
            .iter()
            .find(|tool| tool.id == id)
            .ok_or_else(|| ExternalToolError::UnknownTool(id.into()))?;
        Ok(vec![FrontendEffect::LaunchExternalTool(
            tool.expand(self.tool_context())?,
        )])
    }

    pub(crate) fn test_rom_in_emulator(&self, id: &str) -> Result<Vec<FrontendEffect>, AppError> {
        let EditorMode::Level(level) = self.mode else {
            return Err(AppError::NoLevelView);
        };
        let tool = self
            .external_tools
            .iter()
            .find(|tool| tool.id == id)
            .ok_or_else(|| ExternalToolError::UnknownTool(id.into()))?;
        if !tool.uses_argument_placeholder("rom") {
            return Err(ExternalToolError::EmulatorRequiresRomArgument.into());
        }
        let snapshot = self.controller_snapshot()?;
        Ok(vec![FrontendEffect::StageEmulatorTest(
            EmulatorTestRequest {
                tool: tool.clone(),
                revision: snapshot.revision,
                level,
                rom_bytes: snapshot.rom_bytes,
            },
        )])
    }
}
