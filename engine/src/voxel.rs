//! Voxel meshing: one RGBA sprite frame to one 3D model.
//!
//! Every asset already resolves to RGBA frames, so extruding a frame's opaque
//! pixels into a slab gives a real model of that frame with no new art, no mesh
//! format and no importer change. That is the whole reason 3D can be a rendering
//! mode over the existing pipeline rather than a second asset pipeline.
//!
//! Model space: one voxel is one unit, x right, y **down** to match world space,
//! and the model is centred on x and z with `y = 0` at its top. A yaw therefore
//! turns the model about its own vertical axis without also moving it.
//!
//! A pixel becomes one box of the full slab depth rather than a stack of cubes:
//! the interior layers of a stack are never visible, and collapsing them is what
//! keeps a 48-wide pet under two thousand quads.

use image::RgbaImage;

/// Alpha at or above which a pixel becomes solid.
///
/// Voxels are opaque. A depth-tested translucent voxel would need the mesh
/// sorted back to front per frame per camera, which buys nothing for pixel art
/// whose alpha is almost always 0 or 255.
pub const OPAQUE_ALPHA_THRESHOLD: u8 = 128;

/// Widest voxel grid a frame is extruded at.
///
/// A 192x208 pet frame is 39,936 pixels, and 74 of those frames extruded whole
/// is millions of triangles. Frames are resampled to this before extruding,
/// exactly as `sprite_sources.TERMINAL_SPRITE_MAX_WIDTH` already fits art for
/// the half-block renderer.
pub const DEFAULT_MAX_WIDTH: u32 = 48;
/// Slab thickness, in voxels.
pub const DEFAULT_DEPTH: u32 = 4;

const NORMAL_UNIT: i8 = 127;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoxelOptions {
    pub max_width: u32,
    pub depth: u32,
}

impl Default for VoxelOptions {
    fn default() -> Self {
        Self {
            max_width: DEFAULT_MAX_WIDTH,
            depth: DEFAULT_DEPTH,
        }
    }
}

/// One mesh corner.
///
/// `normal` is `Snorm8x4` and `colour` is `Unorm8x4` on the GPU: the normal of an
/// axis-aligned face is exactly one unit on one axis, and a colour is the source
/// pixel's own bytes, so neither needs a float. It also makes the parity golden
/// exact — a colour that went through a divide would drift between f32 and f64.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [i8; 4],
    pub colour: [u8; 4],
}

/// The six faces of a voxel box, named for the direction they face in model
/// space. `y` is down, so `Top` faces negative y.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

impl Face {
    fn normal(self) -> [i8; 4] {
        match self {
            Face::Front => [0, 0, NORMAL_UNIT, 0],
            Face::Back => [0, 0, -NORMAL_UNIT, 0],
            Face::Left => [-NORMAL_UNIT, 0, 0, 0],
            Face::Right => [NORMAL_UNIT, 0, 0, 0],
            Face::Top => [0, -NORMAL_UNIT, 0, 0],
            Face::Bottom => [0, NORMAL_UNIT, 0, 0],
        }
    }

    /// The four corners, in order, of this face of the box spanning
    /// `[min, max]` on each axis.
    fn corners(self, min: [f32; 3], max: [f32; 3]) -> [[f32; 3]; 4] {
        let (left, right) = (min[0], max[0]);
        let (top, bottom) = (min[1], max[1]);
        let (back, front) = (min[2], max[2]);
        match self {
            Face::Front => [
                [left, top, front],
                [left, bottom, front],
                [right, bottom, front],
                [right, top, front],
            ],
            Face::Back => [
                [right, top, back],
                [right, bottom, back],
                [left, bottom, back],
                [left, top, back],
            ],
            Face::Left => [
                [left, top, back],
                [left, bottom, back],
                [left, bottom, front],
                [left, top, front],
            ],
            Face::Right => [
                [right, top, front],
                [right, bottom, front],
                [right, bottom, back],
                [right, top, back],
            ],
            Face::Top => [
                [left, top, back],
                [left, top, front],
                [right, top, front],
                [right, top, back],
            ],
            Face::Bottom => [
                [left, bottom, front],
                [left, bottom, back],
                [right, bottom, back],
                [right, bottom, front],
            ],
        }
    }
}

/// A frame's opaque pixels, resampled to the voxel grid.
///
/// Nearest neighbour rather than the area average `resample.lua` uses for
/// sprites: voxel occupancy is a binary decision, and an area average puts a
/// partly covered pixel either side of a coverage threshold where f32 and f64
/// fall on opposite sides. That is the knife edge the parity harnesses exist to
/// avoid, and here it is avoidable outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoxelGrid {
    pub cols: u32,
    pub rows: u32,
    /// Row-major RGBA, `cols * rows * 4` bytes. A pixel below the alpha
    /// threshold is stored as four zero bytes.
    pub pixels: Vec<u8>,
}

