use bevy::{
    prelude::*,
    render::{
        render_resource::{
            std140::{AsStd140, DynamicUniform, Std140},
            AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
            BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType,
            BufferBinding, BufferBindingType, BufferSize, DynamicUniformVec, Extent3d, FilterMode,
            Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages, TextureDescriptor,
            TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
            TextureViewDimension, UniformVec,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::{CachedTexture, TextureCache},
        view::ExtractedWindows,
    },
    window::WindowId,
};

use crate::{jfa, outline, JFA_TEXTURE_FORMAT};

pub struct OutlineResources {
    // Stencil target for initial stencil pass.
    pub stencil_output: CachedTexture,

    pub dimensions_bind_group_layout: BindGroupLayout,
    pub dimensions_buffer: UniformVec<jfa::Dimensions>,
    pub dimensions_bind_group: BindGroup,

    pub sampler: Sampler,
    pub jfa_bind_group_layout: BindGroupLayout,
    // Dynamic uniform buffer containing power-of-two JFA distances from 1 to 32768.
    // TODO: use instance ID instead?
    pub jfa_distance_buffer: DynamicUniformVec<jfa::JumpDist>,
    pub jfa_distance_offsets: Vec<u32>,

    // Bind group for jump flood passes targeting the primary output.
    pub jfa_primary_bind_group: BindGroup,
    // Primary jump flood output.
    pub jfa_primary_output: CachedTexture,

    // Bind group for jump flood passes targeting the secondary output.
    pub jfa_secondary_bind_group: BindGroup,
    // Secondary jump flood output.
    pub jfa_secondary_output: CachedTexture,

    pub outline_params_buffer: UniformVec<outline::OutlineParams>,
    pub outline_bind_group_layout: BindGroupLayout,
    pub primary_outline_bind_group: BindGroup,
    pub secondary_outline_bind_group: BindGroup,
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
            input,
            &self.sampler,
        )
    }
}

fn create_jfa_bind_group(
    device: &RenderDevice,
    layout: &BindGroupLayout,
    label: &str,
    dist_buffer: BindingResource,
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
                resource: BindingResource::TextureView(input),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn create_outline_bind_group(
    device: &RenderDevice,
    layout: &BindGroupLayout,
    label: &str,
    buffer: BindingResource,
    target: &TextureView,
    sampler: &Sampler,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some(label),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: buffer,
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(target),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    })
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

        let dims = jfa::Dimensions::new(size.width, size.height);
        let mut dimensions_buffer = UniformVec::default();
        dimensions_buffer.push(dims);
        dimensions_buffer.write_buffer(&device, &queue);
        let dimensions_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("jfa_dimensions_bind_group_layout"),
                entries: &[BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: BufferSize::new(
                            jfa::Dimensions::std140_size_static() as u64
                        ),
                    },
                    count: None,
                }],
            });

        let dimensions_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("jfa_dimensions_bind_group"),
            layout: &dimensions_bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: dimensions_buffer.binding().unwrap(),
            }],
        });

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
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let mut jfa_distance_buffer = DynamicUniformVec::default();
        let mut jfa_distance_offsets = Vec::new();
        for exp in 0_u32..16 {
            // TODO: this should be a DynamicUniformVec
            let ofs = jfa_distance_buffer.push(jfa::JumpDist {
                dist: 2_u32.pow(exp),
            });

            jfa_distance_offsets.push(ofs);
        }
        jfa_distance_buffer.write_buffer(&device, &queue);

        let jfa_primary_output_desc =
            tex_desc("outline_jfa_primary_output", size, JFA_TEXTURE_FORMAT);
        let jfa_primary_output = textures.get(&device, jfa_primary_output_desc);
        let jfa_secondary_output_desc =
            tex_desc("outline_jfa_secondary_output", size, JFA_TEXTURE_FORMAT);
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
            &jfa_secondary_output.default_view,
            &sampler,
        );
        let jfa_secondary_bind_group = create_jfa_bind_group(
            &device,
            &jfa_bind_group_layout,
            "outline_jfa_secondary_bind_group",
            jfa_distance_buffer.binding().unwrap(),
            &jfa_primary_output.default_view,
            &sampler,
        );

        let mut outline_params_buffer = UniformVec::default();
        outline_params_buffer.push(outline::OutlineParams::new(
            Color::hex("b4a2c8").unwrap(),
            size.width,
            size.height,
            32.0,
        ));
        outline_params_buffer.write_buffer(&device, &queue);

        let outline_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("jfa_outline_bind_group_layout"),
                entries: &[
                    // OutlineParams
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: BufferSize::new(
                                outline::OutlineParams::std140_size_static() as u64,
                            ),
                        },
                        count: None,
                    },
                    // JFA texture
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            sample_type: TextureSampleType::Float { filterable: false },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // Sampler
                    BindGroupLayoutEntry {
                        binding: 2,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                        count: None,
                    },
                ],
            });

        let primary_outline_bind_group = create_outline_bind_group(
            &device,
            &outline_bind_group_layout,
            "jfa_primary_outline_bind_group",
            outline_params_buffer.binding().unwrap(),
            &jfa_primary_output.default_view,
            &sampler,
        );
        let secondary_outline_bind_group = create_outline_bind_group(
            &device,
            &outline_bind_group_layout,
            "jfa_secondary_outline_bind_group",
            outline_params_buffer.binding().unwrap(),
            &jfa_secondary_output.default_view,
            &sampler,
        );

        OutlineResources {
            stencil_output,
            dimensions_bind_group_layout,
            dimensions_buffer,
            dimensions_bind_group,
            jfa_bind_group_layout,
            sampler,
            jfa_distance_buffer,
            jfa_distance_offsets,
            jfa_primary_bind_group,
            jfa_primary_output,
            jfa_secondary_bind_group,
            jfa_secondary_output,
            outline_params_buffer,
            outline_bind_group_layout,
            primary_outline_bind_group,
            secondary_outline_bind_group,
        }
    }
}

