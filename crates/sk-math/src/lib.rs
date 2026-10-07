//! Grundlegende Mathematik für Skizzeo: Vektoren und 4×4-Matrizen.
//!
//! Das Modell rechnet in `f64` und Millimetern. Für die GPU werden Matrizen
//! kamerarelativ aufgebaut und erst dann nach `f32` gewandelt.

#![forbid(unsafe_code)]

pub mod polygon;

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

pub const fn vec3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const ZERO: Vec3 = vec3(0.0, 0.0, 0.0);
    pub const X: Vec3 = vec3(1.0, 0.0, 0.0);
    pub const Y: Vec3 = vec3(0.0, 1.0, 0.0);
    pub const Z: Vec3 = vec3(0.0, 0.0, 1.0);

    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        vec3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> Vec3 {
        let l = self.length();
        if l > 0.0 {
            self / l
        } else {
            self
        }
    }

    /// Dreht den Vektor um die (normierte) Achse `axis` um `angle` (Bogenmaß, rechtsdrehend).
    pub fn rotated(self, axis: Vec3, angle: f64) -> Vec3 {
        let (s, c) = angle.sin_cos();
        self * c + axis.cross(self) * s + axis * (axis.dot(self) * (1.0 - c))
    }

    pub fn to_f32(self) -> [f32; 3] {
        [self.x as f32, self.y as f32, self.z as f32]
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        vec3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        vec3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, o: Vec3) {
        *self = *self - o;
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f64) -> Vec3 {
        vec3(self.x * s, self.y * s, self.z * s)
    }
}

impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, s: f64) -> Vec3 {
        vec3(self.x / s, self.y / s, self.z / s)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        vec3(-self.x, -self.y, -self.z)
    }
}

/// Punkt in einer Fläche (Musterkoordinaten u, v in mm).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

pub const fn vec2(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

/// Achsparalleles Rechteck in einer Fläche.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect2 {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect2 {
    /// Rechteck zwischen zwei Ecken, gleich in welcher Reihenfolge.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect2 {
        Rect2 {
            min: vec2(x0.min(x1), y0.min(y1)),
            max: vec2(x0.max(x1), y0.max(y1)),
        }
    }
}

