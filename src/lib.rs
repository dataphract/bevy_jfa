//! A Bevy plugin for generating outlines using the jump flooding algorithm (JFA).
//!
//! Adapted from "The Quest for Very Wide Outlines" by Ben Golus.
//! https://bgolus.medium.com/the-quest-for-very-wide-outlines-ba82ed442cd9

use std::{borrow::Cow, mem, num::NonZeroU64, ops::Deref};

use bevy::{
    app::prelude::*,
    asset::{Assets, Handle, HandleUntyped},
    core::FloatOrd,
    core_pipeline::{node::MAIN_PASS_DEPENDENCIES, Opaque3d, Transparent3d},
    ecs::{
        prelude::*,
        system::{lifetimeless::SRes, SystemParamItem},
    },
    math::prelude::*,
    pbr::{
        DrawMesh, MeshPipeline, MeshPipelineKey, MeshUniform, SetMeshBindGroup,
        SetMeshViewBindGroup,
    },
    reflect::TypeUuid,
    render::{
        camera::{ActiveCameras, CameraPlugin, ExtractedCameraNames},
        prelude::*,
        render_asset::RenderAssets,
        render_graph::{
            Node, NodeRunError, RenderGraph, RenderGraphContext, SlotInfo, SlotType, SlotValue,
        },
        render_phase::{
            AddRenderCommand, CachedPipelinePhaseItem, DrawFunctionId, DrawFunctions,
            EntityPhaseItem, PhaseItem, RenderCommand, RenderCommandResult, RenderPhase,
            SetItemPipeline, TrackedRenderPass,
        },
        render_resource::{
            std140::{AsStd140, DynamicUniform, Std140},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        texture::{CachedTexture, TextureCache},
        view::{ExtractedView, ExtractedWindows, VisibleEntities},
        RenderApp, RenderStage,
    },
    transform::components::GlobalTransform,
    window::WindowId,
};
use coords::CoordsNode;
use jfa::JfaNode;
use stencil::{MeshStencilNode, MeshStencilPipeline};

mod coords;
mod jfa;
mod stencil;

#[derive(Default)]
pub struct OutlinePlugin;

pub struct OutlineResources {
    // Stencil target for initial stencil pass.
    stencil_output: CachedTexture,

    sampler: Sampler,
    jfa_bind_group_layout: BindGroupLayout,
    // Dynamic uniform buffer containing power-of-two JFA distances from 1 to 32768.
    // TODO: use instance ID instead?
    jfa_distance_buffer: UniformVec<DynamicUniform<jfa::JumpDist>>,
    jfa_distance_offsets: Vec<u32>,
    jfa_dims_buffer: UniformVec<jfa::Dimensions>,

    // Bind group for jump flood passes targeting the primary output.
    jfa_primary_bind_group: BindGroup,
    // Primary jump flood output.
    jfa_primary_output: CachedTexture,

    // Bind group for jump flood passes targeting the secondary output.
    jfa_secondary_bind_group: BindGroup,
    // Secondary jump flood output.
    jfa_secondary_output: CachedTexture,

    outline_bind_group: BindGroup,
}

fn create_jfa_bind_group(
    device: &RenderDevice,
    layout: &BindGroupLayout,
    label: &str,
    dist_buffer: BindingResource,
    dims_buffer: BindingResource,
    input: &TextureView,
    sampler: &Sampler,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: dist_buffer,
            },
            BindGroupEntry {
                binding: 1,
                resource: dims_buffer,
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(input),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    })
}

impl OutlineResources {
    fn create_jfa_bind_group(
        &self,
        device: &RenderDevice,
        label: &str,
        input: &TextureView,
    ) -> BindGroup {
        create_jfa_bind_group(
            device,
            &self.jfa_bind_group_layout,
            label,
            self.jfa_distance_buffer.binding().unwrap(),
            self.jfa_dims_buffer.binding().unwrap(),
            input,
            &self.sampler,
        )
    }
}

impl FromWorld for OutlineResources {
    fn from_world(world: &mut World) -> Self {
        let size = Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };

        let device = world.get_resource::<RenderDevice>().unwrap().clone();
        let queue = world.get_resource::<RenderQueue>().unwrap().clone();
        let mut textures = world.get_resource_mut::<TextureCache>().unwrap();

