use bevy::{
    prelude::*,
    render::{
        render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType},
        render_phase::TrackedRenderPass,
        render_resource::{
            BindGroup, CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState,
            LoadOp, MultisampleState, Operations, PipelineCache, RenderPassColorAttachment,
            RenderPassDescriptor, RenderPipelineDescriptor, ShaderType, TextureView, VertexState,
        },
        renderer::RenderContext,
    },
};

use crate::{
    resources::OutlineResources, FULLSCREEN_PRIMITIVE_STATE, JFA_SHADER_HANDLE, JFA_TEXTURE_FORMAT,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ShaderType)]
pub struct JumpDist {
    pub dist: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, ShaderType)]
pub struct Dimensions {
    width: f32,
    height: f32,
    inv_width: f32,
    inv_height: f32,
}

impl Dimensions {
    pub fn new(width: u32, height: u32) -> Dimensions {
        Dimensions {
            width: width as f32,
            height: height as f32,
            inv_width: 1.0 / width as f32,
            inv_height: 1.0 / height as f32,
        }
    }
}

pub struct JfaPipeline {
    cached: CachedRenderPipelineId,
}

impl FromWorld for JfaPipeline {
    fn from_world(world: &mut World) -> Self {
        let res = world.get_resource::<OutlineResources>().unwrap();
        let dimensions_bind_group_layout = res.dimensions_bind_group_layout.clone();
        let jfa_bind_group_layout = res.jfa_bind_group_layout.clone();
        let mut pipeline_cache = world.get_resource_mut::<PipelineCache>().unwrap();
        let cached = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("outline_jfa_pipeline".into()),
            layout: Some(vec![dimensions_bind_group_layout, jfa_bind_group_layout]),
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
                    format: JFA_TEXTURE_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                }],
            }),
            primitive: FULLSCREEN_PRIMITIVE_STATE,
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
        let pipeline_cache = world.get_resource::<PipelineCache>().unwrap();
        let cached_pipeline = match pipeline_cache.get_render_pipeline(pipeline.cached) {
            Some(c) => c,
            // Still queued.
            None => {
                return Ok(());
            }
        };

        // The half-width of the JFA region is 2^(max_exp + 1) - 1.
        //
        // weight < 2^(max_exp + 1) - 1
        // weight + 1 < 2^(max_exp + 1)
        // log2(weight + 1) < max_exp + 1
        // max_exp > log2(weight + 1) - 1

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
                    // TODO: ideally, this would be the equivalent of DONT_CARE, but wgpu doesn't expose that.
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
            tracked_pass.set_render_pipeline(cached_pipeline);
            tracked_pass.set_bind_group(0, &res.dimensions_bind_group, &[]);
            tracked_pass.set_bind_group(1, src, &[res.jfa_distance_offsets[exp]]);
            tracked_pass.draw(0..3, 0..1);
        }

        Ok(())
    }
}
