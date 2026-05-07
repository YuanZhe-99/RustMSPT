#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        // Purpose: Construct a 3D vector from components.
        // Inputs: x/y/z scalar values.
        // Outputs: Vec3 instance.
        Self { x, y, z }
    }

    pub fn add(self, other: Self) -> Self {
        // Purpose: Compute vector addition.
        // Inputs: self and other vector.
        // Outputs: summed vector.
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }

    pub fn sub(self, other: Self) -> Self {
        // Purpose: Compute vector subtraction.
        // Inputs: self and other vector.
        // Outputs: difference vector.
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }

    pub fn scale(self, factor: f64) -> Self {
        // Purpose: Scale a vector by a scalar factor.
        // Inputs: vector and scalar factor.
        // Outputs: scaled vector.
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }

    pub fn dot(self, other: Self) -> f64 {
        // Purpose: Compute dot product between two vectors.
        // Inputs: self and other vector.
        // Outputs: dot product scalar.
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        // Purpose: Compute cross product between two vectors.
        // Inputs: self and other vector.
        // Outputs: orthogonal vector.
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
    pub fn from_size(size: Vec3) -> Self {
        // Purpose: Build axis-aligned box from origin with given size.
        // Inputs: size vector.
        // Outputs: bounding box [0,size].
        Self {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: size,
        }
    }

    pub fn size(&self) -> Vec3 {
        // Purpose: Get side lengths of a bounding box.
        // Inputs: bounding box.
        // Outputs: size vector.
        self.max.sub(self.min)
    }

    pub fn volume(&self) -> f64 {
        // Purpose: Compute non-negative box volume.
        // Inputs: bounding box.
        // Outputs: box volume.
        let s = self.size();
        (s.x.max(0.0)) * (s.y.max(0.0)) * (s.z.max(0.0))
    }

    pub fn contains_point(&self, p: Vec3) -> bool {
        // Purpose: Check whether a point lies inside box bounds.
        // Inputs: bounding box and query point.
        // Outputs: true when point is inside or on boundary.
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
    pub fn empty() -> Self {
        // Purpose: Construct an empty mesh container.
        // Inputs: none.
        // Outputs: mesh with no vertices/faces.
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        // Purpose: Check whether mesh has enough data to represent geometry.
        // Inputs: mesh.
        // Outputs: true when vertices or faces are empty.
        self.vertices.is_empty() || self.faces.is_empty()
    }
}
