pub type Shader<'a> = &'a [u8];

macro_rules! flags {
    ($(#[$doc:meta])* $name:ident { $($flag:ident = $bit:literal),* $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub struct $name(u8);

        impl $name {
            $(pub const $flag: Self = Self(1 << $bit);)*

            pub fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }
        }

        impl std::ops::BitOr for $name {
            type Output = Self;

            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }
    };
}

flags! {
    ShaderStages { VERTEX = 0, FRAGMENT = 1, COMPUTE = 2 }
}

impl ShaderStages {
    pub const VERTEX_FRAGMENT: Self = Self(Self::VERTEX.0 | Self::FRAGMENT.0);
}

flags! {
    BufferUsages {
        VERTEX = 0,
        INDEX = 1,
        UNIFORM = 2,
        STORAGE = 3,
        INDIRECT = 4,
        COPY_SRC = 5,
        COPY_DST = 6,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Extent {
    pub width: u32,
    pub height: u32,
}

impl Extent {
    pub fn aspect_ratio(self) -> f32 {
        self.width as f32 / self.height.max(1) as f32
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn covering(extent: Extent) -> Self {
        Self {
            x: 0,
            y: 0,
            width: extent.width,
            height: extent.height,
        }
    }
}

#[derive(Clone, Copy)]
pub struct TextureRange {
    pub first_mip: u32,
    pub mips: u32,
    pub first_layer: u32,
    pub layers: u32,
}

impl TextureRange {
    pub const BASE: Self = Self {
        first_mip: 0,
        mips: 1,
        first_layer: 0,
        layers: 1,
    };
}

#[derive(Clone, Copy)]
pub struct BufferCopy {
    pub src_offset: u64,
    pub dst_offset: u64,
    pub size: u64,
}

#[derive(Clone, Copy)]
pub struct TexelCopy {
    pub buffer_offset: u64,
    pub mip_level: u32,
    pub layer: u32,
    pub origin: [u32; 2],
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy)]
pub struct Blit {
    pub src_level: u32,
    pub src_size: [u32; 2],
    pub dst_level: u32,
    pub dst_size: [u32; 2],
    pub layers: u32,
}

#[derive(Clone, Copy)]
pub enum TextureUse {
    Undefined,
    CopySrc,
    CopyDst,
    ShaderRead,
    ColorAttachment,
    Present,
    DepthAttachment,
    DepthRead,
    DepthCopySrc,
    DepthCopyDst,
}

#[derive(Clone, Copy)]
pub struct TextureTransition<Texture> {
    pub texture: Texture,
    pub range: TextureRange,
    pub from: TextureUse,
    pub to: TextureUse,
}

impl<Texture> TextureTransition<Texture> {
    pub fn color(texture: Texture, from: TextureUse, to: TextureUse) -> Self {
        Self {
            texture,
            range: TextureRange::BASE,
            from,
            to,
        }
    }
}

#[derive(Clone, Copy)]
pub enum BufferUse {
    CopySrc,
    CopyDst,
    StorageWrite,
    Indirect,
    HostRead,
}

#[derive(Clone, Copy, Debug)]
pub struct IndirectDrawFeatures {
    pub count: bool,
    pub multi_draw: bool,
    pub gpu_cull: bool,
}

pub enum Acquired {
    Image { index: u32, suboptimal: bool },
    OutOfDate,
}

pub enum Presented {
    Done,
    Suboptimal,
    OutOfDate,
}

#[derive(Clone, Copy)]
pub enum MemoryLocation {
    Host,
    Device,
    Readback,
}

flags! {
    TextureUsages {
        COPY_SRC = 0,
        COPY_DST = 1,
        TEXTURE_BINDING = 2,
        RENDER_ATTACHMENT = 3,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextureFormat {
    R8Unorm,
    Rg8Unorm,
    Rgba8Unorm,
    Rgba16Unorm,
    Rgb10a2Unorm,
    R16Float,
    Rg16Float,
    Rgba16Float,
    R32Float,
    Rg32Float,
    Rgba32Float,
    Rg11b10Float,
    Depth32Float,
}

impl TextureFormat {
    pub fn texel_size(self) -> u32 {
        match self {
            Self::R8Unorm => 1,
            Self::Rg8Unorm | Self::R16Float => 2,
            Self::Rgba8Unorm
            | Self::Rgb10a2Unorm
            | Self::Rg16Float
            | Self::R32Float
            | Self::Rg11b10Float
            | Self::Depth32Float => 4,
            Self::Rgba16Unorm | Self::Rgba16Float | Self::Rg32Float => 8,
            Self::Rgba32Float => 16,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextureViewDimension {
    D2,
    D2Array,
    Cube,
}

pub struct TextureDesc<'a> {
    pub label: &'a str,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub mip_levels: u32,
    pub format: TextureFormat,
    pub usage: TextureUsages,
    pub dimension: TextureViewDimension,
}

impl TextureDesc<'static> {
    pub fn swapchain_depth(extent: Extent) -> Self {
        Self::new(
            "depth_image",
            extent.width,
            extent.height,
            TextureFormat::Depth32Float,
            TextureUsages::RENDER_ATTACHMENT,
        )
    }
}

impl<'a> TextureDesc<'a> {
    pub fn new(
        label: &'a str,
        width: u32,
        height: u32,
        format: TextureFormat,
        usage: TextureUsages,
    ) -> Self {
        Self {
            label,
            width,
            height,
            layers: 1,
            mip_levels: 1,
            format,
            usage,
            dimension: TextureViewDimension::D2,
        }
    }
}

#[derive(Clone, Copy)]
pub enum LoadOp<T> {
    Clear(T),
    Load,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreOp {
    Store,
    Discard,
}

#[derive(Clone, Copy)]
pub struct ColorAttachment<Texture, Format> {
    pub texture: Texture,
    pub format: Format,
    pub load: LoadOp<[f32; 4]>,
    pub store: StoreOp,
}

#[derive(Clone, Copy)]
pub struct DepthAttachment<Texture> {
    pub texture: Texture,
    pub load: LoadOp<f32>,
    pub store: StoreOp,
}

impl<Texture> DepthAttachment<Texture> {
    pub fn cleared(texture: Texture) -> Self {
        Self {
            texture,
            load: LoadOp::Clear(1.0),
            store: StoreOp::Discard,
        }
    }
}

pub const MAX_COLOR_TARGETS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PipelineTargets<Format> {
    pub color: Option<Format>,
    pub extra_colors: [Option<Format>; MAX_COLOR_TARGETS - 1],
    pub depth: bool,
}

impl<Format: Copy> PipelineTargets<Format> {
    pub const fn new(color: Format, depth: bool) -> Self {
        Self {
            color: Some(color),
            extra_colors: [None; MAX_COLOR_TARGETS - 1],
            depth,
        }
    }

    pub const fn depth_only() -> Self {
        Self {
            color: None,
            extra_colors: [None; MAX_COLOR_TARGETS - 1],
            depth: true,
        }
    }
}

pub struct RenderPassDesc<Texture, Format> {
    pub color: Option<ColorAttachment<Texture, Format>>,
    pub extra_colors: Vec<ColorAttachment<Texture, Format>>,
    pub depth: Option<DepthAttachment<Texture>>,
    pub extent: Extent,
}

impl<Texture, Format> RenderPassDesc<Texture, Format> {
    pub fn swapchain(
        image: Texture,
        format: Format,
        load: LoadOp<[f32; 4]>,
        depth: Texture,
        extent: Extent,
    ) -> Self {
        Self {
            color: Some(ColorAttachment {
                texture: image,
                format,
                load,
                store: StoreOp::Store,
            }),
            extra_colors: Vec::new(),
            depth: Some(DepthAttachment::cleared(depth)),
            extent,
        }
    }
}

#[derive(Clone, Copy)]
pub enum FilterMode {
    Nearest,
    Linear,
}

#[derive(Clone, Copy)]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
}

#[derive(Clone, Copy)]
pub struct SamplerDesc {
    pub filter: FilterMode,
    pub address_mode: AddressMode,
    pub mipmap_filter: FilterMode,
    pub lod_max_clamp: f32,
    pub compare: Option<CompareFunction>,
}

impl SamplerDesc {
    pub const NEAREST: Self = Self::new(FilterMode::Nearest, AddressMode::ClampToEdge);
    pub const NEAREST_REPEAT: Self = Self::new(FilterMode::Nearest, AddressMode::Repeat);
    pub const LINEAR: Self = Self::new(FilterMode::Linear, AddressMode::ClampToEdge);
    pub const LINEAR_REPEAT: Self = Self::new(FilterMode::Linear, AddressMode::Repeat);

    pub const fn new(filter: FilterMode, address_mode: AddressMode) -> Self {
        Self {
            filter,
            address_mode,
            mipmap_filter: FilterMode::Nearest,
            lod_max_clamp: 0.0,
            compare: None,
        }
    }

    pub fn with_mips(self, mip_levels: u32) -> Self {
        Self {
            mipmap_filter: if mip_levels > 1 {
                FilterMode::Linear
            } else {
                FilterMode::Nearest
            },
            lod_max_clamp: mip_levels.saturating_sub(1) as f32,
            ..self
        }
    }
}

#[derive(Clone, Copy)]
pub enum BindingType {
    UniformBuffer,
    StorageBuffer,
    Texture,
    Sampler,
}

#[derive(Clone, Copy)]
pub struct BindGroupLayoutEntry {
    pub ty: BindingType,
    pub visibility: ShaderStages,
}

impl BindGroupLayoutEntry {
    pub const fn uniform(visibility: ShaderStages) -> Self {
        Self {
            ty: BindingType::UniformBuffer,
            visibility,
        }
    }

    pub const fn storage(visibility: ShaderStages) -> Self {
        Self {
            ty: BindingType::StorageBuffer,
            visibility,
        }
    }

    pub const fn texture(visibility: ShaderStages) -> Self {
        Self {
            ty: BindingType::Texture,
            visibility,
        }
    }

    pub const fn sampler(visibility: ShaderStages) -> Self {
        Self {
            ty: BindingType::Sampler,
            visibility,
        }
    }
}

#[derive(Clone, Copy)]
pub struct PushConstants {
    pub visibility: ShaderStages,
    pub size: u32,
}

#[derive(Clone, Copy)]
pub enum VertexFormat {
    Float32,
    Float32x2,
    Float32x3,
    Float32x4,
    Sint32x3,
    Uint32,
    Sint16x2,
    Uint16,
    Unorm16,
    Unorm16x2,
    Uint8x2,
    Unorm8x4,
    Snorm8x4,
}

impl VertexFormat {
    pub const fn size(self) -> u32 {
        match self {
            Self::Uint8x2 | Self::Uint16 | Self::Unorm16 => 2,
            Self::Float32
            | Self::Uint32
            | Self::Sint16x2
            | Self::Unorm16x2
            | Self::Unorm8x4
            | Self::Snorm8x4 => 4,
            Self::Float32x2 => 8,
            Self::Float32x3 | Self::Sint32x3 => 12,
            Self::Float32x4 => 16,
        }
    }
}

#[derive(Clone, Copy)]
pub struct VertexAttribute {
    pub location: u32,
    pub format: VertexFormat,
    pub offset: u32,
}

pub const fn packed_attributes<const N: usize>(
    first_location: u32,
    formats: [VertexFormat; N],
) -> [VertexAttribute; N] {
    let mut attributes = [VertexAttribute {
        location: 0,
        format: VertexFormat::Float32,
        offset: 0,
    }; N];
    let mut offset = 0;
    let mut i = 0;
    while i < N {
        attributes[i] = VertexAttribute {
            location: first_location + i as u32,
            format: formats[i],
            offset,
        };
        offset += formats[i].size();
        i += 1;
    }
    attributes
}

#[derive(Clone, Copy)]
pub enum VertexStepMode {
    Vertex,
    Instance,
}

#[derive(Clone, Copy)]
pub struct VertexBufferLayout<'a> {
    pub stride: u32,
    pub step_mode: VertexStepMode,
    pub attributes: &'a [VertexAttribute],
}

#[derive(Clone, Copy)]
pub enum Topology {
    TriangleList,
    LineList,
}

#[derive(Clone, Copy)]
pub enum CullMode {
    None,
    Front,
    Back,
}

#[derive(Clone, Copy)]
pub enum FrontFace {
    Ccw,
    Cw,
}

#[derive(Clone, Copy)]
pub enum CompareFunction {
    Less,
    LessEqual,
}

#[derive(Clone, Copy)]
pub struct DepthBias {
    pub constant: i32,
    pub slope: f32,
}

#[derive(Clone, Copy)]
pub struct DepthState {
    pub write: bool,
    pub compare: CompareFunction,
    pub bias: Option<DepthBias>,
}

impl DepthState {
    pub const WRITE: Self = Self {
        write: true,
        compare: CompareFunction::LessEqual,
        bias: None,
    };
    pub const READ_ONLY: Self = Self {
        write: false,
        ..Self::WRITE
    };
}

#[derive(Clone, Copy)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcAlpha,
}

#[derive(Clone, Copy)]
pub struct BlendComponent {
    pub src: BlendFactor,
    pub dst: BlendFactor,
}

impl BlendComponent {
    pub const REPLACE: Self = Self {
        src: BlendFactor::One,
        dst: BlendFactor::Zero,
    };
    const OVER: Self = Self {
        src: BlendFactor::SrcAlpha,
        dst: BlendFactor::OneMinusSrcAlpha,
    };
    const PREMULTIPLIED_OVER: Self = Self {
        src: BlendFactor::One,
        dst: BlendFactor::OneMinusSrcAlpha,
    };
}

#[derive(Clone, Copy)]
pub struct BlendState {
    pub color: BlendComponent,
    pub alpha: BlendComponent,
}

impl BlendState {
    pub const TRANSLUCENT: Self = Self {
        color: BlendComponent::OVER,
        alpha: BlendComponent::PREMULTIPLIED_OVER,
    };
    pub const TRANSLUCENT_PREMULTIPLIED_ALPHA: Self = Self {
        color: BlendComponent::PREMULTIPLIED_OVER,
        alpha: BlendComponent::PREMULTIPLIED_OVER,
    };
    pub const OVERLAY: Self = Self {
        color: BlendComponent {
            src: BlendFactor::SrcAlpha,
            dst: BlendFactor::One,
        },
        alpha: BlendComponent::REPLACE,
    };
    pub const INVERT: Self = Self {
        color: BlendComponent {
            src: BlendFactor::OneMinusDstColor,
            dst: BlendFactor::OneMinusSrcColor,
        },
        alpha: BlendComponent::REPLACE,
    };
    pub const OVER_REPLACE_ALPHA: Self = Self {
        color: BlendComponent::OVER,
        alpha: BlendComponent::REPLACE,
    };
}

#[derive(Clone, Copy)]
pub struct RenderPipelineDesc<'a> {
    pub label: &'a str,
    pub vertex_shader: Shader<'a>,
    pub fragment_shader: Shader<'a>,
    pub vertex_buffers: &'a [VertexBufferLayout<'a>],
    pub topology: Topology,
    pub cull_mode: CullMode,
    pub front_face: FrontFace,
    pub depth: Option<DepthState>,
    pub blend: Option<BlendState>,
    pub depth_clamp: bool,
}

impl<'a> RenderPipelineDesc<'a> {
    pub const fn new(
        label: &'a str,
        vertex_shader: Shader<'a>,
        fragment_shader: Shader<'a>,
        vertex_buffers: &'a [VertexBufferLayout<'a>],
    ) -> Self {
        Self {
            label,
            vertex_shader,
            fragment_shader,
            vertex_buffers,
            topology: Topology::TriangleList,
            cull_mode: CullMode::None,
            front_face: FrontFace::Ccw,
            depth: None,
            blend: None,
            depth_clamp: false,
        }
    }
}
