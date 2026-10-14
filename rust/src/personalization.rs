// Preferencias de apariencia de la ventana (eminus-gui).
//
// Este módulo no conoce la paleta ni el estado de la app: sólo guarda lo
// que elige la persona en el panel «personalizar» y lo aplica sobre el
// estilo de egui (texto, espaciado y zoom). Los colores los sigue poniendo
// la paleta del tema; aquí no se toca `Visuals`.
//
use eframe::egui;
use serde::{Deserialize, Serialize};

// Límites del contrato: fuera de ellos `sanitize` recorta al rango.
const FONT_MIN: f32 = 12.0;
const FONT_DEFAULT: f32 = 16.0;
const FONT_MAX: f32 = 22.0;
const ZOOM_MIN: f32 = 0.8;
const ZOOM_DEFAULT: f32 = 1.0;
const ZOOM_MAX: f32 = 2.0;
const READER_MIN: f32 = 400.0;
const READER_DEFAULT_ON: f32 = 600.0;
const READER_MAX: f32 = 900.0;
// Acento por defecto: el del tema «tokyo night».
const ACCENT_DEFAULT: [u8; 3] = [122, 162, 247];

// Densidad de la interfaz: separa más o menos los controles y cambia el
// alto mínimo de los botones. `Normal` deja el estilo actual tal cual.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Density {
    Compact,
    Normal,
    #[default]
    Comfortable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Text,
    Reading,
    Colors,
    Background,
    Interface,
}

impl Page {
    pub const ALL: [Self; 5] = [Self::Text, Self::Reading, Self::Colors, Self::Background, Self::Interface];

    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Texto",
            Self::Reading => "Lectura",
            Self::Colors => "Colores",
            Self::Background => "Fondo",
            Self::Interface => "Interfaz",
        }
    }
}

impl Density {
    fn label(self) -> &'static str {
        match self {
            Density::Compact => "Compacta",
            Density::Normal => "Normal",
            Density::Comfortable => "Cómoda",
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub font: Option<crate::fonts::FontChoice>,
    pub font_size: f32,
    pub zoom: f32,
    pub monospace: bool,
    pub density: Density,
    pub show_shortcuts: bool,
    pub show_clock: bool,
    pub show_counters: bool,
    pub custom_accent: bool,
    pub accent: [u8; 3],
    // Ancho máximo del detalle en puntos; 0 significa automático.
    pub reader_width: f32,
    pub line_spacing: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            font: None,
            font_size: FONT_DEFAULT,
            zoom: ZOOM_DEFAULT,
            monospace: false,
            density: Density::Comfortable,
            show_shortcuts: false,
            show_clock: true,
            show_counters: true,
            custom_accent: false,
            accent: ACCENT_DEFAULT,
            reader_width: 700.0,
            line_spacing: 1.6,
        }
    }
}

impl Appearance {
    // Deja valores válidos lo que venga de disco o de un control con
    // entrada libre (NaN, infinito o fuera de rango no llegan al estilo).
    pub fn sanitize(&mut self) {
        if !self.font_size.is_finite() {
            self.font_size = FONT_DEFAULT;
        }
        self.font_size = self.font_size.clamp(FONT_MIN, FONT_MAX);
        if !self.zoom.is_finite() {
            self.zoom = ZOOM_DEFAULT;
        }
        self.zoom = self.zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        if !self.reader_width.is_finite() || self.reader_width <= 0.0 {
            self.reader_width = 0.0;
        } else {
            self.reader_width = self.reader_width.clamp(READER_MIN, READER_MAX);
        }
        self.line_spacing = if self.line_spacing.is_finite() { self.line_spacing.clamp(1.2, 2.0) } else { 1.6 };
    }
}

