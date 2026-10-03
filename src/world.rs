use std::collections::HashMap;

pub const CHUNK_SIZE: i32 = 16;
pub const WORLD_HEIGHT: i32 = 64;
pub const RENDER_DISTANCE: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
}

#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub block: Block,
    pub corners: [[f32; 3]; 4],
}

pub struct Chunk {
    /// Block storage is y-major: 16 * 16 * 64 cells per chunk.
    pub blocks: Vec<Block>,
    pub faces: Vec<Face>,
}

/// Seeded 16x16x64 terrain. Chunks retain only exposed faces for rendering.
pub struct World {
    seed: u64,
    chunks: HashMap<(i32, i32), Chunk>,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            chunks: HashMap::new(),
        }
    }

    pub fn ensure_render_distance(&mut self, cx: i32, cz: i32) {
        for z in cz - RENDER_DISTANCE..=cz + RENDER_DISTANCE {
            for x in cx - RENDER_DISTANCE..=cx + RENDER_DISTANCE {
                if (x - cx).pow(2) + (z - cz).pow(2) <= RENDER_DISTANCE.pow(2)
                    && !self.chunks.contains_key(&(x, z))
                {
                    self.chunks.insert((x, z), self.generate_chunk(x, z));
                }
            }
        }
    }

    pub fn visible_faces(&self, cx: i32, cz: i32) -> impl Iterator<Item = &Face> {
        self.chunks
            .iter()
            .filter_map(move |(&(x, z), chunk)| {
                ((x - cx).pow(2) + (z - cz).pow(2) <= RENDER_DISTANCE.pow(2))
                    .then_some(chunk.faces.as_slice())
            })
            .flatten()
    }

    fn generate_chunk(&self, chunk_x: i32, chunk_z: i32) -> Chunk {
        let mut blocks = vec![Block::Air; (CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT) as usize];
        let mut heights = [[0_i32; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];
        let ox = chunk_x * CHUNK_SIZE;
        let oz = chunk_z * CHUNK_SIZE;
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let h = self.surface_height(ox + lx, oz + lz);
                heights[lz as usize][lx as usize] = h;
                for y in 0..=h {
                    blocks[block_index(lx, y, lz)] = block_at_height(h, y);
                }
            }
        }
        let mut faces = Vec::new();
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let x = ox + lx;
                let z = oz + lz;
                let h = heights[lz as usize][lx as usize];
                add_face(
                    &mut faces,
                    blocks[block_index(lx, h, lz)],
                    x,
                    h,
                    z,
                    FaceDirection::Top,
                );
                for (dx, dz, dir) in [
                    (-1, 0, FaceDirection::West),
                    (1, 0, FaceDirection::East),
                    (0, -1, FaceDirection::North),
                    (0, 1, FaceDirection::South),
                ] {
                    for y in self.surface_height(x + dx, z + dz) + 1..=h {
                        add_face(&mut faces, blocks[block_index(lx, y, lz)], x, y, z, dir);
                    }
                }
            }
        }
        Chunk { blocks, faces }
    }

    fn surface_height(&self, x: i32, z: i32) -> i32 {
        // Each axis is a seeded walk: 20% unchanged, 60% +/-1, 10% +/-2,
        // and the rest divided evenly across +/-3, +/-4, +/-5. Averaging
        // both walks makes a continuous 2D terrain with <= 5-layer neighbours.
        (32 + (self.axis_walk(x, 0x9e37_79b9_7f4a_7c15) + self.axis_walk(z, 0xd1b5_4a32_d192_ed03))
            / 2)
        .clamp(1, WORLD_HEIGHT - 2)
    }

    fn axis_walk(&self, coordinate: i32, salt: u64) -> i32 {
        let direction = if coordinate >= 0 { 1 } else { -1 };
        (1..=coordinate.unsigned_abs())
            .map(|step| terrain_step(hash(self.seed, step as i32 * direction, salt)))
            .sum()
    }
}

fn block_index(x: i32, y: i32, z: i32) -> usize {
    ((y * CHUNK_SIZE * CHUNK_SIZE) + (z * CHUNK_SIZE) + x) as usize
}
fn block_at_height(surface: i32, y: i32) -> Block {
    if y == surface {
        Block::Grass
    } else if y >= surface - 4 {
        Block::Dirt
    } else {
        Block::Stone
    }
}
fn terrain_step(r: u64) -> i32 {
    match r % 100 {
        0..20 => 0,
        20..50 => -1,
        50..80 => 1,
        80..85 => -2,
        85..90 => 2,
        90..92 => -3,
        92..94 => 3,
        94..96 => -4,
        96..98 => 4,
        98 => -5,
        _ => 5,
    }
}
fn hash(seed: u64, coordinate: i32, salt: u64) -> u64 {
    let mut v = seed ^ salt ^ (coordinate as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    v ^= v >> 30;
    v = v.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    v ^= v >> 27;
    v = v.wrapping_mul(0x94d0_49bb_1331_11eb);
    v ^ (v >> 31)
}
#[derive(Clone, Copy)]
enum FaceDirection {
    Top,
    West,
    East,
    North,
    South,
}
fn add_face(faces: &mut Vec<Face>, block: Block, x: i32, y: i32, z: i32, d: FaceDirection) {
    let (x, y, z) = (x as f32, y as f32, z as f32);
    let corners = match d {
        FaceDirection::Top => [
            [x, y + 1.0, z],
            [x, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z],
        ],
        FaceDirection::West => [
            [x, y, z + 1.0],
            [x, y + 1.0, z + 1.0],
            [x, y + 1.0, z],
            [x, y, z],
        ],
        FaceDirection::East => [
            [x + 1.0, y, z],
            [x + 1.0, y + 1.0, z],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y, z + 1.0],
        ],
        FaceDirection::North => [
            [x, y, z],
            [x, y + 1.0, z],
            [x + 1.0, y + 1.0, z],
            [x + 1.0, y, z],
        ],
        FaceDirection::South => [
            [x + 1.0, y, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x, y + 1.0, z + 1.0],
            [x, y, z + 1.0],
        ],
    };
    faces.push(Face { block, corners });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terrain_is_bounded_and_continuous() {
        let world = World::new(1234);
        for z in -16..16 {
            for x in -16..16 {
                let h = world.surface_height(x, z);
                assert!((1..WORLD_HEIGHT - 1).contains(&h));
                assert!((h - world.surface_height(x + 1, z)).abs() <= 5);
                assert!((h - world.surface_height(x, z + 1)).abs() <= 5);
            }
        }
    }
}
