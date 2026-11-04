#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererProfile {
    Desktop,
    FirefoxMobile,
}

impl RendererProfile {
    pub fn detect() -> Self {
        #[cfg(target_arch = "wasm32")]
        if let Some(agent) = web_sys::window().and_then(|window| window.navigator().user_agent().ok()) {
            return Self::from_user_agent(&agent);
        }
        Self::Desktop
    }

    pub fn from_user_agent(agent: &str) -> Self {
        let firefox = agent.contains("Firefox/") || agent.contains("FxiOS/");
        let mobile = agent.contains("Android") || agent.contains("Mobile") || agent.contains("FxiOS/");
        if firefox && mobile {
            Self::FirefoxMobile
        } else {
            Self::Desktop
        }
    }

    pub fn is_mobile(self) -> bool {
        matches!(self, Self::FirefoxMobile)
    }

    pub fn forces_webgl2(self) -> bool {
        matches!(self, Self::FirefoxMobile)
    }

    pub fn power_preference(self) -> wgpu::PowerPreference {
        match self {
            Self::Desktop => wgpu::PowerPreference::HighPerformance,
            Self::FirefoxMobile => wgpu::PowerPreference::LowPower,
        }
    }

    pub fn optional_features(self) -> wgpu::Features {
        let shared = wgpu::Features::INDIRECT_FIRST_INSTANCE
            | wgpu::Features::IMMEDIATES
            | wgpu::Features::DEPTH_CLIP_CONTROL
            | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM;
        match self {
            Self::Desktop => {
                shared
                    | wgpu::Features::FLOAT32_BLENDABLE
                    | wgpu::Features::FLOAT32_FILTERABLE
                    | wgpu::Features::MULTI_DRAW_INDIRECT_COUNT
            }
            Self::FirefoxMobile => shared,
        }
    }

    pub fn render_distance(self, requested: u32) -> u32 {
        match self {
            Self::Desktop => requested,
            Self::FirefoxMobile => requested.min(MOBILE_RENDER_DISTANCE),
        }
    }
}

const MOBILE_RENDER_DISTANCE: u32 = 6;

#[cfg(test)]
mod tests {
    use super::RendererProfile;

    #[test]
    fn firefox_on_phones_gets_the_mobile_profile() {
        for agent in [
            "Mozilla/5.0 (Android 14; Mobile; rv:140.0) Gecko/140.0 Firefox/140.0",
            "Mozilla/5.0 (Android 14; Tablet; rv:140.0) Gecko/140.0 Firefox/140.0",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) FxiOS/140.0 Mobile/15E148 Safari/605.1.15",
        ] {
            assert_eq!(
                RendererProfile::from_user_agent(agent),
                RendererProfile::FirefoxMobile
            );
        }
    }

    #[test]
    fn other_browsers_keep_the_desktop_profile() {
        for agent in [
            "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0",
            "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
        ] {
            assert_eq!(
                RendererProfile::from_user_agent(agent),
                RendererProfile::Desktop
            );
        }
    }
}
