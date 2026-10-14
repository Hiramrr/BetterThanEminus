// Lo que comparten la app de terminal (main.rs) y la de ventana (gui.rs):
// la API de Eminus, los modelos, la sesión y el estado de la app.

pub mod api;
pub mod app;
pub mod background;
pub mod files;
pub mod fonts;
pub mod model;
pub mod native_shortcuts;
pub mod personalization;
pub mod store;
pub mod text;
pub mod theme;
pub mod ui;