pub fn recreate_outline_resources(
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

    let new_dims = jfa::Dimensions::new(size.width, size.height);
    let dims = outline.dimensions_buffer.get_mut(0);
    if *dims != new_dims {
        *dims = new_dims;
        outline.dimensions_buffer.write_buffer(&device, &queue);
    }

    let stencil_desc = tex_desc(
        "outline_stencil_output",
        size,
        TextureFormat::Depth24PlusStencil8,
    );
    outline.stencil_output = textures.get(&device, stencil_desc);

    *outline.outline_params_buffer.get_mut(0) =
        outline::OutlineParams::new(Color::hex("b4a2c8").unwrap(), size.width, size.height, 32.0);
    outline.outline_params_buffer.write_buffer(&device, &queue);

    // The JFA passes ping-pong between the primary and secondary outputs, so
    // when the primary target is recreated, the secondary bind group is
    // recreated, and vice-versa.

    let old_jfa_primary = outline.jfa_primary_output.texture.id();
    let jfa_primary_desc = tex_desc("outline_jfa_primary_output", size, JFA_TEXTURE_FORMAT);
    let jfa_primary_output = textures.get(&device, jfa_primary_desc);
    if jfa_primary_output.texture.id() != old_jfa_primary {
        outline.jfa_primary_output = jfa_primary_output;
        outline.jfa_secondary_bind_group = outline.create_jfa_bind_group(
            &device,
            "outline_jfa_secondary_bind_group",
            &outline.jfa_primary_output.default_view,
        );
        outline.primary_outline_bind_group = create_outline_bind_group(
            &device,
            &outline.outline_bind_group_layout,
            "jfa_primary_outline_bind_group",
            outline.outline_params_buffer.binding().unwrap(),
            &outline.jfa_primary_output.default_view,
            &outline.sampler,
        );
    }

    let old_jfa_secondary = outline.jfa_secondary_output.texture.id();
    let jfa_secondary_desc = tex_desc("outline_jfa_secondary_output", size, JFA_TEXTURE_FORMAT);
    let jfa_secondary_output = textures.get(&device, jfa_secondary_desc);
    if jfa_secondary_output.texture.id() != old_jfa_secondary {
        outline.jfa_secondary_output = jfa_secondary_output;
        outline.jfa_primary_bind_group = outline.create_jfa_bind_group(
            &device,
            "outline_jfa_primary_bind_group",
            &outline.jfa_secondary_output.default_view,
        );
        outline.secondary_outline_bind_group = create_outline_bind_group(
            &device,
            &outline.outline_bind_group_layout,
            "jfa_secondary_outline_bind_group",
            outline.outline_params_buffer.binding().unwrap(),
            &outline.jfa_secondary_output.default_view,
            &outline.sampler,
        );
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
