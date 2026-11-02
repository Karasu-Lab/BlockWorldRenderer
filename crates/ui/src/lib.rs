use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UiError {
    #[error("ui rendering failed: {0}")]
    Render(String),
}

impl UiError {
    pub fn code(&self) -> u16 {
        match self {
            UiError::Render(_) => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSwitch {
    enabled: bool,
}

impl UiSwitch {
    pub const fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.enabled
    }
}

impl Default for UiSwitch {
    fn default() -> Self {
        Self::new(false)
    }
}

pub trait UiRenderer {
    fn render_ui(&mut self) -> Result<(), UiError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NullUiRenderer;

impl UiRenderer for NullUiRenderer {
    fn render_ui(&mut self) -> Result<(), UiError> {
        Ok(())
    }
}

pub struct UiLayer {
    switch: UiSwitch,
    active: Box<dyn UiRenderer>,
    inactive: Box<dyn UiRenderer>,
}

impl UiLayer {
    pub fn new(switch: UiSwitch, active: Box<dyn UiRenderer>) -> Self {
        Self {
            switch,
            active,
            inactive: Box::new(NullUiRenderer),
        }
    }

    pub fn switch_mut(&mut self) -> &mut UiSwitch {
        &mut self.switch
    }

    pub fn render(&mut self) -> Result<(), UiError> {
        if self.switch.enabled() {
            self.active.render_ui()
        } else {
            self.inactive.render_ui()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Counting(Arc<AtomicU32>);

    impl UiRenderer for Counting {
        fn render_ui(&mut self) -> Result<(), UiError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn disabled_ui_is_not_drawn() {
        let drawn = Arc::new(AtomicU32::new(0));
        let mut layer = UiLayer::new(UiSwitch::new(false), Box::new(Counting(drawn.clone())));
        layer.render().unwrap();
        assert_eq!(drawn.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn enabled_ui_is_drawn() {
        let drawn = Arc::new(AtomicU32::new(0));
        let mut layer = UiLayer::new(UiSwitch::new(false), Box::new(Counting(drawn.clone())));
        layer.switch_mut().toggle();
        layer.render().unwrap();
        assert_eq!(drawn.load(Ordering::SeqCst), 1);
    }
}