        let stencil_desc = tex_desc(
            "outline_stencil_output",
            size,
            TextureFormat::Depth24PlusStencil8,
        );
        let stencil_output = textures.get(&device, stencil_desc);

        let jfa_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("outline_jfa_bind_group_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: BufferSize::new(
                            jfa::JumpDist::std140_size_static() as u64
                        ),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(
                            <jfa::Dimensions as AsStd140>::std140_size_static()
                                .try_into()
                                .unwrap(),
                        ),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let mut jfa_distance_buffer = UniformVec::default();
        let mut jfa_distance_offsets = Vec::new();
        for exp in 0_u32..16 {
            let ofs = jfa_distance_buffer.push(DynamicUniform(jfa::JumpDist {
                dist: 2_u32.pow(exp),
            }));
            // FIXME this calculation is fragile
            jfa_distance_offsets.push(256 * ofs as u32);
        }
        jfa_distance_buffer.write_buffer(&device, &queue);

        let dims = jfa::Dimensions {
            width: size.width,
            height: size.height,
        };
        let mut jfa_dims_buffer = UniformVec::default();
        jfa_dims_buffer.push(dims);
        jfa_dims_buffer.write_buffer(&device, &queue);

        // TODO: use Rg16Snorm if supported for higher precision
        let jfa_primary_output_desc =
            tex_desc("outline_jfa_primary_output", size, TextureFormat::Rg16Snorm);
        let jfa_primary_output = textures.get(&device, jfa_primary_output_desc);
        let jfa_secondary_output_desc = tex_desc(
            "outline_jfa_secondary_output",
            size,
            TextureFormat::Rg16Snorm,
        );
        let jfa_secondary_output = textures.get(&device, jfa_secondary_output_desc);

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("outline_jfa_sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            compare: None,
            ..Default::default()
        });
        let jfa_primary_bind_group = create_jfa_bind_group(
            &device,
            &jfa_bind_group_layout,
            "outline_jfa_primary_bind_group",
            jfa_distance_buffer.binding().unwrap(),
            jfa_dims_buffer.binding().unwrap(),
            &jfa_secondary_output.default_view,
            &sampler,
        );
        let jfa_secondary_bind_group = create_jfa_bind_group(
            &device,
            &jfa_bind_group_layout,
            "outline_jfa_secondary_bind_group",
            jfa_distance_buffer.binding().unwrap(),
            jfa_dims_buffer.binding().unwrap(),
            &jfa_primary_output.default_view,
            &sampler,
        );

        let outline_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("outline_outline_bind_group_layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            sample_type: TextureSampleType::Float { filterable: false },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                        count: None,
                    },
                ],
            });
        let outline_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("outline_outline_bind_group"),
            layout: &outline_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&jfa_primary_output.default_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });

        OutlineResources {
            stencil_output,
            jfa_bind_group_layout,
            sampler,
            jfa_distance_buffer,
            jfa_distance_offsets,
            jfa_dims_buffer,
            jfa_primary_bind_group,
            jfa_primary_output,
            jfa_secondary_bind_group,
            jfa_secondary_output,
            outline_bind_group,
        }
    }
}

fn tex_desc(label: &'static str, size: Extent3d, format: TextureFormat) -> TextureDescriptor {
    TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
    }
}

