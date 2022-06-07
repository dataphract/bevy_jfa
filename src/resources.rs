use bevy::{
    prelude::*,
    render::{
        render_resource::{
            AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
            BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType,
            BufferBindingType, DynamicUniformBuffer, Extent3d, FilterMode, Sampler,
            SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType, TextureDescriptor,
            TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
            TextureViewDimension, UniformBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::{CachedTexture, TextureCache},
        view::ExtractedWindows,
    },
    window::WindowId,
};

use crate::{jfa, outline, JFA_TEXTURE_FORMAT};

pub struct OutlineResources {
    // Multisample target for initial mask pass.
    pub mask_multisample: CachedTexture,
    // Resolve target for the above.
    pub mask_output: CachedTexture,

    pub dimensions_bind_group_layout: BindGroupLayout,
    pub dimensions_buffer: UniformBuffer<jfa::Dimensions>,
    pub dimensions_bind_group: BindGroup,

    // Non-filtering sampler for all sampling operations.
    pub sampler: Sampler,

    // Bind group and layout for JFA init pass.
    pub jfa_init_bind_group_layout: BindGroupLayout,
    pub jfa_init_bind_group: BindGroup,

    // Bind group layout for JFA iteration passes.
    pub jfa_bind_group_layout: BindGroupLayout,
    // Dynamic uniform buffer containing power-of-two JFA distances from 1 to 32768.
    // TODO: use instance ID instead?
    pub jfa_distance_buffer: DynamicUniformBuffer<jfa::JumpDist>,
    pub jfa_distance_offsets: Vec<u32>,

    // Bind group for jump flood passes targeting the primary output.
    pub jfa_primary_bind_group: BindGroup,
    // Primary jump flood output.
    pub jfa_primary_output: CachedTexture,

    // Bind group for jump flood passes targeting the secondary output.
    pub jfa_secondary_bind_group: BindGroup,
    // Secondary jump flood output.
    pub jfa_secondary_output: CachedTexture,

    // Bind group layout for sampling JFA results in the outline shader.
    pub outline_src_bind_group_layout: BindGroupLayout,
    // Bind group layout for outline style parameters.
    pub outline_params_bind_group_layout: BindGroupLayout,
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

fn create_outline_src_bind_group(
    device: &RenderDevice,
    layout: &BindGroupLayout,
    label: &str,
    target: &TextureView,
    sampler: &Sampler,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(target),
            },
            BindGroupEntry {
                binding: 1,
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

        let mask_output_desc = tex_desc("outline_mask_output", size, TextureFormat::R8Unorm);
        let mask_multisample_desc = TextureDescriptor {
            label: Some("outline_mask_multisample"),
            sample_count: 4,
            ..mask_output_desc.clone()
        };
        let mask_multisample = textures.get(&device, mask_multisample_desc);
        let mask_output = textures.get(&device, mask_output_desc);

        let dims = jfa::Dimensions::new(size.width, size.height);
        let mut dimensions_buffer = UniformBuffer::from(dims);
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
                        min_binding_size: Some(jfa::Dimensions::min_size()),
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

        let jfa_init_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("outline_jfa_init_bind_group_layout"),
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
        let jfa_init_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("outline_jfa_init_bind_group"),
            layout: &jfa_init_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&mask_output.default_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
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
                        min_binding_size: Some(jfa::JumpDist::min_size()),
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
        let mut jfa_distance_buffer = DynamicUniformBuffer::default();
        let mut jfa_distance_offsets = Vec::new();
        for exp in 0_u32..16 {
            // TODO: this should be a DynamicUniformBuffer
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

        let mut outline_params_buffer = UniformBuffer::from(outline::OutlineParams::new(
            Color::hex("b4a2c8").unwrap(),
            32.0,
        ));
        outline_params_buffer.write_buffer(&device, &queue);

        let outline_src_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("jfa_outline_bind_group_layout"),
                entries: &[
                    // JFA texture
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
                    // Sampler
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                        count: None,
                    },
                ],
            });

        let outline_params_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("jfa_outline_params_bind_group_layout"),
                entries: &[
                    // OutlineParams
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(outline::OutlineParams::min_size()),
                        },
                        count: None,
                    },
                ],
            });

        let primary_outline_bind_group = create_outline_src_bind_group(
            &device,
            &outline_src_bind_group_layout,
            "jfa_primary_outline_src_bind_group",
            &jfa_primary_output.default_view,
            &sampler,
        );
        let secondary_outline_bind_group = create_outline_src_bind_group(
            &device,
            &outline_src_bind_group_layout,
            "jfa_secondary_outline_src_bind_group",
            &jfa_secondary_output.default_view,
            &sampler,
        );

        OutlineResources {
            mask_multisample,
            mask_output,
            dimensions_bind_group_layout,
            dimensions_buffer,
            dimensions_bind_group,
            jfa_init_bind_group_layout,
            jfa_init_bind_group,
            jfa_bind_group_layout,
            sampler,
            jfa_distance_buffer,
            jfa_distance_offsets,
            jfa_primary_bind_group,
            jfa_primary_output,
            jfa_secondary_bind_group,
            jfa_secondary_output,
            outline_src_bind_group_layout,
            outline_params_bind_group_layout,
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
    let primary = match windows.get(&WindowId::primary()) {
        Some(w) => w,
        None => return,
    };

    let size = Extent3d {
        width: primary.physical_width,
        height: primary.physical_height,
        depth_or_array_layers: 1,
    };

    let new_dims = jfa::Dimensions::new(size.width, size.height);
    let dims = outline.dimensions_buffer.get_mut();
    if *dims != new_dims {
        *dims = new_dims;
        outline.dimensions_buffer.write_buffer(&device, &queue);
    }

    let old_mask = outline.mask_multisample.texture.id();
    let mask_output_desc = tex_desc("outline_mask_output", size, TextureFormat::R8Unorm);
    let mask_multisample_desc = TextureDescriptor {
        label: Some("outline_mask_multisample"),
        sample_count: 4,
        ..mask_output_desc.clone()
    };

    // Recreate mask output targets.
    outline.mask_output = textures.get(&device, mask_output_desc);
    outline.mask_multisample = textures.get(&device, mask_multisample_desc);

    if outline.mask_output.texture.id() != old_mask {
        // Recreate JFA init pass bind group
        outline.jfa_init_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("outline_jfa_init_bind_group"),
            layout: &outline.jfa_init_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&outline.mask_output.default_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&outline.sampler),
                },
            ],
        });
    }

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
        outline.primary_outline_bind_group = create_outline_src_bind_group(
            &device,
            &outline.outline_src_bind_group_layout,
            "jfa_primary_outline_bind_group",
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
        outline.secondary_outline_bind_group = create_outline_src_bind_group(
            &device,
            &outline.outline_src_bind_group_layout,
            "jfa_secondary_outline_bind_group",
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
