use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use rgb::bytemuck;
use wgpu::util::DeviceExt;
use wgpu::PipelineCompilationOptions;

use crate::image::ImageStore;
use crate::paint::GlyphTexture;
use crate::renderer::ShaderType;
use crate::BlendFactor;
use crate::FillRule;
use crate::ImageId;
use crate::ImageInfo;
use crate::RenderTarget;
use crate::Scissor;

use super::Renderer;

pub use wgpu;

/*
#[repr(C)]
#[derive(Clone, Copy,)]
struct Vertex {
    _pos: [f32; 4],
    _tex_coord: [f32; 2],
}
    */

use super::Params;
use super::Vertex;

#[derive(Clone, Default, PartialEq)]
struct Mat3x4<T>([[T; 4]; 3]);

impl<T> AsRef<[[T; 4]; 3]> for Mat3x4<T> {
    fn as_ref(&self) -> &[[T; 4]; 3] {
        &self.0
    }
}

impl<T> AsMut<[[T; 4]; 3]> for Mat3x4<T> {
    fn as_mut(&mut self) -> &mut [[T; 4]; 3] {
        &mut self.0
    }
}

impl<T: Copy> From<[T; 12]> for Mat3x4<T> {
    fn from(mat: [T; 12]) -> Self {
        Self([
            [mat[0], mat[3], mat[6], mat[9]],
            [mat[1], mat[4], mat[7], mat[10]],
            [mat[2], mat[5], mat[8], mat[11]],
        ])
    }
}

#[derive(Clone, Default, PartialEq)]
struct Vec4<T>([T; 4]);

impl<T> AsRef<[T; 4]> for Vec4<T> {
    fn as_ref(&self) -> &[T; 4] {
        &self.0
    }
}

impl<T> AsMut<[T; 4]> for Vec4<T> {
    fn as_mut(&mut self) -> &mut [T; 4] {
        &mut self.0
    }
}

#[derive(Clone, Default, PartialEq)]
struct Vec2<T>([T; 2]);

impl<T> AsRef<[T; 2]> for Vec2<T> {
    fn as_ref(&self) -> &[T; 2] {
        &self.0
    }
}

impl<T> AsMut<[T; 2]> for Vec2<T> {
    fn as_mut(&mut self) -> &mut [T; 2] {
        &mut self.0
    }
}

#[derive(Clone, Default, PartialEq)]
struct Vec3<T>([T; 3]);

impl<T> AsRef<[T; 3]> for Vec3<T> {
    fn as_ref(&self) -> &[T; 3] {
        &self.0
    }
}

impl<T> AsMut<[T; 3]> for Vec3<T> {
    fn as_mut(&mut self) -> &mut [T; 3] {
        &mut self.0
    }
}

encase::impl_matrix!(3, 4, Mat3x4<T>; using AsRef AsMut From);
encase::impl_vector!(4, Vec4<T>; using AsRef AsMut From);
encase::impl_vector!(2, Vec2<T>; using AsRef AsMut From);
encase::impl_vector!(3, Vec3<T>; using AsRef AsMut From);

#[derive(encase::ShaderType, Clone, Default, PartialEq)]
struct UniformParams {
    scissor_mat: Mat3x4<f32>,
    paint_mat: Mat3x4<f32>,
    inner_col: Vec4<f32>,
    outer_col: Vec4<f32>,
    scissor_ext: Vec2<f32>,
    scissor_scale: Vec2<f32>,
    extent: Vec2<f32>,
    radius: f32,
    feather: f32,
    stroke_mult: f32,
    stroke_thr: f32,
    tex_type: f32,
    _unused_shader_type: f32,
    glyph_texture_type: f32, // 0 -> no glyph rendering, 1 -> alpha mask, 2 -> color texture
    image_blur_filter_sigma: f32,
    image_blur_filter_direction: Vec2<f32>,
    image_blur_filter_coeff: Vec3<f32>,
}

impl UniformParams {
    pub fn set_scissor_mat(&mut self, mat: [f32; 12]) {
        //use encase::matrix::AsMutMatrixParts;
        //self.scissor_mat = mat;
        // self.scissor_mat.as_mut().iter_mut().rev().flatten().zip(mat.iter()).for_each(|(a, b)| *a = *b);
        self.scissor_mat = mat.into();
    }

    pub fn set_paint_mat(&mut self, mat: [f32; 12]) {
        self.paint_mat = mat.into();
        //self.paint_mat.as_mut().iter_mut().flatten().zip(mat.iter()).for_each(|(a, b)| *a = *b);
    }

    pub fn set_inner_col(&mut self, col: [f32; 4]) {
        self.inner_col.0 = col;
        //self.inner_col.as_mut().copy_from_slice(&col);
    }

    pub fn set_outer_col(&mut self, col: [f32; 4]) {
        self.outer_col.0 = col;
        //self.outer_col.as_mut().copy_from_slice(&col);
    }

    pub fn set_scissor_ext(&mut self, ext: [f32; 2]) {
        self.scissor_ext.0 = ext;
        //self.scissor_ext.as_mut().copy_from_slice(&ext);
    }

    pub fn set_scissor_scale(&mut self, scale: [f32; 2]) {
        self.scissor_scale.0 = scale;
        //self.scissor_scale.as_mut().copy_from_slice(&scale);
    }

    pub fn set_extent(&mut self, ext: [f32; 2]) {
        self.extent.0 = ext;
        //self.extent.as_mut().copy_from_slice(&ext);
    }

    pub fn set_radius(&mut self, radius: f32) {
        self.radius = radius;
    }

    pub fn set_feather(&mut self, feather: f32) {
        self.feather = feather;
    }

    pub fn set_stroke_mult(&mut self, stroke_mult: f32) {
        self.stroke_mult = stroke_mult;
    }

    pub fn set_stroke_thr(&mut self, stroke_thr: f32) {
        self.stroke_thr = stroke_thr;
    }

    pub fn set_tex_type(&mut self, tex_type: f32) {
        self.tex_type = tex_type;
    }

    pub fn set_shader_type(&mut self, shader_type: f32) {
        self._unused_shader_type = shader_type;
    }

    pub fn set_glyph_texture_type(&mut self, glyph_texture_type: f32) {
        self.glyph_texture_type = glyph_texture_type;
    }

    pub fn set_image_blur_filter_direction(&mut self, direction: [f32; 2]) {
        self.image_blur_filter_direction.0 = direction;
        //self.image_blur_filter_direction.as_mut().copy_from_slice(&direction);
    }

