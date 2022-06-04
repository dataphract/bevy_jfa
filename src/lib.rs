//! A Bevy library for computing the Jump Flooding Algorithm.
//!
//! Outlines adapted from ["The Quest for Very Wide Outlines" by Ben Golus][0].
//!
//! [0]: https://bgolus.medium.com/the-quest-for-very-wide-outlines-ba82ed442cd9

use bevy::{
    app::prelude::*,
    asset::{Assets, Handle, HandleUntyped},
    ecs::prelude::*,
    math::prelude::*,
    pbr::{DrawMesh, MeshPipelineKey, MeshUniform, SetMeshBindGroup, SetMeshViewBindGroup},
    prelude::Camera3d,
    reflect::TypeUuid,
    render::{
        prelude::*,
        render_asset::RenderAssets,
        render_graph::RenderGraph,
        render_phase::{
            AddRenderCommand, CachedRenderPipelinePhaseItem, DrawFunctionId, DrawFunctions,
            EntityPhaseItem, PhaseItem, RenderPhase, SetItemPipeline,
        },
        render_resource::{ShaderType, *},
        texture::BevyDefault,
        view::{ExtractedView, VisibleEntities},
        RenderApp, RenderStage,
    },
    transform::components::GlobalTransform,
    utils::FloatOrd,
};
use jfa::JfaNode;
use jfa_init::JfaInitNode;
use outline::OutlineNode;
use stencil::{MeshStencilNode, MeshStencilPipeline};

mod jfa;
mod jfa_init;
mod outline;
mod resources;
mod stencil;

pub const JFA_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rg16Snorm;
const FULLSCREEN_PRIMITIVE_STATE: PrimitiveState = PrimitiveState {
    topology: PrimitiveTopology::TriangleList,
    strip_index_format: None,
    front_face: FrontFace::Ccw,
    cull_mode: Some(Face::Back),
    unclipped_depth: false,
    polygon_mode: PolygonMode::Fill,
    conservative: false,
};

#[derive(Default)]
pub struct OutlinePlugin;

pub const STENCIL_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 10400755559809425757);
pub const JFA_INIT_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 11038189062916158841);
pub const JFA_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 5227804998548228051);
pub const FULLSCREEN_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 12099561278220359682);
pub const OUTLINE_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 11094028876979933159);
pub const DIMENSIONS_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 11721531257850828867);

pub mod outline_graph {
    pub const NAME: &'static str = "outline_graph";

    pub mod input {
        pub const VIEW_ENTITY: &'static str = "view_entity";
    }

    pub mod node {
        pub const STENCIL_PASS: &'static str = "stencil_pass";
        pub const JFA_INIT_PASS: &'static str = "jfa_init_pass";
        pub const JFA_PASS: &'static str = "jfa_pass";
        pub const OUTLINE_PASS: &'static str = "outline_pass";
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
        let outline_shader = Shader::from_wgsl(include_str!("shaders/outline.wgsl"));
        shaders.set_untracked(OUTLINE_SHADER_HANDLE, outline_shader);
        let dimensions_shader = Shader::from_wgsl(include_str!("shaders/dimensions.wgsl"))
            .with_import_path("outline::dimensions");
        shaders.set_untracked(DIMENSIONS_SHADER_HANDLE, dimensions_shader);

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(r) => r,
            Err(_) => return,
        };

        render_app
            .init_resource::<DrawFunctions<MeshStencil>>()
            .add_render_command::<MeshStencil, SetItemPipeline>()
            .add_render_command::<MeshStencil, DrawMeshStencil>()
            .init_resource::<resources::OutlineResources>()
            .init_resource::<stencil::MeshStencilPipeline>()
            .init_resource::<SpecializedMeshPipelines<stencil::MeshStencilPipeline>>()
            .init_resource::<jfa_init::JfaInitPipeline>()
            .init_resource::<jfa::JfaPipeline>()
            .init_resource::<outline::OutlinePipeline>()
            .init_resource::<SpecializedRenderPipelines<outline::OutlinePipeline>>()
            .add_system_to_stage(RenderStage::Extract, extract_outlines)
            .add_system_to_stage(RenderStage::Extract, extract_stencil_camera_phase)
            .add_system_to_stage(RenderStage::Prepare, resources::recreate_outline_resources)
            .add_system_to_stage(RenderStage::Queue, queue_mesh_stencils);

