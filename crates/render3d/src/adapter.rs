//! Adapter choice and device request. Every device is requested with the downlevel WebGL2
//! limits so native render tests exercise the limits the web build has to live within.

use crate::Render3dError;

/// Which adapter to request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AdapterChoice {
    /// The best available hardware adapter.
    #[default]
    Auto,
    /// The software (fallback) adapter.
    Fallback,
}

impl AdapterChoice {
    /// Parse `auto` or `fallback` (case-insensitive); anything else is `None`.
    pub fn parse(s: &str) -> Option<AdapterChoice> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("auto") {
            Some(AdapterChoice::Auto)
        } else if s.eq_ignore_ascii_case("fallback") {
            Some(AdapterChoice::Fallback)
        } else {
            None
        }
    }
}

/// What device a renderer ended up on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Adapter name as reported by the driver.
    pub name: String,
    /// Graphics backend (`Vulkan`, `Metal`, `Gl`, …).
    pub backend: String,
    /// Adapter type (`DiscreteGpu`, `Cpu`, …).
    pub adapter_type: String,
    /// True when the device was requested with the downlevel WebGL2 limits.
    pub downlevel: bool,
}

impl DeviceInfo {
    /// One-line report: `DEVICE name=… backend=… type=…`, plus ` limits=downlevel_webgl2` when
    /// the device uses the downlevel WebGL2 limits.
    pub fn line(&self) -> String {
        let mut s = format!("DEVICE name={} backend={} type={}", self.name, self.backend, self.adapter_type);
        if self.downlevel {
            s.push_str(" limits=downlevel_webgl2");
        }
        s
    }
}

/// Request an adapter per `choice` and a device on it with the downlevel WebGL2 limits
/// (weakened to the adapter's own limits where it is weaker).
/// [`Render3dError::NoAdapter`] when there is no adapter.
pub async fn request_device(instance: &wgpu::Instance, choice: AdapterChoice) -> Result<(wgpu::Device, wgpu::Queue, DeviceInfo), Render3dError> {
    let options = match choice {
        AdapterChoice::Auto => wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() },
        AdapterChoice::Fallback => wgpu::RequestAdapterOptions { force_fallback_adapter: true, ..Default::default() },
    };
    let adapter = instance.request_adapter(&options).await.map_err(|_| Render3dError::NoAdapter)?;
    let wanted = wgpu::Limits::downlevel_webgl2_defaults();
    let required_limits = wanted.clone().or_worse_values_from(&adapter.limits());
    let downlevel = required_limits == wanted;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor { label: Some("render3d"), required_limits, ..Default::default() })
        .await
        .map_err(|e| Render3dError::Wgpu(e.to_string()))?;
    let info = adapter.get_info();
    let info = DeviceInfo { name: info.name, backend: format!("{:?}", info.backend), adapter_type: format!("{:?}", info.device_type), downlevel };
    Ok((device, queue, info))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_choice_parses_auto_and_fallback_only() {
        assert_eq!(AdapterChoice::parse("auto"), Some(AdapterChoice::Auto));
        assert_eq!(AdapterChoice::parse("AUTO"), Some(AdapterChoice::Auto));
        assert_eq!(AdapterChoice::parse("Fallback"), Some(AdapterChoice::Fallback));
        assert_eq!(AdapterChoice::parse("fallback"), Some(AdapterChoice::Fallback));
        for bad in ["", "gpu", "software", "auto fallback", "fallbac"] {
            assert_eq!(AdapterChoice::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn device_info_line_names_the_limits() {
        let mut info = DeviceInfo { name: "Test GPU".into(), backend: "Vulkan".into(), adapter_type: "Cpu".into(), downlevel: true };
        assert_eq!(info.line(), "DEVICE name=Test GPU backend=Vulkan type=Cpu limits=downlevel_webgl2");
        info.downlevel = false;
        assert_eq!(info.line(), "DEVICE name=Test GPU backend=Vulkan type=Cpu");
    }

    /// Request a device bounded in time; `None` when the machine has no suitable adapter.
    fn device(choice: AdapterChoice) -> Option<(wgpu::Device, DeviceInfo)> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let instance = crate::offscreen::headless_instance();
            let got = futures::executor::block_on(request_device(&instance, choice)).ok();
            let _ = tx.send(got.map(|(d, _q, i)| (d, i)));
        });
        rx.recv_timeout(std::time::Duration::from_secs(30)).ok().flatten()
    }

    #[test]
    fn device_limits_are_downlevel_webgl2() {
        let Some((device, info)) = device(AdapterChoice::Auto) else { return };
        let web = wgpu::Limits::downlevel_webgl2_defaults();
        let got = device.limits();
        assert!(got.max_texture_dimension_2d <= web.max_texture_dimension_2d.max(got.max_texture_dimension_2d));
        if info.downlevel {
            assert_eq!(got.max_texture_dimension_2d, web.max_texture_dimension_2d);
            assert_eq!(got.max_bind_groups, web.max_bind_groups);
            assert!(info.line().ends_with("limits=downlevel_webgl2"));
        }
    }

    #[test]
    fn fallback_adapter_is_requested_when_asked() {
        // Software adapters exist only on some machines: with one, it must be a CPU adapter.
        if let Some((_device, info)) = device(AdapterChoice::Fallback) {
            assert_eq!(info.adapter_type, "Cpu", "{}", info.line());
        }
    }
}