fn recreate_outline_resources(
    mut outline: ResMut<OutlineResources>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut textures: ResMut<TextureCache>,
    windows: Res<ExtractedWindows>,
) {
    let primary = windows.get(&WindowId::primary()).unwrap();
    let size = Extent3d {
        width: primary.physical_width,
        height: primary.physical_height,
        depth_or_array_layers: 1,
    };

    let dims = outline.jfa_dims_buffer.get_mut(0);
    if size.width != dims.width || size.height != dims.height {
        *dims = jfa::Dimensions {
            width: size.width,
            height: size.height,
        };
        outline.jfa_dims_buffer.write_buffer(&device, &queue);
    }

    let stencil_desc = tex_desc(
        "outline_stencil_output",
        size,
        TextureFormat::Depth24PlusStencil8,
    );
    outline.stencil_output = textures.get(&device, stencil_desc);

    // The JFA passes ping-pong between the primary and secondary outputs, so
    // when the primary target is recreated, the secondary bind group is
    // recreated, and vice-versa.

    let old_jfa_primary = outline.jfa_primary_output.texture.id();
    let jfa_primary_desc = tex_desc("outline_jfa_primary_output", size, TextureFormat::Rg16Snorm);
    let jfa_primary_output = textures.get(&device, jfa_primary_desc);
    if jfa_primary_output.texture.id() != old_jfa_primary {
        outline.jfa_primary_output = jfa_primary_output;
        outline.jfa_secondary_bind_group = outline.create_jfa_bind_group(
            &device,
            "outline_jfa_secondary_bind_group",
            &outline.jfa_primary_output.default_view,
        );
    }

    let old_jfa_secondary = outline.jfa_secondary_output.texture.id();
    let jfa_secondary_desc = tex_desc(
        "outline_jfa_secondary_output",
        size,
        TextureFormat::Rg16Snorm,
    );
    let jfa_secondary_output = textures.get(&device, jfa_secondary_desc);
    if jfa_secondary_output.texture.id() != old_jfa_secondary {
        outline.jfa_secondary_output = jfa_secondary_output;
        outline.jfa_primary_bind_group = outline.create_jfa_bind_group(
            &device,
            "outline_jfa_primary_bind_group",
            &outline.jfa_secondary_output.default_view,
        );
    }
}

pub struct FullscreenTriVertexBuffer(Buffer);

impl Deref for FullscreenTriVertexBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub const STENCIL_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 10400755559809425757);
pub const JFA_INIT_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 11038189062916158841);
pub const JFA_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 5227804998548228051);
pub const FULLSCREEN_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 12099561278220359682);

pub mod node {
    pub const OUTLINE_PASS_DRIVER: &'static str = "outline_pass_driver";
}

pub struct OutlinePassDriverNode;

impl Node for OutlinePassDriverNode {
    fn run(
        &self,
        graph: &mut RenderGraphContext,
        _render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let extracted_cameras = world.get_resource::<ExtractedCameraNames>().unwrap();
        if let Some(camera_3d) = extracted_cameras.entities.get(CameraPlugin::CAMERA_3D) {
            graph.run_sub_graph(outline_graph::NAME, vec![SlotValue::Entity(*camera_3d)])?;
        }

        Ok(())
    }
}

pub mod outline_graph {
    pub const NAME: &'static str = "outline_graph";

    pub mod input {
        pub const VIEW_ENTITY: &'static str = "view_entity";
    }

