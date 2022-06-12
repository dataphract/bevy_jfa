//! A Bevy library for computing the Jump Flooding Algorithm.
//!
//! The **jump flooding algorithm** (JFA) is a fast screen-space algorithm for
//! computing distance fields. Currently, this crate provides a plugin for
//! adding outlines to arbitrary meshes.
//!
//! Outlines adapted from ["The Quest for Very Wide Outlines" by Ben Golus][0].
//!
//! [0]: https://bgolus.medium.com/the-quest-for-very-wide-outlines-ba82ed442cd9
//!
//! # Setup
//!
//! To add an outline to a mesh:
//!
//! 1. Add the [`OutlinePlugin`] to the base `App`.
//! 2. Add the desired [`OutlineStyle`] as an `Asset`.
//! 3. Add a [`CameraOutline`] component with the desired `OutlineStyle` to the
//!    camera which should render the outline.  Currently, outline styling is
//!    tied to the camera rather than the mesh.
//! 4. Add an [`Outline`] component to the mesh with `enabled: true`.

use bevy::{
    app::prelude::*,
    asset::{Assets, Handle, HandleUntyped},
    core_pipeline::core_3d,
    ecs::{prelude::*, system::SystemParamItem},
    pbr::{DrawMesh, MeshPipelineKey, MeshUniform, SetMeshBindGroup, SetMeshViewBindGroup},
    prelude::{AddAsset, Camera3d},
    reflect::TypeUuid,
    render::{
        prelude::*,
        render_asset::{PrepareAssetError, RenderAsset, RenderAssetPlugin, RenderAssets},
        render_graph::RenderGraph,
        render_phase::{
            AddRenderCommand, CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions,
            EntityPhaseItem, PhaseItem, RenderPhase, SetItemPipeline,
        },
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        view::{ExtractedView, VisibleEntities},
        RenderApp, RenderStage,
    },
    utils::FloatOrd,
};

use crate::{
    graph::OutlineDriverNode,
    mask::MeshMaskPipeline,
    outline::{GpuOutlineParams, OutlineParams},
    resources::OutlineResources,
};

mod graph;
mod jfa;
mod jfa_init;
mod mask;
mod outline;
mod resources;

const JFA_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rg16Snorm;
const FULLSCREEN_PRIMITIVE_STATE: PrimitiveState = PrimitiveState {
    topology: PrimitiveTopology::TriangleList,
    strip_index_format: None,
    front_face: FrontFace::Ccw,
    cull_mode: Some(Face::Back),
    unclipped_depth: false,
    polygon_mode: PolygonMode::Fill,
    conservative: false,
};

/// Top-level plugin for enabling outlines.
#[derive(Default)]
pub struct OutlinePlugin;

/// Performance and visual quality settings for JFA-based outlines.
#[derive(Clone)]
pub struct OutlineSettings {
    pub(crate) half_resolution: bool,
}

impl OutlineSettings {
    /// Returns whether the half-resolution setting is enabled.
    pub fn half_resolution(&self) -> bool {
        self.half_resolution
    }

    /// Sets whether the half-resolution setting is enabled.
    pub fn set_half_resolution(&mut self, value: bool) {
        self.half_resolution = value;
    }
}

impl Default for OutlineSettings {
    fn default() -> Self {
        Self {
            half_resolution: false,
        }
    }
}

const MASK_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 10400755559809425757);
const JFA_INIT_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 11038189062916158841);
const JFA_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 5227804998548228051);
const FULLSCREEN_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 12099561278220359682);
const OUTLINE_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 11094028876979933159);
const DIMENSIONS_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 11721531257850828867);

