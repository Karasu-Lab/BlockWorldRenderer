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

/// A renderer that draws into a frame its caller prepares: the data the UI
/// needs (text widths, the screen size) goes in and the drawn elements come
/// out.
pub trait UiFrameRenderer<F: ?Sized> {
    fn render_ui(&mut self, frame: &mut F) -> Result<(), UiError>;
}

/// [`UiLayer`] for a [`UiFrameRenderer`]: off, the frame is left as it came.
pub struct UiFrameLayer<F: ?Sized> {
    switch: UiSwitch,
    active: Box<dyn UiFrameRenderer<F>>,
}

impl<F: ?Sized> UiFrameLayer<F> {
    pub fn new(switch: UiSwitch, active: Box<dyn UiFrameRenderer<F>>) -> Self {
        Self { switch, active }
    }

    pub const fn switch(&self) -> &UiSwitch {
        &self.switch
    }

    pub fn switch_mut(&mut self) -> &mut UiSwitch {
        &mut self.switch
    }

    pub fn render(&mut self, frame: &mut F) -> Result<(), UiError> {
        if self.switch.enabled() {
            self.active.render_ui(frame)
        } else {
            Ok(())
        }
    }
}
