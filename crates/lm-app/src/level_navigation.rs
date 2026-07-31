use std::collections::VecDeque;

use lm_render::{Point, Viewport};

const DEFAULT_HISTORY_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelNavigationDirection {
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelViewport {
    pub origin: Point,
    zoom_numerator: u32,
    zoom_denominator: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelViewportError {
    ZeroZoom,
    ZoomBelowMinimum,
    ZoomAboveMaximum,
}

impl std::fmt::Display for LevelViewportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid level viewport: {self:?}")
    }
}

impl std::error::Error for LevelViewportError {}

impl Default for LevelViewport {
    fn default() -> Self {
        Self {
            origin: Point::default(),
            zoom_numerator: 1,
            zoom_denominator: 1,
        }
    }
}

impl LevelViewport {
    /// Constructs portable level-camera state using the renderer's exact rational zoom rules.
    ///
    /// # Errors
    ///
    /// Returns [`LevelViewportError`] unless zoom is within Lunar Magic's recovered inclusive
    /// 100–5000 percent range.
    pub fn new(
        origin: Point,
        zoom_numerator: u32,
        zoom_denominator: u32,
    ) -> Result<Self, LevelViewportError> {
        if zoom_numerator == 0 || zoom_denominator == 0 {
            return Err(LevelViewportError::ZeroZoom);
        }
        if zoom_numerator < zoom_denominator {
            return Err(LevelViewportError::ZoomBelowMinimum);
        }
        if u64::from(zoom_numerator) > u64::from(zoom_denominator) * 50 {
            return Err(LevelViewportError::ZoomAboveMaximum);
        }
        Viewport::new(origin, 1, 1, zoom_numerator, zoom_denominator)
            .map_err(|_| LevelViewportError::ZeroZoom)?;
        Ok(Self {
            origin,
            zoom_numerator,
            zoom_denominator,
        })
    }

