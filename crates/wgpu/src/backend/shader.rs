use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::{AddressSpace, ImageClass, ImageDimension, ResourceBinding, ScalarKind, TypeInner};

/// How the shader's push constant block (`var<immediate>`) reaches the GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushMode {
    /// Kept as `var<immediate>`; the device has immediate data.
    Immediates,
    /// Rewritten to a `var<uniform>` at `@group(group) @binding(0)`, bound with
    /// a dynamic offset into a ring buffer.
    Uniform { group: u32 },
}

pub struct ProcessedShader {
    pub source: String,
    pub bindings: Vec<ShaderBinding>,
    /// Bytes the push constant block spans, if the shader has one.
    pub push_size: Option<u32>,
}

#[derive(Clone, Copy)]
pub struct ShaderBinding {
    pub group: u32,
    pub binding: u32,
    pub ty: wgpu::BindingType,
}

pub fn process(wgsl: &str, push: PushMode) -> Result<ProcessedShader, String> {
    let mut module =
        naga::front::wgsl::parse_str(wgsl).map_err(|error| error.emit_to_string(wgsl))?;

    let mut rewritten = false;
    let mut push_ty = None;
    for (_, global) in module.global_variables.iter_mut() {
        if global.space != AddressSpace::Immediate {
            continue;
        }
        push_ty = Some(global.ty);
        if let PushMode::Uniform { group } = push {
            global.space = AddressSpace::Uniform;
            global.binding = Some(ResourceBinding { group, binding: 0 });
            rewritten = true;
        }
    }

    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| error.emit_to_string(wgsl))?;

    let push_size = push_ty.map(|ty| module.types[ty].inner.size(module.to_ctx()));

    let mut bindings = Vec::new();
    for (_, global) in module.global_variables.iter() {
        let Some(binding) = global.binding else {
            continue;
        };
        if rewritten
            && global.space == AddressSpace::Uniform
            && is_push(&module, global.ty, push_ty)
        {
            continue;
        }
        bindings.push(ShaderBinding {
            group: binding.group,
            binding: binding.binding,
            ty: binding_type(&module, global.space, global.ty)?,
        });
    }

    let source = if rewritten {
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .map_err(|error| format!("failed to write WGSL: {error}"))?
    } else {
        wgsl.to_owned()
    };
    Ok(ProcessedShader {
        source,
        bindings,
        push_size,
    })
}

fn is_push(
    _module: &naga::Module,
    ty: naga::Handle<naga::Type>,
    push_ty: Option<naga::Handle<naga::Type>>,
) -> bool {
    push_ty == Some(ty)
}

fn binding_type(
    module: &naga::Module,
    space: AddressSpace,
    ty: naga::Handle<naga::Type>,
) -> Result<wgpu::BindingType, String> {
    Ok(match space {
        AddressSpace::Uniform => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        AddressSpace::Storage { access } => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage {
                read_only: !access.contains(naga::StorageAccess::STORE),
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        AddressSpace::Handle => match module.types[ty].inner {
            TypeInner::Sampler { comparison } => wgpu::BindingType::Sampler(if comparison {
                wgpu::SamplerBindingType::Comparison
            } else {
                wgpu::SamplerBindingType::Filtering
            }),
            TypeInner::Image {
                dim,
                arrayed,
                class,
            } => {
                let view_dimension = match (dim, arrayed) {
                    (ImageDimension::D1, _) => wgpu::TextureViewDimension::D1,
                    (ImageDimension::D2, false) => wgpu::TextureViewDimension::D2,
                    (ImageDimension::D2, true) => wgpu::TextureViewDimension::D2Array,
                    (ImageDimension::D3, _) => wgpu::TextureViewDimension::D3,
                    (ImageDimension::Cube, false) => wgpu::TextureViewDimension::Cube,
                    (ImageDimension::Cube, true) => wgpu::TextureViewDimension::CubeArray,
                };
                let (sample_type, multisampled) = match class {
                    ImageClass::Sampled { kind, multi } => (
                        match kind {
                            ScalarKind::Sint => wgpu::TextureSampleType::Sint,
                            ScalarKind::Uint => wgpu::TextureSampleType::Uint,
                            _ => wgpu::TextureSampleType::Float { filterable: true },
                        },
                        multi,
                    ),
                    ImageClass::Depth { multi } => (wgpu::TextureSampleType::Depth, multi),
                    ImageClass::Storage { .. } | ImageClass::External => {
                        return Err("storage and external textures are not part of the RHI".into());
                    }
                };
                wgpu::BindingType::Texture {
                    sample_type,
                    view_dimension,
                    multisampled,
                }
            }
            _ => return Err("unsupported handle binding in a shader".into()),
        },
        other => return Err(format!("unsupported binding address space {other:?}")),
    })
}