    pub fn set_image_blur_filter_sigma(&mut self, sigma: f32) {
        self.image_blur_filter_sigma = sigma;
    }

    pub fn set_image_blur_filter_coeff(&mut self, coeff: [f32; 3]) {
        self.image_blur_filter_coeff.0 = coeff;
        //self.image_blur_filter_coeff.as_mut().copy_from_slice(&coeff);
    }
}

impl From<&Params> for UniformParams {
    fn from(params: &Params) -> Self {
        let mut arr = Self::default();

        arr.set_scissor_mat(params.scissor_mat);
        arr.set_paint_mat(params.paint_mat);
        arr.set_inner_col(params.inner_col);
        arr.set_outer_col(params.outer_col);
        arr.set_scissor_ext(params.scissor_ext);
        arr.set_scissor_scale(params.scissor_scale);
        arr.set_extent(params.extent);
        arr.set_radius(params.radius);
        arr.set_feather(params.feather);
        arr.set_stroke_mult(params.stroke_mult);
        arr.set_stroke_thr(params.stroke_thr);
        arr.set_shader_type(params.shader_type.to_f32());
        arr.set_tex_type(params.tex_type);
        arr.set_glyph_texture_type(params.glyph_texture_type as f32);
        arr.set_image_blur_filter_direction(params.image_blur_filter_direction);
        arr.set_image_blur_filter_sigma(params.image_blur_filter_sigma);
        arr.set_image_blur_filter_coeff(params.image_blur_filter_coeff);

        arr
    }
}

const UNIFORMARRAY_SIZE: usize = 14;

#[derive(Clone, PartialEq)]
pub struct UniformArray([f32; UNIFORMARRAY_SIZE * 4]);

impl Default for UniformArray {
    fn default() -> Self {
        Self([
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ])
    }
}

impl UniformArray {
    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }

    pub fn set_scissor_mat(&mut self, mat: [f32; 12]) {
        self.0[0..12].copy_from_slice(&mat);
    }

    pub fn set_paint_mat(&mut self, mat: [f32; 12]) {
        self.0[12..24].copy_from_slice(&mat);
    }

    pub fn set_inner_col(&mut self, col: [f32; 4]) {
        self.0[24..28].copy_from_slice(&col);
    }

    pub fn set_outer_col(&mut self, col: [f32; 4]) {
        self.0[28..32].copy_from_slice(&col);
    }

    pub fn set_scissor_ext(&mut self, ext: [f32; 2]) {
        self.0[32..34].copy_from_slice(&ext);
    }

    pub fn set_scissor_scale(&mut self, scale: [f32; 2]) {
        self.0[34..36].copy_from_slice(&scale);
    }

    pub fn set_extent(&mut self, ext: [f32; 2]) {
        self.0[36..38].copy_from_slice(&ext);
    }

    pub fn set_radius(&mut self, radius: f32) {
        self.0[38] = radius;
    }

    pub fn set_feather(&mut self, feather: f32) {
        self.0[39] = feather;
    }

    pub fn set_stroke_mult(&mut self, stroke_mult: f32) {
        self.0[40] = stroke_mult;
    }

    pub fn set_stroke_thr(&mut self, stroke_thr: f32) {
        self.0[41] = stroke_thr;
    }

    pub fn set_tex_type(&mut self, tex_type: f32) {
        self.0[42] = tex_type;
    }

    pub fn set_shader_type(&mut self, shader_type: f32) {
        self.0[43] = shader_type;
    }

    pub fn set_glyph_texture_type(&mut self, glyph_texture_type: f32) {
        self.0[44] = glyph_texture_type;
    }

    pub fn set_image_blur_filter_direction(&mut self, direction: [f32; 2]) {
        self.0[46..48].copy_from_slice(&direction);
    }

    pub fn set_image_blur_filter_sigma(&mut self, sigma: f32) {
        self.0[45] = sigma;
    }

    pub fn set_image_blur_filter_coeff(&mut self, coeff: [f32; 3]) {
        self.0[48..51].copy_from_slice(&coeff);
    }
}

impl From<&Params> for UniformArray {
    fn from(params: &Params) -> Self {
        let mut arr = Self::default();

        arr.set_scissor_mat(params.scissor_mat);
        arr.set_paint_mat(params.paint_mat);
        arr.set_inner_col(params.inner_col);
        arr.set_outer_col(params.outer_col);
        arr.set_scissor_ext(params.scissor_ext);
        arr.set_scissor_scale(params.scissor_scale);
        arr.set_extent(params.extent);
        arr.set_radius(params.radius);
        arr.set_feather(params.feather);
        arr.set_stroke_mult(params.stroke_mult);
        arr.set_stroke_thr(params.stroke_thr);
        arr.set_shader_type(params.shader_type.to_f32());
        arr.set_tex_type(params.tex_type);
        arr.set_glyph_texture_type(params.glyph_texture_type as f32);
        arr.set_image_blur_filter_direction(params.image_blur_filter_direction);
        arr.set_image_blur_filter_sigma(params.image_blur_filter_sigma);
        arr.set_image_blur_filter_coeff(params.image_blur_filter_coeff);

        arr
    }
}

pub struct Image {
    texture: Rc<wgpu::Texture>,
    info: ImageInfo,
}

/// WGPU renderer.
pub struct WGPURenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,

    shader_module: Rc<wgpu::ShaderModule>,

    screen_view: [f32; 2],

    empty_texture: Rc<wgpu::Texture>,
    stencil_buffer: Option<Rc<wgpu::Texture>>,
    stencil_buffer_for_textures: HashMap<Rc<wgpu::Texture>, Rc<wgpu::Texture>>,

    bind_group_layout: Rc<wgpu::BindGroupLayout>,
    viewport_bind_group_layout: Rc<wgpu::BindGroupLayout>,
    pipeline_layout: Rc<wgpu::PipelineLayout>,
}

impl WGPURenderer {
    /// Creates a new renderer for the device.
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let module = wgpu::include_wgsl!("wgpu/shader.wgsl");
        let shader_module = Rc::new(device.create_shader_module(module));

