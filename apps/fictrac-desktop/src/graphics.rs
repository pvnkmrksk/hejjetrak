//! Linux graphics policy, independent of capture and numerical tracking.
use eframe::egui_wgpu::WgpuSetupCreateNew;
use std::sync::Arc;

pub fn linux_backends(explicit: Option<wgpu::Backends>, x11: bool) -> wgpu::Backends {
    explicit.unwrap_or(if x11 {
        wgpu::Backends::GL | wgpu::Backends::VULKAN
    } else {
        wgpu::Backends::PRIMARY | wgpu::Backends::GL
    })
}

pub fn configure_setup(
    setup: &mut WgpuSetupCreateNew,
    explicit: Option<wgpu::Backends>,
    x11: bool,
    adapter_name: Option<String>,
) {
    let backends = linux_backends(explicit, x11);
    setup.instance_descriptor.backends = backends;
    // Keep caller-provided selectors and native Wayland's automatic selection.
    if setup.native_adapter_selector.is_some() || (!x11 && adapter_name.is_none()) {
        return;
    }
    let prefer_gles = x11 && explicit.is_none();
    let power_preference = setup.power_preference;
    setup.native_adapter_selector = Some(Arc::new(move |adapters, surface| {
        select_native_adapter(
            adapters,
            surface,
            backends,
            prefer_gles,
            power_preference,
            adapter_name.as_deref(),
        )
    }));
}

struct AdapterCandidate<'a> {
    name: &'a str,
    backend: wgpu::Backend,
    device_type: wgpu::DeviceType,
    surface_compatible: bool,
}