        let stencil_node = MeshStencilNode::new(&mut render_app.world);
        // TODO: BevyDefault for surface texture format is an anti-pattern;
        // the target texture format should be queried from the window when
        // Bevy exposes that functionality.
        let outline_node = OutlineNode::new(&mut render_app.world, TextureFormat::bevy_default());

        let mut root_graph = render_app.world.resource_mut::<RenderGraph>();
        let draw_3d_graph = root_graph
            .get_sub_graph_mut(bevy::core_pipeline::core_3d::graph::NAME)
            .unwrap();

        let input_node_id = draw_3d_graph.input_node().unwrap().id;
        draw_3d_graph.add_node(outline_graph::node::STENCIL_PASS, stencil_node);
        draw_3d_graph
            .add_slot_edge(
                input_node_id,
                outline_graph::input::VIEW_ENTITY,
                outline_graph::node::STENCIL_PASS,
                MeshStencilNode::IN_VIEW,
            )
            .unwrap();
        draw_3d_graph.add_node(outline_graph::node::JFA_INIT_PASS, JfaInitNode);
        draw_3d_graph
            .add_slot_edge(
                outline_graph::node::STENCIL_PASS,
                MeshStencilNode::OUT_STENCIL,
                outline_graph::node::JFA_INIT_PASS,
                JfaInitNode::IN_STENCIL,
            )
            .unwrap();
        draw_3d_graph.add_node(outline_graph::node::JFA_PASS, JfaNode);
        draw_3d_graph
            .add_slot_edge(
                outline_graph::node::JFA_INIT_PASS,
                JfaInitNode::OUT_JFA_INIT,
                outline_graph::node::JFA_PASS,
                JfaNode::IN_BASE,
            )
            .unwrap();
        draw_3d_graph.add_node(outline_graph::node::OUTLINE_PASS, outline_node);
        draw_3d_graph
            .add_slot_edge(
                input_node_id,
                outline_graph::input::VIEW_ENTITY,
                outline_graph::node::OUTLINE_PASS,
                OutlineNode::IN_VIEW,
            )
            .unwrap();
        draw_3d_graph
            .add_slot_edge(
                outline_graph::node::JFA_PASS,
                JfaNode::OUT_JUMP,
                outline_graph::node::OUTLINE_PASS,
                OutlineNode::IN_JFA,
            )
            .unwrap();
    }
}

pub struct MeshStencil {
    distance: f32,
    pipeline: CachedRenderPipelineId,
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

impl CachedRenderPipelinePhaseItem for MeshStencil {
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}

type DrawMeshStencil = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    DrawMesh,
);

/// Component for enabling outlines when rendering with a given camera.
#[derive(Clone, Debug, PartialEq, Component)]
pub struct CameraOutline {
    pub enabled: bool,
}

/// Component for entities that should be outlined.
#[derive(Clone, Debug, PartialEq, Component)]
pub struct Outline {
    pub enabled: bool,
    pub width: u32,
    pub color: Color,
}

#[derive(ShaderType, Clone, Component)]
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

pub fn extract_stencil_camera_phase(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, With<CameraOutline>)>,
) {
    for entity in cameras.iter() {
        commands
            .get_or_spawn(entity)
            .insert(RenderPhase::<MeshStencil>::default());
    }
}

pub fn queue_mesh_stencils(
    mesh_stencil_draw_functions: Res<DrawFunctions<MeshStencil>>,
    mesh_stencil_pipeline: Res<MeshStencilPipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<MeshStencilPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    render_meshes: Res<RenderAssets<Mesh>>,
    outline_meshes: Query<(Entity, &Handle<Mesh>, &MeshUniform)>,
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
                .specialize(
                    &mut pipeline_cache,
                    &mesh_stencil_pipeline,
                    key,
                    &mesh.layout,
                )
                .unwrap();

            mesh_stencil_phase.add(MeshStencil {
                entity,
                pipeline,
                draw_function: draw_outline,
                distance: inv_view_row_2.dot(mesh_uniform.transform.col(3)),
            });
        }
    }
}
