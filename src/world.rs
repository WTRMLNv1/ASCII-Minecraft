use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::collections::HashMap;

pub const CHUNK_SIZE: i32 = 16;
pub const WORLD_HEIGHT: i32 = 64;
pub const RENDER_DISTANCE: i32 = 4;
pub const CLOUD_HEIGHT: i32 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
    Log,
    Leaves,
    Cloud,
    Water,
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
    chunks: HashMap<(i32, i32), Chunk>,
    base_noise: Fbm<Perlin>,
    mask_noise: Perlin,
    mountain_noise: Fbm<Perlin>,
    /// Low-frequency noise selects contiguous forest regions.
    forest_noise: Perlin,
    /// Higher-frequency noise selects occasional trees outside forests.
    sparse_tree_noise: Perlin,
    /// Broad, seed-derived patches of cloud blocks.
    cloud_noise: Fbm<Perlin>,
    cloud_time: f32,
    basin_noise: Perlin,
}

impl World {
    pub fn new(seed: u64) -> Self {
        let seed_u32 = seed as u32;
        Self {
            chunks: HashMap::new(),
            base_noise: Fbm::<Perlin>::new(seed_u32)
                .set_octaves(3)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            mask_noise: Perlin::new(seed_u32.wrapping_add(1)),
            mountain_noise: Fbm::<Perlin>::new(seed_u32.wrapping_add(2)),
            forest_noise: Perlin::new(seed_u32.wrapping_add(3)),
            sparse_tree_noise: Perlin::new(seed_u32.wrapping_add(4)),
            cloud_noise: Fbm::<Perlin>::new(seed_u32.wrapping_add(5))
                .set_octaves(2)
                .set_persistence(0.55)
                .set_lacunarity(2.0),
            // A seed-dependent starting point keeps drift deterministic per world.
            cloud_time: (seed % 10_000) as f32 * 0.001,
            basin_noise: Perlin::new(seed_u32.wrapping_add(6)),
        }
    }

    /// Advances the visual cloud layer without rebuilding chunk meshes.
    pub fn advance_clouds(&mut self, delta_seconds: f32) {
        self.cloud_time = (self.cloud_time + delta_seconds * 0.18) % std::f32::consts::TAU;
    }