fn select_adapter_index(
    adapters: &[AdapterCandidate<'_>],
    backends: wgpu::Backends,
    prefer_gles: bool,
    power_preference: wgpu::PowerPreference,
    adapter_name: Option<&str>,
) -> Option<usize> {
    let name_filter = adapter_name.map(str::to_lowercase);
    adapters
        .iter()
        .enumerate()
        .filter(|(_, adapter)| {
            adapter.surface_compatible
                && backends.contains(wgpu::Backends::from(adapter.backend))
                && name_filter
                    .as_ref()
                    .is_none_or(|name| adapter.name.to_lowercase().contains(name.as_str()))
        })
        .min_by_key(|(index, adapter)| {
            let backend_order = if prefer_gles {
                match adapter.backend {
                    wgpu::Backend::Gl => 0,
                    wgpu::Backend::Vulkan => 1,
                    _ => 2,
                }
            } else {
                0
            };
            (
                backend_order,
                power_order(adapter.device_type, power_preference),
                *index,
            )
        })
        .map(|(index, _)| index)
}

fn power_order(device: wgpu::DeviceType, preference: wgpu::PowerPreference) -> u8 {
    if preference == wgpu::PowerPreference::None {
        return 0;
    }
    // Match wgpu-core 27's request_adapter ordering within the chosen backend.
    match device {
        wgpu::DeviceType::IntegratedGpu if preference == wgpu::PowerPreference::LowPower => 1,
        wgpu::DeviceType::DiscreteGpu if preference == wgpu::PowerPreference::LowPower => 2,
        wgpu::DeviceType::DiscreteGpu => 1,
        wgpu::DeviceType::IntegratedGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::VirtualGpu => 4,
        wgpu::DeviceType::Cpu => 5,
    }
}

fn selection_error(
    adapters: &[AdapterCandidate<'_>],
    backends: wgpu::Backends,
    adapter_name: Option<&str>,
) -> String {
    let available = adapters
        .iter()
        .map(|adapter| {
            format!(
                "{} ({:?}, {})",
                adapter.name,
                adapter.backend,
                if adapter.surface_compatible {
                    "surface supported"
                } else {
                    "surface unsupported"
                }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let named = adapter_name.map_or(String::new(), |name| {
        format!(" matching WGPU_ADAPTER_NAME={name:?}")
    });
    format!(
        "No graphics adapter{named} can present to the current Linux window surface using {backends:?}. Available adapters: {}. Check the Mesa or vendor graphics driver for this desktop session. Correct or unset WGPU_BACKEND/WGPU_ADAPTER_NAME overrides if present; WGPU_BACKEND=gl or WGPU_BACKEND=vulkan can select an installed driver explicitly.",
        if available.is_empty() {
            "none"
        } else {
            &available
        },
    )
}

fn select_native_adapter(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
    backends: wgpu::Backends,
    prefer_gles: bool,
    power_preference: wgpu::PowerPreference,
    adapter_name: Option<&str>,
) -> Result<wgpu::Adapter, String> {
    let information = adapters
        .iter()
        .map(wgpu::Adapter::get_info)
        .collect::<Vec<_>>();
    let candidates = adapters
        .iter()
        .zip(&information)
        .map(|(adapter, info)| AdapterCandidate {
            name: &info.name,
            backend: info.backend,
            device_type: info.device_type,
            surface_compatible: surface.is_none_or(|surface| {
                adapter.is_surface_supported(surface)
                    && eframe::egui_wgpu::preferred_framebuffer_format(
                        &surface.get_capabilities(adapter).formats,
                    )
                    .is_ok()
            }),
        })
        .collect::<Vec<_>>();
    let selected = select_adapter_index(
        &candidates,
        backends,
        prefer_gles,
        power_preference,
        adapter_name,
    )
    .and_then(|index| adapters.get(index))
    .cloned()
    .ok_or_else(|| selection_error(&candidates, backends, adapter_name))?;
    if prefer_gles && selected.get_info().backend == wgpu::Backend::Vulkan {
        eprintln!(
            "Graphics: no matching OpenGL adapter supports this window surface; using the compatible Vulkan adapter"
        );
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(name: &str, backend: wgpu::Backend, compatible: bool) -> AdapterCandidate<'_> {
        AdapterCandidate {
            name,
            backend,
            device_type: wgpu::DeviceType::Other,
            surface_compatible: compatible,
        }
    }

    fn select(adapters: &[AdapterCandidate<'_>]) -> Option<usize> {
        select_adapter_index(
            adapters,
            linux_backends(None, true),
            true,
            wgpu::PowerPreference::HighPerformance,
            None,
        )
    }

    #[test]
    fn compatible_gles_stays_preferred_over_a_discrete_vulkan_adapter() {
        let adapters = [
            AdapterCandidate {
                device_type: wgpu::DeviceType::DiscreteGpu,
                ..candidate("Vulkan GPU", wgpu::Backend::Vulkan, true)
            },
            candidate("NV136", wgpu::Backend::Gl, true),
        ];
        assert_eq!(select(&adapters), Some(1));
    }

    #[test]
    fn incompatible_gles_falls_back_to_surface_compatible_vulkan() {
        let adapters = [
            candidate("GLES without window presentation", wgpu::Backend::Gl, false),
            candidate(
                "Vulkan with window presentation",
                wgpu::Backend::Vulkan,
                true,
            ),
        ];
        assert_eq!(select(&adapters), Some(1));
        assert_eq!(select(&adapters[1..]), Some(0));
    }

    #[test]
    fn incompatible_adapters_fail_with_display_and_override_guidance() {
        let adapters = [
            candidate("GLES", wgpu::Backend::Gl, false),
            candidate("Vulkan", wgpu::Backend::Vulkan, false),
        ];
        assert_eq!(select(&adapters), None);
        assert_eq!(select(&[]), None);
        let error = selection_error(&adapters, linux_backends(None, true), Some("chosen"));
        assert!(error.contains("current Linux window surface"));
        assert!(error.contains("surface unsupported"));
        assert!(error.contains("Mesa or vendor graphics driver"));
        assert!(error.contains("WGPU_ADAPTER_NAME=\"chosen\""));
        assert!(error.contains("WGPU_BACKEND"));
    }

    #[test]
    fn explicit_backend_masks_do_not_enable_or_prefer_an_unrequested_backend() {
        let adapters = [
            AdapterCandidate {
                device_type: wgpu::DeviceType::DiscreteGpu,
                ..candidate("Vulkan GPU", wgpu::Backend::Vulkan, true)
            },
            candidate("GLES GPU", wgpu::Backend::Gl, true),
        ];
        for (backends, expected) in [
            (wgpu::Backends::GL, Some(1)),
            (wgpu::Backends::VULKAN, Some(0)),
            (wgpu::Backends::GL | wgpu::Backends::VULKAN, Some(0)),
            (wgpu::Backends::empty(), None),
        ] {
            let requested = linux_backends(Some(backends), true);
            assert_eq!(requested, backends);
            assert_eq!(
                select_adapter_index(
                    &adapters,
                    requested,
                    false,
                    wgpu::PowerPreference::HighPerformance,
                    None,
                ),
                expected
            );
        }
        assert_eq!(
            select_adapter_index(
                &adapters[..1],
                wgpu::Backends::GL,
                false,
                wgpu::PowerPreference::HighPerformance,
                None,
            ),
            None,
        );
    }

    #[test]
    fn adapter_name_is_a_case_insensitive_filter_without_silent_substitution() {
        let adapters = [
            candidate("GLES GPU", wgpu::Backend::Gl, true),
            candidate("Chosen Vulkan GPU", wgpu::Backend::Vulkan, true),
        ];
        for (name, expected) in [("cHoSeN", Some(1)), ("GLES", Some(0)), ("missing", None)] {
            assert_eq!(
                select_adapter_index(
                    &adapters,
                    linux_backends(None, true),
                    true,
                    wgpu::PowerPreference::HighPerformance,
                    Some(name),
                ),
                expected
            );
        }
    }

    #[test]
    fn power_preferences_remain_effective_within_the_preferred_backend() {
        let adapters = [
            AdapterCandidate {
                device_type: wgpu::DeviceType::IntegratedGpu,
                ..candidate("Integrated GLES", wgpu::Backend::Gl, true)
            },
            AdapterCandidate {
                device_type: wgpu::DeviceType::DiscreteGpu,
                ..candidate("Discrete GLES", wgpu::Backend::Gl, true)
            },
        ];
        for (power, expected) in [
            (wgpu::PowerPreference::HighPerformance, 1),
            (wgpu::PowerPreference::LowPower, 0),
            (wgpu::PowerPreference::None, 0),
        ] {
            assert_eq!(
                select_adapter_index(&adapters, linux_backends(None, true), true, power, None),
                Some(expected)
            );
        }
    }

    #[test]
    fn native_wayland_and_existing_selectors_keep_their_selection_path() {
        let mut setup = WgpuSetupCreateNew::default();
        configure_setup(&mut setup, None, false, None);
        assert!(setup.native_adapter_selector.is_none());
        assert_eq!(
            setup.instance_descriptor.backends,
            wgpu::Backends::PRIMARY | wgpu::Backends::GL
        );
        let existing: eframe::egui_wgpu::NativeAdapterSelectorMethod =
            Arc::new(|_, _| Err("caller-owned selector".into()));
        setup.native_adapter_selector = Some(existing.clone());
        configure_setup(&mut setup, None, true, None);
        assert!(
            setup
                .native_adapter_selector
                .as_ref()
                .is_some_and(|selected| Arc::ptr_eq(selected, &existing))
        );
    }
}
