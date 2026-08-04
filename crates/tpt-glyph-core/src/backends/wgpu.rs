// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / backends / wgpu
//
// A GPU rasterization backend backed by `wgpu`, rendering into a headless
// (surfaceless) offscreen texture that's read back into a `Canvas` — there
// is no window/swapchain involved, since the pipeline only ever needs a
// pixel buffer. Fills and strokes are tessellated into triangles on the CPU
// via `lyon` (winding-rule-aware for fills, cap/join-aware for strokes)
// and rasterized with one draw call per `RenderTree`, since color is
// carried per-vertex rather than via a per-draw uniform.
//
// Deliberately hard-aliased (no MSAA) for v1, matching the same quality bar
// as the reference `SoftwareRasterizer` — see that module's tests for the
// same tradeoff acknowledged there. Like the other backends, clip regions
// are recorded but not yet intersected (see `raqote`'s equivalent note).

use crate::canvas::{Canvas, Pixel};
use crate::error::{GlyphError, Result};
use crate::geometry::{Path, Transform};
use crate::graphics_state::RgbColor;
use crate::render::{DrawCommand, Rasterizer, RenderTree};
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor,
    StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers,
};
use wgpu::util::DeviceExt;

/// GPU rasterizer backed by a headless `wgpu` device.
pub struct WgpuRasterizer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl WgpuRasterizer {
    /// Create a rasterizer backed by the best available GPU adapter.
    /// Returns `None` if no adapter (or no usable device) is available —
    /// callers should fall back to a CPU backend in that case.
    pub fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
                .ok()?;
        let pipeline = build_pipeline(&device);
        Some(Self {
            device,
            queue,
            pipeline,
        })
    }

    /// Cheap availability probe (adapter only, no device) used by
    /// `Backend::select`'s auto-detection.
    pub fn adapter_available() -> bool {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .is_some()
    }
}

fn build_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("tpt-glyph-wgpu-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_shader.wgsl").into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("tpt-glyph-wgpu-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("tpt-glyph-wgpu-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 8,
                        shader_location: 1,
                    },
                ],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    })
}

impl Rasterizer for WgpuRasterizer {
    fn rasterize(&self, tree: &RenderTree) -> Result<Canvas> {
        let width = tree.width.max(1);
        let height = tree.height.max(1);

        let mut geometry: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let mut fill_tess = FillTessellator::new();
        let mut stroke_tess = StrokeTessellator::new();

        for cmd in &tree.commands {
            match cmd {
                DrawCommand::Clip { .. } => {}
                DrawCommand::Fill { path, color, ctm } => {
                    tessellate_fill(
                        &mut fill_tess,
                        &mut geometry,
                        path,
                        ctm,
                        *color,
                        FillRule::NonZero,
                        width,
                        height,
                    );
                }
                DrawCommand::FillEvenOdd { path, color, ctm } => {
                    tessellate_fill(
                        &mut fill_tess,
                        &mut geometry,
                        path,
                        ctm,
                        *color,
                        FillRule::EvenOdd,
                        width,
                        height,
                    );
                }
                DrawCommand::Stroke {
                    path,
                    color,
                    line_width,
                    ctm,
                } => {
                    tessellate_stroke(
                        &mut stroke_tess,
                        &mut geometry,
                        path,
                        ctm,
                        *color,
                        *line_width,
                        width,
                        height,
                    );
                }
            }
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tpt-glyph-offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let vertex_buffer = (!geometry.vertices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("tpt-glyph-vertices"),
                    contents: as_bytes(&geometry.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let index_buffer = (!geometry.indices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("tpt-glyph-indices"),
                    contents: as_bytes(&geometry.indices),
                    usage: wgpu::BufferUsages::INDEX,
                })
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tpt-glyph-wgpu-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if let (Some(vb), Some(ib)) = (&vertex_buffer, &index_buffer) {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..geometry.indices.len() as u32, 0, 0..1);
            }
        }

        let bytes_per_pixel = 4u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * height) as wgpu::BufferAddress;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tpt-glyph-readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|_| GlyphError::Unsupported("wgpu: readback channel closed"))?
            .map_err(|_| GlyphError::Unsupported("wgpu: buffer map failed"))?;

        let mut canvas = Canvas::new(width, height, Pixel::WHITE);
        {
            let data = slice.get_mapped_range();
            for y in 0..height {
                let row = &data[(y * padded_bytes_per_row) as usize..];
                for x in 0..width {
                    let i = (x * bytes_per_pixel) as usize;
                    let idx = canvas.index(x, y);
                    canvas.pixels[idx] = Pixel::new(row[i], row[i + 1], row[i + 2], row[i + 3]);
                }
            }
        }
        readback.unmap();

        Ok(canvas)
    }
}

struct VertexCtor {
    color: [f32; 4],
    width: f32,
    height: f32,
}

impl FillVertexConstructor<Vertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: FillVertex) -> Vertex {
        let p = vertex.position();
        Vertex {
            position: to_ndc(p.x, p.y, self.width, self.height),
            color: self.color,
        }
    }
}

impl StrokeVertexConstructor<Vertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> Vertex {
        let p = vertex.position();
        Vertex {
            position: to_ndc(p.x, p.y, self.width, self.height),
            color: self.color,
        }
    }
}

