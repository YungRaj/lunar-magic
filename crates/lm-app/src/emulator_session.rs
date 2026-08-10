//! Toolkit-independent live-emulator session semantics recovered from Lunar Magic 3.63.

/// Pause modes passed to the emulator backend.
///
/// The numeric representation is the exact `LMSW_Pause` argument used by Lunar Magic 3.63.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum EmulatorPauseMode {
    #[default]
    Running = 0,
    SoftPaused = 1,
    HardPaused = 2,
}

/// Independently accumulated hard-pause reasons used by the live level-testing session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EmulatorPauseReason {
    LevelTransition = 0x01,
    Manual = 0x02,
    Viewport = 0x04,
    Input = 0x08,
    MainWindow = 0x20,
    EditorMode = 0x40,
}

impl EmulatorPauseReason {
    const fn mask(self) -> u32 {
        self as u32
    }
}

/// Backend operations emitted by one accepted session-state transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmulatorSessionAction {
    SetPauseMode(EmulatorPauseMode),
    StepFrame,
}

/// Pure live-emulator lifecycle and pause aggregator.
///
/// State setters are inert while no session is active, matching Lunar Magic's recovered guards.
/// A setter emits `SetPauseMode` only when its corresponding state actually changes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmulatorSessionState {
    active: bool,
    hard_pause_reasons: u32,
    focus_soft_paused: bool,
}

impl EmulatorSessionState {
    #[must_use]
    pub const fn is_active(self) -> bool {
        self.active
    }

    #[must_use]
    pub const fn hard_pause_reasons(self) -> u32 {
        self.hard_pause_reasons
    }

    #[must_use]
    pub const fn is_paused_for(self, reason: EmulatorPauseReason) -> bool {
        self.hard_pause_reasons & reason.mask() != 0
    }

    #[must_use]
    pub const fn pause_mode(self) -> EmulatorPauseMode {
        if self.hard_pause_reasons != 0 {
            EmulatorPauseMode::HardPaused
        } else if self.focus_soft_paused {
            EmulatorPauseMode::SoftPaused
        } else {
            EmulatorPauseMode::Running
        }
    }

    /// Activates a freshly initialized backend session and applies its initial aggregate pause.
    pub fn start(&mut self) -> EmulatorSessionAction {
        self.active = true;
        EmulatorSessionAction::SetPauseMode(self.pause_mode())
    }

    /// Stops the backend session and clears every accumulated pause state.
    pub fn stop(&mut self) {
        self.active = false;
        self.hard_pause_reasons = 0;
        self.focus_soft_paused = false;
    }

    pub fn set_hard_pause_reason(
        &mut self,
        reason: EmulatorPauseReason,
        paused: bool,
    ) -> Option<EmulatorSessionAction> {
        if !self.active || self.is_paused_for(reason) == paused {
            return None;
        }
        self.hard_pause_reasons ^= reason.mask();
        Some(EmulatorSessionAction::SetPauseMode(self.pause_mode()))
    }

    pub fn set_focus_soft_paused(&mut self, paused: bool) -> Option<EmulatorSessionAction> {
        if !self.active || self.focus_soft_paused == paused {
            return None;
        }
        self.focus_soft_paused = paused;
        Some(EmulatorSessionAction::SetPauseMode(self.pause_mode()))
    }

    pub fn toggle_manual_pause(&mut self) -> Option<EmulatorSessionAction> {
        if !self.active {
            return None;
        }
        let paused = !self.is_paused_for(EmulatorPauseReason::Manual);
        self.set_hard_pause_reason(EmulatorPauseReason::Manual, paused)
    }