/// 4×4-Matrix, spaltenweise gespeichert (`c[spalte][zeile]`) wie in OpenGL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub c: [[f64; 4]; 4],
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        c: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Reine Drehung in den Kameraraum (Kamera im Ursprung, Blick entlang -Z).
    pub fn view_rotation(forward: Vec3, right: Vec3, up: Vec3) -> Mat4 {
        let f = forward;
        Mat4 {
            c: [
                [right.x, up.x, -f.x, 0.0],
                [right.y, up.y, -f.y, 0.0],
                [right.z, up.z, -f.z, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Perspektivische Projektion nach OpenGL-Konvention (NDC-Tiefe -1..1).
    pub fn perspective(fov_y: f64, aspect: f64, near: f64, far: f64) -> Mat4 {
        let f = 1.0 / (fov_y * 0.5).tan();
        let nf = 1.0 / (near - far);
        Mat4 {
            c: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, (far + near) * nf, -1.0],
                [0.0, 0.0, 2.0 * far * near * nf, 0.0],
            ],
        }
    }

    /// Parallelprojektion; `half_h` ist die halbe Bildhöhe in Welteinheiten.
    pub fn orthographic(half_h: f64, aspect: f64, near: f64, far: f64) -> Mat4 {
        let nf = 1.0 / (near - far);
        Mat4 {
            c: [
                [1.0 / (half_h * aspect), 0.0, 0.0, 0.0],
                [0.0, 1.0 / half_h, 0.0, 0.0],
                [0.0, 0.0, 2.0 * nf, 0.0],
                [0.0, 0.0, (far + near) * nf, 1.0],
            ],
        }
    }

    pub fn mul_vec4(&self, v: [f64; 4]) -> [f64; 4] {
        let mut r = [0.0; 4];
        for (col, &s) in self.c.iter().zip(v.iter()) {
            for row in 0..4 {
                r[row] += col[row] * s;
            }
        }
        r
    }

    /// Allgemeine Inverse über Kofaktoren. `None`, wenn die Matrix singulär ist.
    #[allow(clippy::needless_range_loop)]
    pub fn inverse(&self) -> Option<Mat4> {
        let m = |col: usize, row: usize| self.c[col][row];
        // Zeilenweise Sicht a[zeile][spalte]
        let a = [
            [m(0, 0), m(1, 0), m(2, 0), m(3, 0)],
            [m(0, 1), m(1, 1), m(2, 1), m(3, 1)],
            [m(0, 2), m(1, 2), m(2, 2), m(3, 2)],
            [m(0, 3), m(1, 3), m(2, 3), m(3, 3)],
        ];
        let s0 = a[0][0] * a[1][1] - a[1][0] * a[0][1];
        let s1 = a[0][0] * a[1][2] - a[1][0] * a[0][2];
        let s2 = a[0][0] * a[1][3] - a[1][0] * a[0][3];
        let s3 = a[0][1] * a[1][2] - a[1][1] * a[0][2];
        let s4 = a[0][1] * a[1][3] - a[1][1] * a[0][3];
        let s5 = a[0][2] * a[1][3] - a[1][2] * a[0][3];
        let c5 = a[2][2] * a[3][3] - a[3][2] * a[2][3];
        let c4 = a[2][1] * a[3][3] - a[3][1] * a[2][3];
        let c3 = a[2][1] * a[3][2] - a[3][1] * a[2][2];
        let c2 = a[2][0] * a[3][3] - a[3][0] * a[2][3];
        let c1 = a[2][0] * a[3][2] - a[3][0] * a[2][2];
        let c0 = a[2][0] * a[3][1] - a[3][0] * a[2][1];
        let det = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;
        if det.abs() < 1e-300 {
            return None;
        }
        let d = 1.0 / det;
        let b = [
            [
                (a[1][1] * c5 - a[1][2] * c4 + a[1][3] * c3) * d,
                (-a[0][1] * c5 + a[0][2] * c4 - a[0][3] * c3) * d,
                (a[3][1] * s5 - a[3][2] * s4 + a[3][3] * s3) * d,
                (-a[2][1] * s5 + a[2][2] * s4 - a[2][3] * s3) * d,
            ],
            [
                (-a[1][0] * c5 + a[1][2] * c2 - a[1][3] * c1) * d,
                (a[0][0] * c5 - a[0][2] * c2 + a[0][3] * c1) * d,
                (-a[3][0] * s5 + a[3][2] * s2 - a[3][3] * s1) * d,
                (a[2][0] * s5 - a[2][2] * s2 + a[2][3] * s1) * d,
            ],
            [
                (a[1][0] * c4 - a[1][1] * c2 + a[1][3] * c0) * d,
                (-a[0][0] * c4 + a[0][1] * c2 - a[0][3] * c0) * d,
                (a[3][0] * s4 - a[3][1] * s2 + a[3][3] * s0) * d,
                (-a[2][0] * s4 + a[2][1] * s2 - a[2][3] * s0) * d,
            ],
            [
                (-a[1][0] * c3 + a[1][1] * c1 - a[1][2] * c0) * d,
                (a[0][0] * c3 - a[0][1] * c1 + a[0][2] * c0) * d,
                (-a[3][0] * s3 + a[3][1] * s1 - a[3][2] * s0) * d,
                (a[2][0] * s3 - a[2][1] * s1 + a[2][2] * s0) * d,
            ],
        ];
        // b ist zeilenweise; zurück in Spalten
        let mut r = Mat4::IDENTITY;
        for row in 0..4 {
            for col in 0..4 {
                r.c[col][row] = b[row][col];
            }
        }
        Some(r)
    }

    pub fn to_f32(&self) -> [f32; 16] {
        let mut out = [0.0f32; 16];
        for col in 0..4 {
            for row in 0..4 {
                out[col * 4 + row] = self.c[col][row] as f32;
            }
        }
        out
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, o: Mat4) -> Mat4 {
        let mut r = Mat4 { c: [[0.0; 4]; 4] };
        for col in 0..4 {
            r.c[col] = self.mul_vec4(o.c[col]);
        }
        r
    }
}

/// Strahl-Dreieck-Schnitt (Möller–Trumbore). Liefert den Strahlparameter `t > 0`.
pub fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

/// Abstand des Punkts `p` von der Strecke `a`–`b` in der Ebene.
pub fn dist_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vy * vy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((p.0 - a.0 - vx * t).powi(2) + (p.1 - a.1 - vy * t).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abstand_zur_strecke() {
        let (a, b) = ((0.0, 0.0), (10.0, 0.0));
        assert!(close(dist_to_segment((5.0, 3.0), a, b), 3.0));
        assert!(close(dist_to_segment((-3.0, 4.0), a, b), 5.0));
        assert!(close(dist_to_segment((13.0, 4.0), a, b), 5.0));
        assert!(close(dist_to_segment((3.0, 4.0), a, a), 5.0));
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn inverse_ergibt_einheitsmatrix() {
        let p = Mat4::perspective(0.8, 1.6, 10.0, 1e6);
        let f = vec3(0.3, 0.8, -0.2).normalized();
        let r = f.cross(Vec3::Z).normalized();
        let u = r.cross(f);
        let m = p * Mat4::view_rotation(f, r, u);
        let i = m * m.inverse().unwrap();
        for col in 0..4 {
            for row in 0..4 {
                let soll = if col == row { 1.0 } else { 0.0 };
                assert!((i.c[col][row] - soll).abs() < 1e-9, "{:?}", i);
            }
        }
    }

    #[test]
    fn drehung_um_z() {
        let v = Vec3::X.rotated(Vec3::Z, std::f64::consts::FRAC_PI_2);
        assert!(close(v.x, 0.0) && close(v.y, 1.0) && close(v.z, 0.0));
    }

    #[test]
    fn strahl_trifft_dreieck() {
        let t = ray_triangle(
            vec3(0.2, 0.2, 5.0),
            vec3(0.0, 0.0, -1.0),
            vec3(0.0, 0.0, 0.0),
            vec3(1.0, 0.0, 0.0),
            vec3(0.0, 1.0, 0.0),
        );
        assert!(close(t.unwrap(), 5.0));
        let daneben = ray_triangle(
            vec3(2.0, 2.0, 5.0),
            vec3(0.0, 0.0, -1.0),
            vec3(0.0, 0.0, 0.0),
            vec3(1.0, 0.0, 0.0),
            vec3(0.0, 1.0, 0.0),
        );
        assert!(daneben.is_none());
    }
}
