use thiserror::Error;
use ui::UiSwitch;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DebuggerError {
    #[error("debug info provider failed: {0}")]
    Provider(String),
}

impl DebuggerError {
    pub fn code(&self) -> u16 {
        match self {
            DebuggerError::Provider(_) => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererInfo {
    pub backend: String,
    pub adapter: String,
    pub driver: String,
}

impl RendererInfo {
    pub fn line(&self) -> String {
        format!("{} ({}, {})", self.backend, self.adapter, self.driver)
    }

    /// The F3 screen's renderer lines: the graphics API, the GPU and, when the
    /// backend reports one, its driver.
    pub fn debug_lines(&self) -> Vec<String> {
        let mut lines = vec![self.backend.clone(), format!("GPU: {}", self.adapter)];
        if !self.driver.is_empty() {
            lines.push(format!("Driver: {}", self.driver));
        }
        lines
    }
}

pub trait RendererInfoSource {
    fn renderer_info(&self) -> RendererInfo;
}

pub trait DebugInfoProvider {
    fn lines(&self) -> Result<Vec<String>, DebuggerError>;
}

pub struct RendererInfoProvider<S: RendererInfoSource>(pub S);

impl<S: RendererInfoSource> DebugInfoProvider for RendererInfoProvider<S> {
    fn lines(&self) -> Result<Vec<String>, DebuggerError> {
        Ok(vec![format!("Renderer: {}", self.0.renderer_info().line())])
    }
}

pub struct DebugScreen {
    switch: UiSwitch,
    providers: Vec<Box<dyn DebugInfoProvider>>,
}

impl DebugScreen {
    pub fn new() -> Self {
        Self {
            switch: UiSwitch::new(false),
            providers: Vec::new(),
        }
    }

    pub fn add_provider(&mut self, provider: Box<dyn DebugInfoProvider>) {
        self.providers.push(provider);
    }

    pub fn switch_mut(&mut self) -> &mut UiSwitch {
        &mut self.switch
    }

    pub fn lines(&self) -> Result<Vec<String>, DebuggerError> {
        if !self.switch.enabled() {
            return Ok(Vec::new());
        }
        let mut lines = Vec::new();
        for provider in &self.providers {
            lines.extend(provider.lines()?);
        }
        Ok(lines)
    }
}

impl Default for DebugScreen {
    fn default() -> Self {
        Self::new()
    }
}