use crate::graph::outline as outline_graph;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(RenderAssetPlugin::<OutlineStyle>::default())
            .add_asset::<OutlineStyle>()
            .init_resource::<OutlineSettings>();

        let mut shaders = app.world.get_resource_mut::<Assets<Shader>>().unwrap();

        let mask_shader = Shader::from_wgsl(include_str!("shaders/mask.wgsl"));
        let jfa_init_shader = Shader::from_wgsl(include_str!("shaders/jfa_init.wgsl"));
        let jfa_shader = Shader::from_wgsl(include_str!("shaders/jfa.wgsl"));
        let fullscreen_shader = Shader::from_wgsl(include_str!("shaders/fullscreen.wgsl"))
            .with_import_path("outline::fullscreen");
        let outline_shader = Shader::from_wgsl(include_str!("shaders/outline.wgsl"));
        let dimensions_shader = Shader::from_wgsl(include_str!("shaders/dimensions.wgsl"))
            .with_import_path("outline::dimensions");

        shaders.set_untracked(MASK_SHADER_HANDLE, mask_shader);
        shaders.set_untracked(JFA_INIT_SHADER_HANDLE, jfa_init_shader);
        shaders.set_untracked(JFA_SHADER_HANDLE, jfa_shader);
        shaders.set_untracked(FULLSCREEN_SHADER_HANDLE, fullscreen_shader);
        shaders.set_untracked(OUTLINE_SHADER_HANDLE, outline_shader);
        shaders.set_untracked(DIMENSIONS_SHADER_HANDLE, dimensions_shader);

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(r) => r,
            Err(_) => return,
        };

        render_app
            .init_resource::<DrawFunctions<MeshMask>>()
            .add_render_command::<MeshMask, SetItemPipeline>()
            .add_render_command::<MeshMask, DrawMeshMask>()
            .init_resource::<resources::OutlineResources>()
            .init_resource::<mask::MeshMaskPipeline>()
            .init_resource::<SpecializedMeshPipelines<mask::MeshMaskPipeline>>()
            .init_resource::<jfa_init::JfaInitPipeline>()
            .init_resource::<jfa::JfaPipeline>()
            .init_resource::<outline::OutlinePipeline>()
            .init_resource::<SpecializedRenderPipelines<outline::OutlinePipeline>>()
            .add_system_to_stage(RenderStage::Extract, extract_outline_settings)
            .add_system_to_stage(RenderStage::Extract, extract_camera_outlines)
            .add_system_to_stage(RenderStage::Extract, extract_mask_camera_phase)
            .add_system_to_stage(RenderStage::Prepare, resources::recreate_outline_resources)
            .add_system_to_stage(RenderStage::Queue, queue_mesh_masks);

        let outline_graph = graph::outline(render_app).unwrap();

        let mut root_graph = render_app.world.resource_mut::<RenderGraph>();
        let draw_3d_graph = root_graph.get_sub_graph_mut(core_3d::graph::NAME).unwrap();
        let draw_3d_input = draw_3d_graph.input_node().unwrap().id;

        draw_3d_graph.add_sub_graph(outline_graph::NAME, outline_graph);
        let outline_driver = draw_3d_graph.add_node(OutlineDriverNode::NAME, OutlineDriverNode);
        draw_3d_graph
            .add_slot_edge(
                draw_3d_input,
                core_3d::graph::input::VIEW_ENTITY,
                outline_driver,
                OutlineDriverNode::INPUT_VIEW,
            )
            .unwrap();
        draw_3d_graph
            .add_node_edge(core_3d::graph::node::MAIN_PASS, outline_driver)
            .unwrap();
    }
}

struct MeshMask {
    distance: f32,
    pipeline: CachedRenderPipelineId,
    entity: Entity,
    draw_function: DrawFunctionId,
}

impl PhaseItem for MeshMask {
    type SortKey = FloatOrd;

    fn sort_key(&self) -> Self::SortKey {
        FloatOrd(self.distance)
    }

    fn draw_function(&self) -> DrawFunctionId {
        self.draw_function
    }
}

impl EntityPhaseItem for MeshMask {
    fn entity(&self) -> Entity {
        self.entity
    }
}

impl CachedRenderPipelinePhaseItem for MeshMask {
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}

type DrawMeshMask = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    DrawMesh,
);

/// Visual style for an outline.
#[derive(Clone, Debug, PartialEq, TypeUuid)]
#[uuid = "256fd556-e497-4df2-8d9c-9bdb1419ee90"]
pub struct OutlineStyle {
    pub color: Color,
    pub width: f32,
}