// Controles del panel «personalizar». Devuelve `true` si algo cambió (para
// guardar las preferencias). Todo con etiquetas en español.
pub fn page_controls(ui: &mut egui::Ui, prefs: &mut Appearance, page: Page) -> bool {
    let before = prefs.clone();
    match page {
        Page::Text => {
            ui.label("Tamaño del texto");
            ui.add(egui::Slider::new(&mut prefs.font_size, FONT_MIN..=FONT_MAX).suffix(" px"));
            ui.add_space(12.0);
            ui.label("Fuente predeterminada");
            ui.add_enabled_ui(prefs.font.is_none(), |ui| {
                egui::ComboBox::from_id_salt("fuente-base").selected_text(if prefs.monospace { "Monoespaciada" } else { "Proporcional" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut prefs.monospace, false, "Proporcional");
                    ui.selectable_value(&mut prefs.monospace, true, "Monoespaciada");
                });
            });
        }
        Page::Reading => {
            ui.label("Espacio entre líneas");
            ui.add(egui::Slider::new(&mut prefs.line_spacing, 1.2..=2.0).fixed_decimals(1));
            ui.add_space(12.0);
            let mut limited = prefs.reader_width > 0.0;
            if ui.checkbox(&mut limited, "Limitar el ancho de lectura").changed() {
                prefs.reader_width = if limited { READER_DEFAULT_ON } else { 0.0 };
            }
            if limited {
                ui.add(egui::Slider::new(&mut prefs.reader_width, READER_MIN..=READER_MAX).suffix(" px"));
            }
            ui.add_space(16.0);
            if ui.button("Usar lectura cómoda").clicked() {
                prefs.font_size = 18.0;
                prefs.monospace = false;
                prefs.density = Density::Comfortable;
                prefs.reader_width = 700.0;
                prefs.line_spacing = 1.6;
                prefs.show_shortcuts = false;
            }
        }
        Page::Colors => {
            ui.checkbox(&mut prefs.custom_accent, "Usar un color de acento propio");
            if prefs.custom_accent {
                ui.color_edit_button_srgb(&mut prefs.accent);
            }
        }
        Page::Interface => {
            ui.label("Espacio entre controles");
            egui::ComboBox::from_id_salt("densidad").selected_text(prefs.density.label()).show_ui(ui, |ui| {
                for density in [Density::Compact, Density::Normal, Density::Comfortable] {
                    ui.selectable_value(&mut prefs.density, density, density.label());
                }
            });
            ui.add_space(12.0);
            ui.label("Zoom de la interfaz");
            ui.add(egui::Slider::new(&mut prefs.zoom, ZOOM_MIN..=ZOOM_MAX).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
            ui.add_space(16.0);
            ui.checkbox(&mut prefs.show_counters, "Mostrar contadores");
            ui.checkbox(&mut prefs.show_clock, "Mostrar reloj");
            ui.checkbox(&mut prefs.show_shortcuts, "Mostrar guía de atajos");
        }
        Page::Background => {}
    }
    prefs.sanitize();
    *prefs != before
}

pub fn settings_style(ui: &mut egui::Ui) {
    ui.style_mut().text_styles.iter_mut().for_each(|(kind, font)| {
        *font = egui::FontId::proportional(if *kind == egui::TextStyle::Heading { 22.0 } else if *kind == egui::TextStyle::Small { 14.0 } else { 16.0 });
    });
    ui.spacing_mut().item_spacing = egui::vec2(12.0, 10.0);
    ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
    ui.spacing_mut().interact_size.y = 30.0;
    ui.spacing_mut().slider_width = 200.0_f32.min(ui.available_width().max(80.0) * 0.6);
}