        let texture_descriptor = wgpu::TextureDescriptor {
            size: wgpu::Extent3d::default(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: None,
            view_formats: &[],
        };
        let empty_texture = Rc::new(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("empty"),
            view_formats: &[],
            ..texture_descriptor
        }));

        queue.write_texture(
            empty_texture.as_image_copy(),
            &[255, 0, 0, 255],
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: None,
            },
            wgpu::Extent3d::default(),
        );

        let viewport_bind_group_layout = Rc::new(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout for Viewport uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        }));

        let bind_group_layout = Rc::new(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        }));

        let pipeline_layout = Rc::new(device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&viewport_bind_group_layout, &bind_group_layout],
            push_constant_ranges: &[],
        }));

        Self {
            device,
            queue,

            shader_module,

            screen_view: [0.0, 0.0],

            empty_texture,
            stencil_buffer: None,
            stencil_buffer_for_textures: HashMap::new(),
            bind_group_layout,
            viewport_bind_group_layout,
            pipeline_layout,
        }
    }
}

impl Renderer for WGPURenderer {
    type Image = Image;
    type NativeTexture = wgpu::Texture;
    type Surface = wgpu::Texture;

    fn set_size(&mut self, _width: u32, _height: u32, _dpi: f32) {}

    fn render(
        &mut self,
        surface_texture: &Self::Surface,
        images: &mut crate::image::ImageStore<Self::Image>,
        verts: &[super::Vertex],
        commands: Vec<super::Command>,
    ) {
        self.screen_view[0] = surface_texture.width() as f32;
        self.screen_view[1] = surface_texture.height() as f32;

        let texture_view = std::rc::Rc::new(surface_texture.create_view(&wgpu::TextureViewDescriptor::default()));

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Main Vertex Buffer"),
            contents: bytemuck::cast_slice(verts),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        if let Some(stencil_buffer) = &self.stencil_buffer {
            if stencil_buffer.size() != surface_texture.size() {
                self.stencil_buffer = None;
            }
        }

        let stencil_buffer = self
            .stencil_buffer
            .get_or_insert_with(|| {
                Rc::new(self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Stencil buffer"),
                    size: wgpu::Extent3d {
                        width: surface_texture.width(),
                        height: surface_texture.height(),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Stencil8,
                    view_formats: &[],
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                }))
            })
            .clone();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let mut render_pass_builder = RenderPassBuilder::new(
            self.device.clone(),
            &mut encoder,
            surface_texture.format(),
            self.screen_view,
            self.viewport_bind_group_layout.clone(),
            &mut self.stencil_buffer_for_textures,
            texture_view,
            stencil_buffer.clone(),
            vertex_buffer,
        );

        let mut pipeline_and_bindgroup_mapper = CommandToPipelineAndBindGroupMapper::new(
            self.device.clone(),
            self.empty_texture.clone(),
            self.shader_module.clone(),
            self.bind_group_layout.clone(),
            self.pipeline_layout.clone(),
        );

        let mut current_render_target = RenderTarget::Screen;