impl RenderAsset for OutlineStyle {
    type ExtractedAsset = OutlineParams;
    type PreparedAsset = GpuOutlineParams;
    type Param = (
        Res<'static, RenderDevice>,
        Res<'static, RenderQueue>,
        Res<'static, OutlineResources>,
    );

    fn extract_asset(&self) -> Self::ExtractedAsset {
        OutlineParams::new(self.color, self.width)
    }

    fn prepare_asset(
        extracted_asset: Self::ExtractedAsset,
        (device, queue, outline_res): &mut SystemParamItem<Self::Param>,
    ) -> Result<Self::PreparedAsset, PrepareAssetError<Self::ExtractedAsset>> {
        let mut buffer = UniformBuffer::from(extracted_asset.clone());
        buffer.write_buffer(device, queue);

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &outline_res.outline_params_bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.buffer().unwrap().as_entire_binding(),
            }],
        });

        Ok(GpuOutlineParams {
            params: extracted_asset,
            _buffer: buffer,
            bind_group,
        })
    }
}

/// Component for enabling outlines when rendering with a given camera.
#[derive(Clone, Debug, PartialEq, Component)]
pub struct CameraOutline {
    pub enabled: bool,
    pub style: Handle<OutlineStyle>,
}

/// Component for entities that should be outlined.
#[derive(Clone, Debug, PartialEq, Component)]
pub struct Outline {
    pub enabled: bool,
}

fn extract_outline_settings(mut commands: Commands, settings: Res<OutlineSettings>) {
    commands.insert_resource(settings.clone());
}

fn extract_camera_outlines(
    mut commands: Commands,
    mut previous_outline_len: Local<usize>,
    cam_outline_query: Query<(Entity, &CameraOutline), With<Camera>>,
) {
    let mut batches = Vec::with_capacity(*previous_outline_len);
    batches.extend(
        cam_outline_query
            .iter()
            .filter_map(|(entity, outline)| outline.enabled.then(|| (entity, (outline.clone(),)))),
    );
    *previous_outline_len = batches.len();
    commands.insert_or_spawn_batch(batches);
}

fn extract_mask_camera_phase(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, With<CameraOutline>)>,
) {
    for entity in cameras.iter() {
        commands
            .get_or_spawn(entity)
            .insert(RenderPhase::<MeshMask>::default());
    }
}

fn queue_mesh_masks(
    mesh_mask_draw_functions: Res<DrawFunctions<MeshMask>>,
    mesh_mask_pipeline: Res<MeshMaskPipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<MeshMaskPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    render_meshes: Res<RenderAssets<Mesh>>,
    outline_meshes: Query<(Entity, &Handle<Mesh>, &MeshUniform)>,
    mut views: Query<(
        &ExtractedView,
        &mut VisibleEntities,
        &mut RenderPhase<MeshMask>,
    )>,
) {
    let draw_outline = mesh_mask_draw_functions
        .read()
        .get_id::<DrawMeshMask>()
        .unwrap();

    for (view, visible_entities, mut mesh_mask_phase) in views.iter_mut() {
        let view_matrix = view.transform.compute_matrix();
        let inv_view_row_2 = view_matrix.inverse().row(2);

        for visible_entity in visible_entities.entities.iter().copied() {
            let (entity, mesh_handle, mesh_uniform) = match outline_meshes.get(visible_entity) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mesh = match render_meshes.get(mesh_handle) {
                Some(m) => m,
                None => continue,
            };

            let key = MeshPipelineKey::from_primitive_topology(mesh.primitive_topology);

            let pipeline = pipelines
                .specialize(&mut pipeline_cache, &mesh_mask_pipeline, key, &mesh.layout)
                .unwrap();

            mesh_mask_phase.add(MeshMask {
                entity,
                pipeline,
                draw_function: draw_outline,
                distance: inv_view_row_2.dot(mesh_uniform.transform.col(3)),
            });
        }
    }
}
