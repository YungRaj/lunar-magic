use crate::{
    AppError, AppState, EditorMode, FrontendEffect, LevelNavigationDirection, LevelViewport,
    ToolEvent,
};

impl AppState {
    pub(crate) fn change_view(
        &mut self,
        mode: EditorMode,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if self.project.is_none() {
            return Err(AppError::NoProject);
        }
        if let EditorMode::Level(level) = mode {
            self.level_navigation.visit(level);
        }
        Ok(self.apply_view(mode))
    }

    pub(crate) fn navigate_level(
        &mut self,
        direction: LevelNavigationDirection,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if self.project.is_none() {
            return Err(AppError::NoProject);
        }
        let Some(view) = self.level_navigation.navigate(direction) else {
            self.status = match direction {
                LevelNavigationDirection::Back => "No earlier level".into(),
                LevelNavigationDirection::Forward => "No later level".into(),
            };
            return Ok(Vec::new());
        };
        let mut effects = self.apply_view(EditorMode::Level(view.level));
        effects.push(FrontendEffect::LevelViewportChanged(view.viewport));
        Ok(effects)
    }

    pub(crate) fn set_level_viewport(
        &mut self,
        viewport: LevelViewport,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if self.project.is_none() {
            return Err(AppError::NoProject);
        }
        if !matches!(self.mode, EditorMode::Level(_)) {
            return Err(AppError::NoLevelView);
        }
        self.level_navigation
            .set_viewport(viewport)
            .ok_or(AppError::NoLevelView)?;
        Ok(vec![FrontendEffect::LevelViewportChanged(viewport)])
    }

    fn apply_view(&mut self, mode: EditorMode) -> Vec<FrontendEffect> {
        let previous_mode = self.mode;
        self.mode = mode;
        self.selection = None;
        self.status = match mode {
            EditorMode::Level(level) => format!("Level {level:03X}"),
            EditorMode::Overworld => "Overworld".into(),
            EditorMode::Map16 => "Map16".into(),
            EditorMode::Graphics(file) => format!("Graphics {file:02X}"),
            EditorMode::Palette(palette) => format!("Palette {palette:03X}"),
            EditorMode::ExAnimation(slot) => format!("ExAnimation {slot:03X}"),
            EditorMode::Layer3(level) => format!("Layer 3 {level:03X}"),
            EditorMode::NoProject => String::new(),
        };
        let mut effects = vec![FrontendEffect::ViewChanged(mode)];
        if matches!(mode, EditorMode::Level(_)) && mode != previous_mode {
            effects.extend(self.external_tool_event_effects(ToolEvent::LevelChanged));
        }
        effects
    }
}