    #[must_use]
    pub const fn zoom(self) -> (u32, u32) {
        (self.zoom_numerator, self.zoom_denominator)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelViewState {
    pub level: u16,
    pub viewport: LevelViewport,
}

impl LevelViewState {
    const fn initial(level: u16) -> Self {
        Self {
            level,
            viewport: LevelViewport {
                origin: Point { x: 0, y: 0 },
                zoom_numerator: 1,
                zoom_denominator: 1,
            },
        }
    }
}

#[derive(Debug)]
pub(crate) struct LevelNavigationHistory {
    current: Option<LevelViewState>,
    back: VecDeque<LevelViewState>,
    forward: Vec<LevelViewState>,
    limit: usize,
}

impl Default for LevelNavigationHistory {
    fn default() -> Self {
        Self {
            current: None,
            back: VecDeque::new(),
            forward: Vec::new(),
            limit: DEFAULT_HISTORY_LIMIT,
        }
    }
}

impl LevelNavigationHistory {
    pub(crate) fn current(&self) -> Option<LevelViewState> {
        self.current
    }

    pub(crate) fn can_navigate(&self, direction: LevelNavigationDirection) -> bool {
        match direction {
            LevelNavigationDirection::Back => !self.back.is_empty(),
            LevelNavigationDirection::Forward => !self.forward.is_empty(),
        }
    }

    pub(crate) fn reset(&mut self, level: Option<u16>) {
        self.current = level.map(LevelViewState::initial);
        self.back.clear();
        self.forward.clear();
    }

    pub(crate) fn visit(&mut self, level: u16) -> bool {
        if self.current.is_some_and(|state| state.level == level) {
            return false;
        }
        if let Some(current) = self.current {
            self.back.push_back(current);
            if self.back.len() > self.limit {
                self.back.pop_front();
            }
        }
        self.current = Some(LevelViewState::initial(level));
        self.forward.clear();
        true
    }

    pub(crate) fn set_viewport(&mut self, viewport: LevelViewport) -> Option<LevelViewState> {
        let current = self.current.as_mut()?;
        current.viewport = viewport;
        Some(*current)
    }

    pub(crate) fn navigate(
        &mut self,
        direction: LevelNavigationDirection,
    ) -> Option<LevelViewState> {
        match direction {
            LevelNavigationDirection::Back => {
                let level = self.back.pop_back()?;
                if let Some(current) = self.current.replace(level) {
                    self.forward.push(current);
                }
                Some(level)
            }
            LevelNavigationDirection::Forward => {
                let level = self.forward.pop()?;
                if let Some(current) = self.current.replace(level) {
                    self.back.push_back(current);
                }
                Some(level)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visits_branch_and_navigate_bidirectionally() {
        let mut history = LevelNavigationHistory::default();
        history.reset(Some(0x105));
        assert!(history.visit(0x106));
        assert!(history.visit(0x107));
        assert_eq!(
            history
                .navigate(LevelNavigationDirection::Back)
                .map(|state| state.level),
            Some(0x106)
        );
        assert_eq!(
            history
                .navigate(LevelNavigationDirection::Back)
                .map(|state| state.level),
            Some(0x105)
        );
        assert_eq!(
            history
                .navigate(LevelNavigationDirection::Forward)
                .map(|state| state.level),
            Some(0x106)
        );
        assert!(history.visit(0x108));
        assert_eq!(history.navigate(LevelNavigationDirection::Forward), None);
        assert!(!history.visit(0x108));
    }

    #[test]
    fn current_reports_the_active_level_view() {
        let mut history = LevelNavigationHistory::default();
        assert_eq!(history.current(), None);
        history.reset(Some(0x105));
        assert_eq!(history.current(), Some(LevelViewState::initial(0x105)));
        history.visit(0x106);
        assert_eq!(history.current(), Some(LevelViewState::initial(0x106)));
    }

    #[test]
    fn history_is_bounded() {
        let mut history = LevelNavigationHistory {
            limit: 2,
            ..LevelNavigationHistory::default()
        };
        history.reset(Some(1));
        history.visit(2);
        history.visit(3);
        history.visit(4);
        assert_eq!(
            history
                .navigate(LevelNavigationDirection::Back)
                .map(|state| state.level),
            Some(3)
        );
        assert_eq!(
            history
                .navigate(LevelNavigationDirection::Back)
                .map(|state| state.level),
            Some(2)
        );
        assert_eq!(history.navigate(LevelNavigationDirection::Back), None);
    }

    #[test]
    fn viewport_is_restored_with_each_level_entry() {
        let mut history = LevelNavigationHistory::default();
        history.reset(Some(0x105));
        let viewport = LevelViewport::new(Point { x: 320, y: 48 }, 3, 2).unwrap();
        history.set_viewport(viewport).unwrap();
        history.visit(0x106);
        let restored = history.navigate(LevelNavigationDirection::Back).unwrap();
        assert_eq!(restored.level, 0x105);
        assert_eq!(restored.viewport, viewport);
    }

    #[test]
    fn viewport_rejects_zero_zoom_components() {
        assert_eq!(
            LevelViewport::new(Point::default(), 0, 1),
            Err(LevelViewportError::ZeroZoom)
        );
        assert_eq!(
            LevelViewport::new(Point::default(), 1, 0),
            Err(LevelViewportError::ZeroZoom)
        );
    }

    #[test]
    fn viewport_accepts_recovered_zoom_boundaries_and_rejects_values_outside_them() {
        assert!(LevelViewport::new(Point::default(), 1, 1).is_ok());
        assert!(LevelViewport::new(Point::default(), 50, 1).is_ok());
        assert_eq!(
            LevelViewport::new(Point::default(), 99, 100),
            Err(LevelViewportError::ZoomBelowMinimum)
        );
        assert_eq!(
            LevelViewport::new(Point::default(), 5001, 100),
            Err(LevelViewportError::ZoomAboveMaximum)
        );
    }
}
