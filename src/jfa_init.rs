use bevy::{
    prelude::*,
    render::{
        render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType},
        render_phase::TrackedRenderPass,
        render_resource::{
            CachedRenderPipelineId, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
            DepthStencilState, Face, FragmentState, FrontFace, LoadOp, MultisampleState,
            Operations, PipelineCache, PolygonMode, PrimitiveState, PrimitiveTopology,
            RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
            RenderPipelineDescriptor, StencilFaceState, StencilOperation, StencilState,
            TextureFormat, VertexState,
        },
        renderer::RenderContext,
    },
};

use crate::{resources::OutlineResources, JFA_INIT_SHADER_HANDLE, JFA_TEXTURE_FORMAT};

pub struct JfaInitPipeline {
    cached: CachedRenderPipelineId,
}

impl FromWorld for JfaInitPipeline {
    fn from_world(world: &mut World) -> Self {
        let mut pipeline_cache = world.get_resource_mut::<PipelineCache>().unwrap();
        let cached = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("outline_jfa_init_pipeline".into()),
            layout: None,
            vertex: VertexState {
                shader: JFA_INIT_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: vec![],
                entry_point: "vertex".into(),
                buffers: vec![],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: false,
                depth_compare: CompareFunction::Always,
                stencil: StencilState {
                    front: StencilFaceState {
                        compare: CompareFunction::Equal,
                        fail_op: StencilOperation::Keep,
                        depth_fail_op: StencilOperation::Keep,
                        pass_op: StencilOperation::Keep,
                    },
                    back: StencilFaceState::IGNORE,
                    read_mask: !0,
                    write_mask: 0,
                },
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                shader: JFA_INIT_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: vec![],
                entry_point: "fragment".into(),
                targets: vec![ColorTargetState {
                    format: JFA_TEXTURE_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                }],
            }),
        });

        JfaInitPipeline { cached }
    }
}

/// Render graph node for the JFA initialization pass.
pub struct JfaInitNode;

impl JfaInitNode {
    /// The input stencil buffer.
    ///
    /// This should have the format `TextureFormat::Depth24PlusStencil8`.
    /// Fragments in the JFA initialization pass will pass the stencil test if
    /// the corresponding stencil buffer value is 255, and fail otherwise.
    /// The depth aspect is ignored.
    pub const IN_STENCIL: &'static str = "in_stencil";

    /// The produced initialized JFA buffer.
    ///
    /// This has the format `bevy_jfa::JFA_TEXTURE_FORMAT`. Fragments that pass
    /// the stencil test are assigned their framebuffer coordinates. Fragments
    /// that fail the stencil test are assigned a value of (-1, -1).
    pub const OUT_JFA_INIT: &'static str = "out_jfa_init";
}

impl Node for JfaInitNode {
    fn input(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::IN_STENCIL, SlotType::TextureView)]
    }

    fn output(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::OUT_JFA_INIT, SlotType::TextureView)]
    }

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let res = world.get_resource::<OutlineResources>().unwrap();
        graph
            .set_output(
                Self::OUT_JFA_INIT,
                res.jfa_primary_output.default_view.clone(),
            )
            .unwrap();

        let stencil = graph.get_input_texture(Self::IN_STENCIL).unwrap();

        let pipeline = world.get_resource::<JfaInitPipeline>().unwrap();
        let pipeline_cache = world.get_resource::<PipelineCache>().unwrap();
        let cached_pipeline = match pipeline_cache.get_render_pipeline(pipeline.cached) {
            Some(c) => c,
            // Still queued.
            None => {
                return Ok(());
            }
        };

        let render_pass = render_context
            .command_encoder
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("outline_jfa_init"),
                color_attachments: &[RenderPassColorAttachment {
                    view: &res.jfa_primary_output.default_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(
                            Color::RgbaLinear {
                                red: -1.0,
                                green: -1.0,
                                blue: 0.0,
                                alpha: 0.0,
                            }
                            .into(),
                        ),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: stencil,
                    depth_ops: None,
                    stencil_ops: Some(Operations {
                        load: LoadOp::Load,
                        store: false,
                    }),
                }),
            });
        let mut tracked_pass = TrackedRenderPass::new(render_pass);
        tracked_pass.set_render_pipeline(&cached_pipeline);
        tracked_pass.set_stencil_reference(!0);
        tracked_pass.draw(0..3, 0..1);

        Ok(())
    }
}