        for command in commands {
            let blend_state = wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: blend_factor(command.composite_operation.src_rgb),
                    dst_factor: blend_factor(command.composite_operation.dst_rgb),
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: blend_factor(command.composite_operation.src_alpha),
                    dst_factor: blend_factor(command.composite_operation.dst_alpha),
                    operation: wgpu::BlendOperation::Add,
                },
            };

            match command.cmd_type {
                super::CommandType::SetRenderTarget(render_target) => {
                    current_render_target = render_target;
                    //eprintln!("SRT {:?}", render_target);
                    //assert!(matches!(render_target, RenderTarget::Screen));
                    match render_target {
                        RenderTarget::Screen => {
                            render_pass_builder.set_render_target_screen();
                        }
                        RenderTarget::Image(image_id) => {
                            render_pass_builder.set_render_target_image(images, image_id, wgpu::LoadOp::Load);
                        }
                    }
                }
                super::CommandType::ClearRect { color } => {
                    let mut params = Params::new(
                        images,
                        &Default::default(),
                        &crate::paint::PaintFlavor::Color(color),
                        &Default::default(),
                        &Scissor::default(),
                        0.,
                        0.,
                        0.,
                    );
                    params.shader_type = ShaderType::FillColorUnclipped;
                    if let Some((start, count)) = command.triangles_verts {
                        pipeline_and_bindgroup_mapper.update_renderpass(
                            &mut render_pass_builder,
                            Some(wgpu::BlendState {
                                color: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::One,
                                    dst_factor: wgpu::BlendFactor::Zero,
                                    operation: wgpu::BlendOperation::Add,
                                },
                                alpha: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::One,
                                    dst_factor: wgpu::BlendFactor::Zero,
                                    operation: wgpu::BlendOperation::Add,
                                },
                            }),
                            wgpu::PrimitiveTopology::TriangleList,
                            StencilTest::Disabled, // ### clear stencil mask
                            None,
                            &params,
                            images,
                            None,
                            Default::default(),
                        );

                        render_pass_builder.draw(start as u32..(start + count) as u32);
                    }
                }
                super::CommandType::ConvexFill { ref params } => {
                    for drawable in &command.drawables {
                        if let Some((start, count)) = drawable.fill_verts {
                            pipeline_and_bindgroup_mapper.update_renderpass(
                                &mut render_pass_builder,
                                blend_state.into(),
                                wgpu::PrimitiveTopology::TriangleList,
                                StencilTest::Disabled,
                                Some(wgpu::Face::Back),
                                params,
                                images,
                                command.image.map(ImageOrTexture::Image),
                                command.glyph_texture,
                            );
                            render_pass_builder.draw(start as u32..(start + count) as u32);
                        }

                        if let Some((start, count)) = drawable.stroke_verts {
                            pipeline_and_bindgroup_mapper.update_renderpass(
                                &mut render_pass_builder,
                                blend_state.into(),
                                wgpu::PrimitiveTopology::TriangleStrip,
                                StencilTest::Disabled,
                                Some(wgpu::Face::Back),
                                params,
                                images,
                                command.image.map(ImageOrTexture::Image),
                                command.glyph_texture,
                            );
                            render_pass_builder.draw(start as u32..(start + count) as u32);
                        }
                    }
                }
                super::CommandType::ConcaveFill {
                    ref stencil_params,
                    ref fill_params,
                } => {
                    if command.drawables.iter().any(|drawable| drawable.fill_verts.is_some()) {
                        pipeline_and_bindgroup_mapper.update_renderpass(
                            &mut render_pass_builder,
                            None,
                            wgpu::PrimitiveTopology::TriangleList,
                            StencilTest::Enabled {
                                stencil_state: wgpu::StencilState {
                                    front: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::Always,
                                        fail_op: wgpu::StencilOperation::Keep,
                                        depth_fail_op: wgpu::StencilOperation::Keep,
                                        pass_op: wgpu::StencilOperation::IncrementWrap,
                                    },
                                    back: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::Always,
                                        fail_op: wgpu::StencilOperation::Keep,
                                        depth_fail_op: wgpu::StencilOperation::Keep,
                                        pass_op: wgpu::StencilOperation::DecrementWrap,
                                    },
                                    read_mask: !0,
                                    write_mask: !0,
                                },
                                stencil_reference: 0,
                            },
                            None,
                            stencil_params,
                            images,
                            None,
                            GlyphTexture::None,
                        );

                        for drawable in &command.drawables {
                            if let Some((start, count)) = drawable.fill_verts {
                                render_pass_builder.draw(start as u32..(start + count) as u32);
                            }
                        }
                    }

                    if command.drawables.iter().any(|drawable| drawable.stroke_verts.is_some()) {
                        for drawable in &command.drawables {
                            // draw fringes
                            pipeline_and_bindgroup_mapper.update_renderpass(
                                &mut render_pass_builder,
                                blend_state.into(),
                                wgpu::PrimitiveTopology::TriangleStrip,
                                StencilTest::Enabled {
                                    stencil_state: wgpu::StencilState {
                                        front: wgpu::StencilFaceState {
                                            compare: wgpu::CompareFunction::Equal,
                                            fail_op: wgpu::StencilOperation::Keep,
                                            depth_fail_op: wgpu::StencilOperation::Keep,
                                            pass_op: wgpu::StencilOperation::Keep,
                                        },
                                        back: wgpu::StencilFaceState {
                                            compare: wgpu::CompareFunction::Equal,
                                            fail_op: wgpu::StencilOperation::Keep,
                                            depth_fail_op: wgpu::StencilOperation::Keep,
                                            pass_op: wgpu::StencilOperation::Keep,
                                        },
                                        read_mask: match command.fill_rule {
                                            FillRule::NonZero => 0xff,
                                            FillRule::EvenOdd => 0x1,
                                        },
                                        write_mask: match command.fill_rule {
                                            FillRule::NonZero => 0xff,
                                            FillRule::EvenOdd => 0x1,
                                        },
                                    },
                                    stencil_reference: 0,
                                },
                                Some(wgpu::Face::Back),
                                fill_params,
                                images,
                                command.image.map(ImageOrTexture::Image),
                                command.glyph_texture,
                            );

                            if let Some((start, count)) = drawable.stroke_verts {
                                render_pass_builder.draw(start as u32..(start + count) as u32);
                            }
                        }
                    }

                    if let Some((start, count)) = command.triangles_verts {
                        pipeline_and_bindgroup_mapper.update_renderpass(
                            &mut render_pass_builder,
                            blend_state.into(),
                            wgpu::PrimitiveTopology::TriangleStrip,
                            StencilTest::Enabled {
                                stencil_state: wgpu::StencilState {
                                    front: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::NotEqual,
                                        fail_op: wgpu::StencilOperation::Zero,
                                        depth_fail_op: wgpu::StencilOperation::Zero,
                                        pass_op: wgpu::StencilOperation::Zero,
                                    },
                                    back: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::NotEqual,
                                        fail_op: wgpu::StencilOperation::Zero,
                                        depth_fail_op: wgpu::StencilOperation::Zero,
                                        pass_op: wgpu::StencilOperation::Zero,
                                    },
                                    read_mask: match command.fill_rule {
                                        FillRule::NonZero => 0xff,
                                        FillRule::EvenOdd => 0x1,
                                    },
                                    write_mask: match command.fill_rule {
                                        FillRule::NonZero => 0xff,
                                        FillRule::EvenOdd => 0x1,
                                    },
                                },
                                stencil_reference: 0,
                            },
                            Some(wgpu::Face::Back),
                            fill_params,
                            images,
                            command.image.map(ImageOrTexture::Image),
                            command.glyph_texture,
                        );
                        render_pass_builder.draw(start as u32..(start + count) as u32);
                    }
                }
                super::CommandType::Stroke { params } => {
                    for drawable in &command.drawables {
                        if let Some((start, count)) = drawable.stroke_verts {
                            pipeline_and_bindgroup_mapper.update_renderpass(
                                &mut render_pass_builder,
                                blend_state.into(),
                                wgpu::PrimitiveTopology::TriangleStrip,
                                StencilTest::Disabled,
                                Some(wgpu::Face::Back),
                                &params,
                                images,
                                command.image.map(ImageOrTexture::Image),
                                command.glyph_texture,
                            );
                            render_pass_builder.draw(start as u32..(start + count) as u32);
                        }
                    }
                }
                super::CommandType::StencilStroke { params1, params2 } => {
                    if command
                        .drawables
                        .iter()
                        .any(|drawable: &super::Drawable| drawable.stroke_verts.is_some())
                    {
                        // Fill the stroke base without overlap

                        pipeline_and_bindgroup_mapper.update_renderpass(
                            &mut render_pass_builder,
                            blend_state.into(),
                            wgpu::PrimitiveTopology::TriangleStrip,
                            StencilTest::Enabled {
                                stencil_state: wgpu::StencilState {
                                    front: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::Equal,
                                        fail_op: wgpu::StencilOperation::Keep,
                                        depth_fail_op: wgpu::StencilOperation::Keep,
                                        pass_op: wgpu::StencilOperation::IncrementClamp,
                                    },
                                    back: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::Equal,
                                        fail_op: wgpu::StencilOperation::Keep,
                                        depth_fail_op: wgpu::StencilOperation::Keep,
                                        pass_op: wgpu::StencilOperation::IncrementClamp,
                                    },
                                    read_mask: !0,
                                    write_mask: !0,
                                },
                                stencil_reference: 0,
                            },
                            Some(wgpu::Face::Back),
                            &params2,
                            images,
                            command.image.map(ImageOrTexture::Image),
                            command.glyph_texture,
                        );

                        for drawable in &command.drawables {
                            if let Some((start, count)) = drawable.stroke_verts {
                                render_pass_builder.draw(start as u32..(start + count) as u32);
                            }
                        }

                        // Draw anti-aliased pixels.

                        pipeline_and_bindgroup_mapper.update_renderpass(
                            &mut render_pass_builder,
                            blend_state.into(),
                            wgpu::PrimitiveTopology::TriangleStrip,
                            StencilTest::Enabled {
                                stencil_state: wgpu::StencilState {
                                    front: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::Equal,
                                        fail_op: wgpu::StencilOperation::Keep,
                                        depth_fail_op: wgpu::StencilOperation::Keep,
                                        pass_op: wgpu::StencilOperation::Keep,
                                    },
                                    back: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::Equal,
                                        fail_op: wgpu::StencilOperation::Keep,
                                        depth_fail_op: wgpu::StencilOperation::Keep,
                                        pass_op: wgpu::StencilOperation::Keep,
                                    },
                                    read_mask: !0,
                                    write_mask: !0,
                                },
                                stencil_reference: 0,
                            },
                            Some(wgpu::Face::Back),
                            &params1,
                            images,
                            command.image.map(ImageOrTexture::Image),
                            command.glyph_texture,
                        );

                        for drawable in &command.drawables {
                            if let Some((start, count)) = drawable.stroke_verts {
                                render_pass_builder.draw(start as u32..(start + count) as u32);
                            }
                        }

                        // clear stencil buffer

                        pipeline_and_bindgroup_mapper.update_renderpass(
                            &mut render_pass_builder,
                            None,
                            wgpu::PrimitiveTopology::TriangleStrip,
                            StencilTest::Enabled {
                                stencil_state: wgpu::StencilState {
                                    front: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::Always,
                                        fail_op: wgpu::StencilOperation::Zero,
                                        depth_fail_op: wgpu::StencilOperation::Zero,
                                        pass_op: wgpu::StencilOperation::Zero,
                                    },
                                    back: wgpu::StencilFaceState {
                                        compare: wgpu::CompareFunction::Always,
                                        fail_op: wgpu::StencilOperation::Zero,
                                        depth_fail_op: wgpu::StencilOperation::Zero,
                                        pass_op: wgpu::StencilOperation::Zero,
                                    },
                                    read_mask: !0,
                                    write_mask: !0,
                                },
                                stencil_reference: 0,
                            },
                            Some(wgpu::Face::Back),
                            &params1,
                            images,
                            command.image.map(ImageOrTexture::Image),
                            command.glyph_texture,
                        );

                        for drawable in &command.drawables {
                            if let Some((start, count)) = drawable.stroke_verts {
                                render_pass_builder.draw(start as u32..(start + count) as u32);
                            }
                        }
                    }
                }
                super::CommandType::Triangles { ref params } => {
                    if let Some((start, count)) = command.triangles_verts {
                        pipeline_and_bindgroup_mapper.update_renderpass(
                            &mut render_pass_builder,
                            blend_state.into(),
                            wgpu::PrimitiveTopology::TriangleList,
                            StencilTest::Disabled,
                            Some(wgpu::Face::Back),
                            params,
                            images,
                            command.image.map(ImageOrTexture::Image),
                            command.glyph_texture,
                        );
                        render_pass_builder.draw(start as u32..(start + count) as u32);
                    }
                }
                super::CommandType::RenderFilteredImage { target_image, filter } => {
                    match filter {
                        crate::ImageFilter::GaussianBlur { sigma } => {
                            let previous_render_target = current_render_target;

                            let source_image = images.get(command.image.unwrap()).unwrap();

                            let image_paint = crate::Paint::image(
                                command.image.unwrap(),
                                0.,
                                0.,
                                source_image.texture.width() as _,
                                source_image.texture.height() as _,
                                0.,
                                1.,
                            );

                            let mut blur_params = Params::new(
                                images,
                                &Default::default(),
                                &image_paint.flavor,
                                &Default::default(),
                                &Scissor::default(),
                                0.,
                                0.,
                                0.,
                            );
                            blur_params.shader_type = ShaderType::FilterImage;

                            let gauss_coeff_x = 1. / ((2. * std::f32::consts::PI).sqrt() * sigma);
                            let gauss_coeff_y = f32::exp(-0.5 / (sigma * sigma));
                            let gauss_coeff_z = gauss_coeff_y * gauss_coeff_y;

                            blur_params.image_blur_filter_coeff[0] = gauss_coeff_x;
                            blur_params.image_blur_filter_coeff[1] = gauss_coeff_y;
                            blur_params.image_blur_filter_coeff[2] = gauss_coeff_z;

                            blur_params.image_blur_filter_direction = [1.0, 0.0];
                            // GLES 2.0 does not allow non-constant loop indices, so limit the standard devitation to allow for a upper fixed limit
                            // on the number of iterations in the fragment shader.
                            blur_params.image_blur_filter_sigma = sigma.min(8.);

                            let horizontal_blur_buffer =
                                Rc::new(self.device.create_texture(&wgpu::TextureDescriptor {
                                    label: Some("blur horizontal"),
                                    size: wgpu::Extent3d {
                                        width: source_image.texture.width(),
                                        height: source_image.texture.height(),
                                        depth_or_array_layers: 1,
                                    },
                                    mip_level_count: 1,
                                    sample_count: 1,
                                    dimension: wgpu::TextureDimension::D2,
                                    format: source_image.texture.format(),
                                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                                        | wgpu::TextureUsages::RENDER_ATTACHMENT,
                                    view_formats: &[],
                                }));

                            render_pass_builder.set_render_target_texture(
                                &horizontal_blur_buffer,
                                None,
                                wgpu::LoadOp::Clear(wgpu::Color::default()),
                            );

                            if let Some((start, count)) = command.triangles_verts {
                                pipeline_and_bindgroup_mapper.update_renderpass(
                                    &mut render_pass_builder,
                                    blend_state.into(),
                                    wgpu::PrimitiveTopology::TriangleList,
                                    StencilTest::Disabled,
                                    Some(wgpu::Face::Back),
                                    &blur_params,
                                    images,
                                    command.image.map(ImageOrTexture::Image),
                                    command.glyph_texture,
                                );
                                render_pass_builder.draw(start as u32..(start + count) as u32);
                            }

                            render_pass_builder.set_render_target_image(
                                images,
                                target_image,
                                wgpu::LoadOp::Clear(wgpu::Color::default()),
                            );

                            blur_params.image_blur_filter_direction = [0.0, 1.0];

                            if let Some((start, count)) = command.triangles_verts {
                                pipeline_and_bindgroup_mapper.update_renderpass(
                                    &mut render_pass_builder,
                                    blend_state.into(),
                                    wgpu::PrimitiveTopology::TriangleList,
                                    StencilTest::Disabled,
                                    Some(wgpu::Face::Back),
                                    &blur_params,
                                    images,
                                    Some(ImageOrTexture::Texture(horizontal_blur_buffer)),
                                    command.glyph_texture,
                                );
                                render_pass_builder.draw(start as u32..(start + count) as u32);
                            }

                            current_render_target = previous_render_target;
                            match current_render_target {
                                RenderTarget::Screen => {
                                    render_pass_builder.set_render_target_screen();
                                }
                                RenderTarget::Image(image_id) => {
                                    render_pass_builder.set_render_target_image(images, image_id, wgpu::LoadOp::Load);
                                }
                            }
                        }
                    }
                }
            }

            //            render_pass_builder.finish();
        }

        drop(render_pass_builder);

        self.queue.submit(Some(encoder.finish()));
    }

    fn alloc_image(&mut self, info: crate::ImageInfo) -> Result<Self::Image, crate::ErrorKind> {
        Ok(Image {
            texture: Rc::new(self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: info.width() as u32,
                    height: info.height() as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: match info.format() {
                    crate::PixelFormat::Rgb8 => wgpu::TextureFormat::Rgba8Unorm,
                    crate::PixelFormat::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
                    crate::PixelFormat::Gray8 => wgpu::TextureFormat::R8Unorm,
                },
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })),
            info,
        })
    }

    fn create_image_from_native_texture(
        &mut self,
        native_texture: Self::NativeTexture,
        info: crate::ImageInfo,
    ) -> Result<Self::Image, crate::ErrorKind> {
        Ok(Image {
            texture: Rc::new(native_texture),
            info,
        })
    }

    fn update_image(
        &mut self,
        image: &mut Self::Image,
        data: crate::ImageSource,
        x: usize,
        y: usize,
    ) -> Result<(), crate::ErrorKind> {
        #[cfg(target_arch = "wasm32")]
        if let crate::ImageSource::HtmlImageElement(htmlimage) = data {
            self.queue.copy_external_image_to_texture(
                &wgpu::ImageCopyExternalImage {
                    source: wgpu::ExternalImageSource::HTMLImageElement(htmlimage.clone()),
                    origin: wgpu::Origin2d::ZERO,
                    flip_y: false,
                },
                wgpu::ImageCopyTextureTagged {
                    texture: &image.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                    color_space: wgpu::PredefinedColorSpace::Srgb,
                    premultiplied_alpha: true,
                },
                wgpu::Extent3d {
                    width: data.dimensions().width as _,
                    height: data.dimensions().height as _,
                    depth_or_array_layers: 1,
                },
            );
        }

        use rgb::ComponentBytes;

        let converted_rgba;
        let (bytes, bpp) = match data {
            crate::ImageSource::Rgb(img) => {
                converted_rgba = img
                    .pixels()
                    .map(|rgb| rgb::Rgba {
                        r: rgb.r,
                        g: rgb.g,
                        b: rgb.b,
                        a: 255,
                    })
                    .collect::<Vec<_>>();
                (converted_rgba.as_bytes(), 4)
            }
            crate::ImageSource::Rgba(img) => (img.buf().as_bytes(), 4),
            crate::ImageSource::Gray(img) => (img.buf().as_bytes(), 1),
            #[cfg(target_arch = "wasm32")]
            crate::ImageSource::HtmlImageElement(..) => {
                unreachable!()
            }
        };

        let mut target = image.texture.as_image_copy();
        target.origin.x = x as _;
        target.origin.y = y as _;

        self.queue.write_texture(
            target,
            bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bpp * image.texture.width()),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: data.dimensions().width as _,
                height: data.dimensions().height as _,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    fn delete_image(&mut self, image: Self::Image, _image_id: crate::ImageId) {
        self.stencil_buffer_for_textures.remove(&image.texture);
        drop(image);
    }

    fn screenshot(&mut self) -> Result<imgref::ImgVec<rgb::RGBA8>, crate::ErrorKind> {
        return Err(crate::ErrorKind::UnsupportedOperation);
    }
}