impl VoxelGrid {
    pub fn fit(frame: &RgbaImage, max_width: u32) -> Self {
        let (source_cols, source_rows) = frame.dimensions();
        let cols = source_cols.min(max_width.max(1)).max(1);
        let rows = if cols == source_cols {
            source_rows.max(1)
        } else {
            ((source_rows as u64 * cols as u64) / source_cols.max(1) as u64).max(1) as u32
        };

        let mut pixels = vec![0u8; (cols as usize) * (rows as usize) * 4];
        for row in 0..rows {
            let source_row =
                (row as u64 * source_rows as u64 / rows as u64).min(source_rows as u64 - 1) as u32;
            for col in 0..cols {
                let source_col = (col as u64 * source_cols as u64 / cols as u64)
                    .min(source_cols as u64 - 1) as u32;
                let source = frame.get_pixel(source_col, source_row).0;
                if source[3] < OPAQUE_ALPHA_THRESHOLD {
                    continue;
                }
                let offset = ((row as usize) * (cols as usize) + col as usize) * 4;
                pixels[offset..offset + 3].copy_from_slice(&source[0..3]);
                pixels[offset + 3] = u8::MAX;
            }
        }

        Self { cols, rows, pixels }
    }

    pub fn is_opaque(&self, col: i64, row: i64) -> bool {
        if col < 0 || row < 0 || col >= self.cols as i64 || row >= self.rows as i64 {
            return false;
        }
        let offset = ((row as usize) * (self.cols as usize) + col as usize) * 4;
        self.pixels[offset + 3] >= OPAQUE_ALPHA_THRESHOLD
    }

    pub fn colour(&self, col: u32, row: u32) -> [u8; 4] {
        let offset = ((row as usize) * (self.cols as usize) + col as usize) * 4;
        [
            self.pixels[offset],
            self.pixels[offset + 1],
            self.pixels[offset + 2],
            self.pixels[offset + 3],
        ]
    }
}

/// One frame's model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoxelMesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    /// Grid the mesh was built on, in voxels: `[cols, rows, depth]`. The
    /// instance transform scales by this to reach pixels.
    pub extent: [u32; 3],
}

impl VoxelMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn quad_count(&self) -> usize {
        self.indices.len() / 6
    }
}

/// Extrudes one frame into a mesh.
///
/// A face is emitted only where the neighbour that would hide it is transparent,
/// so a solid pet costs two quads a pixel plus its silhouette rather than six.
pub fn build(frame: &RgbaImage, options: VoxelOptions) -> VoxelMesh {
    let grid = VoxelGrid::fit(frame, options.max_width);
    let depth = options.depth.max(1);
    let half_width = grid.cols as f32 * 0.5;
    let half_depth = depth as f32 * 0.5;

    let mut mesh = VoxelMesh {
        extent: [grid.cols, grid.rows, depth],
        ..Default::default()
    };

    for row in 0..grid.rows {
        for col in 0..grid.cols {
            if !grid.is_opaque(col as i64, row as i64) {
                continue;
            }
            let min = [col as f32 - half_width, row as f32, -half_depth];
            let max = [min[0] + 1.0, min[1] + 1.0, half_depth];
            let colour = grid.colour(col, row);

            for face in exposed_faces(&grid, col, row) {
                push_quad(&mut mesh, face, (min, max), colour);
            }
        }
    }

    mesh
}

/// Which of a voxel's faces something else is not already hiding.
///
/// Front and back always show: the slab is one box deep, so nothing is behind
/// either of them.
fn exposed_faces(grid: &VoxelGrid, col: u32, row: u32) -> Vec<Face> {
    let (col, row) = (col as i64, row as i64);
    let mut faces = vec![Face::Front, Face::Back];
    for (face, neighbour) in [
        (Face::Left, (col - 1, row)),
        (Face::Right, (col + 1, row)),
        (Face::Top, (col, row - 1)),
        (Face::Bottom, (col, row + 1)),
    ] {
        if !grid.is_opaque(neighbour.0, neighbour.1) {
            faces.push(face);
        }
    }
    faces
}

fn push_quad(mesh: &mut VoxelMesh, face: Face, span: ([f32; 3], [f32; 3]), colour: [u8; 4]) {
    let base = mesh.vertices.len() as u32;
    let normal = face.normal();
    for position in face.corners(span.0, span.1) {
        mesh.vertices.push(MeshVertex {
            position,
            normal,
            colour,
        });
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}