    pub mod node {
        pub const STENCIL_PASS: &'static str = "stencil_pass";
        pub const JFA_INIT_PASS: &'static str = "jfa_init_pass";
        pub const JFA_PASS: &'static str = "jfa_pass";
    }
}

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        let mut shaders = app.world.get_resource_mut::<Assets<Shader>>().unwrap();
        let mask_shader = Shader::from_wgsl(include_str!("shaders/stencil.wgsl"));
        shaders.set_untracked(STENCIL_SHADER_HANDLE, mask_shader);
        let jfa_init_shader = Shader::from_wgsl(include_str!("shaders/jfa_init.wgsl"));
        shaders.set_untracked(JFA_INIT_SHADER_HANDLE, jfa_init_shader);
        let jfa_shader = Shader::from_wgsl(include_str!("shaders/jfa.wgsl"));
        shaders.set_untracked(JFA_SHADER_HANDLE, jfa_shader);
        let fullscreen_shader = Shader::from_wgsl(include_str!("shaders/fullscreen.wgsl"))
            .with_import_path("outline::fullscreen");
        shaders.set_untracked(FULLSCREEN_SHADER_HANDLE, fullscreen_shader);

        // TODO:
        // - extract outline components
        // - queue meshes

        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            let verts: [Vec2; 3] = [
                Vec2::new(-1.0, -1.0),
                Vec2::new(3.0, -1.0),
                Vec2::new(-1.0, 3.0),
            ];
            let device = render_app.world.get_resource::<RenderDevice>().unwrap();
            let buf = device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("outline_fullscreen_vertex_buffer"),
                contents: verts.as_std140().as_bytes(),
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            });
            let fullscreen_verts = FullscreenTriVertexBuffer(buf);

            render_app
                .init_resource::<DrawFunctions<MeshStencil>>()
                .add_render_command::<MeshStencil, SetItemPipeline>()
                .add_render_command::<MeshStencil, DrawMeshStencil>()
                .init_resource::<OutlineResources>()
                .init_resource::<stencil::MeshStencilPipeline>()
                .init_resource::<SpecializedPipelines<stencil::MeshStencilPipeline>>()
                .init_resource::<coords::CoordsPipeline>()
                .init_resource::<jfa::JfaPipeline>()
                .insert_resource(fullscreen_verts)
                .add_system_to_stage(RenderStage::Extract, extract_outlines)
                .add_system_to_stage(RenderStage::Extract, extract_stencil_camera_phase)
                .add_system_to_stage(RenderStage::Prepare, recreate_outline_resources)
                .add_system_to_stage(RenderStage::Queue, queue_mesh_masks);

            let mut outline_graph = RenderGraph::default();
            let stencil_node = MeshStencilNode::new(&mut render_app.world);
            let input_node_id = outline_graph.set_input(vec![SlotInfo::new(
                outline_graph::input::VIEW_ENTITY,
                SlotType::Entity,
            )]);
            outline_graph.add_node(outline_graph::node::STENCIL_PASS, stencil_node);
            outline_graph
                .add_slot_edge(
                    input_node_id,
                    outline_graph::input::VIEW_ENTITY,
                    outline_graph::node::STENCIL_PASS,
                    MeshStencilNode::IN_VIEW,
                )
                .unwrap();
            outline_graph.add_node(outline_graph::node::JFA_INIT_PASS, CoordsNode);
            outline_graph
                .add_slot_edge(
                    outline_graph::node::STENCIL_PASS,
                    MeshStencilNode::OUT_STENCIL,
                    outline_graph::node::JFA_INIT_PASS,
                    CoordsNode::IN_STENCIL,
                )
                .unwrap();
            outline_graph.add_node(outline_graph::node::JFA_PASS, JfaNode);
            outline_graph
                .add_slot_edge(
                    outline_graph::node::JFA_INIT_PASS,
                    CoordsNode::OUT_COORDS,
                    outline_graph::node::JFA_PASS,
                    JfaNode::IN_BASE,
                )
                .unwrap();

            let mut root_graph = render_app.world.get_resource_mut::<RenderGraph>().unwrap();
            root_graph.add_sub_graph(outline_graph::NAME, outline_graph);
            root_graph.add_node(node::OUTLINE_PASS_DRIVER, OutlinePassDriverNode);
            root_graph
                .add_node_edge(MAIN_PASS_DEPENDENCIES, node::OUTLINE_PASS_DRIVER)
                .unwrap();
        }
    }
}

pub struct MeshStencil {
    distance: f32,
    pipeline: CachedPipelineId,
    entity: Entity,
    draw_function: DrawFunctionId,
}

impl PhaseItem for MeshStencil {
    type SortKey = FloatOrd;

    fn sort_key(&self) -> Self::SortKey {
        FloatOrd(self.distance)
    }

    fn draw_function(&self) -> DrawFunctionId {
        self.draw_function
    }
}

impl EntityPhaseItem for MeshStencil {
    fn entity(&self) -> Entity {
        self.entity
    }
}

impl CachedPipelinePhaseItem for MeshStencil {
    fn cached_pipeline(&self) -> CachedPipelineId {
        self.pipeline
    }
}

type DrawMeshStencil = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    DrawMesh,
);

/// Component for entities that should be outlined.
#[derive(Clone, Debug, PartialEq, Component)]
pub struct Outline {
    pub enabled: bool,
    pub width: u32,
    pub color: Color,
}

#[derive(AsStd140, Clone, Component)]
pub struct OutlineUniform {
    pub color: Vec4,
    pub width: u32,
}

pub fn extract_outlines(
    mut commands: Commands,
    mut previous_outlined_len: Local<usize>,
    outlined_query: Query<(Entity, &Outline), (With<GlobalTransform>, With<Handle<Mesh>>)>,
) {
    let mut batches = Vec::with_capacity(*previous_outlined_len);
    batches.extend(outlined_query.iter().filter_map(|(entity, outline)| {
        outline.enabled.then(|| {
            (
                entity,
                (OutlineUniform {
                    color: outline.color.as_linear_rgba_f32().into(),
                    width: outline.width,
                },),
            )
        })
    }));
    *previous_outlined_len = batches.len();
    commands.insert_or_spawn_batch(batches);
}