#[derive(Clone, PartialEq, Debug)]
enum StencilTest {
    Disabled,
    Enabled {
        stencil_state: wgpu::StencilState,
        stencil_reference: u32,
    },
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct PipelineState {
    shader_type: ShaderType,
    enable_glyph_texture: bool,
    render_to_texture: bool,
    color_target_state: wgpu::ColorTargetState,
    primitive_topology: wgpu::PrimitiveTopology,
    cull_mode: Option<wgpu::Face>,
    stencil_state: Option<wgpu::StencilState>,
}

impl PipelineState {
    fn new(
        color_blend: Option<wgpu::BlendState>,
        stencil_test: StencilTest,
        format: wgpu::TextureFormat,
        shader_type: ShaderType,
        enable_glyph_texture: bool,
        render_to_texture: bool,
        primitive_topology: wgpu::PrimitiveTopology,
        cull_mode: Option<wgpu::Face>,
        has_stencil_buffer: bool,
    ) -> Self {
        let (stencil_state, color_target_state) = match &stencil_test {
            StencilTest::Enabled { stencil_state, .. } => (
                stencil_state.clone(),
                wgpu::ColorTargetState {
                    format,
                    blend: color_blend,
                    write_mask: if color_blend.is_some() {
                        wgpu::ColorWrites::ALL
                    } else {
                        wgpu::ColorWrites::empty()
                    },
                },
            ),
            StencilTest::Disabled => (
                wgpu::StencilState {
                    front: wgpu::StencilFaceState::IGNORE,
                    back: wgpu::StencilFaceState::IGNORE,
                    read_mask: !0,
                    write_mask: !0,
                },
                wgpu::ColorTargetState {
                    format,
                    blend: color_blend,
                    write_mask: wgpu::ColorWrites::ALL,
                },
            ),
        };
        Self {
            shader_type,
            enable_glyph_texture,
            render_to_texture,
            color_target_state,
            primitive_topology,
            cull_mode,
            stencil_state: has_stencil_buffer.then_some(stencil_state),
        }
    }

