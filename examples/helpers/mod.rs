use super::run;

mod perf_graph;
pub use perf_graph::PerfGraph;

pub trait WindowSurface {
    type Renderer: femtovg::Renderer + 'static;
    fn resize(&mut self, width: u32, height: u32);
    fn present(&self, canvas: &mut femtovg::Canvas<Self::Renderer>);
}

#[cfg(not(feature = "wgpu"))]
mod opengl;
#[cfg(not(feature = "wgpu"))]
pub use opengl::start_opengl as start;

#[cfg(feature = "wgpu")]
mod wgpu;
#[cfg(feature = "wgpu")]
pub use wgpu::start_wgpu as start;
