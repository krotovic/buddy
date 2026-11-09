//! GPU rendering: Vello draws the scene into an offscreen texture, and a tiny
//! pass copies it to the transparent window surface with premultiplied alpha.

use std::sync::Arc;

use anyhow::{Context, anyhow};
use vello::wgpu;
use winit::window::Window;

pub struct Gpu {
    instance: wgpu::Instance,
    adapter: Option<wgpu::Adapter>,
    device: Option<(wgpu::Device, wgpu::Queue)>,
    renderer: Option<vello::Renderer>,
}

pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    target_view: wgpu::TextureView,
    blit_pipeline: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    blit_group: wgpu::BindGroup,
    /// Human-readable description of the surface, logged once at start.
    pub info: String,
}

impl Gpu {
    pub fn new() -> Self {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if cfg!(target_os = "windows") {
            // A swapchain made straight from the HWND is always opaque. One
            // made from a DirectComposition visual supports per-pixel alpha.
            desc.backends = wgpu::Backends::DX12;
            desc.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
        } else if cfg!(target_os = "macos") {
            desc.backends = wgpu::Backends::METAL;
        }
        Self {
            instance: wgpu::Instance::new(desc),
            adapter: None,
            device: None,
            renderer: None,
        }
    }

    pub fn create_surface(&mut self, window: Arc<Window>) -> anyhow::Result<WindowSurface> {
        let size = window.inner_size();
        let surface = self
            .instance
            .create_surface(window)
            .context("creating window surface")?;

        if self.adapter.is_none() {
            let adapter =
                pollster::block_on(self.instance.request_adapter(&wgpu::RequestAdapterOptions {
                    // The cube is tiny; keep laptops on the integrated GPU.
                    power_preference: wgpu::PowerPreference::LowPower,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }))
                .context("no GPU adapter for this window")?;
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("buddy"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                }))
                .context("creating GPU device")?;
            let renderer = vello::Renderer::new(
                &device,
                vello::RendererOptions {
                    antialiasing_support: vello::AaSupport::area_only(),
                    ..Default::default()
                },
            )
            .map_err(|e| anyhow!("creating vello renderer: {e}"))?;
            self.adapter = Some(adapter);
            self.device = Some((device, queue));
            self.renderer = Some(renderer);
        }
        let adapter = self.adapter.as_ref().unwrap();
        let (device, _) = self.device.as_ref().unwrap();

        let caps = surface.get_capabilities(adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| anyhow!("surface has no formats"))?;
        let alpha_mode = pick_alpha_mode(&caps.alpha_modes);
        let info = format!(
            "adapter {:?} ({:?}), format {format:?}, alpha {alpha_mode:?} of {:?}",
            adapter.get_info().name,
            adapter.get_info().backend,
            caps.alpha_modes
        );
        if alpha_mode == wgpu::CompositeAlphaMode::Opaque {
            log::warn!("this surface cannot be transparent: {info}");
        }

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(device, &config);

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("blit.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let target_view = create_target(device, config.width, config.height);
        let blit_group = create_blit_group(device, &blit_layout, &target_view);

        Ok(WindowSurface {
            surface,
            config,
            target_view,
            blit_pipeline,
            blit_layout,
            blit_group,
            info,
        })
    }

    pub fn resize(&self, s: &mut WindowSurface, width: u32, height: u32) {
        let Some((device, _)) = &self.device else {
            return;
        };
        if width == 0 || height == 0 || (width, height) == (s.config.width, s.config.height) {
            return;
        }
        s.config.width = width;
        s.config.height = height;
        s.surface.configure(device, &s.config);
        s.target_view = create_target(device, width, height);
        s.blit_group = create_blit_group(device, &s.blit_layout, &s.target_view);
    }

    pub fn render(&mut self, s: &mut WindowSurface, scene: &vello::Scene) -> anyhow::Result<()> {
        let (Some((device, queue)), Some(renderer)) = (&self.device, &mut self.renderer) else {
            return Ok(());
        };
        renderer
            .render_to_texture(
                device,
                queue,
                scene,
                &s.target_view,
                &vello::RenderParams {
                    base_color: vello::peniko::color::palette::css::TRANSPARENT,
                    width: s.config.width,
                    height: s.config.height,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .map_err(|e| anyhow!("vello render: {e}"))?;

        let frame = match s.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                s.surface.configure(device, &s.config);
                return Ok(());
            }
            _ => return Ok(()),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("blit"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&s.blit_pipeline);
            pass.set_bind_group(0, &s.blit_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        frame.present();
        Ok(())
    }
}

/// Prefer modes that keep per-pixel alpha. Our blit outputs premultiplied
/// colour, which is what DirectComposition and Core Animation expect (Metal
/// reports this as PostMultiplied but composites premultiplied).
fn pick_alpha_mode(modes: &[wgpu::CompositeAlphaMode]) -> wgpu::CompositeAlphaMode {
    use wgpu::CompositeAlphaMode as M;
    [M::PreMultiplied, M::PostMultiplied, M::Inherit]
        .into_iter()
        .find(|m| modes.contains(m))
        .unwrap_or(M::Auto)
}

fn create_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("vello target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            format: wgpu::TextureFormat::Rgba8Unorm,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_blit_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::CompositeAlphaMode as M;

    #[test]
    fn prefers_premultiplied() {
        assert_eq!(
            pick_alpha_mode(&[M::Opaque, M::PostMultiplied, M::PreMultiplied]),
            M::PreMultiplied
        );
        assert_eq!(
            pick_alpha_mode(&[M::Opaque, M::PostMultiplied]),
            M::PostMultiplied
        );
        assert_eq!(pick_alpha_mode(&[M::Opaque]), M::Auto);
    }
}