    fn materialize(
        &self,
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        shader_module: &wgpu::ShaderModule,
    ) -> wgpu::RenderPipeline {
        let constants = HashMap::from([
            ("shader_type".to_string(), self.shader_type.to_f32() as f64),
            (
                "enable_glyph_texture".to_string(),
                if self.enable_glyph_texture { 1.0 } else { 0.0 },
            ),
            (
                "render_to_texture".to_string(),
                if self.render_to_texture { 1.0 } else { 0. },
            ),
        ]);

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader_module,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                }],
                compilation_options: PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
            },
            fragment: Some(wgpu::FragmentState {
                module: shader_module,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                targets: &[Some(self.color_target_state.clone())],
            }),
            primitive: wgpu::PrimitiveState {
                topology: self.primitive_topology,
                front_face: if self.render_to_texture {
                    wgpu::FrontFace::Cw
                } else {
                    wgpu::FrontFace::Ccw
                },
                cull_mode: self.cull_mode,
                ..Default::default()
            },
            depth_stencil: self
                .stencil_state
                .as_ref()
                .map(|stencil_state| wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Stencil8,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: stencil_state.clone(),
                    bias: Default::default(),
                }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }
}

#[derive(Clone, PartialEq)]
enum ImageOrTexture {
    Image(ImageId),
    Texture(Rc<wgpu::Texture>),
}

