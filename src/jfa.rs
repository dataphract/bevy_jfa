use bevy::{
    prelude::*,
    render::{
        render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType},
        render_phase::TrackedRenderPass,
        render_resource::{
            std140::AsStd140, BindGroup, CachedPipelineId, ColorTargetState, ColorWrites, Face,
            FragmentState, FrontFace, LoadOp, MultisampleState, Operations, PolygonMode,
            PrimitiveState, PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor,
            RenderPipelineCache, RenderPipelineDescriptor, TextureFormat, TextureView, VertexState,
        },
        renderer::RenderContext,
    },
};

use crate::{OutlineResources, JFA_SHADER_HANDLE};

pub const TEXTURE_FORMAT: TextureFormat = TextureFormat::Rg16Snorm;

#[derive(Copy, Clone, Debug, PartialEq, Eq, AsStd140)]
pub struct JumpDist {
    pub dist: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, AsStd140)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

pub struct JfaPipeline {
    cached: CachedPipelineId,
}

impl FromWorld for JfaPipeline {
    fn from_world(world: &mut World) -> Self {
        let res = world.get_resource::<OutlineResources>().unwrap();
        let jfa_bind_group_layout = res.jfa_bind_group_layout.clone();
        let mut pipeline_cache = world.get_resource_mut::<RenderPipelineCache>().unwrap();
        let cached = pipeline_cache.queue(RenderPipelineDescriptor {
            label: Some("outline_coords_pipeline".into()),
            layout: Some(vec![jfa_bind_group_layout]),
            vertex: VertexState {
                shader: JFA_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: vec![],
                entry_point: "vertex".into(),
                buffers: vec![],
            },
            fragment: Some(FragmentState {
                shader: JFA_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: vec![],
                entry_point: "fragment".into(),
                targets: vec![ColorTargetState {
                    format: TEXTURE_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                }],
            }),
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
        });

        JfaPipeline { cached }
    }
}

pub struct JfaNode;

impl JfaNode {
    pub const IN_BASE: &'static str = "in_base";
    pub const OUT_JUMP: &'static str = "out_jump";
}

impl Node for JfaNode {
    fn input(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::IN_BASE, SlotType::TextureView)]
    }

    fn output(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::OUT_JUMP, SlotType::TextureView)]
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
                Self::OUT_JUMP,
                res.jfa_secondary_output.default_view.clone(),
            )
            .unwrap();

        let pipeline = world.get_resource::<JfaPipeline>().unwrap();
        let pipeline_cache = world.get_resource::<RenderPipelineCache>().unwrap();
        let cached_pipeline = match pipeline_cache.get(pipeline.cached) {
            Some(c) => c,
            // Still queued.
            None => {
                return Ok(());
            }
        };

        let max_exp = 8;
        for it in 0..=max_exp {
            let exp = max_exp - it;

            let target: &TextureView;
            let src: &BindGroup;
            if it % 2 == 1 {
                target = &res.jfa_primary_output.default_view;
                src = &res.jfa_primary_bind_group;
            } else {
                target = &res.jfa_secondary_output.default_view;
                src = &res.jfa_secondary_bind_group;
            }

            let attachment = RenderPassColorAttachment {
                view: target,
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
            };
            let render_pass =
                render_context
                    .command_encoder
                    .begin_render_pass(&RenderPassDescriptor {
                        label: Some("outline_jfa"),
                        color_attachments: &[attachment],
                        depth_stencil_attachment: None,
                    });
            let mut tracked_pass = TrackedRenderPass::new(render_pass);
            tracked_pass.set_render_pipeline(&cached_pipeline);
            tracked_pass.set_bind_group(0, src, &[res.jfa_distance_offsets[exp]]);
            tracked_pass.draw(0..3, 0..1);
        }

        Ok(())
    }
}
