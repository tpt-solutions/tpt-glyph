# Quickstart

Build a [`RenderTree`] for a filled rectangle and rasterize it with the
auto-selected backend (`tpt_glyph_core::backend::SelectedBackend`). The backend
selection prefers a GPU device when one is available, then the `raqote` CPU
rasterizer, then the dependency-free reference rasterizer — all behind the
shared [`Rasterizer`](tpt_glyph_core::backend::Rasterizer) trait, so callers
are backend-agnostic.

```rust,no_run
use tpt_glyph_core::backend::SelectedBackend;
use tpt_glyph_core::geometry::{CubicBezier, Path, Point, Subpath};
use tpt_glyph_core::graphics_state::{GraphicsState, RgbColor};
use tpt_glyph_core::render::RenderTree;

let mut tree = RenderTree::new(64, 64);
let state = GraphicsState::new().with_fill_color(RgbColor::new(0.9, 0.2, 0.1));
tree.fill(&state, square(Point::new(8.0, 8.0), 48.0));

let backend = SelectedBackend::auto(None);
let canvas = backend.rasterize(&tree).expect("rasterize");
assert_eq!(canvas.width, 64);
assert_eq!(canvas.height, 64);
# fn square(origin: Point, size: f64) -> Path {
#     Path { subpaths: vec![Subpath {
#         start: origin,
#         segments: vec![
#             hline(origin.x, origin.x + size, origin.y),
#             vline(origin.x + size, origin.y, origin.y + size),
#             hline(origin.x + size, origin.x, origin.y + size),
#             vline(origin.x, origin.y + size, origin.y),
#         ],
#         closed: true,
#     }] }
# }
# fn hline(x0: f64, x1: f64, y: f64) -> CubicBezier {
#     CubicBezier { start: Point::new(x0, y), control1: Point::new((x0 + x1) / 2.0, y),
#         control2: Point::new((x0 + x1) / 2.0, y), end: Point::new(x1, y) }
# }
# fn vline(x: f64, y0: f64, y1: f64) -> CubicBezier {
#     CubicBezier { start: Point::new(x, y0), control1: Point::new(x, (y0 + y1) / 2.0),
#         control2: Point::new(x, (y0 + y1) / 2.0), end: Point::new(x, y1) }
# }
```
