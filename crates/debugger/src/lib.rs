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

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;

    impl RendererInfoSource for Fixed {
        fn renderer_info(&self) -> RendererInfo {
            RendererInfo {
                backend: "wgpu".into(),
                adapter: "test".into(),
                driver: "1".into(),
            }
        }
    }

    #[test]
    fn hidden_screen_has_no_lines() {
        let mut screen = DebugScreen::new();
        screen.add_provider(Box::new(RendererInfoProvider(Fixed)));
        assert!(screen.lines().unwrap().is_empty());
    }

    #[test]
    fn shown_screen_reports_the_renderer() {
        let mut screen = DebugScreen::new();
        screen.add_provider(Box::new(RendererInfoProvider(Fixed)));
        screen.switch_mut().set(true);
        assert_eq!(screen.lines().unwrap(), ["Renderer: wgpu (test, 1)"]);
    }
}