pub struct MeshMaskPipeline {
    mesh_pipeline: MeshPipeline,
    mask_shader: Handle<Shader>,
}

impl FromWorld for MeshMaskPipeline {
    fn from_world(world: &mut World) -> Self {
        let mesh_pipeline = world.get_resource::<MeshPipeline>().unwrap().clone();

        MeshMaskPipeline {
            mesh_pipeline,
            mask_shader: STENCIL_SHADER_HANDLE.typed::<Shader>(),
        }
    }
}

impl SpecializedPipeline for MeshMaskPipeline {
    type Key = ();

    fn specialize(&self, (): Self::Key) -> RenderPipelineDescriptor {
        let mut descriptor = self.mesh_pipeline.specialize(MeshPipelineKey::empty());

        descriptor.vertex.shader = self.mask_shader.clone();
        descriptor.vertex.entry_point = Cow::from("vertex");

        descriptor.fragment = None;

        descriptor.layout = Some(vec![
            self.mesh_pipeline.view_layout.clone(),
            self.mesh_pipeline.mesh_layout.clone(),
        ]);

        descriptor
    }
}

pub struct SetMaskPipeline;

impl RenderCommand<Transparent3d> for SetMaskPipeline {
    // This pipeline cache lookup is not necessary, but it allows reuse of
    // `Transparent3d`. Should probably not set the pipeline for each mesh.
    type Param = SRes<RenderPipelineCache>;

    fn render<'w>(
        _: Entity,
        item: &Transparent3d,
        pipeline_cache: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_render_pipeline(
            pipeline_cache
                .into_inner()
                .get_state(item.pipeline)
                .unwrap(),
        );

        RenderCommandResult::Success
    }
}

pub fn extract_stencil_camera_phase(mut commands: Commands, active_cameras: Res<ActiveCameras>) {
    if let Some(camera_3d) = active_cameras.get(CameraPlugin::CAMERA_3D) {
        if let Some(entity) = camera_3d.entity {
            commands
                .get_or_spawn(entity)
                .insert(RenderPhase::<MeshStencil>::default());
        }
    }
}

pub fn queue_mesh_masks(
    mesh_stencil_draw_functions: Res<DrawFunctions<MeshStencil>>,
    mesh_stencil_pipeline: Res<MeshStencilPipeline>,
    mut pipelines: ResMut<SpecializedPipelines<MeshStencilPipeline>>,
    mut pipeline_cache: ResMut<RenderPipelineCache>,
    render_meshes: Res<RenderAssets<Mesh>>,
    outline_meshes: Query<(Entity, &Handle<Mesh>, &MeshUniform, &OutlineUniform)>,
    mut views: Query<(
        &ExtractedView,
        &mut VisibleEntities,
        &mut RenderPhase<MeshStencil>,
    )>,
) {
    let draw_outline = mesh_stencil_draw_functions
        .read()
        .get_id::<DrawMeshStencil>()
        .unwrap();

    for (view, visible_entities, mut mesh_stencil_phase) in views.iter_mut() {
        let view_matrix = view.transform.compute_matrix();
        let inv_view_row_2 = view_matrix.inverse().row(2);

        for visible_entity in visible_entities.entities.iter().copied() {
            if let Ok((entity, mesh_handle, mesh_uniform, outline_uniform)) =
                outline_meshes.get(visible_entity)
            {
                if let Some(mesh) = render_meshes.get(mesh_handle) {
                    let key = {
                        let mut k =
                            MeshPipelineKey::from_primitive_topology(mesh.primitive_topology);
                        if mesh.has_tangents {
                            k |= MeshPipelineKey::VERTEX_TANGENTS;
                        }
                        k
                    };

                    let pipeline =
                        pipelines.specialize(&mut pipeline_cache, &mesh_stencil_pipeline, key);
                    mesh_stencil_phase.add(MeshStencil {
                        entity,
                        pipeline,
                        draw_function: draw_outline,
                        distance: inv_view_row_2.dot(mesh_uniform.transform.col(3)),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