#[derive(Clone, PartialEq)]
struct BindGroupState {
    image: Option<ImageOrTexture>,
    glyph_texture: GlyphTexture,
    uniforms: UniformParams,
}

impl BindGroupState {
    fn materialize(
        &self,
        device: &wgpu::Device,
        images: &ImageStore<Image>,
        bind_group_layout: &wgpu::BindGroupLayout,
        empty_texture: &Rc<wgpu::Texture>,
    ) -> wgpu::BindGroup {
        /*
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fragment Uniform Buffer"),
            contents: bytemuck::cast_slice(self.uniforms.as_slice()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        */

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fragment Uniform Buffer"),
            size: encase::ShaderType::size(&self.uniforms).get(),
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        let mut buffer = uniform_buf.slice(..).get_mapped_range_mut();
        encase::UniformBuffer::new(buffer.as_mut())
            .write(&self.uniforms)
            .unwrap();
        drop(buffer);

        let (main_texture_view, main_sampler) =
            RenderPassBuilder::create_texture_view_and_sampler(device, images, self.image.as_ref(), empty_texture);
        let (glyph_texture_view, glyph_sampler) = RenderPassBuilder::create_texture_view_and_sampler(
            device,
            images,
            self.glyph_texture.image_id().map(ImageOrTexture::Image).as_ref(),
            empty_texture,
        );

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&main_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&main_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&glyph_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&glyph_sampler),
                },
            ],
            label: None,
        })
    }
}

struct RenderPassBuilder<'a> {
    device: Arc<wgpu::Device>,
    encoder: &'a mut wgpu::CommandEncoder,
    surface_view: std::rc::Rc<wgpu::TextureView>,
    surface_format: wgpu::TextureFormat,
    texture_view: std::rc::Rc<wgpu::TextureView>,
    stencil_buffer: Option<Rc<wgpu::Texture>>,
    viewport: [f32; 2],
    vertex_buffer: wgpu::Buffer,
    rendering_to_texture: bool,
    viewport_bind_group_layout: Rc<wgpu::BindGroupLayout>,
    current_bind_group_state: Option<BindGroupState>,
    rpass: Option<wgpu::RenderPass<'a>>,
    screen_stencil_buffer: Rc<wgpu::Texture>,
    screen_view: [f32; 2],
    screen_surface_format: wgpu::TextureFormat,
    stencil_buffer_for_textures: &'a mut HashMap<Rc<wgpu::Texture>, Rc<wgpu::Texture>>,
    viewport_bind_group: wgpu::BindGroup,
}

impl<'a> RenderPassBuilder<'a> {
    fn new(
        device: Arc<wgpu::Device>,
        encoder: &'a mut wgpu::CommandEncoder,
        screen_surface_format: wgpu::TextureFormat,
        screen_view: [f32; 2],
        viewport_bind_group_layout: Rc<wgpu::BindGroupLayout>,
        stencil_buffer_for_textures: &'a mut HashMap<Rc<wgpu::Texture>, Rc<wgpu::Texture>>,
        texture_view: Rc<wgpu::TextureView>,
        stencil_buffer: Rc<wgpu::Texture>,
        vertex_buffer: wgpu::Buffer,
    ) -> Self {
        let viewport_bind_group = Self::create_viewport_bind_group(&device, &screen_view, &viewport_bind_group_layout);
        Self {
            device: device.clone(),
            encoder,
            surface_view: texture_view.clone(),
            surface_format: screen_surface_format,
            texture_view,
            stencil_buffer: Some(stencil_buffer.clone()),
            viewport: screen_view,
            vertex_buffer,
            rendering_to_texture: false,
            viewport_bind_group_layout,
            current_bind_group_state: None,
            rpass: None,
            screen_stencil_buffer: stencil_buffer,
            screen_view,
            screen_surface_format,
            stencil_buffer_for_textures,
            viewport_bind_group,
        }
    }

    fn set_viewport(&mut self, viewport: [f32; 2]) {
        if self.viewport == viewport {
            return;
        }
        self.viewport = viewport;
        self.viewport_bind_group =
            Self::create_viewport_bind_group(&self.device, &self.viewport, &self.viewport_bind_group_layout);
    }

