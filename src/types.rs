#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    // AI-FUNC-SUMMARY: Construct a 3D vector from x/y/z components; returns Vec3; side effects: None.
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    // AI-FUNC-SUMMARY: Compute vector addition (self + other); returns summed Vec3; side effects: None.
    pub fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }

    // AI-FUNC-SUMMARY: Compute vector subtraction (self - other); returns difference Vec3; side effects: None.
    pub fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }

    // AI-FUNC-SUMMARY: Scale a vector by a scalar factor; returns scaled Vec3; side effects: None.
    pub fn scale(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }

    // AI-FUNC-SUMMARY: Compute dot product of two vectors; returns f64 scalar; side effects: None.
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    // AI-FUNC-SUMMARY: Compute cross product of two vectors; returns orthogonal Vec3; side effects: None.
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl BoundingBox {
    // AI-FUNC-SUMMARY: Build axis-aligned box from origin to given size; returns BoundingBox [0,size]; side effects: None.
    pub fn from_size(size: Vec3) -> Self {
        Self {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: size,
        }
    }

    // AI-FUNC-SUMMARY: Compute side lengths of the box; returns Vec3 of dimensions; side effects: None.
    pub fn size(&self) -> Vec3 {
        self.max.sub(self.min)
    }

    // AI-FUNC-SUMMARY: Compute non-negative box volume (clamps negative extents to zero); returns f64; side effects: None.
    pub fn volume(&self) -> f64 {
        let s = self.size();
        (s.x.max(0.0)) * (s.y.max(0.0)) * (s.z.max(0.0))
    }

    // AI-FUNC-SUMMARY: Check whether a point lies inside or on the boundary of the box; returns bool; side effects: None.
    pub fn contains_point(&self, p: Vec3) -> bool {
        p.x >= self.min.x
            && p.y >= self.min.y
            && p.z >= self.min.z
            && p.x <= self.max.x
            && p.y <= self.max.y
            && p.z <= self.max.z
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Triangle {
    pub a: usize,
    pub b: usize,
    pub c: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    pub faces: Vec<Triangle>,
}

impl Mesh {
    // AI-FUNC-SUMMARY: Construct an empty mesh with no vertices or faces; returns Mesh; side effects: None.
    pub fn empty() -> Self {
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
        }
    }

    // AI-FUNC-SUMMARY: Check whether the mesh lacks enough data to represent geometry; returns true when vertices or faces are empty; side effects: None.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() || self.faces.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl RenderedImage {
    // AI-FUNC-SUMMARY: Construct an RGBA8 image buffer; returns RenderedImage; side effects: None.
    // Notes: rgba must hold exactly width * height * 4 bytes, row-major from the top row.
    pub fn new(width: usize, height: usize, rgba: Vec<u8>) -> Self {
        debug_assert_eq!(rgba.len(), width * height * 4);
        Self {
            width,
            height,
            rgba,
        }
    }

    // AI-FUNC-SUMMARY: Construct a solid-color RGBA8 image; returns RenderedImage; side effects: allocates width*height*4 bytes.
    pub fn filled(width: usize, height: usize, color: [u8; 4]) -> Self {
        let mut rgba = vec![0u8; width * height * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&color);
        }
        Self {
            width,
            height,
            rgba,
        }
    }
}
