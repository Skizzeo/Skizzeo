//! Netz der Ansicht und Ausbau innen in die Ablagen des Renderers (Review
//! #137 B1): Das Netz des Modells ohne den Ausbau, dazu die Teile des
//! Ausbaus in eigenen Ablagen, nur die geänderten
//! ([`Scene::ausbau_liefern`](crate::scene::Scene::ausbau_liefern)).

use crate::gelaende_platte::MESH_PLATE;
use crate::scene::Plane;
use crate::{App, MESH_GHOST, MESH_LIVE, MESH_MODEL, MESH_PREVIEW};
use sk_render::MeshData;

/// Erste Ablage der Teile des Ausbaus, hinter den Netzen aus `main.rs` und
/// der Geländeplatte (Review #137 B4).
pub(crate) const MESH_AUSBAU: usize = MESH_PLATE + 1;
/// Erste Ablage hinter den Teilen des Ausbaus (Netz der GPU-Probe).
#[cfg(all(test, target_os = "linux"))]
pub(crate) const MESH_FREI: usize = MESH_AUSBAU + crate::scene::scene_ausbau::TEILE;

#[cfg(all(test, target_os = "linux"))]
#[path = "ausbau_istbilder.rs"]
mod istbilder;

// Jede Ablage hat ihren eigenen Platz: lückenlos der Reihe nach, keine
// zweimal
const _: () = {
    let a = [
        MESH_MODEL,
        MESH_PREVIEW,
        MESH_LIVE,
        MESH_GHOST,
        MESH_PLATE,
        MESH_AUSBAU,
    ];
    let mut i = 0;
    while i < a.len() {
        assert!(a[i] == i);
        i += 1;
    }
};

impl App {
    /// Rechnet das Netz der Ansicht und lädt es hoch, dazu die geänderten
    /// Teile des Ausbaus.
    pub(crate) fn netz_hochladen(&mut self, plane: Option<Plane>) -> MeshData {
        self.scene.ausbau_getrennt();
        let mesh = self.scene.mesh(self.ui.view, plane, &self.live_runs);
        self.renderer.set_mesh(MESH_MODEL, &mesh);
        let r = &mut self.renderer;
        self.scene
            .ausbau_liefern(|j, m| r.set_mesh(MESH_AUSBAU + j, m));
        mesh
    }
}
