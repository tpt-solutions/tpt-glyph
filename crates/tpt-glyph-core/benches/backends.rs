// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / benchmarks
//
// Phase 6 + Phase 7 benchmarks:
//   * `rasterize_reference` — reference software scanline rasterizer.
//   * `rasterize_raqote`    — raqote CPU backend (only when compiled in).
//   * `rasterize_wgpu`      — wgpu GPU backend (only when compiled in and an
//     adapter is available at runtime).
//   * `render_pages_parallel` / `render_pages_serial` — multi-page PDF throughput
//     (Phase 7) comparing concurrent vs single-threaded rendering.

use criterion::{criterion_group, criterion_main, Criterion};
use tpt_glyph_core::geometry::{CubicBezier, Path, Subpath};
use tpt_glyph_core::graphics_state::{GraphicsState, RgbColor};
use tpt_glyph_core::render::{Rasterizer, RenderTree};

fn sample_tree() -> RenderTree {
    let mut tree = RenderTree::new(512, 512);
    let fill = GraphicsState::new().with_fill_color(RgbColor::new(0.8, 0.2, 0.1));
    let stroke = GraphicsState::new()
        .with_stroke_color(RgbColor::new(0.1, 0.3, 0.9))
        .with_line_width(2.0);
    for i in 0..20 {
        let o = i as f64 * 12.0;
        tree.fill(&fill, square(o, o, 80.0));
    }
    tree.stroke(&stroke, diagonal());
    tree
}

fn square(x: f64, y: f64, s: f64) -> Path {
    Path {
        subpaths: vec![Subpath {
            start: tpt_glyph_core::geometry::Point::new(x, y),
            segments: vec![
                hline(x, x + s, y),
                vline(x + s, y, y + s),
                hline(x + s, x, y + s),
                vline(x, y + s, y),
            ],
            closed: true,
        }],
    }
}

fn hline(x0: f64, x1: f64, y: f64) -> CubicBezier {
    CubicBezier {
        start: tpt_glyph_core::geometry::Point::new(x0, y),
        control1: tpt_glyph_core::geometry::Point::new((x0 + x1) / 2.0, y),
        control2: tpt_glyph_core::geometry::Point::new((x0 + x1) / 2.0, y),
        end: tpt_glyph_core::geometry::Point::new(x1, y),
    }
}

fn vline(x: f64, y0: f64, y1: f64) -> CubicBezier {
    CubicBezier {
        start: tpt_glyph_core::geometry::Point::new(x, y0),
        control1: tpt_glyph_core::geometry::Point::new(x, (y0 + y1) / 2.0),
        control2: tpt_glyph_core::geometry::Point::new(x, (y0 + y1) / 2.0),
        end: tpt_glyph_core::geometry::Point::new(x, y1),
    }
}

fn diagonal() -> Path {
    Path {
        subpaths: vec![Subpath {
            start: tpt_glyph_core::geometry::Point::new(10.0, 10.0),
            segments: vec![CubicBezier {
                start: tpt_glyph_core::geometry::Point::new(10.0, 10.0),
                control1: tpt_glyph_core::geometry::Point::new(500.0, 500.0),
                control2: tpt_glyph_core::geometry::Point::new(500.0, 500.0),
                end: tpt_glyph_core::geometry::Point::new(500.0, 500.0),
            }],
            closed: false,
        }],
    }
}

fn rasterize_reference(c: &mut Criterion) {
    let tree = sample_tree();
    c.bench_function("rasterize_reference", |b| {
        b.iter(|| {
            let canvas = tpt_glyph_core::raster::SoftwareRasterizer
                .rasterize(&tree)
                .unwrap();
            criterion::black_box(canvas);
        })
    });
}

#[cfg(feature = "raqote-backend")]
fn rasterize_raqote(c: &mut Criterion) {
    use tpt_glyph_core::backends::raqote::RaqoteRasterizer;
    let tree = sample_tree();
    c.bench_function("rasterize_raqote", |b| {
        b.iter(|| {
            let canvas = RaqoteRasterizer.rasterize(&tree).unwrap();
            criterion::black_box(canvas);
        })
    });
}

#[cfg(feature = "wgpu-backend")]
fn rasterize_wgpu(c: &mut Criterion) {
    use tpt_glyph_core::backends::wgpu::WgpuRasterizer;
    let Some(rasterizer) = WgpuRasterizer::new() else {
        eprintln!("skipping rasterize_wgpu: no wgpu adapter available in this environment");
        return;
    };
    let tree = sample_tree();
    c.bench_function("rasterize_wgpu", |b| {
        b.iter(|| {
            let canvas = rasterizer.rasterize(&tree).unwrap();
            criterion::black_box(canvas);
        })
    });
}

#[cfg(all(feature = "raqote-backend", feature = "wgpu-backend"))]
criterion_group!(
    benches,
    rasterize_reference,
    rasterize_raqote,
    rasterize_wgpu
);
#[cfg(all(feature = "raqote-backend", not(feature = "wgpu-backend")))]
criterion_group!(benches, rasterize_reference, rasterize_raqote);
#[cfg(all(not(feature = "raqote-backend"), feature = "wgpu-backend"))]
criterion_group!(benches, rasterize_reference, rasterize_wgpu);
#[cfg(all(not(feature = "raqote-backend"), not(feature = "wgpu-backend")))]
criterion_group!(benches, rasterize_reference);
criterion_main!(benches);
