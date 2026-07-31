use crate::{
    AppCapabilities, AppState, FrontendConfig, FrontendConfigError, HistoryCapabilities,
    LevelNavigationDirection, LocalizationCatalog, LocalizationError, NavigationCapabilities,
    ProfileStatus, ProjectStatus, RecentDocuments, SaveStatus, SelectionCapabilities,
    ShortcutConfig, ShortcutError, ShortcutGesture, ToolbarAction, ToolbarActivation,
    ToolbarConfig, ToolbarError,
};

impl AppState {
    /// Replaces the frontend-persisted recent-document list without touching project state.
    pub fn set_recent_documents(&mut self, recent_documents: RecentDocuments) {
        self.recent_documents = recent_documents;
    }

    /// Returns most-recent-first document paths for native menu construction.
    #[must_use]
    pub const fn recent_documents(&self) -> &RecentDocuments {
        &self.recent_documents
    }

    /// Installs localization, toolbar, and shortcuts as one validated frontend transaction.
    ///
    /// # Errors
    ///
    /// Returns [`FrontendConfigError`] without replacing any active component if validation fails.
    pub fn set_frontend_config(
        &mut self,
        config: FrontendConfig,
    ) -> Result<(), FrontendConfigError> {
        config.validate()?;
        let FrontendConfig {
            localization,
            toolbar,
            shortcuts,
        } = config;
        self.localization = Some(localization);
        self.toolbar = Some(toolbar);
        self.shortcuts = Some(shortcuts);
        Ok(())
    }

    /// Atomically replaces the complete localization catalog after validation.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] without changing the active catalog when validation fails.
    pub fn set_localization(
        &mut self,
        catalog: LocalizationCatalog,
    ) -> Result<(), LocalizationError> {
        catalog.validate()?;
        self.localization = Some(catalog);
        Ok(())
    }

    /// Returns the active typed localization catalog, if a frontend installed one.
    #[must_use]
    pub const fn localization(&self) -> Option<&LocalizationCatalog> {
        self.localization.as_ref()
    }

    /// Restores the frontend's built-in English text instead of a custom catalog.
    pub fn clear_localization(&mut self) {
        self.localization = None;
    }

    /// Atomically replaces the portable toolbar layout after structural validation.
    ///
    /// # Errors
    ///
    /// Returns [`ToolbarError`] without changing the active layout when validation fails.
    pub fn set_toolbar(&mut self, toolbar: ToolbarConfig) -> Result<(), ToolbarError> {
        toolbar.validate()?;
        self.toolbar = Some(toolbar);
        Ok(())
    }

    /// Returns the active toolbar layout, if configured.
    #[must_use]
    pub const fn toolbar(&self) -> Option<&ToolbarConfig> {
        self.toolbar.as_ref()
    }

    /// Restores the frontend's built-in toolbar instead of a configured portable layout.
    pub fn clear_toolbar(&mut self) {
        self.toolbar = None;
    }

    /// Atomically replaces portable keyboard bindings after complete validation.
    ///
    /// # Errors
    ///
    /// Returns [`ShortcutError`] without changing active bindings when validation fails.
    pub fn set_shortcuts(&mut self, shortcuts: ShortcutConfig) -> Result<(), ShortcutError> {
        shortcuts.validate()?;
        self.shortcuts = Some(shortcuts);
        Ok(())
    }

    /// Returns the active portable keyboard configuration, if installed.
    #[must_use]
    pub const fn shortcuts(&self) -> Option<&ShortcutConfig> {
        self.shortcuts.as_ref()
    }

    /// Resolves one frontend-normalized keyboard gesture to a typed shell action.
    #[must_use]
    pub fn shortcut_action(&self, gesture: ShortcutGesture) -> Option<ToolbarAction> {
        self.shortcuts
            .as_ref()
            .and_then(|shortcuts| shortcuts.action_for(gesture))
    }

    /// Resolves an enabled toolbar or shortcut action into shared frontend work.
    ///
    /// `None` means the action is currently disabled. Clipboard actions return typed requests
    /// because only the active editor/frontend owns selection serialization or platform clipboard
    /// access; all parameterless operations return the exact application [`crate::Command`].
    #[must_use]
    pub fn activate_toolbar_action(&self, action: ToolbarAction) -> Option<ToolbarActivation> {
        self.toolbar_action_enabled(action)
            .then(|| action.activation())
    }

    /// Returns authoritative action availability for toolbars, menus, and keyboard shortcuts.
    #[must_use]
    pub fn toolbar_action_enabled(&self, action: ToolbarAction) -> bool {
        let capabilities = self.capabilities();
        match action {
            ToolbarAction::Open => self.pending_save.is_none() && self.pending_open.is_none(),
            ToolbarAction::Save => capabilities.can_save(),
            ToolbarAction::SaveAs => {
                self.project.is_some() && self.pending_save.is_none() && self.pending_open.is_none()
            }
            ToolbarAction::Undo => capabilities.history.undo,
            ToolbarAction::Redo => capabilities.history.redo,
            ToolbarAction::LevelBack => capabilities.navigation.level_back,
            ToolbarAction::LevelForward => capabilities.navigation.level_forward,
            ToolbarAction::Copy => capabilities.selection.copy,
            ToolbarAction::Cut => capabilities.selection.cut,
            ToolbarAction::Paste | ToolbarAction::ShowOverworld | ToolbarAction::ShowMap16 => {
                self.project.is_some()
            }
        }
    }

    /// Returns frontend command availability without exposing toolkit-specific state.
    #[must_use]
    pub fn capabilities(&self) -> AppCapabilities {
        let Some(project) = self.project.as_ref() else {
            return AppCapabilities::default();
        };
        AppCapabilities {
            project: if project.is_modified() {
                ProjectStatus::OpenModified
            } else {
                ProjectStatus::OpenClean
            },
            profile: if self.revision_profile.is_some() {
                ProfileStatus::Loaded
            } else {
                ProfileStatus::Missing
            },
            history: HistoryCapabilities {
                undo: project.history.can_undo(),
                redo: project.history.can_redo(),
            },
            navigation: NavigationCapabilities {
                level_back: self
                    .level_navigation
                    .can_navigate(LevelNavigationDirection::Back),
                level_forward: self
                    .level_navigation
                    .can_navigate(LevelNavigationDirection::Forward),
            },
            save: if self.pending_save.is_some() {
                SaveStatus::Pending
            } else {
                SaveStatus::Idle
            },
            selection: SelectionCapabilities {
                copy: self.selection.is_some(),
                cut: self.selection.is_some(),
            },
        }
    }
}
