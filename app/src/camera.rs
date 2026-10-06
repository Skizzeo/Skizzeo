//! Kamera mit Z nach oben und SketchUp-artiger Navigation:
//! Drehen um den Punkt unter dem Mauszeiger, Verschieben, Zoomen zum Mauszeiger.

use sk_math::{vec3, Mat4, Vec3};
use sk_render::View;

const PITCH_LIMIT: f64 = 89.5 * std::f64::consts::PI / 180.0;

#[derive(Clone, Debug)]
pub struct Camera {
    pub eye: Vec3,
    /// Blickrichtung waagerecht, Bogenmaß gegen den Uhrzeigersinn ab +X.
    pub yaw: f64,
    /// Neigung, Bogenmaß; positiv = nach oben.
    pub pitch: f64,
    /// Senkrechter Öffnungswinkel.
    pub fov_y: f64,
    /// Typischer Abstand zum betrachteten Objekt; steuert Nahebene und Drehpunkt ins Leere.
    pub focus: f64,
    /// Parallelprojektion mit dieser halben Bildhöhe (mm); `None` = perspektivisch.
    pub ortho: Option<f64>,
}

impl Camera {
    pub fn looking_at(eye: Vec3, target: Vec3, fov_y_deg: f64) -> Camera {
        let d = target - eye;
        Camera {
            eye,
            yaw: d.y.atan2(d.x),
            pitch: d.z.atan2((d.x * d.x + d.y * d.y).sqrt()),
            fov_y: fov_y_deg.to_radians(),
            focus: d.length(),
            ortho: None,
        }
    }

    /// Parallelprojektion in Richtung `yaw`/`pitch` auf `target`, halbe Bildhöhe `half_h`.
    pub fn parallel(target: Vec3, yaw: f64, pitch: f64, half_h: f64) -> Camera {
        // Weit genug zurück, dass das ganze Modell vor der Kamera liegt
        let dist = 200_000.0;
        let mut c = Camera {
            eye: target,
            yaw,
            pitch,
            fov_y: 45f64.to_radians(),
            focus: dist,
            ortho: Some(half_h),
        };
        c.eye = target - c.forward() * dist;
        c
    }

    pub fn forward(&self) -> Vec3 {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        vec3(cp * cy, cp * sy, sp)
    }