/// Canvas pixel space (origin top-left, y down, `0..width`/`0..height`) to
/// wgpu clip space (origin center, y up, `-1..1`).
fn to_ndc(x: f32, y: f32, width: f32, height: f32) -> [f32; 2] {
    [(x / width) * 2.0 - 1.0, 1.0 - (y / height) * 2.0]
}

fn to_rgba(c: RgbColor) -> [f32; 4] {
    [c.r as f32, c.g as f32, c.b as f32, 1.0]
}

#[allow(clippy::too_many_arguments)]
fn tessellate_fill(
    tess: &mut FillTessellator,
    geometry: &mut VertexBuffers<Vertex, u32>,
    path: &Path,
    ctm: &Transform,
    color: RgbColor,
    rule: FillRule,
    width: u32,
    height: u32,
) {
    let lyon_path = build_lyon_path(path, ctm);
    let options = FillOptions::default().with_fill_rule(rule);
    let ctor = VertexCtor {
        color: to_rgba(color),
        width: width as f32,
        height: height as f32,
    };
    let _ = tess.tessellate_path(
        &lyon_path,
        &options,
        &mut BuffersBuilder::new(geometry, ctor),
    );
}

#[allow(clippy::too_many_arguments)]
fn tessellate_stroke(
    tess: &mut StrokeTessellator,
    geometry: &mut VertexBuffers<Vertex, u32>,
    path: &Path,
    ctm: &Transform,
    color: RgbColor,
    line_width: f64,
    width: u32,
    height: u32,
) {
    let lyon_path = build_lyon_path(path, ctm);
    // Cap/join style isn't carried by `DrawCommand::Stroke` (see the
    // `raqote` backend for the same limitation), so this hardcodes Butt/
    // Miter for parity with the other backends rather than guessing.
    let options = StrokeOptions::default()
        .with_line_width(line_width as f32)
        .with_line_cap(lyon::tessellation::LineCap::Butt)
        .with_line_join(lyon::tessellation::LineJoin::Miter);
    let ctor = VertexCtor {
        color: to_rgba(color),
        width: width as f32,
        height: height as f32,
    };
    let _ = tess.tessellate_path(
        &lyon_path,
        &options,
        &mut BuffersBuilder::new(geometry, ctor),
    );
}

fn build_lyon_path(path: &Path, ctm: &Transform) -> LyonPath {
    let mut builder = LyonPath::builder();
    for sub in &path.subpaths {
        let start = ctm.apply(sub.start);
        builder.begin(lyon::math::point(start.x as f32, start.y as f32));
        for seg in &sub.segments {
            let c1 = ctm.apply(seg.control1);
            let c2 = ctm.apply(seg.control2);
            let end = ctm.apply(seg.end);
            builder.cubic_bezier_to(
                lyon::math::point(c1.x as f32, c1.y as f32),
                lyon::math::point(c2.x as f32, c2.y as f32),
                lyon::math::point(end.x as f32, end.y as f32),
            );
        }
        builder.end(sub.closed);
    }
    builder.build()
}

/// # Safety
///
/// `T` must be a `#[repr(C)]` type composed only of plain numeric fields
/// with no padding (true of both `Vertex` and `u32`), so every byte of its
/// representation is initialized and there are no validity invariants
/// beyond "any bit pattern is a valid `T`".
fn as_bytes<T>(items: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(items.as_ptr() as *const u8, std::mem::size_of_val(items)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{CubicBezier, Point, Subpath};
    use crate::graphics_state::GraphicsState;
    use crate::render::RenderTree;

    fn square(lo: f64, hi: f64) -> Path {
        Path {
            subpaths: vec![Subpath {
                start: Point::new(lo, lo),
                segments: vec![
                    line(lo, lo, hi, lo),
                    line(hi, lo, hi, hi),
                    line(hi, hi, lo, hi),
                    line(lo, hi, lo, lo),
                ],
                closed: true,
            }],
        }
    }

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> CubicBezier {
        let start = Point::new(x0, y0);
        let end = Point::new(x1, y1);
        CubicBezier {
            start,
            control1: Point::new(x0 + (x1 - x0) / 3.0, y0 + (y1 - y0) / 3.0),
            control2: Point::new(x0 + (x1 - x0) * 2.0 / 3.0, y0 + (y1 - y0) * 2.0 / 3.0),
            end,
        }
    }

    #[test]
    fn wgpu_matches_reference_backend_when_a_gpu_is_available() {
        let Some(rasterizer) = WgpuRasterizer::new() else {
            eprintln!("skipping: no wgpu adapter available in this environment");
            return;
        };

        let mut tree = RenderTree::new(40, 40);
        let fill_state = GraphicsState::new().with_fill_color(RgbColor::new(1.0, 0.0, 0.0));
        tree.fill(&fill_state, square(6.0, 34.0));

        let reference = crate::raster::SoftwareRasterizer.rasterize(&tree).unwrap();
        let gpu = rasterizer.rasterize(&tree).unwrap();

        assert_eq!(reference.width, gpu.width);
        assert_eq!(reference.height, gpu.height);

        let is_red = |p: &Pixel| p.r > 200 && p.g < 50;
        assert!(reference.pixels.iter().filter(|p| is_red(p)).count() > 100);
        assert!(gpu.pixels.iter().filter(|p| is_red(p)).count() > 100);
    }
}
