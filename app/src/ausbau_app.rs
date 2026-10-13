//! Netz der Ansicht und Ausbau innen in die Ablagen des Renderers (Review
//! #137 B1): Das Netz des Modells ohne den Ausbau, dazu die Teile des
//! Ausbaus in eigenen Ablagen, nur die geänderten
//! ([`Scene::ausbau_liefern`](crate::scene::Scene::ausbau_liefern)).

use crate::scene::Plane;
use crate::{App, MESH_GHOST, MESH_MODEL};
use sk_render::MeshData;

/// Erste Ablage der Teile des Ausbaus, hinter den Netzen aus `main.rs`.
const MESH_AUSBAU: usize = MESH_GHOST + 1;

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