// Adapta texto, espaciado y zoom al estilo actual. No toca colores (eso es
// de la paleta) y no pide repintado: con los mismos valores el estilo queda
// igual, así que llamarlo en cada arranque o cambio de tema es barato.
pub fn apply(ctx: &egui::Context, prefs: &Appearance) {
    let mut clean = prefs.clone();
    clean.sanitize();
    // `set_zoom_factor` ya ignora el valor repetido; el `if` evita
    // además el trabajo del `style_mut` cuando nada cambió. El zoom se
    // activa en el siguiente pase de egui.
    if (ctx.zoom_factor() - clean.zoom).abs() > f32::EPSILON {
        ctx.set_zoom_factor(clean.zoom);
    }
    let (item_spacing, button_padding, interact_y) = match clean.density {
        Density::Compact => ([6.0, 1.0], [4.0, 2.0], 20.0),
        Density::Normal => ([8.0, 2.0], [6.0, 3.0], 22.0),
        Density::Comfortable => ([10.0, 6.0], [10.0, 6.0], 28.0),
    };
    let family = if clean.font.is_some() {
        egui::FontFamily::Name(crate::fonts::USER_FONT.into())
    } else if clean.monospace {
        egui::FontFamily::Monospace
    } else {
        egui::FontFamily::Proportional
    };
    let body = egui::FontId::new(clean.font_size, family.clone());
    let small = egui::FontId::new((clean.font_size - 2.0).max(10.0), family);
    ctx.style_mut(|style| {
        for (text_style, font_id) in style.text_styles.iter_mut() {
            *font_id = if matches!(text_style, egui::TextStyle::Small) {
                small.clone()
            } else {
                body.clone()
            };
        }
        style.spacing.item_spacing = item_spacing.into();
        style.spacing.button_padding = button_padding.into();
        style.spacing.interact_size.y = interact_y;
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.animation_time = 0.0;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_priorizan_la_lectura() {
        let prefs = Appearance::default();
        assert_eq!(prefs.font_size, 16.0);
        assert_eq!(prefs.zoom, 1.0);
        assert!(!prefs.monospace);
        assert!(prefs.density == Density::Comfortable);
        assert!(!prefs.show_shortcuts && prefs.show_clock && prefs.show_counters);
        assert!(!prefs.custom_accent);
        assert_eq!(prefs.accent, [122, 162, 247]);
        assert_eq!(prefs.reader_width, 700.0);
        assert_eq!(prefs.line_spacing, 1.6);
        assert!(prefs.font.is_none());
        // Lo que ya había en disco (sin apariencia) sigue valiendo.
        let old: Appearance = serde_json::from_str("{}").unwrap();
        assert!(old == Appearance::default());
    }

    #[test]
    fn preferencias_anteriores_conservan_la_fuente_y_densidad() {
        let prefs: Appearance = serde_json::from_str(r#"{"font_size":14.0,"monospace":true,"density":"Compact","reader_width":0.0}"#).unwrap();
        assert_eq!(prefs.font_size, 14.0);
        assert!(prefs.monospace && prefs.density == Density::Compact);
        assert_eq!(prefs.reader_width, 0.0);
        assert_eq!(prefs.line_spacing, 1.6);
        assert!(prefs.font.is_none());
    }

    #[test]
    fn sanitize_recorta_invalidos() {
        let mut prefs = Appearance {
            font_size: f32::NAN,
            zoom: f32::INFINITY,
            reader_width: f32::NEG_INFINITY,
            ..Appearance::default()
        };
        prefs.sanitize();
        assert_eq!(prefs.font_size, 16.0);
        assert_eq!(prefs.zoom, 1.0);
        assert_eq!(prefs.reader_width, 0.0);

        let mut prefs = Appearance {
            font_size: 99.0,
            zoom: -2.0,
            reader_width: 50.0,
            ..Appearance::default()
        };
        prefs.sanitize();
        assert_eq!(prefs.font_size, 22.0);
        assert_eq!(prefs.zoom, 0.8);
        assert_eq!(prefs.reader_width, 400.0);

        let mut prefs = Appearance {
            font_size: 8.0,
            zoom: 9.0,
            reader_width: 5000.0,
            ..Appearance::default()
        };
        prefs.sanitize();
        assert_eq!(prefs.font_size, 12.0);
        assert_eq!(prefs.zoom, 2.0);
        assert_eq!(prefs.reader_width, 900.0);

        // 0 es automático y se conserva; los negativos también son automático.
        let mut prefs = Appearance {
            reader_width: 0.0,
            ..Appearance::default()
        };
        prefs.sanitize();
        assert_eq!(prefs.reader_width, 0.0);
        let mut prefs = Appearance {
            reader_width: -30.0,
            ..Appearance::default()
        };
        prefs.sanitize();
        assert_eq!(prefs.reader_width, 0.0);
    }

    #[test]
    fn sanitize_acepta_rangos_validos() {
        let mut prefs = Appearance {
            font_size: 18.5,
            zoom: 1.25,
            reader_width: 640.0,
            ..Appearance::default()
        };
        prefs.sanitize();
        assert_eq!(
            (prefs.font_size, prefs.zoom, prefs.reader_width),
            (18.5, 1.25, 640.0)
        );
    }

    fn headless_input(width: f32) -> egui::RawInput {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 480.0),
        ));
        input
    }

    #[test]
    fn apply_por_defecto_es_comodo_y_proporcional() {
        let ctx = egui::Context::default();
        let before_visuals = ctx.style().visuals.clone();
        apply(&ctx, &Appearance::default());
        let _ = ctx.run(headless_input(1180.0), |_| {});
        let style = ctx.style();
        let body = egui::TextStyle::Body.resolve(&style);
        assert_eq!(body.family, egui::FontFamily::Proportional);
        assert_eq!(body.size, 16.0);
        let small = egui::TextStyle::Small.resolve(&style);
        assert_eq!(small.size, 14.0);
        assert_eq!(style.spacing.item_spacing, egui::vec2(10.0, 6.0));
        assert_eq!(style.spacing.button_padding, egui::vec2(10.0, 6.0));
        assert_eq!(style.spacing.interact_size.y, 28.0);
        assert_eq!(style.animation_time, 0.0);
        assert!((ctx.pixels_per_point() - 1.0).abs() < f32::EPSILON);
        // Los colores no son de este módulo: quedan intactos.
        assert_eq!(ctx.style().visuals, before_visuals);
    }

    #[test]
    fn apply_es_idempotente_y_no_repinta_de_mas() {
        let ctx = egui::Context::default();
        apply(&ctx, &Appearance::default());
        let _ = ctx.run(headless_input(1180.0), |_| {});
        let style = (*ctx.style()).clone();
        let zoom = ctx.pixels_per_point();
        apply(&ctx, &Appearance::default());
        let _ = ctx.run(headless_input(1180.0), |_| {});
        assert_eq!(*ctx.style(), style);
        assert!((ctx.pixels_per_point() - zoom).abs() <= f32::EPSILON);
    }

    #[test]
    fn apply_usa_densidad_fuente_y_zoom() {
        let ctx = egui::Context::default();
        let prefs = Appearance {
            monospace: false,
            density: Density::Comfortable,
            zoom: 1.25,
            font_size: 18.0,
            ..Appearance::default()
        };
        apply(&ctx, &prefs);
        let _ = ctx.run(headless_input(1180.0), |_| {});
        let style = ctx.style();
        let body = egui::TextStyle::Body.resolve(&style);
        assert_eq!(body.family, egui::FontFamily::Proportional);
        assert_eq!(body.size, 18.0);
        assert_eq!(egui::TextStyle::Small.resolve(&style).size, 16.0);
        // Cómoda: botones más altos y con más aire, sin animaciones.
        assert!(style.spacing.interact_size.y > 22.0);
        assert!(style.spacing.button_padding.x > 6.0 && style.spacing.button_padding.y > 3.0);
        assert_eq!(style.animation_time, 0.0);
        assert!((ctx.pixels_per_point() - 1.25).abs() < 0.01);

        let ctx = egui::Context::default();
        apply(
            &ctx,
            &Appearance {
                density: Density::Compact,
                ..Appearance::default()
            },
        );
        let style = ctx.style();
        assert!(style.spacing.interact_size.y < 22.0);
    }

    #[test]
    fn controls_en_720px_no_cambia_nada_solo() {
        let ctx = egui::Context::default();
        let mut prefs = Appearance::default();
        for _ in 0..2 {
            let mut changed = false;
            let _ = ctx.run(headless_input(720.0), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed = page_controls(ui, &mut prefs, Page::Text);
                });
            });
            assert!(!changed);
        }
        assert!(prefs == Appearance::default());
        // El foco lo gestiona el panel que integra los controles: aquí no
        // se pide ni se quita el foco del teclado.
        assert!(!ctx.wants_keyboard_input());
    }
}
