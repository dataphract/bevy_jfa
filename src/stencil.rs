use bevy::{
    pbr::{MeshPipeline, MeshPipelineKey},
    prelude::*,
    render::{
        render_graph::{Node, RenderGraphContext, SlotInfo, SlotType},
        render_phase::{DrawFunctions, PhaseItem, RenderPhase, TrackedRenderPass},
        render_resource::{
            CompareFunction, DepthStencilState, LoadOp, MultisampleState, Operations,
            RenderPassDepthStencilAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
            SpecializedPipeline, StencilFaceState, StencilOperation, StencilState, TextureFormat,
        },
        renderer::RenderContext,
    },
};

use crate::{MeshStencil, OutlineResources, STENCIL_SHADER_HANDLE};

pub struct MeshStencilPipeline {
    mesh_pipeline: MeshPipeline,
}

impl FromWorld for MeshStencilPipeline {
    fn from_world(world: &mut World) -> Self {
        let mesh_pipeline = world.get_resource::<MeshPipeline>().unwrap().clone();

        MeshStencilPipeline { mesh_pipeline }
    }
}

impl SpecializedPipeline for MeshStencilPipeline {
    type Key = MeshPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut desc = self.mesh_pipeline.specialize(key);

        desc.layout = Some(vec![
            self.mesh_pipeline.view_layout.clone(),
            self.mesh_pipeline.mesh_layout.clone(),
        ]);

        desc.vertex.shader = STENCIL_SHADER_HANDLE.typed::<Shader>();

        // We only care about the stencil buffer output, so no fragment stage is necessary.
        desc.fragment = None;

        desc.depth_stencil = desc.depth_stencil.map(|ds| DepthStencilState {
            format: TextureFormat::Depth24PlusStencil8,
            stencil: StencilState {
                front: StencilFaceState {
                    compare: CompareFunction::Always,
                    fail_op: StencilOperation::Replace,
                    depth_fail_op: StencilOperation::Replace,
                    pass_op: StencilOperation::Replace,
                },
                back: StencilFaceState::IGNORE,
                read_mask: 0,
                write_mask: !0,
            },
            ..ds
        });

        desc.multisample = MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        desc.label = Some("mesh_stencil_pipeline".into());
        desc
    }
}

pub struct MeshStencilNode {
    query: QueryState<&'static RenderPhase<MeshStencil>>,
}

impl MeshStencilNode {
    pub const IN_VIEW: &'static str = "view";
    pub const OUT_STENCIL: &'static str = "stencil";

    pub fn new(world: &mut World) -> MeshStencilNode {
        MeshStencilNode {
            query: QueryState::new(world),
        }
    }
}

impl Node for MeshStencilNode {
    fn input(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::IN_VIEW, SlotType::Entity)]
    }

    fn output(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::OUT_STENCIL, SlotType::TextureView)]
    }

    fn update(&mut self, world: &mut World) {
        self.query.update_archetypes(world);
    }

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), bevy::render::render_graph::NodeRunError> {
        let res = world.get_resource::<OutlineResources>().unwrap();

        graph
            .set_output(Self::OUT_STENCIL, res.stencil_output.default_view.clone())
            .unwrap();

        let view_entity = graph.get_input_entity(Self::IN_VIEW).unwrap();
        let stencil_phase = match self.query.get_manual(world, view_entity) {
            Ok(q) => q,
            Err(_) => return Ok(()),
        };

        let pass_raw = render_context
            .command_encoder
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("outline_stencil_render_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &res.stencil_output.default_view,
                    depth_ops: None,
                    stencil_ops: Some(Operations {
                        load: LoadOp::Clear(0),
                        store: true,
                    }),
                }),
            });
        let mut pass = TrackedRenderPass::new(pass_raw);

        pass.set_stencil_reference(!0);
        let draw_functions = world.get_resource::<DrawFunctions<MeshStencil>>().unwrap();
        let mut draw_functions = draw_functions.write();
        for item in stencil_phase.items.iter() {
            let draw_function = draw_functions.get_mut(item.draw_function()).unwrap();
            draw_function.draw(world, &mut pass, view_entity, item);
        }

        Ok(())
    }
}