    /// Slow, gentle horizontal drift applied to cloud faces during rendering.
    pub fn cloud_drift(&self) -> (f32, f32) {
        (
            self.cloud_time.sin() * 3.0,
            (self.cloud_time * 0.7).cos() * 1.5,
        )
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
        (cz - RENDER_DISTANCE..=cz + RENDER_DISTANCE)
            .flat_map(move |z| {
                (cx - RENDER_DISTANCE..=cx + RENDER_DISTANCE).filter_map(move |x| {
                    ((x - cx).pow(2) + (z - cz).pow(2) <= RENDER_DISTANCE.pow(2))
                        .then(|| self.chunks.get(&(x, z)).map(|chunk| chunk.faces.as_slice()))
                        .flatten()
                })
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
            .unwrap_or_else(|| self.generated_block_at(x, y, z))
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
        if lx == 0 {
            self.refresh_chunk_faces(cx - 1, cz);
        }
        if lx == CHUNK_SIZE - 1 {
            self.refresh_chunk_faces(cx + 1, cz);
        }
        if lz == 0 {
            self.refresh_chunk_faces(cx, cz - 1);
        }
        if lz == CHUNK_SIZE - 1 {
            self.refresh_chunk_faces(cx, cz + 1);
        }
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
                        if block == Block::Air {
                            continue;
                        }

                        let x = ox + lx;
                        let z = oz + lz;

                        if y == WORLD_HEIGHT - 1 || self.get_block(x, y + 1, z) == Block::Air {
                            faces_to_add.push((block, x, y, z, FaceDirection::Top));
                        }
                        if y == 0 || self.get_block(x, y - 1, z) == Block::Air {
                            faces_to_add.push((block, x, y, z, FaceDirection::Bottom));
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

    pub fn raycast(
        &self,
        origin: crate::math::vec::Vec3,
        direction: crate::math::vec::Vec3,
        max_dist: f32,
    ) -> Option<(crate::math::vec::IVec3, FaceDirection, f32)> {
        let mut voxel_x = origin.x.floor() as i32;
        let mut voxel_y = origin.y.floor() as i32;
        let mut voxel_z = origin.z.floor() as i32;

        let step_x = if direction.x > 0.0 { 1 } else { -1 };
        let step_y = if direction.y > 0.0 { 1 } else { -1 };
        let step_z = if direction.z > 0.0 { 1 } else { -1 };

        let t_delta_x = if direction.x != 0.0 {
            (1.0 / direction.x).abs()
        } else {
            f32::INFINITY
        };
        let t_delta_y = if direction.y != 0.0 {
            (1.0 / direction.y).abs()
        } else {
            f32::INFINITY
        };
        let t_delta_z = if direction.z != 0.0 {
            (1.0 / direction.z).abs()
        } else {
            f32::INFINITY
        };

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
                let face = if step_x > 0 {
                    FaceDirection::West
                } else {
                    FaceDirection::East
                };
                if self.get_block(voxel_x, voxel_y, voxel_z) != Block::Air {
                    return Some((
                        crate::math::vec::IVec3::new(voxel_x, voxel_y, voxel_z),
                        face,
                        t,
                    ));
                }
            } else if t_max_y < t_max_z {
                voxel_y += step_y;
                t = t_max_y;
                t_max_y += t_delta_y;
                let face = if step_y > 0 {
                    FaceDirection::Bottom
                } else {
                    FaceDirection::Top
                };
                if self.get_block(voxel_x, voxel_y, voxel_z) != Block::Air {
                    return Some((
                        crate::math::vec::IVec3::new(voxel_x, voxel_y, voxel_z),
                        face,
                        t,
                    ));
                }
            } else {
                voxel_z += step_z;
                t = t_max_z;
                t_max_z += t_delta_z;
                let face = if step_z > 0 {
                    FaceDirection::North
                } else {
                    FaceDirection::South
                };
                if self.get_block(voxel_x, voxel_y, voxel_z) != Block::Air {
                    return Some((
                        crate::math::vec::IVec3::new(voxel_x, voxel_y, voxel_z),
                        face,
                        t,
                    ));
                }
            }
        }

        None
    }

    fn generate_chunk(&self, chunk_x: i32, chunk_z: i32) -> Chunk {
        let mut blocks = vec![Block::Air; (CHUNK_SIZE * CHUNK_SIZE * WORLD_HEIGHT) as usize];
        let ox = chunk_x * CHUNK_SIZE;
        let oz = chunk_z * CHUNK_SIZE;
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let h = self.surface_height(ox + lx, oz + lz);
                for y in 0..=h {
                    blocks[block_index(lx, y, lz)] = block_at_height(h, y);
                }
            }
        }
        self.populate_trees(&mut blocks, chunk_x, chunk_z);
        self.populate_clouds(&mut blocks, chunk_x, chunk_z);

        // Water fill pass
        const SEA_LEVEL: i32 = 30;
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let h = self.surface_height(ox + lx, oz + lz);
                for y in (h + 1)..=SEA_LEVEL {
                    if y < WORLD_HEIGHT {
                        let index = block_index(lx, y, lz);
                        if blocks[index] == Block::Air {
                            blocks[index] = Block::Water;
                        }
                    }
                }
            }
        }

        let mut faces = Vec::new();
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let x = ox + lx;
                let z = oz + lz;
                for y in 0..WORLD_HEIGHT {
                    let block = blocks[block_index(lx, y, lz)];
                    if block == Block::Air {
                        continue;
                    }
                    if y == WORLD_HEIGHT - 1 || blocks[block_index(lx, y + 1, lz)] == Block::Air {
                        add_face(&mut faces, block, x, y, z, FaceDirection::Top);
                    }
                    if y == 0 || blocks[block_index(lx, y - 1, lz)] == Block::Air {
                        add_face(&mut faces, block, x, y, z, FaceDirection::Bottom);
                    }
                    if (lx == 0 && self.generated_block_at(x - 1, y, z) == Block::Air)
                        || (lx > 0 && blocks[block_index(lx - 1, y, lz)] == Block::Air)
                    {
                        add_face(&mut faces, block, x, y, z, FaceDirection::West);
                    }
                    if (lx == CHUNK_SIZE - 1 && self.generated_block_at(x + 1, y, z) == Block::Air)
                        || (lx < CHUNK_SIZE - 1 && blocks[block_index(lx + 1, y, lz)] == Block::Air)
                    {
                        add_face(&mut faces, block, x, y, z, FaceDirection::East);
                    }
                    if (lz == 0 && self.generated_block_at(x, y, z - 1) == Block::Air)
                        || (lz > 0 && blocks[block_index(lx, y, lz - 1)] == Block::Air)
                    {
                        add_face(&mut faces, block, x, y, z, FaceDirection::North);
                    }
                    if (lz == CHUNK_SIZE - 1 && self.generated_block_at(x, y, z + 1) == Block::Air)
                        || (lz < CHUNK_SIZE - 1 && blocks[block_index(lx, y, lz + 1)] == Block::Air)
                    {
                        add_face(&mut faces, block, x, y, z, FaceDirection::South);
                    }
                }
            }
        }
        Chunk { blocks, faces }
    }

    fn generated_block_at(&self, x: i32, y: i32, z: i32) -> Block {
        if y < 0 || y >= WORLD_HEIGHT {
            return Block::Air;
        }
        let surface = self.surface_height(x, z);
        if y <= surface {
            return block_at_height(surface, y);
        }
        if y == CLOUD_HEIGHT && self.is_cloud_at(x, z) {
            return Block::Cloud;
        }
        self.tree_block_at(x, y, z).unwrap_or(Block::Air)
    }

    fn populate_clouds(&self, blocks: &mut [Block], chunk_x: i32, chunk_z: i32) {
        let ox = chunk_x * CHUNK_SIZE;
        let oz = chunk_z * CHUNK_SIZE;
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let x = ox + lx;
                let z = oz + lz;
                let index = block_index(lx, CLOUD_HEIGHT, lz);
                if blocks[index] == Block::Air && self.is_cloud_at(x, z) {
                    blocks[index] = Block::Cloud;
                }
            }
        }
    }

    fn is_cloud_at(&self, x: i32, z: i32) -> bool {
        self.cloud_noise.get([x as f64 * 0.035, z as f64 * 0.035]) > 0.18
    }

    fn populate_trees(&self, blocks: &mut [Block], chunk_x: i32, chunk_z: i32) {
        let ox = chunk_x * CHUNK_SIZE;
        let oz = chunk_z * CHUNK_SIZE;
        // A canopy extends two blocks from its trunk, so inspect origins just outside this chunk.
        for tree_z in oz - 2..=oz + CHUNK_SIZE + 1 {
            for tree_x in ox - 2..=ox + CHUNK_SIZE + 1 {
                let Some(_) = self.tree_kind_at_origin(tree_x, tree_z) else {
                    continue;
                };
                let trunk_base = self.surface_height(tree_x, tree_z) + 1;
                for z in (tree_z - 2).max(oz)..=(tree_z + 2).min(oz + CHUNK_SIZE - 1) {
                    for x in (tree_x - 2).max(ox)..=(tree_x + 2).min(ox + CHUNK_SIZE - 1) {
                        for y in trunk_base..=(trunk_base + 6).min(WORLD_HEIGHT - 1) {
                            if let Some(block) =
                                self.tree_block_from_origin(tree_x, trunk_base, tree_z, x, y, z)
                            {
                                let lx = x - ox;
                                let lz = z - oz;
                                let index = block_index(lx, y, lz);
                                if blocks[index] == Block::Air {
                                    blocks[index] = block;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn tree_block_at(&self, x: i32, y: i32, z: i32) -> Option<Block> {
        // Origins occur only at fixed lattice points, making adjacent canopies non-overlapping.
        for grid in [6, 12] {
            let cell_x = x.div_euclid(grid);
            let cell_z = z.div_euclid(grid);
            for cz in cell_z - 1..=cell_z + 1 {
                for cx in cell_x - 1..=cell_x + 1 {
                    let origin_x = cx * grid + grid / 2;
                    let origin_z = cz * grid + grid / 2;
                    if self.tree_kind_at_origin(origin_x, origin_z).is_some() {
                        let base = self.surface_height(origin_x, origin_z) + 1;
                        if let Some(block) =
                            self.tree_block_from_origin(origin_x, base, origin_z, x, y, z)
                        {
                            return Some(block);
                        }
                    }
                }
            }
        }
        None
    }

    fn tree_kind_at_origin(&self, x: i32, z: i32) -> Option<()> {
        let forest_origin = x.rem_euclid(6) == 3 && z.rem_euclid(6) == 3;
        let forest_value = self.forest_noise.get([x as f64 * 0.025, z as f64 * 0.025]);
        if forest_origin && forest_value > 0.12 {
            return Some(());
        }

        let sparse_origin = x.rem_euclid(12) == 6 && z.rem_euclid(12) == 6;
        if !sparse_origin
            || self
                .sparse_tree_noise
                .get([x as f64 * 0.11, z as f64 * 0.11])
                <= 0.58
        {
            return None;
        }

        // Keep standalone trees clear of any nearby forest canopy.
        let first_forest_x = (x - 6).div_euclid(6) * 6 + 3;
        let first_forest_z = (z - 6).div_euclid(6) * 6 + 3;
        for forest_z in (first_forest_z..=z + 6).step_by(6) {
            for forest_x in (first_forest_x..=x + 6).step_by(6) {
                if self
                    .forest_noise
                    .get([forest_x as f64 * 0.025, forest_z as f64 * 0.025])
                    > 0.12
                {
                    return None;
                }
            }
        }
        Some(())
    }

    fn tree_block_from_origin(
        &self,
        origin_x: i32,
        trunk_base: i32,
        origin_z: i32,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<Block> {
        if x == origin_x && z == origin_z && (trunk_base..trunk_base + 4).contains(&y) {
            return Some(Block::Log);
        }
        let radius = match y - trunk_base {
            3 => 1,
            4 | 5 => 2,
            6 => 1,
            _ => return None,
        };
        ((x - origin_x).abs() <= radius && (z - origin_z).abs() <= radius).then_some(Block::Leaves)
    }

    fn surface_height(&self, x: i32, z: i32) -> i32 {
        let x_f = x as f64;
        let z_f = z as f64;

        // Layer 1: Base rolling terrain
        let base_val = self.base_noise.get([x_f * 0.01, z_f * 0.01]);
        let base_h = base_val * 6.0;

        // Layer 2: Mountain mask
        let mask_val_raw = self.mask_noise.get([x_f * 0.003, z_f * 0.003]);
        let mask_normalized = (mask_val_raw + 1.0) / 2.0;
        let mask = smoothstep(0.5, 1.0, mask_normalized);

        // Layer 3: Mountain height
        let mt_val = self.mountain_noise.get([x_f * 0.02, z_f * 0.02]);
        let mt_h = mt_val * 25.0;

        // Layer 4: Basin offset
        let basin_val = self.basin_noise.get([x_f * 0.01, z_f * 0.01]);
        let basin_offset = if basin_val < -0.2 {
            (basin_val + 0.2) * 10.0
        } else {
            0.0
        };

        let final_h = 32.0 + base_h + (mask * mt_h) + basin_offset;

        (final_h.round() as i32).clamp(1, WORLD_HEIGHT - 2)
    }
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    Bottom,
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
        FaceDirection::Bottom => [
            [x, y, z],
            [x + 1.0, y, z],
            [x + 1.0, y, z + 1.0],
            [x, y, z + 1.0],
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

    #[test]
    fn generated_trees_have_logs_and_opaque_leaf_blocks() {
        let world = World::new(1234);
        let tree = (-48..48)
            .flat_map(|z| (-48..48).map(move |x| (x, z)))
            .find(|&(x, z)| world.tree_kind_at_origin(x, z).is_some())
            .expect("the sampled world should contain a tree");
        let trunk_base = world.surface_height(tree.0, tree.1) + 1;

        assert_eq!(
            world.generated_block_at(tree.0, trunk_base, tree.1),
            Block::Log
        );
        assert!(
            world.generated_block_at(tree.0 + 2, trunk_base + 4, tree.1) == Block::Leaves,
            "the tree canopy should use leaf blocks"
        );
    }

    #[test]
    fn clouds_are_seeded_white_block_candidates_at_y_60() {
        let world = World::new(1234);
        let cloud = (-128..128)
            .flat_map(|z| (-128..128).map(move |x| (x, z)))
            .find(|&(x, z)| world.is_cloud_at(x, z))
            .expect("the sampled world should contain clouds");
        assert_eq!(
            world.generated_block_at(cloud.0, CLOUD_HEIGHT, cloud.1),
            Block::Cloud
        );
    }
}