    fn create_viewport_bind_group(
        device: &wgpu::Device,
        viewport: &[f32; 2],
        viewport_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        let view_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Uniform Buffer for Viewport"),
            contents: bytemuck::cast_slice(viewport),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: viewport_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buf.as_entire_binding(),
            }],
            label: None,
        });

        viewport_bind_group
    }

    fn create_texture_view_and_sampler(
        device: &wgpu::Device,
        images: &ImageStore<Image>,
        image: Option<&ImageOrTexture>,
        empty_texture: &Rc<wgpu::Texture>,
    ) -> (wgpu::TextureView, wgpu::Sampler) {
        let texture_and_flags = image.and_then(|image_or_texture| match image_or_texture {
            ImageOrTexture::Image(image_id) => images.get(*image_id).map(|img| (img.texture.clone(), img.info.flags())),
            ImageOrTexture::Texture(texture) => Some((texture.clone(), crate::ImageFlags::empty())),
        });
        let texture_view = texture_and_flags
            .as_ref()
            .map_or_else(|| empty_texture, |(texture, _)| texture)
            .create_view(&Default::default());

        let flags = texture_and_flags.map_or(crate::ImageFlags::empty(), |(_, flags)| flags);

        let filter_mode = if flags.contains(crate::ImageFlags::NEAREST) {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        };

        // ### Share
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: if flags.contains(crate::ImageFlags::REPEAT_X) {
                wgpu::AddressMode::Repeat
            } else {
                wgpu::AddressMode::ClampToEdge
            },
            address_mode_v: if flags.contains(crate::ImageFlags::REPEAT_Y) {
                wgpu::AddressMode::Repeat
            } else {
                wgpu::AddressMode::ClampToEdge
            },
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: filter_mode,
            min_filter: filter_mode,
            ..Default::default()
        });
        (texture_view, sampler)
    }

    fn set_render_target_texture(
        &mut self,
        texture: &wgpu::Texture,
        stencil_buffer: Option<Rc<wgpu::Texture>>,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        self.texture_view = std::rc::Rc::new(texture.create_view(&Default::default()));
        self.set_viewport([texture.width() as f32, texture.height() as f32]);
        self.stencil_buffer = stencil_buffer;
        self.surface_format = texture.format();
        self.rendering_to_texture = true;

        self.recreate_render_pass(load);
    }

    fn set_render_target_image(
        &mut self,
        images: &mut ImageStore<Image>,
        image_id: ImageId,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let image = images.get(image_id).unwrap();

        let stencil_buffer = self
            .stencil_buffer_for_textures
            .entry(image.texture.clone())
            .or_insert_with(|| {
                Rc::new(self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Stencil buffer"),
                    size: wgpu::Extent3d {
                        width: image.texture.width(),
                        height: image.texture.height(),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Stencil8,
                    view_formats: &[],
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                }))
            })
            .clone();

        self.set_render_target_texture(&image.texture.clone(), Some(stencil_buffer), load);
    }

    fn set_render_target_screen(&mut self) {
        self.texture_view = self.surface_view.clone();
        self.stencil_buffer = Some(self.screen_stencil_buffer.clone());
        self.set_viewport(self.screen_view);
        self.surface_format = self.screen_surface_format;
        self.rendering_to_texture = false;

        self.recreate_render_pass(wgpu::LoadOp::Load);
    }

    fn recreate_render_pass(&mut self, load: wgpu::LoadOp<wgpu::Color>) {
        drop(self.rpass.take());
        let stencil_view = self
            .stencil_buffer
            .as_ref()
            .map(|buffer| buffer.create_view(&Default::default()));

        let mut rpass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: stencil_view
                .as_ref()
                .map(|view| wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_viewport(0., 0., self.viewport[0], self.viewport[1], 0., 0.);
        self.current_bind_group_state.take();
        rpass.set_bind_group(0, &self.viewport_bind_group, &[]);
        self.rpass = Some(rpass.forget_lifetime());
    }

    fn draw(&mut self, vertices: std::ops::Range<u32>) {
        self.rpass.as_mut().unwrap().draw(vertices, 0..1);
    }
}

struct CommandToPipelineAndBindGroupMapper {
    device: Arc<wgpu::Device>,
    empty_texture: Rc<wgpu::Texture>,
    shader_module: Rc<wgpu::ShaderModule>,

    current_bind_group_state: Option<BindGroupState>,
    current_bind_group: Option<wgpu::BindGroup>,
    bind_group_layout: Rc<wgpu::BindGroupLayout>,
    current_pipeline_state: Option<PipelineState>,
    pipeline_cache: HashMap<PipelineState, wgpu::RenderPipeline>,
    pipeline_layout: Rc<wgpu::PipelineLayout>,
}

impl CommandToPipelineAndBindGroupMapper {
    fn new(
        device: Arc<wgpu::Device>,
        empty_texture: Rc<wgpu::Texture>,
        shader_module: Rc<wgpu::ShaderModule>,
        bind_group_layout: Rc<wgpu::BindGroupLayout>,
        pipeline_layout: Rc<wgpu::PipelineLayout>,
    ) -> Self {
        Self {
            device: device.clone(),
            empty_texture,
            shader_module,
            current_bind_group_state: None,
            current_bind_group: None,
            bind_group_layout,
            current_pipeline_state: None,
            pipeline_cache: Default::default(),
            pipeline_layout,
        }
    }

    fn update_renderpass<'a>(
        &mut self,
        render_pass_builder: &'a mut RenderPassBuilder<'_>,
        color_blend: Option<wgpu::BlendState>,
        primitive_topology: wgpu::PrimitiveTopology,
        stencil_test: StencilTest,
        cull_mode: Option<wgpu::Face>,
        params: &Params,
        images: &'a ImageStore<Image>,
        image: Option<ImageOrTexture>,
        glyph_texture: GlyphTexture,
    ) {
        let render_pass = render_pass_builder.rpass.as_mut().unwrap();

        if let StencilTest::Enabled { stencil_reference, .. } = &stencil_test {
            render_pass.set_stencil_reference(*stencil_reference);
        } else {
            render_pass.set_stencil_reference(0);
        }

        let bind_group_state = BindGroupState {
            image,
            glyph_texture,
            uniforms: UniformParams::from(params),
        };

        if self.current_bind_group_state != Some(bind_group_state.clone()) {
            self.current_bind_group = bind_group_state
                .materialize(&self.device, images, &self.bind_group_layout, &self.empty_texture)
                .into();
            self.current_bind_group_state = Some(bind_group_state);
        }
        render_pass.set_bind_group(1, self.current_bind_group.as_ref().unwrap(), &[]);

        let pipeline_state = PipelineState::new(
            color_blend,
            stencil_test,
            render_pass_builder.surface_format,
            params.shader_type,
            params.uses_glyph_texture(),
            render_pass_builder.rendering_to_texture,
            primitive_topology,
            cull_mode,
            render_pass_builder.stencil_buffer.is_some(),
        );

        if self.current_pipeline_state.as_ref() != Some(&pipeline_state) {
            self.current_pipeline_state = Some(pipeline_state.clone());
            let render_pipeline = self.pipeline_cache.entry(pipeline_state.clone()).or_insert_with(|| {
                pipeline_state.materialize(&self.device, &self.pipeline_layout, &self.shader_module)
            });

            render_pass.set_pipeline(&render_pipeline);
        }
    }
}

fn blend_factor(factor: BlendFactor) -> wgpu::BlendFactor {
    match factor {
        BlendFactor::Zero => wgpu::BlendFactor::Zero,
        BlendFactor::One => wgpu::BlendFactor::One,
        BlendFactor::SrcColor => wgpu::BlendFactor::Src,
        BlendFactor::OneMinusSrcColor => wgpu::BlendFactor::OneMinusSrc,
        BlendFactor::DstColor => wgpu::BlendFactor::Dst,
        BlendFactor::OneMinusDstColor => wgpu::BlendFactor::OneMinusDst,
        BlendFactor::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
        BlendFactor::OneMinusSrcAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
        BlendFactor::DstAlpha => wgpu::BlendFactor::DstAlpha,
        BlendFactor::OneMinusDstAlpha => wgpu::BlendFactor::OneMinusDstAlpha,
        BlendFactor::SrcAlphaSaturate => wgpu::BlendFactor::SrcAlphaSaturated,
    }
}