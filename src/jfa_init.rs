use bevy::{
    prelude::*,
    render::{
        render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType},
        render_resource::{
            CachedRenderPipelineId, ColorTargetState, ColorWrites, Face, FragmentState, FrontFace,
            LoadOp, MultisampleState, Operations, PipelineCache, PolygonMode, PrimitiveState,
            PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor,
            RenderPipelineDescriptor, VertexState,
        },
        renderer::RenderContext,
    },
};

use crate::{resources::OutlineResources, JFA_INIT_SHADER_HANDLE, JFA_TEXTURE_FORMAT};

#[derive(Resource)]
pub struct JfaInitPipeline {
    cached: CachedRenderPipelineId,
}

impl FromWorld for JfaInitPipeline {
    fn from_world(world: &mut World) -> Self {
        let res = world.resource::<OutlineResources>();
        let dims_layout = res.dimensions_bind_group_layout.clone();
        let init_layout = res.jfa_init_bind_group_layout.clone();

        let pipeline_cache = world.get_resource_mut::<PipelineCache>().unwrap();
        let cached = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("outline_jfa_init_pipeline".into()),
            layout: vec![dims_layout, init_layout],
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
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                shader: JFA_INIT_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: vec![],
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: JFA_TEXTURE_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            push_constant_ranges: vec![],
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
    pub const IN_MASK: &'static str = "in_stencil";

    /// The produced initialized JFA buffer.
    ///
    /// This has the format `bevy_jfa::JFA_TEXTURE_FORMAT`. Fragments that pass
    /// the stencil test are assigned their framebuffer coordinates. Fragments
    /// that fail the stencil test are assigned a value of (-1, -1).
    pub const OUT_JFA_INIT: &'static str = "out_jfa_init";
}

impl Node for JfaInitNode {
    fn input(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::IN_MASK, SlotType::TextureView)]
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

        let pipeline = world.get_resource::<JfaInitPipeline>().unwrap();
        let pipeline_cache = world.get_resource::<PipelineCache>().unwrap();
        let cached_pipeline = match pipeline_cache.get_render_pipeline(pipeline.cached) {
            Some(c) => c,
            // Still queued.
            None => {
                return Ok(());
            }
        };

        let mut tracked_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("outline_jfa_init"),
            color_attachments: &[Some(RenderPassColorAttachment {
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
            })],
            depth_stencil_attachment: None,
        });
        tracked_pass.set_render_pipeline(cached_pipeline);
        tracked_pass.set_bind_group(0, &res.dimensions_bind_group, &[]);
        tracked_pass.set_bind_group(1, &res.jfa_init_bind_group, &[]);
        tracked_pass.draw(0..3, 0..1);

        Ok(())
    }
}
