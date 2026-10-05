use std::collections::HashMap;
use noise::{NoiseFn, Fbm, Perlin};

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
    terrain_noise: Fbm<Perlin>,
    direction_noise: Perlin,
    scale: f64,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            chunks: HashMap::new(),
            terrain_noise: Fbm::<Perlin>::new(seed as u32),
            direction_noise: Perlin::new(seed as u32),
            scale: 0.05,
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

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Block {
        if y < 0 || y >= WORLD_HEIGHT {
            return Block::Air;
        }
        let cx = x.div_euclid(CHUNK_SIZE);
        let cz = z.div_euclid(CHUNK_SIZE);
        let lx = x.rem_euclid(CHUNK_SIZE);
        let lz = z.rem_euclid(CHUNK_SIZE);

        self.chunks
            .get(&(cx, cz))
            .map(|chunk| chunk.blocks[block_index(lx, y, lz)])
            .unwrap_or_else(|| {
                let h = self.surface_height(x, z);
                if y <= h {
                    block_at_height(h, y)
                } else {
                    Block::Air
                }
            })
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if y < 0 || y >= WORLD_HEIGHT {
            return;
        }
        let cx = x.div_euclid(CHUNK_SIZE);
        let cz = z.div_euclid(CHUNK_SIZE);
        let lx = x.rem_euclid(CHUNK_SIZE);
        let lz = z.rem_euclid(CHUNK_SIZE);

        if !self.chunks.contains_key(&(cx, cz)) {
            self.chunks.insert((cx, cz), self.generate_chunk(cx, cz));
        }

        if let Some(chunk) = self.chunks.get_mut(&(cx, cz)) {
            chunk.blocks[block_index(lx, y, lz)] = block;
        }

        // Refresh faces for this chunk and neighbors if on boundary
        self.refresh_chunk_faces(cx, cz);
        if lx == 0 { self.refresh_chunk_faces(cx - 1, cz); }
        if lx == CHUNK_SIZE - 1 { self.refresh_chunk_faces(cx + 1, cz); }
        if lz == 0 { self.refresh_chunk_faces(cx, cz - 1); }
        if lz == CHUNK_SIZE - 1 { self.refresh_chunk_faces(cx, cz + 1); }
    }

    fn refresh_chunk_faces(&mut self, cx: i32, cz: i32) {
        let mut faces_to_add = Vec::new();

        if let Some(chunk) = self.chunks.get(&(cx, cz)) {
            let ox = cx * CHUNK_SIZE;
            let oz = cz * CHUNK_SIZE;

            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    for y in 0..WORLD_HEIGHT {
                        let block = chunk.blocks[block_index(lx, y, lz)];
                        if block == Block::Air { continue; }

                        let x = ox + lx;
                        let z = oz + lz;

                        if y == WORLD_HEIGHT - 1 || self.get_block(x, y + 1, z) == Block::Air {
                            faces_to_add.push((block, x, y, z, FaceDirection::Top));
                        }
                        if self.get_block(x - 1, y, z) == Block::Air {
                            faces_to_add.push((block, x, y, z, FaceDirection::West));
                        }
                        if self.get_block(x + 1, y, z) == Block::Air {
                            faces_to_add.push((block, x, y, z, FaceDirection::East));
                        }
                        if self.get_block(x, y, z - 1) == Block::Air {
                            faces_to_add.push((block, x, y, z, FaceDirection::North));
                        }
                        if self.get_block(x, y, z + 1) == Block::Air {
                            faces_to_add.push((block, x, y, z, FaceDirection::South));
                        }
                    }
                }
            }
        }

        if let Some(chunk) = self.chunks.get_mut(&(cx, cz)) {
            chunk.faces.clear();
            for (block, x, y, z, dir) in faces_to_add {
                add_face(&mut chunk.faces, block, x, y, z, dir);
            }
        }
    }

    pub fn raycast(&self, origin: crate::math::vec::Vec3, direction: crate::math::vec::Vec3, max_dist: f32) -> Option<(crate::math::vec::IVec3, FaceDirection, f32)> {
        let mut voxel_x = origin.x.floor() as i32;
        let mut voxel_y = origin.y.floor() as i32;
        let mut voxel_z = origin.z.floor() as i32;

        let step_x = if direction.x > 0.0 { 1 } else { -1 };
        let step_y = if direction.y > 0.0 { 1 } else { -1 };
        let step_z = if direction.z > 0.0 { 1 } else { -1 };

        let t_delta_x = if direction.x != 0.0 { (1.0 / direction.x).abs() } else { f32::INFINITY };
        let t_delta_y = if direction.y != 0.0 { (1.0 / direction.y).abs() } else { f32::INFINITY };
        let t_delta_z = if direction.z != 0.0 { (1.0 / direction.z).abs() } else { f32::INFINITY };

        let mut t_max_x = if direction.x > 0.0 {
            ((voxel_x as f32 + 1.0) - origin.x) / direction.x
        } else if direction.x < 0.0 {
            (voxel_x as f32 - origin.x) / direction.x
        } else {
            f32::INFINITY
        };

        let mut t_max_y = if direction.y > 0.0 {
            ((voxel_y as f32 + 1.0) - origin.y) / direction.y
        } else if direction.y < 0.0 {
            (voxel_y as f32 - origin.y) / direction.y
        } else {
            f32::INFINITY
        };

        let mut t_max_z = if direction.z > 0.0 {
            ((voxel_z as f32 + 1.0) - origin.z) / direction.z
        } else if direction.z < 0.0 {
            (voxel_z as f32 - origin.z) / direction.z
        } else {
            f32::INFINITY
        };

        let mut t = 0.0;
        while t < max_dist {
            if t_max_x < t_max_y && t_max_x < t_max_z {
                voxel_x += step_x;
                t = t_max_x;
                t_max_x += t_delta_x;
                let face = if step_x > 0 { FaceDirection::West } else { FaceDirection::East };
                if self.get_block(voxel_x, voxel_y, voxel_z) != Block::Air {
                    return Some((crate::math::vec::IVec3::new(voxel_x, voxel_y, voxel_z), face, t));
                }
            } else if t_max_y < t_max_z {
                voxel_y += step_y;
                t = t_max_y;
                t_max_y += t_delta_y;
                // For Y axis, we'll use Top if stepping up, but our FaceDirection is limited.
                // Let's just use Top for simplicity as it's the most common.
                let face = if step_y > 0 { FaceDirection::Top } else { FaceDirection::Top };
                if self.get_block(voxel_x, voxel_y, voxel_z) != Block::Air {
                    return Some((crate::math::vec::IVec3::new(voxel_x, voxel_y, voxel_z), face, t));
                }
            } else {
                voxel_z += step_z;
                t = t_max_z;
                t_max_z += t_delta_z;
                let face = if step_z > 0 { FaceDirection::North } else { FaceDirection::South };
                if self.get_block(voxel_x, voxel_y, voxel_z) != Block::Air {
                    return Some((crate::math::vec::IVec3::new(voxel_x, voxel_y, voxel_z), face, t));
                }
            }
        }

        None
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
        let n = self.terrain_noise.get([x as f64 * self.scale, z as f64 * self.scale]);
        let roll = (n + 1.0) / 2.0;

        let magnitude = if roll < 0.20 {
            0
        } else if roll < 0.80 {
            1
        } else if roll < 0.90 {
            2
        } else {
            5
        };

        let dir_n = self.direction_noise.get([x as f64 * self.scale, z as f64 * self.scale]);
        let sign = if dir_n >= 0.0 { 1 } else { -1 };

        (32 + (magnitude * sign))
            .clamp(1, WORLD_HEIGHT - 2)
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceDirection {
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