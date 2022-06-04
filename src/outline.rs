use bevy::{
    prelude::*,
    render::{
        camera::ExtractedCamera,
        render_asset::RenderAssets,
        render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType},
        render_phase::TrackedRenderPass,
        render_resource::{
            BindGroupLayout, BlendComponent, BlendFactor, BlendOperation, BlendState,
            CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState, LoadOp,
            MultisampleState, Operations, PipelineCache, RenderPassColorAttachment,
            RenderPassDescriptor, RenderPipelineDescriptor, ShaderType, SpecializedRenderPipeline,
            SpecializedRenderPipelines, TextureFormat, TextureSampleType, TextureUsages,
            VertexState,
        },
        renderer::RenderContext,
        view::ExtractedWindows,
    },
};

use crate::{
    resources::{self, OutlineResources},
    FULLSCREEN_PRIMITIVE_STATE, OUTLINE_SHADER_HANDLE,
};

#[derive(Clone, Debug, Default, PartialEq, ShaderType)]
pub struct OutlineParams {
    // Outline color.
    color: Vec4,
    // Inverse aspect ratio (height / width).
    inv_aspect: f32,
    // Outline weight in pixels.
    weight: f32,
}

impl OutlineParams {
    pub fn new(color: Color, width: u32, height: u32, weight: f32) -> OutlineParams {
        let color: Vec4 = color.as_rgba_f32().into();

        let w = width as f32;
        let h = height as f32;
        let inv_aspect = h / w;

        OutlineParams {
            color,
            inv_aspect,
            weight,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutlinePipeline {
    dimensions_layout: BindGroupLayout,
    input_layout: BindGroupLayout,
}

impl FromWorld for OutlinePipeline {
    fn from_world(world: &mut World) -> Self {
        let res = world.get_resource::<resources::OutlineResources>().unwrap();
        let dimensions_layout = res.dimensions_bind_group_layout.clone();
        let input_layout = res.outline_bind_group_layout.clone();

        OutlinePipeline {
            dimensions_layout,
            input_layout,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct OutlinePipelineKey {
    format: TextureFormat,
}

impl OutlinePipelineKey {
    pub fn new(format: TextureFormat) -> Option<OutlinePipelineKey> {
        let info = format.describe();

        if info.sample_type == TextureSampleType::Depth {
            // Can't use this format as a color attachment.
            return None;
        }

        if info
            .guaranteed_format_features
            .allowed_usages
            .contains(TextureUsages::RENDER_ATTACHMENT)
        {
            Some(OutlinePipelineKey { format })
        } else {
            None
        }
    }
}

impl SpecializedRenderPipeline for OutlinePipeline {
    type Key = OutlinePipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let blend = BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::OneMinusSrcAlpha,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::Zero,
                operation: BlendOperation::Add,
            },
        };

        RenderPipelineDescriptor {
            label: Some("jfa_outline_pipeline".into()),
            layout: Some(vec![
                self.dimensions_layout.clone(),
                self.input_layout.clone(),
            ]),
            vertex: VertexState {
                shader: OUTLINE_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: vec![],
                entry_point: "vertex".into(),
                buffers: vec![],
            },
            fragment: Some(FragmentState {
                shader: OUTLINE_SHADER_HANDLE.typed::<Shader>(),
                shader_defs: vec![],
                entry_point: "fragment".into(),
                targets: vec![ColorTargetState {
                    format: key.format,
                    blend: Some(blend),
                    write_mask: ColorWrites::ALL,
                }],
            }),
            primitive: FULLSCREEN_PRIMITIVE_STATE,
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        }
    }
}

pub struct OutlineNode {
    pipeline_id: CachedRenderPipelineId,
    query: QueryState<&'static ExtractedCamera>,
}

impl OutlineNode {
    pub const IN_VIEW: &'static str = "in_view";
    pub const IN_JFA: &'static str = "in_jfa";
    pub const OUT_VIEW: &'static str = "out_view";

    pub fn new(world: &mut World, target_format: TextureFormat) -> OutlineNode {
        let pipeline_id = world.resource_scope(|world, mut cache: Mut<PipelineCache>| {
            let base = world.get_resource::<OutlinePipeline>().unwrap().clone();
            let mut spec = world
                .get_resource_mut::<SpecializedRenderPipelines<OutlinePipeline>>()
                .unwrap();
            let key =
                OutlinePipelineKey::new(target_format).expect("invalid format for OutlineNode");
            spec.specialize(&mut cache, &base, key)
        });

        let query = QueryState::new(world);

        OutlineNode { pipeline_id, query }
    }
}

impl Node for OutlineNode {
    fn input(&self) -> Vec<SlotInfo> {
        vec![
            SlotInfo {
                name: Self::IN_JFA.into(),
                slot_type: SlotType::TextureView,
            },
            SlotInfo {
                name: Self::IN_VIEW.into(),
                slot_type: SlotType::Entity,
            },
        ]
    }

    fn output(&self) -> Vec<SlotInfo> {
        vec![SlotInfo {
            name: Self::OUT_VIEW.into(),
            slot_type: SlotType::Entity,
        }]
    }

    fn update(&mut self, world: &mut World) {
        self.query.update_archetypes(world)
    }

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let camera = graph.get_input_entity(Self::IN_VIEW)?.clone();
        let target = &self.query.get_manual(world, camera).unwrap().target;

        let windows = world.resource::<ExtractedWindows>();
        let images = world.resource::<RenderAssets<Image>>();
        let target_view = target.get_texture_view(windows, images).unwrap();

        graph.set_output(Self::OUT_VIEW, camera)?;

        let res = world.get_resource::<OutlineResources>().unwrap();

        let pipelines = world.get_resource::<PipelineCache>().unwrap();
        let pipeline = match pipelines.get_render_pipeline(self.pipeline_id) {
            Some(p) => p,
            None => return Ok(()),
        };

        let render_pass = render_context
            .command_encoder
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("jfa_outline"),
                color_attachments: &[RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: true,
                    },
                }],
                // TODO: support outlines being occluded by world geometry
                depth_stencil_attachment: None,
            });

        let mut tracked_pass = TrackedRenderPass::new(render_pass);
        tracked_pass.set_render_pipeline(&pipeline);
        tracked_pass.set_bind_group(0, &res.dimensions_bind_group, &[]);
        tracked_pass.set_bind_group(1, &res.primary_outline_bind_group, &[]);
        tracked_pass.draw(0..3, 0..1);

        Ok(())
    }
}