    /// Ensures manual hard pause is set, applies it when newly set, then steps exactly one frame.
    #[must_use]
    pub fn step_frame(&mut self) -> Vec<EmulatorSessionAction> {
        if !self.active {
            return Vec::new();
        }
        let mut actions = Vec::with_capacity(2);
        if let Some(pause) = self.set_hard_pause_reason(EmulatorPauseReason::Manual, true) {
            actions.push(pause);
        }
        actions.push(EmulatorSessionAction::StepFrame);
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_reason_masks_are_stable() {
        assert_eq!(EmulatorPauseReason::LevelTransition.mask(), 0x01);
        assert_eq!(EmulatorPauseReason::Manual.mask(), 0x02);
        assert_eq!(EmulatorPauseReason::Viewport.mask(), 0x04);
        assert_eq!(EmulatorPauseReason::Input.mask(), 0x08);
        assert_eq!(EmulatorPauseReason::MainWindow.mask(), 0x20);
        assert_eq!(EmulatorPauseReason::EditorMode.mask(), 0x40);
    }

    #[test]
    fn hard_pause_reasons_override_focus_soft_pause_until_all_clear() {
        let mut state = EmulatorSessionState::default();
        assert_eq!(
            state.start(),
            EmulatorSessionAction::SetPauseMode(EmulatorPauseMode::Running)
        );
        assert_eq!(
            state.set_focus_soft_paused(true),
            Some(EmulatorSessionAction::SetPauseMode(
                EmulatorPauseMode::SoftPaused
            ))
        );
        assert_eq!(
            state.set_hard_pause_reason(EmulatorPauseReason::Viewport, true),
            Some(EmulatorSessionAction::SetPauseMode(
                EmulatorPauseMode::HardPaused
            ))
        );
        assert_eq!(
            state.set_hard_pause_reason(EmulatorPauseReason::Input, true),
            Some(EmulatorSessionAction::SetPauseMode(
                EmulatorPauseMode::HardPaused
            ))
        );
        assert_eq!(
            state.set_hard_pause_reason(EmulatorPauseReason::Viewport, false),
            Some(EmulatorSessionAction::SetPauseMode(
                EmulatorPauseMode::HardPaused
            ))
        );
        assert_eq!(
            state.set_hard_pause_reason(EmulatorPauseReason::Input, false),
            Some(EmulatorSessionAction::SetPauseMode(
                EmulatorPauseMode::SoftPaused
            ))
        );
        assert_eq!(
            state.set_focus_soft_paused(false),
            Some(EmulatorSessionAction::SetPauseMode(
                EmulatorPauseMode::Running
            ))
        );
    }

    #[test]
    fn setters_are_inert_when_stopped_and_duplicate_updates_are_suppressed() {
        let mut state = EmulatorSessionState::default();
        assert_eq!(state.set_focus_soft_paused(true), None);
        assert_eq!(
            state.set_hard_pause_reason(EmulatorPauseReason::EditorMode, true),
            None
        );
        assert_eq!(state.toggle_manual_pause(), None);
        assert!(state.step_frame().is_empty());
        state.start();
        assert!(
            state
                .set_hard_pause_reason(EmulatorPauseReason::MainWindow, true)
                .is_some()
        );
        assert_eq!(
            state.set_hard_pause_reason(EmulatorPauseReason::MainWindow, true),
            None
        );
    }

    #[test]
    fn stepping_establishes_manual_pause_before_the_first_frame_only() {
        let mut state = EmulatorSessionState::default();
        state.start();
        assert_eq!(
            state.step_frame(),
            vec![
                EmulatorSessionAction::SetPauseMode(EmulatorPauseMode::HardPaused),
                EmulatorSessionAction::StepFrame,
            ]
        );
        assert!(state.is_paused_for(EmulatorPauseReason::Manual));
        assert_eq!(state.step_frame(), vec![EmulatorSessionAction::StepFrame]);
    }

    #[test]
    fn stop_clears_pause_state_before_a_new_session() {
        let mut state = EmulatorSessionState::default();
        state.start();
        state.set_focus_soft_paused(true);
        state.set_hard_pause_reason(EmulatorPauseReason::LevelTransition, true);
        state.stop();
        assert!(!state.is_active());
        assert_eq!(state.hard_pause_reasons(), 0);
        assert_eq!(state.pause_mode(), EmulatorPauseMode::Running);
        assert_eq!(
            state.start(),
            EmulatorSessionAction::SetPauseMode(EmulatorPauseMode::Running)
        );
    }
}