    pub fn right(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        vec3(sy, -cy, 0.0)
    }

    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward())
    }

    /// Blickstrahl durch einen Bildpunkt (Pixel, Ursprung links oben): Ursprung und Richtung.
    pub fn ray(&self, px: f64, py: f64, w: f64, h: f64) -> (Vec3, Vec3) {
        let sx = (2.0 * px / w - 1.0) * (w / h);
        let sy = 1.0 - 2.0 * py / h;
        match self.ortho {
            Some(half) => (
                self.eye + self.right() * (sx * half) + self.up() * (sy * half),
                self.forward(),
            ),
            None => {
                let t = (self.fov_y * 0.5).tan();
                let d = self.forward() + self.right() * (sx * t) + self.up() * (sy * t);
                (self.eye, d.normalized())
            }
        }
    }

    /// Bildpunkt eines Weltpunkts (Pixel, Ursprung links oben); `None` hinter der Kamera.
    pub fn project(&self, p: Vec3, w: f64, h: f64) -> Option<(f64, f64)> {
        let r = p - self.eye;
        let z = r.dot(self.forward());
        if z <= self.near() {
            return None;
        }
        let s = match self.ortho {
            Some(half) => half,
            None => z * (self.fov_y * 0.5).tan(),
        };
        let x = r.dot(self.right()) / (s * (w / h));
        let y = r.dot(self.up()) / s;
        Some(((x * 0.5 + 0.5) * w, (0.5 - y * 0.5) * h))
    }

    /// Schnittpunkt des Blickstrahls durch einen Bildpunkt mit dem Boden (z = 0).
    pub fn ground_point(&self, px: f64, py: f64, w: f64, h: f64) -> Option<Vec3> {
        let (o, d) = self.ray(px, py, w, h);
        if d.z.abs() < 1e-12 {
            return None;
        }
        let t = -o.z / d.z;
        (t > 0.0 && t < self.far()).then(|| {
            let p = o + d * t;
            vec3(p.x, p.y, 0.0)
        })
    }

    pub fn near(&self) -> f64 {
        match self.ortho {
            Some(_) => 10.0,
            None => (self.focus * 0.002).clamp(1.0, 500.0),
        }
    }

    pub fn far(&self) -> f64 {
        match self.ortho {
            // Tiefe ist linear: knapp halten, damit die Genauigkeit reicht
            Some(_) => self.focus * 2.0,
            None => self.near() * 2.0e6,
        }
    }

    pub fn view(&self, w: u32, h: u32) -> View {
        let aspect = w.max(1) as f64 / h.max(1) as f64;
        let rot = Mat4::view_rotation(self.forward(), self.right(), self.up());
        let proj = match self.ortho {
            Some(half) => Mat4::orthographic(half, aspect, self.near(), self.far()),
            None => Mat4::perspective(self.fov_y, aspect, self.near(), self.far()),
        };
        let vp = proj * rot;
        let inv = vp.inverse().unwrap_or(Mat4::IDENTITY);
        let ndc_h = match self.ortho {
            // Waagerechter Blick: Horizont auf Höhe des Bodens; sonst weit oberhalb
            Some(half) if self.pitch.abs() < 1e-6 => -self.eye.z / half,
            Some(_) => 1000.0,
            None => (-self.pitch).tan() / (self.fov_y * 0.5).tan(),
        };
        View {
            view_proj: vp.to_f32(),
            inv_view_proj: inv.to_f32(),
            origin_rel: (Vec3::ZERO - self.eye).to_f32(),
            eye_z: self.eye.z as f32,
            horizon_px: ((ndc_h * 0.5 + 0.5) * h as f64) as f32,
            // Parallelprojektion hat w = 1: Kanten nie am Auge abschneiden
            near: if self.ortho.is_some() {
                0.0
            } else {
                self.near() as f32
            },
            pull: match self.ortho {
                Some(_) => {
                    let f = self.forward() * -50.0;
                    [f.x as f32, f.y as f32, f.z as f32, 1.0]
                }
                None => [0.0, 0.0, 0.0, 0.997],
            },
        }
    }

    /// Drehen um `pivot`; `dx`,`dy` in Pixeln. Mausbewegung nach rechts dreht das
    /// Modell nach rechts, nach unten kippt es die Oberseite zum Betrachter.
    pub fn orbit(&mut self, pivot: Vec3, dx: f64, dy: f64) {
        let k = 0.35f64.to_radians();
        let new_pitch = (self.pitch - dy * k).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        let dpitch = new_pitch - self.pitch;
        let dyaw = -dx * k;
        let mut rel = self.eye - pivot;
        rel = rel.rotated(self.right(), dpitch);
        rel = rel.rotated(Vec3::Z, dyaw);
        self.eye = pivot + rel;
        self.pitch = new_pitch;
        self.yaw += dyaw;
    }

    /// Verschieben, sodass der gegriffene Punkt in Tiefe `depth` unter der Maus bleibt.
    pub fn pan(&mut self, depth: f64, dx: f64, dy: f64, h: f64) {
        let per_px = match self.ortho {
            Some(half) => 2.0 * half / h,
            None => 2.0 * depth.max(self.near()) * (self.fov_y * 0.5).tan() / h,
        };
        self.eye = self.eye - self.right() * (dx * per_px) + self.up() * (dy * per_px);
    }

    /// Zoomen auf `point` zu (Rasten > 0) oder von ihm weg.
    pub fn zoom(&mut self, point: Vec3, steps: f64) {
        if let Some(half) = self.ortho {
            let s = 0.8f64.powf(steps);
            let new_half = (half * s).clamp(100.0, 1.0e6);
            let s = new_half / half;
            let f = self.forward();
            let rel = self.eye - point;
            let depth = rel.dot(f);
            self.eye = point + f * depth + (rel - f * depth) * s;
            self.ortho = Some(new_half);
            return;
        }
        let rel = self.eye - point;
        let dist = rel.length();
        let mut s = 0.8f64.powf(steps);
        if dist * s < self.near() * 4.0 {
            s = (self.near() * 4.0 / dist).min(1.0);
        }
        self.eye = point + rel * s;
        self.focus = (dist * s).max(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(c: &Camera, p: Vec3, w: u32, h: u32) -> (f64, f64) {
        let v = c.view(w, h);
        let m = v.view_proj;
        let r = p - c.eye;
        let clip = |row: usize| {
            m[row] as f64 * r.x
                + m[4 + row] as f64 * r.y
                + m[8 + row] as f64 * r.z
                + m[12 + row] as f64
        };
        let wc = clip(3);
        let (nx, ny) = (clip(0) / wc, clip(1) / wc);
        ((nx * 0.5 + 0.5) * w as f64, (0.5 - ny * 0.5) * h as f64)
    }

    #[test]
    fn drehpunkt_bleibt_stehen() {
        let mut c = Camera::looking_at(vec3(-6000.0, -8000.0, 3000.0), vec3(0.0, 0.0, 0.0), 45.0);
        let pivot = vec3(1500.0, 1500.0, 3000.0);
        let before = project(&c, pivot, 1200, 800);
        c.orbit(pivot, 120.0, -40.0);
        let after = project(&c, pivot, 1200, 800);
        assert!(
            (before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3,
            "{before:?} {after:?}"
        );
    }

    #[test]
    fn verschieben_folgt_der_maus() {
        let mut c = Camera::looking_at(vec3(-6000.0, -8000.0, 3000.0), vec3(0.0, 0.0, 0.0), 45.0);
        let p = vec3(0.0, 0.0, 0.0);
        let depth = (p - c.eye).dot(c.forward());
        let before = project(&c, p, 1200, 800);
        c.pan(depth, 30.0, -12.0, 800.0);
        let after = project(&c, p, 1200, 800);
        assert!(
            (after.0 - before.0 - 30.0).abs() < 1e-3,
            "{before:?} {after:?}"
        );
        assert!(
            (after.1 - before.1 + 12.0).abs() < 1e-3,
            "{before:?} {after:?}"
        );
    }

    #[test]
    fn zoom_haelt_punkt_unter_der_maus() {
        let mut c = Camera::looking_at(vec3(-6000.0, -8000.0, 3000.0), vec3(0.0, 0.0, 0.0), 45.0);
        let (o, dir) = c.ray(900.0, 300.0, 1200.0, 800.0);
        let p = o + dir * 5000.0;
        c.zoom(p, 2.0);
        let s = project(&c, p, 1200, 800);
        assert!(
            (s.0 - 900.0).abs() < 1e-3 && (s.1 - 300.0).abs() < 1e-3,
            "{s:?}"
        );
    }

    #[test]
    fn projektion_passt_zum_strahl() {
        let c = Camera::looking_at(vec3(-6000.0, -8000.0, 3000.0), vec3(0.0, 0.0, 0.0), 45.0);
        let p = c.ground_point(700.0, 500.0, 1200.0, 800.0).unwrap();
        let (x, y) = c.project(p, 1200.0, 800.0).unwrap();
        assert!((x - 700.0).abs() < 1e-6 && (y - 500.0).abs() < 1e-6);
    }

    #[test]
    fn parallel_projektion_passt_zum_strahl() {
        let c = Camera::parallel(
            vec3(2000.0, 1000.0, 0.0),
            0.3,
            -std::f64::consts::FRAC_PI_2,
            8000.0,
        );
        let p = c.ground_point(700.0, 500.0, 1200.0, 800.0).unwrap();
        let (x, y) = c.project(p, 1200.0, 800.0).unwrap();
        assert!((x - 700.0).abs() < 1e-6 && (y - 500.0).abs() < 1e-6);
        let s = project(&c, p, 1200, 800);
        assert!(
            (s.0 - 700.0).abs() < 1e-2 && (s.1 - 500.0).abs() < 1e-2,
            "{s:?}"
        );
    }

    #[test]
    fn parallel_zoom_haelt_punkt() {
        let mut c = Camera::parallel(
            vec3(0.0, 0.0, 0.0),
            0.0,
            -std::f64::consts::FRAC_PI_2,
            8000.0,
        );
        let p = c.ground_point(900.0, 200.0, 1200.0, 800.0).unwrap();
        c.zoom(p, 2.0);
        let (x, y) = c.project(p, 1200.0, 800.0).unwrap();
        assert!((x - 900.0).abs() < 1e-6 && (y - 200.0).abs() < 1e-6);
    }

    #[test]
    fn horizont_bei_waagerechtem_blick_in_der_mitte() {
        let c = Camera::looking_at(vec3(0.0, 0.0, 1600.0), vec3(1000.0, 0.0, 1600.0), 45.0);
        assert!((c.view(800, 600).horizon_px - 300.0).abs() < 1e-3);
    }
}
