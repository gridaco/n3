//! Native capture by default; CI can explicitly require Linux software rendering.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Profile {
    Metal,
    Vulkan,
    Lavapipe,
}

impl Profile {
    pub(super) fn current() -> Result<Self, String> {
        let requested = std::env::var("N3_DOCS_RENDERER").ok();
        Self::for_host(std::env::consts::OS, requested.as_deref())
    }

    fn for_host(os: &str, requested: Option<&str>) -> Result<Self, String> {
        match requested {
            None | Some("native") => {}
            Some("lavapipe") if os == "linux" => return Ok(Self::Lavapipe),
            Some(value) => {
                return Err(format!(
                    "Unsupported documentation renderer {value:?} on {os}. Use native, or lavapipe on Linux."
                ));
            }
        }
        match os {
            "macos" => Ok(Self::Metal),
            "linux" => Ok(Self::Vulkan),
            other => Err(format!(
                "Documentation capture does not support host OS {other}."
            )),
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Metal => "macos-metal",
            Self::Vulkan => "linux-vulkan",
            Self::Lavapipe => "linux-vulkan-lavapipe",
        }
    }

    pub(super) fn backends(self) -> wgpu::Backends {
        match self {
            Self::Metal => wgpu::Backends::METAL,
            Self::Vulkan | Self::Lavapipe => wgpu::Backends::VULKAN,
        }
    }

    pub(super) fn force_fallback_adapter(self) -> bool {
        self == Self::Lavapipe
    }

    pub(super) fn validate(self, info: &wgpu::AdapterInfo) -> Result<(), String> {
        let accepted = match self {
            Self::Metal => info.backend == wgpu::Backend::Metal,
            Self::Vulkan => info.backend == wgpu::Backend::Vulkan,
            // Mesa exposes lavapipe's Vulkan driver under the llvmpipe name.
            // The pinned container selects its ICD; these checks also prevent
            // accidentally approving hardware or another software renderer.
            Self::Lavapipe => {
                info.backend == wgpu::Backend::Vulkan
                    && info.device_type == wgpu::DeviceType::Cpu
                    && matches!(info.driver.as_str(), "llvmpipe" | "lavapipe")
            }
        };
        if accepted {
            Ok(())
        } else {
            let required = match self {
                Self::Metal => "the Metal backend",
                Self::Vulkan => "the Vulkan backend",
                Self::Lavapipe => "the Mesa lavapipe Vulkan CPU adapter",
            };
            Err(format!(
                "Capture profile {} requires {required}; no other adapter is substituted. Got {}.",
                self.name(),
                self.receipt(info),
            ))
        }
    }

    pub(super) fn receipt(self, info: &wgpu::AdapterInfo) -> String {
        format!(
            "profile={} name={:?} backend={:?} device_type={:?} driver={:?} driver_info={:?}",
            self.name(),
            info.name,
            info.backend,
            info.device_type,
            info.driver,
            info.driver_info,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lavapipe() -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: "llvmpipe (LLVM 20.1.2, 256 bits)".into(),
            driver: "llvmpipe".into(),
            driver_info: "Mesa 25.0.3 (LLVM 20.1.2)".into(),
            ..wgpu::AdapterInfo::new(wgpu::DeviceType::Cpu, wgpu::Backend::Vulkan)
        }
    }

    #[test]
    fn native_capture_needs_no_ci_marker_or_container() {
        for requested in [None, Some("native")] {
            assert_eq!(
                Profile::for_host("macos", requested).unwrap(),
                Profile::Metal
            );
            assert_eq!(
                Profile::for_host("linux", requested).unwrap(),
                Profile::Vulkan
            );
        }
        assert!(!Profile::Vulkan.force_fallback_adapter());
        let hardware = wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        assert!(Profile::Vulkan.validate(&hardware).is_ok());
        assert!(Profile::Vulkan.validate(&lavapipe()).is_ok());
    }

    #[test]
    fn ci_profile_is_explicit_and_does_not_silently_fall_back() {
        assert_eq!(
            Profile::for_host("linux", Some("lavapipe")).unwrap(),
            Profile::Lavapipe
        );
        assert!(Profile::for_host("macos", Some("lavapipe")).is_err());
        for value in ["", "unknown", "lavapipe "] {
            assert!(Profile::for_host("linux", Some(value)).is_err());
        }
    }

    #[test]
    fn linux_capture_rejects_hardware_and_other_software_renderers() {
        let profile = Profile::Lavapipe;
        assert_eq!(profile.backends(), wgpu::Backends::VULKAN);
        assert!(profile.force_fallback_adapter());
        assert!(profile.validate(&lavapipe()).is_ok());
        for device_type in [
            wgpu::DeviceType::DiscreteGpu,
            wgpu::DeviceType::IntegratedGpu,
            wgpu::DeviceType::VirtualGpu,
            wgpu::DeviceType::Other,
        ] {
            let mut info = lavapipe();
            info.device_type = device_type;
            assert!(profile.validate(&info).is_err());
        }
        let mut info = lavapipe();
        info.backend = wgpu::Backend::Gl;
        assert!(profile.validate(&info).is_err());
        info.backend = wgpu::Backend::Vulkan;
        info.driver = "SwiftShader".into();
        assert!(profile.validate(&info).is_err());
    }

    #[test]
    fn macos_keeps_native_metal_and_rejects_the_linux_profile_adapter() {
        let profile = Profile::Metal;
        assert_eq!(profile.backends(), wgpu::Backends::METAL);
        assert!(!profile.force_fallback_adapter());
        let info = wgpu::AdapterInfo::new(wgpu::DeviceType::IntegratedGpu, wgpu::Backend::Metal);
        assert!(profile.validate(&info).is_ok());
        assert!(profile.validate(&lavapipe()).is_err());
    }

    #[test]
    fn receipt_identifies_the_renderer_and_driver_version() {
        let info = lavapipe();
        let receipt = Profile::Lavapipe.receipt(&info);
        for value in [
            "linux-vulkan-lavapipe",
            info.name.as_str(),
            "Vulkan",
            "Cpu",
            info.driver.as_str(),
            info.driver_info.as_str(),
        ] {
            assert!(receipt.contains(value), "Missing {value} from {receipt}");
        }
    }
}
