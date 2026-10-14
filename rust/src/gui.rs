// eminus con ventana: la misma app (mismo estado, misma red, misma sesión)
// dibujada con egui en vez de en la terminal. Los botones hacen lo que
// harían las teclas de la versión de terminal.

use eframe::egui::{self, Align, Color32, Key, Layout, RichText};
use eminus::app::{detail_key, line_of, App, Slot, View};
use eminus::model::{self, DeliveryInfo, Entry, Row, Tone};
use eminus::files::{self, FileKind};
use eminus::{personalization, store};
use eminus::personalization::Appearance;
use eminus::background::{Background, BackgroundPrefs};
use eminus::fonts::SystemFonts;
use eminus::native_shortcuts::{self, Action};
use eminus::text;
use eminus::theme::{Theme, THEMES};
use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;

fn color(c: Color, fallback: Color32) -> Color32 {
    match c {
        Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        _ => fallback,
    }
}

// Los colores del tema elegido, ya en el formato de egui.
struct Palette {
    bg: Color32,
    panel: Color32,
    fg: Color32,
    dim: Color32,
    border: Color32,
    accent: Color32,
    on_accent: Color32,
    sel: Color32,
    red: Color32,
    orange: Color32,
    yellow: Color32,
    green: Color32,
}

impl Palette {
    fn of(t: &Theme) -> Self {
        Palette {
            bg: color(t.bg, Color32::from_rgb(0x1b, 0x1b, 0x1b)),
            panel: if t.name == "eminus" { Color32::WHITE } else { color(t.bg, Color32::from_rgb(0x1b, 0x1b, 0x1b)) },
            fg: color(t.fg, Color32::from_rgb(0xe6, 0xe6, 0xe6)),
            dim: color(t.dim, Color32::from_rgb(0x9a, 0x9a, 0x9a)),
            border: color(t.border, Color32::from_rgb(0x44, 0x44, 0x44)),
            accent: color(t.accent, Color32::from_rgb(0x4f, 0xc3, 0xd9)),
            on_accent: color(t.on_accent, Color32::BLACK),
            sel: color(t.sel, Color32::from_rgb(0x33, 0x33, 0x33)),
            red: color(t.red, Color32::from_rgb(0xf0, 0x5a, 0x5a)),
            orange: color(t.orange, Color32::from_rgb(0xf0, 0x90, 0x50)),
            yellow: color(t.yellow, Color32::from_rgb(0xe6, 0xc0, 0x50)),
            green: color(t.green, Color32::from_rgb(0x8c, 0xc8, 0x60)),
        }
    }

    fn tone(&self, tone: Tone) -> Color32 {
        match tone {
            Tone::Done => self.green,
            Tone::Future => self.dim,
            Tone::Overdue => self.red,
            Tone::Imminent => self.orange,
            Tone::Urgent => self.yellow,
            Tone::Normal | Tone::Plain => self.fg,
        }
    }

    fn apply(&self, ctx: &egui::Context) {
        let dark = u32::from(self.bg.r()) + u32::from(self.bg.g()) + u32::from(self.bg.b()) < 384;
        let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = self.panel;
        v.window_fill = self.panel.to_opaque();
        v.extreme_bg_color = self.bg;
        v.faint_bg_color = self.sel;
        v.override_text_color = Some(self.fg);
        v.hyperlink_color = self.accent;
        v.selection.bg_fill = self.sel;
        v.selection.stroke = egui::Stroke::new(1.0_f32, self.fg);
        v.window_stroke = egui::Stroke::new(1.0_f32, self.border);
        v.window_rounding = egui::Rounding::same(4.0);
        v.menu_rounding = egui::Rounding::same(4.0);
        v.window_shadow = egui::epaint::Shadow::NONE;
        v.popup_shadow = egui::epaint::Shadow::NONE;
        v.warn_fg_color = self.yellow;
        v.error_fg_color = self.red;
        for widget in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            widget.rounding = egui::Rounding::same(4.0);
            widget.bg_fill = self.panel;
            widget.weak_bg_fill = self.panel;
            widget.bg_stroke = egui::Stroke::new(1.0_f32, self.border);
            widget.fg_stroke = egui::Stroke::new(1.0_f32, self.fg);
            widget.expansion = 0.0;
        }
        v.widgets.hovered.bg_stroke.color = self.accent;
        v.widgets.inactive.bg_fill = self.sel;
        v.widgets.hovered.bg_fill = self.sel;
        v.widgets.active.bg_fill = self.accent;
        v.widgets.hovered.weak_bg_fill = self.sel;
        v.widgets.active.bg_stroke.color = self.accent;
        v.widgets.active.weak_bg_fill = self.sel;
        ctx.set_visuals(v);
        ctx.style_mut(|style| {
            for font in style.text_styles.values_mut() {
                font.family = egui::FontFamily::Monospace;
                font.size = 14.0;
            }
            style.text_styles.insert(egui::TextStyle::Small, egui::FontId::monospace(12.0));
            style.spacing.item_spacing = egui::vec2(8.0, 2.0);
            style.spacing.button_padding = egui::vec2(6.0, 3.0);
            style.spacing.interact_size.y = 22.0;
            style.spacing.scroll = egui::style::ScrollStyle::solid();
            style.animation_time = 0.0;
        });
    }

    fn customize(&mut self, prefs: &Appearance) {
        if !prefs.custom_accent {
            return;
        }
        let chosen = Color32::from_rgb(prefs.accent[0], prefs.accent[1], prefs.accent[2]);
        self.accent = (0..=100).map(|step| {
            let part = |a: u8, b: u8| ((u16::from(a) * (100 - step) + u16::from(b) * step) / 100) as u8;
            Color32::from_rgb(part(chosen.r(), self.fg.r()), part(chosen.g(), self.fg.g()), part(chosen.b(), self.fg.b()))
        }).find(|c| contrast(*c, self.bg) >= 4.5 && contrast(*c, self.sel) >= 4.5).unwrap_or(self.fg);
        self.on_accent = if contrast(self.accent, Color32::BLACK) >= contrast(self.accent, Color32::WHITE) { Color32::BLACK } else { Color32::WHITE };
    }
}

struct CatLogo {
    texture: egui::TextureHandle,
    size: egui::Vec2,
    frame_ends: Vec<f64>,
    elapsed: f64,
    started: Option<f64>,
}

impl CatLogo {
    fn new(ctx: &egui::Context) -> Self {
        use image::{AnimationDecoder, ImageDecoder};
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(include_bytes!("../assets/hi-otag.gif")))
            .expect("el GIF incluido debe ser válido");
        let (width, height) = decoder.dimensions();
        let frames = decoder.into_frames().collect_frames().expect("los cuadros del logo deben ser válidos");
        let mut atlas = image::RgbaImage::new(width * frames.len() as u32, height);
        let mut frame_ends = Vec::with_capacity(frames.len());
        let mut end = 0.0;
        for (index, frame) in frames.iter().enumerate() {
            image::imageops::replace(&mut atlas, frame.buffer(), index as i64 * i64::from(width), 0);
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            end += (f64::from(numerator) / f64::from(denominator) / 1000.0).max(0.01);
            frame_ends.push(end);
        }
        let pixels = egui::ColorImage::from_rgba_unmultiplied([atlas.width() as usize, height as usize], atlas.as_raw());
        Self { texture: ctx.load_texture("cat-logo", pixels, egui::TextureOptions::LINEAR), size: egui::vec2(width as f32, height as f32), frame_ends, elapsed: 0.0, started: None }
    }

    fn frame_at(&mut self, now: f64, playing: bool) -> (usize, Option<std::time::Duration>) {
        if playing {
            let start = *self.started.get_or_insert(now - self.elapsed);
            self.elapsed = (now - start).rem_euclid(*self.frame_ends.last().unwrap());
        } else {
            // En reposo vuelve al primer cuadro y no pide más repintados.
            self.started = None;
            self.elapsed = 0.0;
        }
        let frame = self.frame_ends.iter().position(|end| self.elapsed < *end).unwrap_or(0);
        let wait = playing.then(|| std::time::Duration::from_secs_f64(self.frame_ends[frame] - self.elapsed));
        (frame, wait)
    }

    fn show(&mut self, ui: &mut egui::Ui, width: f32) {
        let size = self.size * (width / self.size.x);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Gato saludando"));
        let (now, active) = ui.input(|i| (i.time, i.focused && i.viewport().minimized != Some(true)));
        let visible = ui.is_rect_visible(rect);
        // Sólo se anima con el ratón encima: cada cuadro del GIF repinta la
        // ventana entera, y animado sin parar gastaba ~10 % de CPU.
        let (frame, wait) = self.frame_at(now, active && visible && response.hovered());
        if visible {
            let count = self.frame_ends.len() as f32;
            let uv = egui::Rect::from_min_max(egui::pos2(frame as f32 / count, 0.0), egui::pos2((frame + 1) as f32 / count, 1.0));
            ui.painter().image(self.texture.id(), rect, uv, Color32::WHITE);
        }
        if let Some(wait) = wait { ui.ctx().request_repaint_after(wait); }
    }
}

struct Gui {
    app: App,
    theme: usize,
    scroll_to_selection: bool,
    focus_filter: bool,
    detail_offset: f32,
    detail_step: f32,
    detail_id: String,
    appearance: Appearance,
    applied_appearance: Option<Appearance>,
    settings_open: bool,
    settings_page: personalization::Page,
    settings_note: String,
    fonts: SystemFonts,
    reading_full: bool,
    background: Background,
    background_prefs: BackgroundPrefs,
    applied_opacity: u8,
    logo: Option<CatLogo>,
    avatar: Option<(std::sync::Arc<image::RgbaImage>, egui::TextureHandle)>,
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Cursos").with_inner_size([1180.0, 720.0]).with_min_inner_size([600.0, 400.0]).with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "eminus",
        options,
        Box::new(|cc| {
            let mut app = App::new();
            let prefs = store::load_prefs();
            if prefs.tema.is_empty() { app.theme = eminus::theme::index_of("eminus"); }
            let mut gui = Gui::new(app, prefs.appearance);
            gui.background_prefs = prefs.background;
            gui.background_prefs.sanitize();
            gui.fonts.update(&cc.egui_ctx, &gui.appearance.font);
            gui.apply_style(&cc.egui_ctx);
            Ok(Box::new(gui))
        }),
    )
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show(ctx);
    }
}

impl Gui {
    fn new(app: App, mut appearance: Appearance) -> Self {
        appearance.sanitize();
        Self { theme: app.theme, app, scroll_to_selection: false, focus_filter: false, detail_offset: 0.0, detail_step: 0.0, detail_id: String::new(), appearance, applied_appearance: None, settings_open: false, settings_page: Default::default(), settings_note: String::new(), fonts: SystemFonts::default(), reading_full: false, background: Background::default(), background_prefs: BackgroundPrefs::default(), applied_opacity: 255, logo: None, avatar: None }
    }

    fn palette(&self) -> Palette {
        let mut p = Palette::of(&THEMES[self.app.theme]);
        p.customize(&self.appearance);
        let opacity = self.panel_opacity();
        p.panel = Color32::from_rgba_unmultiplied(p.panel.r(), p.panel.g(), p.panel.b(), opacity);
        p
    }

    fn panel_opacity(&self) -> u8 {
        if self.background.has_image() { (self.background_prefs.panel_opacity.clamp(0.65, 1.0) * 255.0).round() as u8 } else { 255 }
    }

    fn apply_style(&mut self, ctx: &egui::Context) {
        if self.theme != self.app.theme || self.applied_appearance.as_ref() != Some(&self.appearance) || self.applied_opacity != self.panel_opacity() {
            self.theme = self.app.theme;
            self.palette().apply(ctx);
            personalization::apply(ctx, &self.appearance);
            self.applied_appearance = Some(self.appearance.clone());
            self.applied_opacity = self.panel_opacity();
        }
    }

    fn show(&mut self, ctx: &egui::Context) {
        while let Ok((epoch, msg)) = self.app.pool.inbox.try_recv() {
            self.app.on_msg(epoch, msg);
        }
        self.app.tick();
        self.native_keyboard(ctx);
        if self.fonts.update(ctx, &self.appearance.font) {
            self.applied_appearance = None;
        }
        if self.background.update_prefs(ctx, &mut self.background_prefs) {
            if let Err(err) = store::save_background(&self.background_prefs) {
                self.settings_note = format!("no se pudo guardar el fondo: {err}");
            }
        }
        self.apply_style(ctx);
        let p = self.palette();
        self.background.paint(ctx, &self.background_prefs, p.bg, p.accent);

        self.status(ctx, &p);
        if self.app.login.is_some() {
            self.avatar = None;
            egui::CentralPanel::default().frame(egui::Frame::none().fill(p.bg).inner_margin(16.0)).show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.login(ui, &p));
            });
        } else {
            self.keyboard(ctx);
            self.top(ctx, &p);
            self.app.tick();
            self.lists(ctx, &p);
            egui::CentralPanel::default().frame(pane(&p, false)).show(ctx, |ui| {
                if workspace_width(ctx) < 700.0 && !self.reading_full {
                    self.list_body(ui, &p, self.app.view == View::Courses && self.app.focus_mid);
                    return;
                }
                if self.appearance.reader_width > 0.0 {
                    ui.set_max_width(ui.available_width().min(self.appearance.reader_width));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(match self.app.target() {
                        Some(Entry::Act(_)) => "Detalle de la actividad",
                        Some(Entry::Content(_)) => "Material del curso",
                        Some(Entry::Course(_)) => "Acerca del curso",
                        None => "Detalle",
                    }).small().color(p.dim));
                    if self.reading_full && ui.button("Volver a la lista").clicked() {
                        self.reading_full = false;
                    } else if !self.reading_full && ui.button("Ampliar lectura").clicked() {
                        self.reading_full = true;
                    }
                });
                ui.separator();
                ui.add_space(12.0);
                let mut scroll = egui::ScrollArea::vertical().id_salt("detalle").auto_shrink([false, false]);
                let id = match self.app.target() {
                    Some(Entry::Act(a)) => format!("act:{}:{}", a.course_id, a.id),
                    Some(Entry::Content(c)) => format!("content:{}", c.id),
                    Some(Entry::Course(c)) => format!("course:{}", c.id),
                    None => String::new(),
                };
                if self.detail_id != id {
                    self.detail_id = id;
                    scroll = scroll.vertical_scroll_offset(0.0);
                } else if self.detail_step != 0.0 {
                    scroll = scroll.vertical_scroll_offset((self.detail_offset + self.detail_step * ui.available_height() * 0.8).max(0.0));
                }
                self.detail_step = 0.0;
                self.detail_offset = scroll.show(ui, |ui| self.detail(ui, &p)).state.offset.y;
            });
            if !self.settings_open {
                self.delivery(ctx, &p);
                self.offer(ctx, &p);
                self.help(ctx, &p);
            }
        }
        self.settings(ctx, &p);
        if self.app.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Sólo se vuelve a pintar cuando hace falta: mientras hay algo en la
        // red, o cuando toca mover el reloj.
        if std::mem::take(&mut self.app.dirty) {
            ctx.request_repaint();
        }
        ctx.request_repaint_after(self.app.wait());
    }

    fn press(&mut self, c: char) {
        self.app.press(KeyCode::Char(c));
    }

    fn save_appearance(&mut self) {
        self.settings_note = store::save_appearance(&self.appearance).err().map(|e| format!("No se pudieron guardar los ajustes: {e}")).unwrap_or_default();
    }

    fn native_keyboard(&mut self, ctx: &egui::Context) {
        let actions = ctx.input_mut(|i| {
            for event in &mut i.events { native_shortcuts::normalize_editing(event); }
            let mut actions = Vec::new();
            i.events.retain(|event| {
                if let Some(action) = native_shortcuts::action(event) {
                    actions.push(action);
                    false
                } else {
                    true
                }
            });
            actions
        });
        for action in actions {
            let main = self.app.login.is_none() && self.app.delivery.is_none() && self.app.offer.is_none() && !self.settings_open;
            match action {
                Action::Quit => {
                    if self.app.save_delivery_draft() { self.app.quit = true; }
                }
                Action::Close => {
                    if self.settings_open {
                        self.settings_open = false;
                    } else if self.app.help {
                        self.app.help = false;
                    } else if self.app.delivery.is_some() {
                        self.app.press(KeyCode::Esc);
                    } else if self.app.offer.is_some() {
                        self.press('n');
                    } else {
                        self.app.quit = true;
                    }
                }
                Action::Preferences => self.settings_open = true,
                Action::Find if main => {
                    self.focus_filter = true;
                    self.reading_full = false;
                }
                Action::Refresh if self.app.delivery.is_some() && !self.settings_open => self.app.retry_delivery(),
                Action::Refresh if main => self.press('r'),
                Action::SaveDraft if !self.settings_open && self.app.delivery.as_ref().is_some_and(|d| !d.busy) => {
                    if self.app.save_delivery_draft() && !self.app.account.is_empty() {
                        self.app.delivery.as_mut().unwrap().note = "Borrador guardado en este equipo".into();
                    }
                }
                Action::Submit if !self.settings_open && self.app.delivery.as_ref().is_some_and(|d| !d.busy && !d.confirm) => {
                    self.app.on_key(ratatui::crossterm::event::KeyEvent::new(KeyCode::Char('s'), ratatui::crossterm::event::KeyModifiers::CONTROL));
                }
                Action::View(index) if main => {
                    self.app.set_view(View::ALL[index]);
                    self.reading_full = false;
                    self.scroll_to_selection = true;
                }
                Action::ZoomIn | Action::ZoomOut | Action::ZoomReset => {
                    self.appearance.zoom = match action {
                        Action::ZoomIn => self.appearance.zoom + 0.1,
                        Action::ZoomOut => self.appearance.zoom - 0.1,
                        _ => 1.0,
                    };
                    self.appearance.sanitize();
                    self.save_appearance();
                    ctx.request_repaint();
                }
                _ => {}
            }
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() || ctx.input(|i| i.modifiers.command || i.modifiers.mac_cmd) || self.app.delivery.is_some() || self.app.offer.is_some() || self.settings_open {
            return;
        }
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Key { key: Key::D | Key::U, pressed: true, modifiers, .. } = &event {
                if modifiers.ctrl && !modifiers.alt {
                    let page = (ctx.available_rect().height() / 48.0).floor().max(1.0) as isize;
                    self.app.move_sel(if matches!(event, egui::Event::Key { key: Key::D, .. }) { page } else { -page });
                    self.scroll_to_selection = true;
                    continue;
                }
            }
            if let Some(code) = shortcut(&event) {
                if self.app.help {
                    self.app.help = false;
                } else if code == KeyCode::Char('/') {
                    self.focus_filter = true;
                    ctx.input_mut(|i| i.events.retain(|event| !matches!(event, egui::Event::Text(text) if text == "/")));
                } else if code == KeyCode::Char('p') {
                    self.settings_open = true;
                } else if matches!(code, KeyCode::PageDown | KeyCode::Char('J')) {
                    self.detail_step += 1.0;
                } else if matches!(code, KeyCode::PageUp | KeyCode::Char('K')) {
                    self.detail_step -= 1.0;
                } else {
                    self.app.press(code);
                    if workspace_width(ctx) < 700.0 {
                        if matches!(code, KeyCode::Enter | KeyCode::Char('o')) && !matches!(self.app.target(), Some(Entry::Course(_))) {
                            self.reading_full = true;
                        } else if matches!(code, KeyCode::Esc | KeyCode::Char('1'..='5')) {
                            self.reading_full = false;
                        }
                    }
                    self.scroll_to_selection |= matches!(code, KeyCode::Down | KeyCode::Up | KeyCode::Home | KeyCode::End | KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char('j' | 'k' | 'g' | 'G' | 'h' | 'l' | 'o' | 'O' | '1'..='5' | 'a'));
                }
                if self.app.help || self.app.login.is_some() || self.app.delivery.is_some() || self.app.offer.is_some() || self.settings_open || self.app.quit {
                    break;
                }
            }
        }
    }

    fn change_view(&mut self, view: View) {
        self.app.set_view(view);
        self.reading_full = false;
        self.scroll_to_selection = true;
    }

    fn navigation(&mut self, ctx: &egui::Context, p: &Palette) {
        let width = navigation_width(ctx);
        if width == 0.0 { return; }
        egui::SidePanel::left("navegacion").exact_width(width).resizable(false)
            .frame(egui::Frame::none().fill(p.panel).inner_margin(16.0)).show(ctx, |ui| {
                self.logo.get_or_insert_with(|| CatLogo::new(ctx)).show(ui, 64.0);
                ui.add_space(16.0);
                egui::ScrollArea::vertical().id_salt("secciones").show(ui, |ui| {
                    for (index, view) in View::ALL.into_iter().enumerate() {
                        let active = self.app.view == view;
                        let label = RichText::new(view_title(view)).color(if active { p.accent } else { p.fg });
                        let response = ui.add_sized([ui.available_width(), 38.0], egui::Button::new(if active { label.strong() } else { label })
                            .selected(active).frame(active));
                        if active {
                            ui.painter().vline(response.rect.left(), response.rect.y_range().shrink(8.0), egui::Stroke::new(3.0_f32, p.accent));
                        }
                        if response.on_hover_text(format!("{}{}", command_label(), index + 1)).clicked() { self.change_view(view); }
                    }
                    ui.add_space(24.0);
                    ui.separator();
                    ui.add_space(8.0);
                    if ui.add_sized([ui.available_width(), 36.0], egui::Button::new("Personalizar")).clicked() { self.settings_open = true; }
                    ui.add_space(12.0);
                    if ui.add_sized([ui.available_width(), 36.0], egui::Button::new("Ayuda y atajos")).clicked() { self.app.help = true; }
                });
            });
    }

    fn update_avatar(&mut self, ctx: &egui::Context) {
        let Some(photo) = self.app.user_info.photo.as_ref().filter(|_| self.app.login.is_none()) else {
            self.avatar = None;
            return;
        };
        if self.avatar.as_ref().is_some_and(|(previous, _)| std::sync::Arc::ptr_eq(previous, photo)) { return; }
        let pixels = egui::ColorImage::from_rgba_unmultiplied([photo.width() as usize, photo.height() as usize], photo.as_raw());
        self.avatar = Some((photo.clone(), ctx.load_texture("foto-de-perfil", pixels, egui::TextureOptions::LINEAR)));
    }

    fn account_menu(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.set_max_width((ui.ctx().screen_rect().width() - 48.0).clamp(180.0, 360.0));
        ui.horizontal(|ui| {
            if let Some((_, texture)) = &self.avatar {
                ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(44.0, 44.0)).rounding(22.0))
                    .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Foto de perfil"));
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(self.app.display_name()).strong());
                let username = if self.app.user_info.username.is_empty() { &self.app.account } else { &self.app.user_info.username };
                if !username.is_empty() && username != self.app.display_name() {
                    ui.label(RichText::new(username).small().color(p.dim));
                }
            });
        });
        ui.separator();
        if ui.button("Personalizar").clicked() { self.settings_open = true; ui.close_menu(); }
        if ui.button("Ayuda y atajos").clicked() { self.app.help = true; ui.close_menu(); }
        if ui.button("Cambiar de tema").clicked() { self.press('c'); ui.close_menu(); }
        if ui.button("Cerrar sesión").clicked() { self.press('X'); ui.close_menu(); }
    }

    fn top(&mut self, ctx: &egui::Context, p: &Palette) {
        self.update_avatar(ctx);
        self.navigation(ctx, p);
        let narrow = workspace_width(ctx) < 700.0;
        egui::TopBottomPanel::top("barra")
            .frame(egui::Frame::none().fill(p.panel).inner_margin(egui::Margin::symmetric(20.0, 14.0))).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if navigation_width(ctx) == 0.0 {
                    self.logo.get_or_insert_with(|| CatLogo::new(ctx)).show(ui, 32.0);
                    let mut view = self.app.view;
                    egui::ComboBox::from_id_salt("vista").selected_text(view_title(view)).width(140.0).show_ui(ui, |ui| {
                        for v in View::ALL { ui.selectable_value(&mut view, v, view_title(v)); }
                    });
                    if view != self.app.view { self.change_view(view); }
                } else {
                    ui.label(RichText::new(view_title(self.app.view)).size(26.0).strong());
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let avatar = self.avatar.as_ref().map(|(_, texture)| texture.clone());
                    if let Some(texture) = avatar {
                        let image = egui::Image::new(&texture).fit_to_exact_size(egui::vec2(24.0, 24.0)).rounding(12.0);
                        ui.menu_image_text_button(image, "Mi cuenta", |ui| self.account_menu(ui, p));
                    } else {
                        ui.menu_button("Mi cuenta", |ui| self.account_menu(ui, p));
                    }
                });
            });
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                let hint = match self.app.view {
                    View::Courses => "Buscar en mis cursos",
                    View::Content => "Buscar en el contenido",
                    View::Grades => "Buscar en mis calificaciones",
                    _ => "Buscar actividad o materia",
                };
                let filter = egui::TextEdit::singleline(&mut self.app.filter).id_salt("filtro").hint_text(hint)
                    .margin(egui::vec2(10.0, 7.0)).desired_width((ui.available_width() - 20.0).clamp(0.0, 420.0));
                let response = ui.add(filter).on_hover_text(format!("Buscar en esta sección · {}F", command_label()));
                if self.focus_filter {
                    response.request_focus();
                    self.focus_filter = false;
                }
                if response.changed() {
                    self.app.refilter();
                    self.scroll_to_selection = true;
                }
                if (response.has_focus() || response.lost_focus()) && ui.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Enter)) {
                    if ui.input(|i| i.key_pressed(Key::Escape)) { self.app.clear_filter(); }
                    response.surrender_focus();
                }
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if !self.app.filter.is_empty() && ui.button("Limpiar").clicked() {
                    self.app.clear_filter();
                    self.scroll_to_selection = true;
                }
                if narrow {
                    ui.menu_button("Opciones", |ui| self.toolbar_controls(ui));
                } else { self.toolbar_controls(ui); }
            });
        });
    }

    fn toolbar_controls(&mut self, ui: &mut egui::Ui) {
        if matches!(self.app.view, View::Agenda | View::Courses) {
            let what = if self.app.view == View::Courses { "Cursos anteriores" } else { "Mostrar entregadas" };
            let mut show_all = self.app.show_all;
            if ui.checkbox(&mut show_all, what).changed() {
                self.press('a');
                self.scroll_to_selection = true;
            }
        }
        let loading = self.app.loading || (self.app.view == View::Content && self.app.content_loading);
        if ui.add_enabled(!loading, egui::Button::new(if loading { "Actualizando…" } else { "Actualizar" }))
            .on_hover_text(format!("{}R", command_label())).clicked() { self.press('r'); }
    }

    fn status(&mut self, ctx: &egui::Context, p: &Palette) {
        egui::TopBottomPanel::bottom("estado").show(ctx, |ui| {
            if self.app.login.is_none() && self.appearance.show_shortcuts && ctx.screen_rect().width() >= 700.0 {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("j/k mover  / filtrar  enter ver  E entregar  V mi entrega  w web  ? ayuda").color(p.dim));
                });
                ui.separator();
            }
            ui.horizontal_wrapped(|ui| {
                if self.app.loading || self.app.content_loading || self.app.pool.pending > 0 {
                    ui.label(RichText::new("cargando…").color(p.accent));
                }
                ui.label(RichText::new(self.app.message().replace(" · ? ayuda", "")).color(if self.app.error { p.red } else { p.dim }));
                if self.app.login.is_some() {
                    if ui.button("personalizar").clicked() {
                        self.settings_open = true;
                    }
                    return;
                }
                ui.label(RichText::new("|").color(p.border));
                {
                    if let Some(left) = self.app.session_left().filter(|l| *l < 20 * 60) {
                        let tone = if left < 3 * 60 { p.red } else if left < 10 * 60 { p.yellow } else { p.dim };
                        ui.label(RichText::new(format!("sesión {} min", (left / 60).max(0))).color(tone));
                    }
                    if self.appearance.show_counters && ctx.screen_rect().width() >= 600.0 {
                        let c = &self.app.counts;
                        for (n, label, tone) in [(c.overdue, "vencidas", p.red), (c.today, "hoy", p.orange), (c.soon, "48 h", p.yellow), (c.week, "7 días", p.fg)] {
                            ui.label(RichText::new(format!("{label} {n}")).color(if n > 0 { tone } else { p.dim }));
                        }
                    }
                }
                if self.appearance.show_clock && ctx.screen_rect().width() >= 460.0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(text::clock()).color(p.dim));
                    });
                }
            });
        });
    }

    fn lists(&mut self, ctx: &egui::Context, p: &Palette) {
        let width = workspace_width(ctx);
        if self.reading_full || width < 700.0 { return; }
        let fill = if self.background.has_image() { p.panel } else { p.bg };
        let courses = self.app.view == View::Courses;
        let both = courses && width >= 1050.0;
        egui::SidePanel::left(if courses { "cursos" } else { "lista" })
            .resizable(true).default_width(if both { 220.0 } else { 320.0 })
            .width_range(190.0..=width * if both { 0.25 } else { 0.45 })
            .show_separator_line(true).frame(pane(p, false).fill(fill)).show(ctx, |ui| {
                self.list_body(ui, p, courses && !both && self.app.focus_mid);
            });
        if both {
            egui::SidePanel::left("actividades").resizable(true).default_width(280.0)
                .width_range(190.0..=width * 0.3).show_separator_line(true).frame(pane(p, false).fill(fill)).show(ctx, |ui| {
                    self.list_body(ui, p, true);
                });
        }
    }

    fn list_body(&mut self, ui: &mut egui::Ui, p: &Palette, mid: bool) {
        let compact_courses = self.app.view == View::Courses && workspace_width(ui.ctx()) < 1050.0;
        if compact_courses && mid {
            if ui.button("Volver a cursos").clicked() {
                self.app.press(KeyCode::Left);
                self.scroll_to_selection = true;
            }
            if let Some(c) = self.app.selected_course() { ui.label(RichText::new(c.name.as_ref()).strong()); }
            ui.add_space(8.0);
        }
        let rows_ref = if mid { &self.app.mid_rows } else { &self.app.rows };
        let sel = if mid { self.app.mid.sel } else { self.app.list().sel };
        let count = rows_ref.iter().filter(|r| matches!(r, Row::Item { .. })).count();
        let title = if mid { "Actividades del curso" } else { view_title(self.app.view) };
        let focused = self.app.view != View::Courses || self.app.focus_mid == mid;
        pane_title(ui, p, &format!("{title} · {count}"), focused);
        let empty = if mid {
            match self.app.selected_course() {
                Some(c) if self.app.acts_pending(&c.id) => "Cargando actividades…",
                Some(_) => "Sin actividades publicadas",
                None => "Elige un curso",
            }
        } else if self.app.loading && self.app.loaded_at == 0 {
            "cargando…"
        } else if !self.app.filter.is_empty() {
            "nada coincide con el filtro"
        } else {
            match self.app.view {
                View::Agenda => "sin pendientes por ahora",
                View::Miyu => "todo entregado",
                View::Grades => "aún no hay entregas calificadas",
                View::Content if self.app.content_loading || !self.app.content_tried => "cargando contenido…",
                View::Content => "tus cursos actuales no tienen contenido publicado",
                View::Courses => "sin cursos",
            }
        };
        let scroll = self.scroll_to_selection && focused;
        if focused { self.scroll_to_selection = false; }
        if let Some(index) = rows(ui, p, rows_ref, sel, focused, scroll, empty) {
            self.app.select(mid, index);
            if compact_courses && !mid {
                self.app.press(KeyCode::Enter);
            } else if workspace_width(ui.ctx()) < 700.0 {
                self.reading_full = true;
            }
        }
    }

    fn detail(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(entry) = self.app.target().cloned() else {
            ui.add_space(24.0);
            ui.label(RichText::new("Elige una actividad o un curso").size(22.0).strong());
            ui.label(RichText::new("Selecciona un elemento de la lista para ver las instrucciones, los archivos y tu entrega.").color(p.dim));
            return;
        };
        let now = text::now();
        let today = self.app.clock.today;
        ui.add_space(6.0);
        match entry {
            Entry::Act(act) => {
                let tone = model::urgency(&act, now);
                ui.label(RichText::new(act.title.as_ref()).size(self.appearance.font_size + 10.0).strong().color(p.fg));
                ui.label(RichText::new(act.course_name.as_ref()).color(p.dim));
                ui.add_space(6.0);
                let status = match tone {
                    Tone::Done => format!("Entregada {}", if act.delivered_at != 0 { text::full_date(act.delivered_at) } else { String::new() }),
                    Tone::Future => "por iniciar".into(),
                    Tone::Overdue => "vencida, sin entregar".into(),
                    Tone::Imminent => "vence en menos de 24 h".into(),
                    Tone::Urgent => "vence en menos de 48 h".into(),
                    _ => "pendiente".into(),
                };
                let slot = self.app.details.get(&detail_key(&act.course_id, &act.id));
                let (has_files, blocked) = match slot {
                    Some(Slot::Ready(d)) => (!d.files.is_empty(), d.blocked.as_deref()),
                    _ => (false, None),
                };
                kv_list(ui, p, vec![
                    ("Estado", RichText::new(&status).color(p.tone(tone))),
                    ("Vence", RichText::new(format!("{} · {}", text::full_date(act.deadline), text::rel_due(act.deadline, now, today)))),
                ]);
                egui::CollapsingHeader::new("Fechas y calificación").id_salt("actividad-fechas").show(ui, |ui| {
                    let mut data = vec![
                        ("Inicia", RichText::new(text::full_date(act.start))),
                        ("Valor", RichText::new(act.value.map_or("Sin valor publicado".into(), model::fmt_num))),
                        ("Calificación", RichText::new(act.grade.map_or("Sin calificar".into(), model::fmt_num))),
                    ];
                    if act.extension != 0 { data.push(("Prórroga", RichText::new(text::full_date(act.extension)))); }
                    kv_list(ui, p, data);
                });
                ui.add_space(8.0);
                let mut action = None;
                let mut download = None;
                let mut submission_download = None;
                let mut review_download = None;
                let mut course_view = None;
                ui.horizontal_wrapped(|ui| {
                    let deliver = egui::Button::new(RichText::new(if act.delivered { "Volver a entregar" } else { "Entregar" }).strong().color(p.on_accent)).fill(p.accent);
                    if ui.add_enabled(blocked.is_none(), deliver).on_disabled_hover_text(blocked.unwrap_or_default()).clicked() {
                        action = Some(KeyCode::Char('E'));
                    }
                    if ui.add_enabled(has_files, egui::Button::new("Descargar adjuntos")).clicked() {
                        action = Some(KeyCode::Char('D'));
                    }
                    ui.menu_button("Más", |ui| {
                        if ui.button("Actualizar entrega y retroalimentación").clicked() { action = Some(KeyCode::Char('V')); ui.close_menu(); }
                        if ui.button("Descargar mi entrega y revisión").clicked() { action = Some(KeyCode::Char('B')); ui.close_menu(); }
                        if ui.button("Ver curso").clicked() { course_view = Some(View::Courses); ui.close_menu(); }
                        if ui.add_enabled(self.app.courses.iter().any(|c| c.id == act.course_id && c.current), egui::Button::new("Contenido del curso")).clicked() { course_view = Some(View::Content); ui.close_menu(); }
                        if ui.button("Abrir en Eminus web").clicked() { action = Some(KeyCode::Char('w')); ui.close_menu(); }
                    });
                });
                if let Some(reason) = blocked {
                    ui.label(RichText::new(reason).color(p.yellow));
                }
                ui.add_space(24.0);
                ui.label(RichText::new("Instrucciones").size(self.appearance.font_size + 2.0).strong());
                match slot {
                    None | Some(Slot::Pending) => {
                        ui.label(RichText::new("cargando...").color(p.dim));
                    }
                    Some(Slot::Failed(note)) => {
                        ui.label(RichText::new(format!("no se pudo cargar la descripción ({note})")).color(p.dim));
                    }
                    Some(Slot::Ready(detail)) => {
                        if detail.text.is_empty() {
                            ui.label(RichText::new("sin descripción").color(p.dim));
                        } else {
                            read_text(ui, &detail.text, self.appearance.line_spacing, self.appearance.reader_width);
                        }
                        if !detail.files.is_empty() {
                            ui.add_space(10.0);
                            ui.label(RichText::new("Archivos del docente").strong());
                            for file in &detail.files {
                                ui.horizontal_wrapped(|ui| {
                                    file_icon(ui, p, &file.name);
                                    if ui.add_enabled(!file.id.is_empty(), egui::Button::new(format!("descargar {}", file.name))).clicked() {
                                        download = Some((file.id.clone(), file.name.clone()));
                                    }
                                    ui.label(RichText::new(&file.size).color(p.dim));
                                });
                            }
                        }
                    }
                }
                ui.add_space(16.0);
                ui.separator();
                ui.label(RichText::new("Mi entrega").size(self.appearance.font_size + 2.0).strong());
                match self.app.deliveries.get(&detail_key(&act.course_id, &act.id)) {
                    None | Some(Slot::Pending) => {
                        ui.label(RichText::new("consultando entrega…").color(p.dim));
                    }
                    Some(Slot::Failed(note)) => {
                        ui.label(RichText::new(format!("no se pudo consultar tu entrega: {note}")).color(p.yellow));
                        if ui.button("reintentar consulta [V]").clicked() {
                            action = Some(KeyCode::Char('V'));
                        }
                    }
                    Some(Slot::Ready(info)) => {
                        ui.label(RichText::new(delivery_state_text(info)).color(if info.sent { p.green } else { p.dim }));
                        if !info.text.is_empty() {
                            read_text(ui, &info.text, self.appearance.line_spacing, self.appearance.reader_width);
                        }
                        for file in &info.files {
                            ui.horizontal_wrapped(|ui| {
                                file_icon(ui, p, &file.name);
                                if ui.add_enabled(!file.id.is_empty(), egui::Button::new(format!("descargar {}", file.name))).clicked() {
                                    submission_download = Some((file.id.clone(), file.name.clone()));
                                }
                                ui.label(RichText::new(&file.size).color(p.dim));
                            });
                        }
                        ui.add_space(16.0);
                        ui.label(RichText::new("Retroalimentación").size(self.appearance.font_size + 2.0).strong());
                        if let Some(grade) = info.grade {
                            ui.label(format!("calificación {}", model::fmt_num(grade)));
                        }
                        if info.reviewed_at != 0 {
                            ui.label(RichText::new(format!("revisada {}", text::full_date(info.reviewed_at))).color(p.dim));
                        }
                        if info.feedback.is_empty() {
                            ui.label(RichText::new("sin comentarios del docente").color(p.dim));
                        } else {
                            read_text(ui, &info.feedback, self.appearance.line_spacing, self.appearance.reader_width);
                        }
                        if !info.review_files.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("archivos de la revisión").strong().color(p.dim));
                            for file in &info.review_files {
                                ui.horizontal_wrapped(|ui| {
                                    file_icon(ui, p, &file.name);
                                    if ui.add_enabled(!file.id.is_empty(), egui::Button::new(format!("descargar {}", file.name))).clicked() {
                                        review_download = Some((file.id.clone(), file.name.clone()));
                                    }
                                    ui.label(RichText::new(&file.size).color(p.dim));
                                });
                            }
                        }
                    }
                }
                if let Some((id, name)) = download {
                    self.app.download_activity_file(&act, &id, &name);
                }
                if let Some((id, name)) = submission_download {
                    self.app.download_delivery_file(&act, &id, &name);
                }
                if let Some((id, name)) = review_download {
                    self.app.download_review_file(&act, &id, &name);
                }
                if let Some(view) = course_view {
                    self.app.course_view(&act.course_id, view);
                    self.scroll_to_selection = true;
                }
                if let Some(code) = action {
                    self.app.press(code);
                }
            }
            Entry::Content(item) => {
                let slot = self.app.content_details.get(&item.id);
                let loading = matches!(slot, None | Some(Slot::Pending));
                let (description, files) = match slot {
                    Some(Slot::Ready(d)) => (d.text.as_str(), d.files.as_slice()),
                    _ => (item.text.as_str(), item.files.as_slice()),
                };
                ui.label(RichText::new(item.title.as_ref()).size(self.appearance.font_size + 10.0).strong().color(p.fg));
                ui.label(RichText::new(item.course_name.as_ref()).color(p.dim));
                ui.add_space(6.0);
                let content_kind = model::content_label(item.unit, files.len(), item.has_files);
                if kv_wide(ui) {
                    egui::Grid::new("contenido-datos").num_columns(2).min_col_width(40.0).max_col_width((ui.available_width() - 128.0).max(64.0)).spacing([18.0, 4.0]).show(ui, |ui| {
                        ui.label(RichText::new("tipo").color(p.dim));
                        ui.label(&content_kind);
                        ui.end_row();
                        if !item.unit {
                            ui.label(RichText::new("módulo").color(p.dim));
                            ui.label(&item.unit_name);
                            ui.end_row();
                        }
                        ui.label(RichText::new("publicado").color(p.dim));
                        ui.label(text::full_date(item.published));
                        ui.end_row();
                    });
                } else {
                    let mut rows = vec![("tipo", RichText::new(&content_kind))];
                    if !item.unit {
                        rows.push(("módulo", RichText::new(&item.unit_name)));
                    }
                    rows.push(("publicado", RichText::new(text::full_date(item.published))));
                    kv_list(ui, p, rows);
                }
                ui.add_space(8.0);
                let mut action = None;
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!files.is_empty(), egui::Button::new(RichText::new("Descargar archivos").strong().color(p.on_accent)).fill(p.accent)).clicked() {
                        action = Some(KeyCode::Char('D'));
                    }
                    if ui.button("Abrir en Eminus web").clicked() {
                        action = Some(KeyCode::Char('w'));
                    }
                });
                ui.add_space(24.0);
                ui.label(RichText::new("Instrucciones").size(self.appearance.font_size + 2.0).strong());
                if description.is_empty() && loading {
                    ui.label(RichText::new("cargando...").color(p.dim));
                } else if description.is_empty() {
                    ui.label(RichText::new("sin descripción").color(p.dim));
                } else {
                    read_text(ui, description, self.appearance.line_spacing, self.appearance.reader_width);
                }
                ui.add_space(10.0);
                ui.label(RichText::new("archivos").strong().color(p.dim));
                if files.is_empty() {
                    ui.label(RichText::new(if loading { "buscando archivos…" } else { "sin archivos" }).color(p.dim));
                }
                for file in files {
                    ui.horizontal_wrapped(|ui| {
                        file_icon(ui, p, &file.name);
                        ui.label(&file.name);
                        ui.label(RichText::new(&file.size).color(p.dim));
                    });
                }
                if let Some(code) = action {
                    self.app.press(code);
                }
            }
            Entry::Course(course) => {
                let none = Vec::new();
                let acts = self.app.acts.get(&course.id).unwrap_or(&none);
                let done = acts.iter().filter(|a| a.delivered).count();
                let (pending_count, pending) = upcoming_activities(acts, now);
                let overdue = acts.iter().filter(|a| model::urgency(a, now) == Tone::Overdue).count();
                ui.label(RichText::new(course.name.as_ref()).size(self.appearance.font_size + 10.0).strong().color(p.fg));
                ui.label(RichText::new(format!("{}{}", if course.current { "curso actual" } else { "curso anterior" }, if course.favorite { " · favorito" } else { "" })).color(p.dim));
                ui.add_space(6.0);
                let progress = format!("{done} de {} actividades", acts.len());
                ui.add(egui::ProgressBar::new(if acts.is_empty() { 0.0 } else { done as f32 / acts.len() as f32 })
                    .fill(p.accent).animate(false).desired_height(6.0).rounding(2.0));
                ui.add_space(8.0);
                if kv_wide(ui) {
                    egui::Grid::new("curso-datos").num_columns(2).min_col_width(40.0).max_col_width((ui.available_width() - 128.0).max(64.0)).spacing([18.0, 4.0]).show(ui, |ui| {
                        ui.label(RichText::new("termina").color(p.dim));
                        ui.label(text::full_date(course.end));
                        ui.end_row();
                        ui.label(RichText::new("entregadas").color(p.dim));
                        ui.label(RichText::new(&progress).color(p.green));
                        ui.end_row();
                        ui.label(RichText::new("pendientes").color(p.dim));
                        ui.label(pending_count.to_string());
                        ui.end_row();
                        ui.label(RichText::new("vencidas").color(p.dim));
                        ui.label(RichText::new(overdue.to_string()).color(if overdue > 0 { p.red } else { p.fg }));
                        ui.end_row();
                    });
                } else {
                    kv_list(ui, p, vec![
                        ("termina", RichText::new(text::full_date(course.end))),
                        ("entregadas", RichText::new(&progress).color(p.green)),
                        ("pendientes", RichText::new(pending_count.to_string())),
                        ("vencidas", RichText::new(overdue.to_string()).color(if overdue > 0 { p.red } else { p.fg })),
                    ]);
                }
                if pending_count > 0 {
                    ui.add_space(10.0);
                    ui.label(RichText::new("lo próximo").strong().color(p.dim));
                    for a in pending.into_iter().flatten() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(text::rel_due(a.deadline, now, today)).color(p.tone(model::urgency(a, now))));
                            ui.label(a.title.as_ref());
                        });
                    }
                }
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Ver actividades").on_hover_text("explora el curso en la app").clicked() {
                        self.app.press(KeyCode::Enter);
                        self.scroll_to_selection = true;
                    }
                    if ui.add_enabled(course.current, egui::Button::new("contenido del curso")).on_disabled_hover_text("la app carga el contenido de cursos actuales").clicked() {
                        self.app.course_view(&course.id, View::Content);
                        self.scroll_to_selection = true;
                    }
                    if ui.button("Abrir en Eminus web").clicked() {
                        self.app.press(KeyCode::Char('w'));
                    }
                });
            }
        }
    }

    fn login(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let profiles = self.app.profiles.perfiles.clone();
        let mut keys: Vec<KeyCode> = Vec::new();
        let mut pick = None;
        let logo = self.logo.get_or_insert_with(|| CatLogo::new(ui.ctx()));
        let Some(login) = &mut self.app.login else { return };
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.16);
            logo.show(ui, 80.0);
            ui.add_space(14.0);
            let width = ui.available_width().min(360.0);
            ui.allocate_ui_with_layout(egui::vec2(width, 0.0), Layout::top_down_justified(Align::Center), |ui| {
                ui.set_max_width(width);
                ui.add_enabled_ui(!login.busy, |ui| {
                    if login.picker {
                        ui.label(RichText::new("¿quién entra?").color(p.dim));
                        ui.add_space(4.0);
                        for (i, profile) in profiles.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let name = if profile.name == profile.user { profile.user.clone() } else { format!("{}  ·  {}", profile.name, profile.user) };
                                if ui.add_sized([(width - 42.0).max(60.0), 30.0], egui::Button::new(RichText::new(name).strong())).clicked() {
                                    pick = Some(i);
                                    keys.push(KeyCode::Enter);
                                }
                                if ui.add_sized([30.0, 30.0], egui::Button::new("x")).on_hover_text("quitar este perfil (pide confirmar)").clicked() {
                                    pick = Some(i);
                                    keys.push(KeyCode::Char('x'));
                                }
                            });
                        }
                        ui.add_space(6.0);
                        if ui.button("+ iniciar con otro perfil").clicked() {
                            keys.push(KeyCode::Char('+'));
                        }
                    } else {
                        ui.label(RichText::new("usuario").color(p.dim));
                        let user = ui.add(egui::TextEdit::singleline(&mut login.user));
                        ui.label(RichText::new("contraseña").color(p.dim));
                        let pass = ui.add(egui::TextEdit::singleline(&mut login.pass).password(true));
                        let submitted = (user.lost_focus() || pass.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter));
                        ui.add_space(4.0);
                        if ui.add(egui::Button::new(RichText::new("entrar").strong().color(p.on_accent)).fill(p.accent)).clicked() || submitted {
                            keys.push(KeyCode::Enter);
                        }
                        ui.add_space(10.0);
                        ui.collapsing("entrar con un token", |ui| {
                            ui.add(egui::TextEdit::singleline(&mut login.token).password(true).hint_text("pega aquí el token de sesión"));
                            ui.label(RichText::new("en la consola del navegador: copy(localStorage.accessToken)").color(p.dim).small());
                        });
                        if !profiles.is_empty() && ui.button("← volver a los perfiles").clicked() {
                            keys.push(KeyCode::Esc);
                        }
                    }
                });
                ui.add_space(8.0);
                if login.busy {
                    ui.label(RichText::new("cargando...").color(p.dim));
                }
                if !login.note.is_empty() {
                    ui.label(RichText::new(&login.note).color(if login.busy { p.accent } else { p.yellow }));
                }
            });
        });
        if let Some(i) = pick {
            login.pick = i;
        }
        for key in keys {
            self.app.press(key);
        }
    }

    fn delivery(&mut self, ctx: &egui::Context, p: &Palette) {
        if self.app.delivery.is_none() {
            return;
        }
        // Soltar archivos sobre la ventana los agrega a la entrega.
        let dropped: Vec<String> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.as_ref()).map(|path| path.to_string_lossy().into_owned()).collect());
        let mut add: Vec<String> = dropped;
        let mut keys: Vec<KeyCode> = Vec::new();
        let mut retry = false;
        let Some(d) = &mut self.app.delivery else { return };
        egui::Window::new("Entregar actividad").collapsible(false).resizable(true).default_size([560.0, 500.0]).min_width(240.0).max_width((ctx.screen_rect().width() - 32.0).max(240.0)).max_height((ctx.screen_rect().height() - 64.0).max(100.0)).constrain_to(ctx.screen_rect()).show(ctx, |ui| {
            egui::ScrollArea::vertical().id_salt("edicion-entrega").auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(d.act.title.as_ref()).strong());
            ui.label(RichText::new(d.act.course_name.as_ref()).color(p.dim));
            ui.add_space(4.0);
            match &d.info {
                Slot::Pending => {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("cargando...").color(p.dim));
                        ui.label(RichText::new("consultando tu entrega en eminus…").color(p.dim));
                    });
                }
                Slot::Failed(note) => {
                    ui.label(RichText::new(format!("no se pudo consultar tu entrega ({note})")).color(p.yellow));
                    if ui.button("reintentar consulta [Ctrl+r]").clicked() {
                        retry = true;
                    }
                }
                Slot::Ready(info) => {
                    let (state, tone) = if info.sent {
                        ("ya la entregaste: al enviar se actualiza el comentario y se agregan los archivos nuevos", p.yellow)
                    } else if info.id > 0 {
                        ("tienes un borrador en eminus: se completa con lo de aquí", p.dim)
                    } else {
                        ("aún no hay entrega en eminus", p.dim)
                    };
                    ui.label(RichText::new(state).color(tone));
                    for file in &info.files {
                        ui.horizontal_wrapped(|ui| {
                            file_icon(ui, p, &file.name);
                            ui.label(RichText::new(format!("{}   {} · ya en eminus", file.name, file.size)).color(p.green));
                        });
                    }
                }
            }
            ui.add_space(8.0);
            ui.add_enabled_ui(!d.busy && !d.confirm, |ui| {
                ui.label(RichText::new("archivos por subir").strong().color(p.dim));
                let mut remove = None;
                for (i, (path, size)) in d.files.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        file_icon(ui, p, &name);
                        ui.label(name).on_hover_text(path.display().to_string());
                        ui.label(RichText::new(model::file_size(*size as f64)).color(p.dim));
                    });
                }
                if let Some(i) = remove {
                    d.files.remove(i);
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("elegir archivos…").clicked() {
                        if let Some(paths) = rfd::FileDialog::new().set_title("Archivos para entregar").pick_files() {
                            add.extend(paths.into_iter().map(|path| path.to_string_lossy().into_owned()));
                        }
                    }
                    ui.label(RichText::new("o suéltalos sobre la ventana").color(p.dim));
                });
                ui.add_space(8.0);
                ui.label(RichText::new("comentario (eminus lo exige)").strong().color(p.dim));
                ui.add(egui::TextEdit::multiline(&mut d.text).desired_rows(6).desired_width(f32::INFINITY));
            });
            ui.add_space(8.0);
            if d.busy {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("cargando...").color(p.dim));
                    ui.label(RichText::new(&d.note).color(p.accent));
                });
            } else if d.confirm && !ctx.input(|i| i.modifiers.command || i.modifiers.mac_cmd || i.modifiers.ctrl || i.modifiers.alt) {
                let files = match d.files.len() {
                    0 => "sin archivos nuevos".to_string(),
                    1 => "con 1 archivo".to_string(),
                    n => format!("con {n} archivos"),
                };
                ui.label(RichText::new(format!("¿Entregar {files}? Se envía a tu docente en eminus.")).strong().color(p.orange));
                ui.horizontal_wrapped(|ui| {
                    if ui.add(egui::Button::new(RichText::new("sí, entregar").strong().color(p.on_accent)).fill(p.accent)).clicked() {
                        keys.push(KeyCode::Char('s'));
                    }
                    if ui.button("volver").clicked() {
                        keys.push(KeyCode::Char('n'));
                    }
                });
            } else {
                if !d.note.is_empty() {
                    ui.label(RichText::new(&d.note).color(p.yellow));
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.add(egui::Button::new(RichText::new("entregar…").strong().color(p.on_accent)).fill(p.accent)).clicked() {
                        // Enter en el campo de comentario pide la confirmación.
                        d.field = 1;
                        keys.push(KeyCode::Enter);
                    }
                    if ui.button("guardar borrador y cerrar").clicked() {
                        keys.push(KeyCode::Esc);
                    }
                });
            }
            });
        });
        if !d.busy {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                keys.push(KeyCode::Esc);
            } else if d.confirm {
                for event in ctx.input(|i| i.events.clone()) {
                    if let egui::Event::Text(value) = event {
                        if matches!(value.as_str(), "s" | "S" | "n" | "N") {
                            keys.push(KeyCode::Char(value.chars().next().unwrap()));
                        }
                    }
                }
            }
        }
        if self.app.delivery.as_ref().is_some_and(|d| !d.busy && !d.confirm) {
            add.iter().for_each(|path| self.app.add_delivery_file(path));
        }
        if retry || ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::R)) {
            self.app.retry_delivery();
        }
        for key in keys {
            self.app.press(key);
        }
    }

    fn offer(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some((user, _)) = &self.app.offer else { return };
        let mut key = None;
        egui::Window::new("perfiles").collapsible(false).resizable(true).default_width(420.0).max_width((ctx.screen_rect().width() - 32.0).max(240.0)).show(ctx, |ui| {
            ui.label(RichText::new(format!("¿Guardar el perfil de {user}?")).strong());
            ui.label(RichText::new("La contraseña queda en el llavero del sistema y la próxima vez entras sin escribirla. No lo hagas en una computadora compartida.").color(p.dim));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("guardar").strong().color(p.on_accent)).fill(p.accent)).clicked() {
                    key = Some('s');
                }
                if ui.button("ahora no").clicked() {
                    key = Some('n');
                }
            });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            key = Some('n');
        } else if ctx.input(|i| i.key_pressed(Key::Enter)) {
            key = Some('s');
        } else if !ctx.wants_keyboard_input() && !ctx.input(|i| i.modifiers.command || i.modifiers.mac_cmd || i.modifiers.ctrl || i.modifiers.alt) {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Text(value) = event {
                    if matches!(value.as_str(), "s" | "S" | "y" | "Y") {
                        key = Some('s');
                    } else if matches!(value.as_str(), "n" | "N") {
                        key = Some('n');
                    }
                }
            }
        }
        if let Some(c) = key {
            self.press(c);
        }
    }

    fn help(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.app.help {
            return;
        }
        let mut open = true;
        egui::Window::new("atajos").open(&mut open).collapsible(false).resizable(true).default_size([620.0, 480.0]).min_width(300.0).max_width((ctx.screen_rect().width() - 32.0).max(280.0)).max_height((ctx.screen_rect().height() - 64.0).max(100.0)).constrain_to(ctx.screen_rect()).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Atajos del sistema").strong());
            let cmd = command_label();
            for (key, action) in [
                (",", "Personalizar"), ("F", "Filtrar"), ("R", "Actualizar"),
                ("S", "Guardar borrador local"), ("Enter", "Pedir confirmación de entrega"),
                ("+ / − / 0", "Ampliar, reducir o restablecer zoom"),
                ("1..5", "Cambiar de vista"), ("W", "Cerrar el diálogo o la ventana"), ("Q", "Salir"),
            ] { ui.label(format!("{cmd}{key}   {action}")); }
            ui.label(format!("{cmd}A, C, X, V, Z y Shift+Z funcionan en los campos de texto."));
            ui.add_space(16.0);
            ui.label(RichText::new("Navegación sin foco de texto").strong());
            ui.vertical(|ui| {
                for (key, action) in [
                    ("1..5", "agenda, cursos, miyu, calificadas, contenido"),
                    ("j/k o flechas", "mover la selección"),
                    ("g/G o inicio/fin", "primera y última fila"),
                    ("h/l o izquierda/derecha", "cambiar de panel en cursos"),
                    ("J/K o avanzar/retroceder página", "desplazar el detalle"),
                    ("/", "filtrar, enter termina, esc borra"),
                    ("a", "ver entregadas o cursos anteriores"),
                    ("enter/o, O", "ver detalle o explorar el curso en la app"),
                    ("V", "actualizar entrega y retroalimentación"),
                    ("w, W", "abrir la web o el curso en la web"),
                    ("E, D, B", "entregar, descargar adjuntos o mi entrega"),
                    ("r, c", "actualizar o cambiar de tema"),
                    ("X, q", "cerrar sesión o salir de la app"),
                ] {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(key).color(p.accent));
                        ui.label(action);
                    });
                }
            });
            ui.separator();
            ui.label(RichText::new("Mayúsculas y minúsculas hacen acciones distintas.").color(p.dim));
            });
        });
        self.app.help = open && !ctx.input(|i| i.key_pressed(Key::Escape));
    }

    fn settings(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.settings_open { return; }
        let mut open = true;
        let mut appearance_changed = false;
        let mut background_changed = false;
        egui::Window::new("Personalizar").open(&mut open).collapsible(false).resizable(true)
            .default_size([620.0, 460.0]).min_width(240.0)
            .max_width((ctx.screen_rect().width() - 32.0).max(240.0))
            .max_height((ctx.screen_rect().height() - 64.0).max(100.0)).constrain_to(ctx.screen_rect()).show(ctx, |ui| {
                personalization::settings_style(ui);
                if ui.available_width() < 440.0 {
                    egui::ComboBox::from_id_salt("pagina-ajustes").selected_text(self.settings_page.label()).show_ui(ui, |ui| {
                        for page in personalization::Page::ALL { ui.selectable_value(&mut self.settings_page, page, page.label()); }
                    });
                } else { ui.horizontal_wrapped(|ui| {
                    for page in personalization::Page::ALL {
                        ui.selectable_value(&mut self.settings_page, page, page.label());
                    }
                }); }
                ui.separator();
                ui.add_space(12.0);
                egui::ScrollArea::vertical().id_salt("ajustes").auto_shrink([false, false]).max_height((ui.available_height() - 64.0).max(24.0)).show(ui, |ui| {
                    if self.settings_page == personalization::Page::Colors {
                        ui.label("Tema");
                        let before = self.app.theme;
                        egui::ComboBox::from_id_salt("elegir-tema").selected_text(THEMES[self.app.theme].name).show_ui(ui, |ui| {
                            for (i, theme) in THEMES.iter().enumerate() { ui.selectable_value(&mut self.app.theme, i, theme.name); }
                        });
                        if self.app.theme != before { store::save_theme(THEMES[self.app.theme].name); }
                        ui.add_space(16.0);
                    }
                    appearance_changed |= personalization::page_controls(ui, &mut self.appearance, self.settings_page);
                    match self.settings_page {
                        personalization::Page::Text => {
                            ui.add_space(24.0);
                            appearance_changed |= self.fonts.controls(ui, &mut self.appearance.font);
                            ui.add_space(24.0);
                            ui.label("Vista previa");
                            ui.scope(|ui| {
                                let mut font = egui::TextStyle::Body.resolve(ui.style());
                                font.size = self.appearance.font_size;
                                font.family = if self.appearance.font.is_some() { egui::FontFamily::Name(eminus::fonts::USER_FONT.into()) } else if self.appearance.monospace { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
                                ui.style_mut().override_font_id = Some(font);
                                read_text(ui, "Actividad, entrega y retroalimentación. El texto debe leerse sin esfuerzo.", self.appearance.line_spacing, 0.0);
                            });
                        }
                        personalization::Page::Reading => {
                            ui.add_space(24.0);
                            read_text(ui, "Las instrucciones y los comentarios del docente usan este espacio entre líneas. Puedes ampliar la lectura para ocultar las listas.", self.appearance.line_spacing, 0.0);
                        }
                        personalization::Page::Colors if self.appearance.custom_accent => { ui.weak("El acento se ajusta para mantener el texto legible."); }
                        personalization::Page::Background => { background_changed = self.background.show_controls(ui, &mut self.background_prefs); }
                        _ => {}
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Restablecer apariencia").clicked() {
                        self.appearance = Appearance::default();
                        appearance_changed = true;
                    }
                    if ui.available_width() >= 220.0 || !self.settings_note.is_empty() {
                        ui.label(RichText::new(if self.settings_note.is_empty() { "Los cambios se guardan solos" } else { &self.settings_note }).small().color(if self.settings_note.is_empty() { p.dim } else { p.yellow }));
                    }
                });
            });
        if appearance_changed { self.save_appearance(); }
        if background_changed {
            self.settings_note = store::save_background(&self.background_prefs).err().map(|e| format!("No se pudo guardar el fondo: {e}")).unwrap_or_default();
        }
        if appearance_changed || background_changed { ctx.request_repaint(); }
        self.settings_open = open && !ctx.input(|i| i.key_pressed(Key::Escape));
    }

}

fn view_title(view: View) -> &'static str {
    match view {
        View::Agenda => "Actividades",
        View::Courses => "Mis cursos",
        View::Miyu => "Resumen",
        View::Grades => "Calificaciones",
        View::Content => "Contenido",
    }
}

fn navigation_width(ctx: &egui::Context) -> f32 {
    if ctx.screen_rect().width() >= 1000.0 { 200.0 } else { 0.0 }
}

fn workspace_width(ctx: &egui::Context) -> f32 {
    ctx.screen_rect().width() - navigation_width(ctx)
}

fn pane(p: &Palette, focused: bool) -> egui::Frame {
    egui::Frame::none().fill(p.panel).inner_margin(20.0).stroke(egui::Stroke::new(1.0_f32, if focused { p.accent } else { p.border }))
}

// Sólo hacen falta las cinco próximas, sin copiar ni ordenar todo el curso por cuadro.
fn upcoming_activities(acts: &[std::sync::Arc<model::Activity>], now: i64) -> (usize, [Option<&model::Activity>; 5]) {
    let mut next: [Option<&model::Activity>; 5] = [None; 5];
    let mut count = 0;
    for act in acts.iter().filter(|a| !a.delivered && a.deadline > now) {
        count += 1;
        if let Some(index) = next.iter().position(|a| a.is_none_or(|a| act.deadline < a.deadline)) {
            next.copy_within(index..4, index + 1);
            next[index] = Some(act);
        }
    }
    (count, next)
}

fn contrast(a: Color32, b: Color32) -> f64 {
    let lum = |c: Color32| {
        let linear = |n: u8| { let n = f64::from(n) / 255.0; if n <= 0.04045 { n / 12.92 } else { ((n + 0.055) / 1.055).powf(2.4) } };
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    };
    let (a, b) = (lum(a), lum(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn file_icon(ui: &mut egui::Ui, p: &Palette, name: &str) {
    let kind = files::kind(name);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 26.0), egui::Sense::hover());
    response.on_hover_text(kind.label());
    let painter = ui.painter_at(rect);
    let at = |x, y| rect.min + egui::vec2(x, y);
    let stroke = egui::Stroke::new(1.25_f32, p.accent);
    let line = |a: (f32, f32), b: (f32, f32)| painter.line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
    painter.add(egui::Shape::closed_line(vec![at(3.0, 1.0), at(14.0, 1.0), at(20.0, 7.0), at(20.0, 25.0), at(3.0, 25.0)], stroke));
    line((14.0, 1.0), (14.0, 7.0));
    line((14.0, 7.0), (20.0, 7.0));
    match kind {
        FileKind::Pdf | FileKind::Document | FileKind::Code => {
            let text = match kind { FileKind::Pdf => "PDF", FileKind::Document => "W", _ => "</>" };
            painter.text(at(11.5, 17.0), egui::Align2::CENTER_CENTER, text, egui::FontId::monospace(8.0), p.accent);
        }
        FileKind::Spreadsheet => {
            for y in [11.0, 15.0, 19.0, 23.0] { line((6.0, y), (17.0, y)); }
            for x in [6.0, 11.5, 17.0] { line((x, 11.0), (x, 23.0)); }
        }
        FileKind::Presentation => {
            painter.rect_stroke(egui::Rect::from_min_max(at(6.0, 11.0), at(17.0, 19.0)), 0.0, stroke);
            line((11.5, 19.0), (11.5, 23.0));
            line((8.0, 23.0), (15.0, 23.0));
        }
        FileKind::Image => {
            painter.circle_stroke(at(8.0, 12.0), 1.5, stroke);
            painter.add(egui::Shape::line(vec![at(6.0, 22.0), at(10.0, 16.0), at(13.0, 19.0), at(16.0, 14.0), at(18.0, 22.0)], stroke));
        }
        FileKind::Archive => {
            for y in [10.0, 13.0, 16.0, 19.0, 22.0] { line((10.0, y), (13.0, y)); }
            line((11.5, 8.0), (11.5, 23.0));
        }
        FileKind::Audio => {
            line((11.0, 11.0), (11.0, 20.0));
            line((11.0, 11.0), (16.0, 13.0));
            painter.circle_filled(at(8.5, 20.0), 2.5, p.accent);
        }
        FileKind::Video => {
            painter.add(egui::Shape::convex_polygon(vec![at(8.0, 11.0), at(17.0, 17.0), at(8.0, 23.0)], p.accent, egui::Stroke::NONE));
        }
        FileKind::Text => {
            for y in [12.0, 16.0, 20.0] { line((6.0, y), (17.0, y)); }
        }
        FileKind::Other => {
            painter.text(at(11.5, 17.0), egui::Align2::CENTER_CENTER, "?", egui::FontId::monospace(12.0), p.accent);
        }
    }
}

// El estado de la entrega con las mismas palabras en el detalle y en la
// ventana de entrega.
fn delivery_state_text(info: &DeliveryInfo) -> &'static str {
    if info.sent {
        "entrega enviada"
    } else if info.id > 0 {
        "borrador en eminus, pendiente de enviar"
    } else {
        "sin entrega"
    }
}

// En un detalle estrecho (ventana mínima de 720 px con los dos paneles
// abiertos) la cuadrícula de dos columnas aprieta el texto: se listan los
// pares uno debajo de otro.
fn kv_wide(ui: &egui::Ui) -> bool {
    ui.available_width() >= 280.0
}

fn kv_list(ui: &mut egui::Ui, p: &Palette, rows: Vec<(&str, RichText)>) {
    for (key, value) in rows {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{key} ")).color(p.dim));
            ui.label(value);
        });
    }
}

fn pane_title(ui: &mut egui::Ui, p: &Palette, title: &str, focused: bool) {
    ui.label(RichText::new(title).color(if focused { p.accent } else { p.dim }));
    ui.separator();
}

fn shortcut(event: &egui::Event) -> Option<KeyCode> {
    match event {
        egui::Event::Text(text) if text.chars().count() == 1 => {
            let c = text.chars().next()?;
            "12345jkhloOgGJKacprDEBVwWXq?/".contains(c).then_some(KeyCode::Char(c))
        }
        egui::Event::Key { key, pressed: true, modifiers, .. } if !modifiers.command && !modifiers.ctrl && !modifiers.alt => match key {
            Key::ArrowDown => Some(KeyCode::Down),
            Key::ArrowUp => Some(KeyCode::Up),
            Key::ArrowLeft => Some(KeyCode::Left),
            Key::ArrowRight => Some(KeyCode::Right),
            Key::Home => Some(KeyCode::Home),
            Key::End => Some(KeyCode::End),
            Key::PageDown => Some(KeyCode::PageDown),
            Key::PageUp => Some(KeyCode::PageUp),
            Key::Enter => Some(KeyCode::Enter),
            Key::Escape => Some(KeyCode::Esc),
            Key::F1 => Some(KeyCode::Char('?')),
            _ => None,
        },
        _ => None,
    }
}

fn command_label() -> &'static str {
    if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" }
}

fn read_text(ui: &mut egui::Ui, text: &str, line_spacing: f32, max_width: f32) {
    let font = ui.style().override_font_id.clone().unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat {
        line_height: Some(font.size * line_spacing),
        font_id: font,
        color: ui.visuals().text_color(),
        ..Default::default()
    });
    job.wrap.max_width = ui.available_width().min(if max_width > 0.0 { max_width } else { ui.available_width() }).max(1.0);
    let galley = ui.fonts(|f| f.layout_job(job));
    ui.add(egui::Label::new(galley).selectable(true));
}

fn row_line(ui: &egui::Ui, at: egui::Pos2, width: f32, text: &str, font: egui::FontId, color: Color32) {
    let mut job = egui::text::LayoutJob::simple(text.into(), font, color, width.max(1.0));
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(at, galley, color);
}

// Sólo dibuja las filas visibles, también en listas largas.
fn rows(ui: &mut egui::Ui, p: &Palette, rows: &[Row], selected: usize, focused: bool, scroll: bool, empty: &str) -> Option<usize> {
    if rows.is_empty() {
        ui.label(RichText::new(empty).color(p.dim));
        return None;
    }
    let font = egui::TextStyle::Body.resolve(ui.style());
    let small = egui::TextStyle::Small.resolve(ui.style());
    let row_height = (font.size + small.size + 14.0).max(ui.spacing().interact_size.y);
    let mut area = egui::ScrollArea::vertical().id_salt("filas").auto_shrink([false, false]);
    if scroll {
        if let Some(line) = line_of(rows, selected) {
            let offset = line as f32 * (row_height + ui.spacing().item_spacing.y) - ui.available_height() * 0.5;
            area = area.vertical_scroll_offset(offset.max(0.0));
        }
    }
    let mut clicked = None;
    area.show_rows(ui, row_height, rows.len(), |ui, range| {
        let mut index = rows[..range.start].iter().filter(|r| matches!(r, Row::Item { .. })).count();
        for row in &rows[range] {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_height), egui::Sense::click());
            let at = rect.min + egui::vec2(10.0, 5.0);
            let width = rect.width() - 20.0;
            match row {
                Row::Group { label, count, tone } => {
                    row_line(ui, at + egui::vec2(0.0, 10.0), width, &format!("{label} · {count}"), small.clone(), if *tone == Tone::Plain { p.accent } else { p.tone(*tone) });
                }
                Row::Item { title, course, due, meta, tone, .. } => {
                    if index == selected || response.hovered() { ui.painter().rect_filled(rect, 0.0, p.sel); }
                    if index == selected && focused { ui.painter().vline(rect.left(), rect.y_range(), egui::Stroke::new(2.0_f32, p.accent)); }
                    row_line(ui, at, width, title, font.clone(), p.fg);
                    let meta = meta.trim_start_matches(['█', '░']).trim();
                    let mut secondary = String::with_capacity(due.len() + course.len() + meta.len() + 8);
                    for part in [due.as_str(), course.as_str(), meta].into_iter().filter(|part| !part.is_empty()) {
                        if !secondary.is_empty() { secondary.push_str(" · "); }
                        secondary.push_str(part);
                    }
                    row_line(ui, at + egui::vec2(0.0, font.size + 6.0), width, &secondary, small.clone(), if *tone == Tone::Normal || *tone == Tone::Plain { p.dim } else { p.tone(*tone) });
                    if response.clicked() { clicked = Some(index); }
                    response.on_hover_ui(|ui| { ui.label(title.as_ref()); ui.label(&secondary); });
                    index += 1;
                }
            }
        }
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cat_logo_preserves_gif_timing_and_pauses_without_a_timer() {
        let ctx = egui::Context::default();
        let mut logo = CatLogo::new(&ctx);
        assert_eq!(logo.size, egui::vec2(164.0, 128.0));
        assert_eq!(logo.texture.size(), [984, 128]);
        assert_eq!(logo.frame_ends.len(), 6);
        for (i, end) in logo.frame_ends.iter().enumerate() {
            assert!((*end - (i + 1) as f64 * 0.04).abs() < 0.000001);
        }
        for (time, frame) in [(0.0, 0), (0.041, 1), (0.081, 2), (0.201, 5), (0.241, 0)] {
            let (actual, wait) = logo.frame_at(time, true);
            assert_eq!(actual, frame);
            assert!(wait.unwrap() <= std::time::Duration::from_millis(41));
        }
        assert_eq!(logo.frame_at(5.0, false), (0, None));
        assert_eq!(logo.frame_at(50.0, false), (0, None));
        assert!(!logo.started.is_some());
        assert_eq!(logo.frame_at(100.0, true).0, 0);
        assert_eq!(logo.frame_at(100.05, true).0, 1);
        assert_eq!(logo.frame_at(100.1, false), (0, None), "al soltarlo vuelve al primer cuadro");
    }

    #[test]
    fn cat_logo_only_animates_under_the_pointer() {
        let ctx = egui::Context::default();
        let mut logo = CatLogo::new(&ctx);
        let mut frame = |time: f64, pointer: Option<egui::Pos2>| {
            let output = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
                time: Some(time),
                events: pointer.map(egui::Event::PointerMoved).into_iter().collect(),
                ..Default::default()
            }, |ctx| { egui::CentralPanel::default().show(ctx, |ui| logo.show(ui, 64.0)); });
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
        };
        frame(0.0, None); // el primer cuadro siempre pide otro para acomodar
        frame(0.05, None);
        for time in [0.1, 0.2] {
            assert_eq!(frame(time, None), std::time::Duration::MAX, "el logo repinta sin el ratón encima");
        }
        frame(0.3, Some(egui::pos2(30.0, 30.0)));
        assert!(frame(0.31, None) <= std::time::Duration::from_millis(41), "el logo no se anima con el ratón encima");
        frame(0.4, Some(egui::pos2(390.0, 290.0)));
        frame(0.41, None); // el cambio de hover pide un cuadro más
        assert_eq!(frame(0.42, None), std::time::Duration::MAX, "el logo sigue animado al quitar el ratón");
    }

    #[test]
    fn header_uses_the_cat_without_taglines_and_reuses_its_texture() {
        for width in [600.0, 1180.0] {
            let mut gui = demo_gui();
            gui.focus_filter = true;
            let ctx = ctx_for(&gui);
            let mut frame = |time| ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 720.0))),
                time: Some(time), ..Default::default()
            }, |ctx| gui.top(ctx, &gui.palette()));
            frame(0.0);
            let output = frame(0.081);
            let filter_id = ctx.memory(|m| m.focused()).unwrap();
            let filter = ctx.read_response(filter_id).unwrap().rect;
            let center = navigation_width(&ctx) + workspace_width(&ctx) * 0.5;
            assert!((filter.center().x - center).abs() <= 1.0, "buscador descentrado con {width} px: {filter:?}");
            if width >= 1000.0 {
                let label_rect = |label| output.shapes.iter().find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => Some(t.galley.rect.translate(t.pos.to_vec2())),
                    _ => None,
                }).unwrap();
                assert!(label_rect("Ayuda y atajos").top() - label_rect("Personalizar").bottom() >= 12.0, "los botones del menú siguen pegados");
            }
            let text = texts(&output);
            for old in ["Mi espacio", "Mi aprendizaje", "Eminus", "Tus pendientes", "Tu próxima entrega"] {
                assert!(!text.contains(old), "sigue apareciendo {old} en el encabezado");
            }
            let texture = gui.logo.as_ref().unwrap().texture.id();
            assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Mesh(m) if m.texture_id == texture)));
            assert!(!output.textures_delta.set.iter().any(|(id, _)| *id == texture), "el logo se volvió a subir a la GPU");
        }
    }

    fn course_row(title: &str) -> Row {
        Row::Item {
            glyph: "★", tone: Tone::Plain, due: String::new(), course: String::new(),
            title: title.into(), meta: "███░░░░░ 3/8".into(),
            entry: Entry::Course(model::Course { id: "1".into(), name: title.into(), end: 0, favorite: true, current: true }),
        }
    }

    #[test]
    fn rows_fit_narrow_panels_and_keep_progress() {
        let ctx = egui::Context::default();
        let p = Palette::of(&THEMES[0]);
        p.apply(&ctx);
        personalization::apply(&ctx, &Appearance::default());
        for width in [180.0, 320.0, 700.0] {
            let output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 200.0))), ..Default::default() }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rows(ui, &p, &[course_row("programación de sistemas 84513")], 0, true, false, "");
                });
            });
            assert!(texts(&output).contains("3/8"));
            assert!(!texts(&output).contains(['█', '░']));
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    assert!(text.pos.x + text.galley.rect.right() <= width + 1.0, "Texto fuera del panel de {width} px");
                }
            }
        }
    }

    #[test]
    fn shortcuts_distinguish_case_and_ignore_released_keys() {
        assert_eq!(shortcut(&egui::Event::Text("j".into())), Some(KeyCode::Char('j')));
        assert_eq!(shortcut(&egui::Event::Text("J".into())), Some(KeyCode::Char('J')));
        assert_eq!(shortcut(&egui::Event::Text("/".into())), Some(KeyCode::Char('/')));
        assert_eq!(shortcut(&egui::Event::Text("texto pegado".into())), None);
        // La GUI expone el detalle nativo con enter/o, la web con w y la
        // entrega con V sin abrir el editor; cada botón tiene su atajo.
        for c in ['o', 'O', 'w', 'W', 'V', 'B', 'E', 'D'] {
            assert_eq!(shortcut(&egui::Event::Text(c.into())), Some(KeyCode::Char(c)), "{c} sin atajo");
        }
        // Minúsculas y mayúsculas hacen acciones distintas: v sola no consulta.
        assert_eq!(shortcut(&egui::Event::Text("v".into())), None);
        let key = |pressed, modifiers| egui::Event::Key { key: Key::ArrowDown, physical_key: None, pressed, repeat: false, modifiers };
        assert_eq!(shortcut(&key(true, egui::Modifiers::NONE)), Some(KeyCode::Down));
        assert_eq!(shortcut(&key(false, egui::Modifiers::NONE)), None);
        assert_eq!(shortcut(&key(true, egui::Modifiers::CTRL)), None);
    }

    fn command_key(key: Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers { command: true, mac_cmd: true, ..Default::default() } }
    }

    #[test]
    fn command_save_never_submits_and_enter_only_confirms() {
        let mut gui = demo_gui();
        let act = demo_act();
        gui.app.rows = act_rows(&act);
        gui.app.on_msg(0, Msg::Detail("7:3".into(), Ok(model::Detail { text: String::new(), files: vec![], blocked: None })));
        gui.app.on_msg(0, Msg::Delivery("7:3".into(), Ok(DeliveryInfo::default())));
        gui.app.press(KeyCode::Char('E'));
        gui.app.delivery.as_mut().unwrap().text = "Comentario de prueba".into();
        gui.app.delivery.as_mut().unwrap().confirm = true;
        run(&mut gui, 720.0, vec![command_key(Key::S)], |gui, ctx| gui.native_keyboard(ctx));
        assert!(gui.app.delivery.as_ref().unwrap().confirm);
        assert!(!gui.app.delivery.as_ref().unwrap().busy);
        assert_eq!(gui.app.pool.pending, 0);
        gui.app.delivery.as_mut().unwrap().confirm = false;
        run(&mut gui, 720.0, vec![command_key(Key::Enter)], |gui, ctx| gui.native_keyboard(ctx));
        assert!(gui.app.delivery.as_ref().unwrap().confirm);
        run(&mut gui, 720.0, vec![command_key(Key::Enter)], |gui, ctx| gui.native_keyboard(ctx));
        assert!(!gui.app.delivery.as_ref().unwrap().busy);
        assert_eq!(gui.app.pool.pending, 0);
        run(&mut gui, 720.0, vec![command_key(Key::W)], |gui, ctx| gui.native_keyboard(ctx));
        assert!(!gui.app.delivery.as_ref().unwrap().confirm);
        run(&mut gui, 720.0, vec![command_key(Key::W)], |gui, ctx| gui.native_keyboard(ctx));
        assert!(gui.app.delivery.is_none());
        assert!(!gui.app.quit);
    }

    #[test]
    fn command_editing_and_preferences_work_with_text_focus() {
        let mut gui = demo_gui();
        let ctx = ctx_for(&gui);
        let id = egui::Id::new("texto-prueba");
        let mut value = "Retroalimentación".to_string();
        let mut time = 0.0;
        let mut frame = |gui: &mut Gui, value: &mut String, events| {
            time += 1.0;
            ctx.run(egui::RawInput { time: Some(time), screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0))), events, modifiers: egui::Modifiers { command: true, mac_cmd: true, ..Default::default() }, ..Default::default() }, |ctx| {
                gui.native_keyboard(ctx);
                gui.keyboard(ctx);
                egui::CentralPanel::default().show(ctx, |ui| { ui.add(egui::TextEdit::multiline(value).id(id)); });
            })
        };
        frame(&mut gui, &mut value, vec![]);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut gui, &mut value, vec![]);
        frame(&mut gui, &mut value, vec![command_key(Key::A)]);
        let copied = frame(&mut gui, &mut value, vec![egui::Event::Copy]);
        assert_eq!(copied.platform_output.copied_text, "Retroalimentación");
        let cut = frame(&mut gui, &mut value, vec![egui::Event::Cut]);
        assert_eq!(cut.platform_output.copied_text, "Retroalimentación");
        assert!(value.is_empty());
        frame(&mut gui, &mut value, vec![egui::Event::Paste("Primera línea\nSegunda línea".into())]);
        assert_eq!(value, "Primera línea\nSegunda línea");
        frame(&mut gui, &mut value, vec![]);
        frame(&mut gui, &mut value, vec![command_key(Key::Z)]);
        assert!(value.is_empty());
        let mut redo = command_key(Key::Z);
        if let egui::Event::Key { modifiers, .. } = &mut redo { modifiers.shift = true; }
        frame(&mut gui, &mut value, vec![redo]);
        assert_eq!(value, "Primera línea\nSegunda línea");
        frame(&mut gui, &mut value, vec![command_key(Key::Comma)]);
        assert!(gui.settings_open);
        frame(&mut gui, &mut value, vec![command_key(Key::W)]);
        assert!(!gui.settings_open && !gui.app.quit);
        assert_eq!(gui.app.theme, 0);
    }

    #[test]
    fn selection_focus_and_slider_rails_are_visible_in_every_theme() {
        let ctx = egui::Context::default();
        for theme in THEMES {
            Palette::of(&theme).apply(&ctx);
            let style = ctx.style();
            assert_ne!(style.visuals.selection.bg_fill, style.visuals.extreme_bg_color);
            assert!(style.visuals.selection.stroke.width > 0.0);
            assert_ne!(style.visuals.widgets.inactive.bg_fill, style.visuals.window_fill);
        }
    }

    #[test]
    fn reading_wraps_at_the_available_width_and_uses_line_spacing() {
        let ctx = egui::Context::default();
        personalization::apply(&ctx, &Appearance::default());
        let content = "Una descripción larga con instrucciones y retroalimentación. ".repeat(12);
        for (width, limit) in [(300.0, 700.0), (1180.0, 700.0)] {
            let output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 1200.0))), ..Default::default() }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| read_text(ui, &content, 1.6, limit));
            });
            let galley = output.shapes.iter().find_map(|s| if let egui::Shape::Text(t) = &s.shape { Some(&t.galley) } else { None }).unwrap();
            assert!(galley.rows.len() > 1);
            assert!(galley.rect.width() <= width.min(limit));
            assert_eq!(galley.job.text, content);
            assert_eq!(galley.job.sections[0].format.line_height, Some(16.0 * 1.6));
        }
    }

    #[test]
    fn zoom_at_two_hundred_percent_keeps_settings_within_the_window() {
        let mut gui = demo_gui();
        gui.settings_open = true;
        gui.appearance.zoom = 2.0;
        let ctx = ctx_for(&gui);
        gui.fonts.update(&ctx, &gui.appearance.font);
        gui.apply_style(&ctx);
        for page in personalization::Page::ALL {
            gui.settings_page = page;
            for _ in 0..3 {
                let _ = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0))), ..Default::default() }, |ctx| gui.show(ctx));
            }
            let rect = ctx.used_rect();
            let screen = ctx.screen_rect();
            assert!(rect.right() <= screen.right() + 1.0 && rect.bottom() <= screen.bottom() + 1.0, "{} desborda: {rect:?}, pantalla {screen:?}", page.label());
        }
    }

    #[test]
    fn dragging_the_panel_divider_changes_its_width() {
        let mut gui = demo_gui();
        let ctx = ctx_for(&gui);
        let mut frame = |events| ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1180.0, 720.0))), events, ..Default::default() }, |ctx| {
            gui.lists(ctx, &gui.palette());
            egui::CentralPanel::default().show(ctx, |_| {});
        });
        for _ in 0..3 { frame(vec![]); }
        let panel = || egui::containers::panel::PanelState::load(&ctx, egui::Id::new("lista")).unwrap().rect;
        let before = panel();
        let start = egui::pos2(before.right(), before.center().y);
        let end = start - egui::vec2(80.0, 0.0);
        frame(vec![egui::Event::PointerMoved(start)]);
        frame(vec![egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
        frame(vec![egui::Event::PointerMoved(end)]);
        frame(vec![]);
        frame(vec![egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        assert!(panel().width() < before.width() - 60.0, "La división no cambió el ancho: {before:?} -> {:?}", panel());
    }

    #[test]
    fn settings_pages_fit_small_windows_with_old_compact_preferences() {
        let mut gui = demo_gui();
        gui.settings_open = true;
        gui.appearance = Appearance { font_size: 14.0, monospace: true, density: personalization::Density::Compact, reader_width: 0.0, ..Default::default() };
        for width in [360.0, 600.0, 1180.0] {
            let ctx = ctx_for(&gui);
            gui.fonts.update(&ctx, &gui.appearance.font);
            for page in personalization::Page::ALL {
                gui.settings_page = page;
                let mut output = egui::FullOutput::default();
                for _ in 0..3 {
                    output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 420.0))), ..Default::default() }, |ctx| {
                        gui.settings(ctx, &gui.palette());
                    });
                }
                assert!(texts(&output).contains("Restablecer apariencia"), "Pie inaccesible en {} con {width} px", page.label());
                assert!(ctx.used_rect().right() <= width + 1.0 && ctx.used_rect().bottom() <= 421.0, "{} desborda: {:?}", page.label(), ctx.used_rect());
            }
        }
        assert_eq!(gui.appearance.font_size, 14.0);
        assert!(gui.appearance.monospace);
    }

    #[test]
    fn themes_use_simple_controls_without_shadows() {
        let ctx = egui::Context::default();
        for theme in &THEMES {
            Palette::of(theme).apply(&ctx);
            let style = ctx.style();
            assert!(style.text_styles.values().all(|f| f.family == egui::FontFamily::Monospace));
            assert_eq!(style.visuals.widgets.inactive.rounding, egui::Rounding::same(4.0));
            assert_eq!(style.visuals.window_shadow, egui::epaint::Shadow::NONE);
        }
    }

    #[test]
    fn upcoming_keeps_the_first_five_and_counts_every_pending_activity() {
        let mut acts: Vec<_> = [109, 102, 0, 101, 110, 105, 103, 104, 99, 100].into_iter()
            .map(|deadline| { let mut act = demo_act(); Arc::get_mut(&mut act).unwrap().deadline = deadline; act }).collect();
        let mut delivered = demo_act();
        Arc::get_mut(&mut delivered).unwrap().delivered = true;
        acts.push(delivered);
        let (count, next) = upcoming_activities(&acts, 100);
        assert_eq!(count, 7);
        assert_eq!(next.into_iter().flatten().map(|a| a.deadline).collect::<Vec<_>>(), [101, 102, 103, 104, 105]);
        let (count, next) = upcoming_activities(&acts, 110);
        assert_eq!(count, 0);
        assert!(next.iter().all(Option::is_none));
        let (count, next) = upcoming_activities(&acts[..1], 100);
        assert_eq!(count, 1);
        assert_eq!(next[0].unwrap().deadline, 109);
        assert!(next[1..].iter().all(Option::is_none));
    }

    #[test]
    fn sidebar_changes_views_by_mouse_and_collapses_before_crowding_the_detail() {
        let mut gui = demo_gui();
        let ctx = ctx_for(&gui);
        personalization::apply(&ctx, &gui.appearance);
        let mut frame = |events| ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1180.0, 720.0))), events, ..Default::default()
        }, |ctx| { gui.top(ctx, &gui.palette()); });
        frame(vec![]);
        let output = frame(vec![]);
        let pos = output.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == "Mis cursos" => Some(t.pos + t.galley.rect.center().to_vec2()),
            _ => None,
        }).unwrap();
        frame(vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        assert!(gui.app.view == View::Courses);
        assert!(!gui.reading_full);
        assert!(gui.scroll_to_selection);

        for width in [600.0, 999.0, 1000.0, 1180.0, 1440.0] {
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 720.0))), ..Default::default()
            }, |ctx| {
                assert_eq!(navigation_width(ctx) > 0.0, width >= 1000.0);
                assert_eq!(workspace_width(ctx), width - navigation_width(ctx));
                gui.top(ctx, &gui.palette());
                gui.lists(ctx, &gui.palette());
                egui::CentralPanel::default().show(ctx, |ui| {
                    if width >= 700.0 { assert!(ui.available_width() >= 280.0, "detalle demasiado estrecho con {width}"); }
                });
            });
        }
    }

    #[test]
    fn custom_accent_keeps_labels_and_buttons_readable() {
        for theme in &THEMES {
            for accent in [[0, 0, 0], [255, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255]] {
                let mut p = Palette::of(theme);
                p.customize(&Appearance { custom_accent: true, accent, ..Default::default() });
                assert!(contrast(p.accent, p.bg) >= 4.5, "{} fondo", theme.name);
                assert!(contrast(p.accent, p.sel) >= 4.5, "{} selección", theme.name);
                assert!(contrast(p.accent, p.on_accent) >= 4.5, "{} botón", theme.name);
            }
        }
    }

    #[test]
    fn appearance_reaches_the_window_and_hidden_status_controls() {
        let mut gui = demo_gui();
        gui.appearance = Appearance { font_size: 18.0, monospace: false, zoom: 1.25, density: personalization::Density::Comfortable, show_clock: false, show_counters: false, show_shortcuts: false, ..Default::default() };
        let ctx = egui::Context::default();
        gui.apply_style(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        assert_eq!(egui::TextStyle::Body.resolve(&ctx.style()).size, 18.0);
        assert_eq!(egui::TextStyle::Body.resolve(&ctx.style()).family, egui::FontFamily::Proportional);
        assert!((ctx.zoom_factor() - 1.25).abs() < 0.01);
        let text = texts(&run(&mut gui, 720.0, vec![], |gui, ctx| {
            let p = gui.palette();
            gui.status(ctx, &p);
        }));
        assert!(!text.contains("vencidas") && !text.contains("j/k mover") && !text.contains(&text::clock()));
    }

    #[test]
    fn settings_open_by_keyboard_and_stop_list_shortcuts() {
        let mut gui = demo_gui();
        gui.app.rows = act_rows(&demo_act());
        let _ = run(&mut gui, 720.0, vec![egui::Event::Text("p".into()), egui::Event::Text("j".into())], |gui, ctx| gui.keyboard(ctx));
        assert!(gui.settings_open);
        assert_eq!(gui.app.list().sel, 0);
        let shown = texts(&run(&mut gui, 720.0, vec![], |gui, ctx| {
            let p = gui.palette();
            gui.settings(ctx, &p);
        }));
        for expected in ["Personalizar", "Texto", "Lectura", "Colores", "Fondo", "Interfaz", "Tamaño del texto", "Tipo de letra"] {
            assert!(shown.contains(expected), "falta {expected}");
        }
    }

    #[test]
    fn photo_is_drawn_before_translucent_panels() {
        let dir = std::env::temp_dir().join(format!("eminus-gui-fondo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("imagen.png");
        image::RgbImage::from_pixel(40, 30, image::Rgb([160, 90, 210])).save(&path).unwrap();
        let mut gui = demo_gui();
        gui.background_prefs.image = Some(path);
        let ctx = egui::Context::default();
        let mut rendered = false;
        for _ in 0..400 {
            let output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0))), ..Default::default() }, |ctx| gui.show(ctx));
            if gui.background.has_image() {
                assert!(gui.palette().panel.a() < 255);
                assert_eq!(ctx.style().visuals.window_fill.a(), 255, "los diálogos deben seguir opacos sobre la foto");
                rendered = output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default()));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(rendered, "la foto no se dibujó detrás de los paneles");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn long_lists_render_only_visible_rows_and_can_jump_to_last() {
        let ctx = egui::Context::default();
        let p = Palette::of(&THEMES[0]);
        p.apply(&ctx);
        let list: Vec<Row> = (0..10_000).map(|i| course_row(&format!("curso {i:05}"))).collect();
        for selected in [0, 9_999] {
            let output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 240.0))), ..Default::default() }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { rows(ui, &p, &list, selected, true, true, ""); });
            });
            let labels: Vec<_> = output.shapes.iter().filter_map(|s| if let egui::Shape::Text(t) = &s.shape { Some(t.galley.job.text.as_str()) } else { None }).collect();
            assert!(labels.len() < 30, "dibujó {} filas", labels.len());
            assert!(labels.iter().any(|t| t.contains(&format!("curso {selected:05}"))), "no mostró la fila seleccionada");
        }
    }

    // ---- Flujo de GUI con datos falsos (sin red, sin cuentas, sin llavero) ----
    //
    // App::with_client evita leer cuentas del disco. Los cuadros usan
    // datos de prueba en caché y un token vacío, sin acceso a Eminus.

    use eminus::api::{Client, Msg};
    use std::sync::Arc;

    fn demo_gui() -> Gui {
        let mut app = App::with_client(Client::new(""));
        app.login = None;
        Gui::new(app, Appearance::default())
    }

    #[test]
    fn account_menu_shows_the_name_and_photo_and_reuses_the_texture() {
        let mut gui = demo_gui();
        gui.app.account = "zs123".into();
        gui.app.user_info = model::UserInfo {
            name: "PÉREZ LÓPEZ ANA".into(), username: "zs123".into(),
            photo: Some(Arc::new(image::RgbaImage::from_pixel(128, 128, image::Rgba([30, 60, 90, 255])))),
        };
        let ctx = ctx_for(&gui);
        gui.update_avatar(&ctx);
        let texture = gui.avatar.as_ref().unwrap().1.id();
        let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 720.0))), ..Default::default() };
        let _ = ctx.run(input(), |ctx| gui.top(ctx, &gui.palette()));
        let header = ctx.run(input(), |ctx| gui.top(ctx, &gui.palette()));
        let button = header.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(text) if text.galley.job.text == "Mi cuenta" => Some(text.galley.rect.translate(text.pos.to_vec2()).center()),
            _ => None,
        }).unwrap();
        let mut click = input();
        click.events = vec![egui::Event::PointerMoved(button),
            egui::Event::PointerButton { pos: button, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
            egui::Event::PointerButton { pos: button, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }];
        let _ = ctx.run(click, |ctx| gui.top(ctx, &gui.palette()));
        let output = ctx.run(input(), |ctx| gui.top(ctx, &gui.palette()));
        assert!(texts(&output).contains("PÉREZ LÓPEZ ANA"));
        assert!(texts(&output).contains("zs123"));
        assert!(ctx.tessellate(output.shapes.clone(), output.pixels_per_point).iter().any(|s| matches!(&s.primitive, egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture)));
        assert!(!output.textures_delta.set.iter().any(|(id, _)| *id == texture));
        gui.app.user_info = model::UserInfo::default();
        gui.update_avatar(&ctx);
        assert!(gui.avatar.is_none());
    }

    fn demo_act() -> Arc<model::Activity> {
        Arc::new(model::Activity {
            id: "3".into(), course_id: "7".into(), course_name: Arc::from("curso demo"), title: "tarea 1".into(),
            start: 0, extension: 0, deadline: 2_000_000_000, delivered: false, delivered_at: 0, grade: None, value: Some(10.0),
        })
    }

    #[test]
    #[ignore = "medición de dibujo sin red; ejecutar en release con --nocapture"]
    fn bench_gui_frames() {
        let mut gui = demo_gui();
        let act = demo_act();
        gui.app.rows = (0..10_000).map(|_| act_rows(&act).remove(0)).collect();
        gui.app.on_msg(0, Msg::Detail("7:3".into(), Ok(model::Detail {
            text: "Describe el protocolo y adjunta tu informe. ".repeat(400), files: vec![], blocked: None,
        })));
        gui.app.on_msg(0, Msg::Delivery("7:3".into(), Ok(DeliveryInfo::default())));
        let ctx = ctx_for(&gui);
        personalization::apply(&ctx, &gui.appearance);
        let mut samples = Vec::with_capacity(300);
        let mut shapes = 0;
        for frame in 0..320 {
            let start = std::time::Instant::now();
            let output = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1180.0, 720.0))),
                ..Default::default()
            }, |ctx| {
                let p = gui.palette();
                gui.top(ctx, &p);
                gui.lists(ctx, &p);
                egui::CentralPanel::default().show(ctx, |ui| { gui.detail(ui, &p); });
            });
            if frame >= 20 { samples.push(start.elapsed()); }
            shapes = output.shapes.len();
        }
        samples.sort_unstable();
        eprintln!("GUI 10.000 filas, 16.800 caracteres, 300 cuadros: mediana {:?}, p95 {:?}, {shapes} formas", samples[150], samples[285]);
        assert!(shapes < 500, "la lista dejó de dibujar sólo las filas visibles");
    }

    fn act_rows(act: &Arc<model::Activity>) -> Vec<Row> {
        vec![
            Row::Item { glyph: "○", tone: Tone::Normal, due: "manana".into(), course: "curso".into(), title: act.title.clone(), meta: String::new(), entry: Entry::Act(act.clone()) },
            Row::Item { glyph: "○", tone: Tone::Normal, due: "pasado".into(), course: "curso".into(), title: "tarea 2".into(), meta: String::new(), entry: Entry::Act(Arc::new(model::Activity { id: "4".into(), title: "tarea 2".into(), course_id: "7".into(), course_name: Arc::from("curso demo"), start: 0, extension: 0, deadline: 2_000_000_000, delivered: false, delivered_at: 0, grade: None, value: None })) },
        ]
    }

    fn texts(output: &egui::FullOutput) -> String {
        output.shapes.iter().filter_map(|s| if let egui::Shape::Text(t) = &s.shape { Some(t.galley.job.text.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
    }

    fn ctx_for(gui: &Gui) -> egui::Context {
        let ctx = egui::Context::default();
        Palette::of(&THEMES[gui.theme]).apply(&ctx);
        ctx
    }

    // Las ventanas (entrega, ayuda, perfiles) calculan su tamaño en la
    // primera vuelta: se pintan dos veces sobre el mismo contexto y se lee
    // la segunda.
    fn run(gui: &mut Gui, width: f32, events: Vec<egui::Event>, mut show: impl FnMut(&mut Gui, &egui::Context)) -> egui::FullOutput {
        let ctx = ctx_for(gui);
        let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 1200.0))), ..Default::default() };
        let _ = ctx.run(input(), |ctx| show(gui, ctx));
        ctx.run(egui::RawInput { events, ..input() }, |ctx| show(gui, ctx))
    }

    fn detail_text(gui: &mut Gui, width: f32) -> String {
        texts(&run(gui, width, vec![], |gui, ctx| {
            let p = Palette::of(&THEMES[gui.theme]);
            egui::CentralPanel::default().show(ctx, |ui| gui.detail(ui, &p));
        }))
    }

    #[test]
    fn delivery_state_text_covers_all_cases() {
        let sent = DeliveryInfo { sent: true, ..Default::default() };
        assert_eq!(delivery_state_text(&sent), "entrega enviada");
        let draft = DeliveryInfo { id: 4, ..Default::default() };
        assert_eq!(delivery_state_text(&draft), "borrador en eminus, pendiente de enviar");
        assert_eq!(delivery_state_text(&DeliveryInfo::default()), "sin entrega");
    }

    #[test]
    fn lists_show_empty_and_loading_states() {
        let mut gui = demo_gui();
        for (view, expected) in [
            (View::Agenda, "sin pendientes"),
            (View::Miyu, "todo entregado"),
            (View::Grades, "calificadas"),
            (View::Content, "contenido"),
            (View::Courses, "sin cursos"),
        ] {
            gui.app.set_view(view);
            let shown = texts(&run(&mut gui, 1180.0, vec![], |gui, ctx| {
                let p = Palette::of(&THEMES[gui.theme]);
                gui.lists(ctx, &p);
            }));
            assert!(shown.contains(expected), "{expected} no aparece en {}", view.label());
        }
        gui.app.filter = "zzz".into();
        let shown = texts(&run(&mut gui, 1180.0, vec![], |gui, ctx| {
            let p = Palette::of(&THEMES[gui.theme]);
            gui.lists(ctx, &p);
        }));
        assert!(shown.contains("nada coincide"), "el filtro sin resultados no avisa");
        gui.app.loading = true;
        let shown = texts(&run(&mut gui, 1180.0, vec![], |gui, ctx| {
            let p = Palette::of(&THEMES[gui.theme]);
            gui.status(ctx, &p);
        }));
        assert!(shown.contains("cargando"), "cargando no se anuncia en la barra de estado");
    }

    #[test]
    fn detail_guides_without_selection_and_while_loading() {
        let mut gui = demo_gui();
        assert!(detail_text(&mut gui, 1180.0).contains("Elige una actividad o un curso"));
        let act = demo_act();
        gui.app.rows = act_rows(&act);
        let shown = detail_text(&mut gui, 1180.0);
        for expected in ["tarea 1", "cargando...", "consultando entrega"] {
            assert!(shown.contains(expected), "{expected} no aparece en el detalle pendiente");
        }
    }

    #[test]
    fn detail_shows_files_delivery_and_feedback_natively() {
        let mut gui = demo_gui();
        let act = demo_act();
        gui.app.rows = act_rows(&act);
        gui.app.on_msg(0, Msg::Detail("7:3".into(), Ok(model::Detail {
            text: "Describe el protocolo.".into(),
            files: vec![model::AFile { id: "1".into(), name: "instrucciones.pdf".into(), size: "12 KB".into() }],
            blocked: None,
        })));
        gui.app.on_msg(0, Msg::Delivery("7:3".into(), Ok(DeliveryInfo {
            id: 41, text: "mi informe".into(), sent: true, feedback: "Revisa la explicación.".into(), grade: Some(8.0), reviewed_at: 1_700_000_000,
            files: vec![model::AFile { id: "2".into(), name: "informe.pdf".into(), size: "20 KB".into() }],
            review_files: vec![model::AFile { id: "4".into(), name: "revision.pdf".into(), size: "1 KB".into() }],
        })));
        for width in [360.0, 1180.0] {
            let shown = detail_text(&mut gui, width);
            for expected in ["Describe el protocolo", "instrucciones.pdf", "Mi entrega", "entrega enviada", "mi informe", "informe.pdf", "Retroalimentación", "Revisa la explicación", "revision.pdf", "Entregar", "Descargar"] {
                assert!(shown.contains(expected), "{expected} falta con {width} px");
            }
        }
    }

    #[test]
    fn detail_shows_blocked_reason() {
        let mut gui = demo_gui();
        let act = demo_act();
        gui.app.rows = act_rows(&act);
        gui.app.on_msg(0, Msg::Detail("7:3".into(), Ok(model::Detail {
            text: String::new(), files: vec![], blocked: Some("ya está calificada: eminus no acepta otra entrega".into()),
        })));
        let shown = detail_text(&mut gui, 1180.0);
        assert!(shown.contains("ya está calificada"), "el motivo de bloqueo no se explica");
    }

    #[test]
    fn delivery_window_opens_pending_and_keeps_feedback() {
        let mut gui = demo_gui();
        let act = demo_act();
        gui.app.rows = act_rows(&act);
        gui.app.press(KeyCode::Char('E'));
        assert!(gui.app.delivery.is_some(), "E no abre la entrega");
        let shown = texts(&run(&mut gui, 1180.0, vec![], |gui, ctx| {
            let p = Palette::of(&THEMES[gui.theme]);
            gui.delivery(ctx, &p);
        }));
        assert!(shown.contains("consultando tu entrega"), "la ventana no dice que está consultando: [{shown}]");
        gui.app.delivery.as_mut().unwrap().info = Slot::Ready(DeliveryInfo {
            id: 41, text: String::new(), sent: true, feedback: "bien hecho".into(), grade: Some(9.0), reviewed_at: 1_700_000_000,
            files: vec![model::AFile { id: "2".into(), name: "informe.pdf".into(), size: "20 KB".into() }],
            review_files: vec![model::AFile { id: "4".into(), name: "rubrica.pdf".into(), size: "3 KB".into() }],
        });
        let shown = texts(&run(&mut gui, 1180.0, vec![], |gui, ctx| {
            let p = Palette::of(&THEMES[gui.theme]);
            gui.delivery(ctx, &p);
        }));
        for expected in ["ya la entregaste", "informe.pdf", "comentario"] {
            assert!(shown.contains(expected), "{expected} falta en la ventana de entrega");
        }
    }

    #[test]
    fn shortcuts_do_not_fire_while_delivery_or_offer_open() {
        let mut gui = demo_gui();
        let act = demo_act();
        gui.app.rows = act_rows(&act);
        gui.app.press(KeyCode::Char('E'));
        assert!(gui.app.delivery.is_some());
        run(&mut gui, 1180.0, vec![egui::Event::Text("j".into())], |gui, ctx| gui.keyboard(ctx));
        assert_eq!(gui.app.list().sel, 0, "j movió la selección con la entrega abierta");
        assert!(!gui.focus_filter, "/ no debería enfocar con la entrega abierta");
        gui.app.delivery = None;
        gui.app.offer = Some(("zs00000000".into(), "secreto".into()));
        run(&mut gui, 1180.0, vec![egui::Event::Text("1".into())], |gui, ctx| gui.keyboard(ctx));
        assert!(gui.app.view == View::Agenda, "un atajo cambió de pestaña con el diálogo abierto");
        let shown = texts(&run(&mut gui, 1180.0, vec![], |gui, ctx| {
            let p = Palette::of(&THEMES[gui.theme]);
            gui.offer(ctx, &p);
        }));
        assert!(shown.contains("perfil"), "el diálogo de perfil no se entiende");
    }

    #[test]
    fn slash_focuses_filter_and_help_lists_shortcuts() {
        let mut gui = demo_gui();
        run(&mut gui, 1180.0, vec![egui::Event::Text("/".into())], |gui, ctx| gui.keyboard(ctx));
        assert!(gui.focus_filter, "/ no pide el foco del filtro");
        gui.app.help = true;
        let shown = texts(&run(&mut gui, 1180.0, vec![], |gui, ctx| {
            let p = Palette::of(&THEMES[gui.theme]);
            gui.help(ctx, &p);
        }));
        for expected in ["Atajos del sistema", "Guardar borrador local", "Pedir confirmación de entrega"] {
            assert!(shown.contains(expected), "{expected} falta en la ayuda");
        }
    }

    #[test]
    fn window_layout_and_keyboard() {
        let mut app = App::with_client(eminus::api::Client::new(""));
        app.login = None;
        app.loaded_at = text::now();
        let course = model::Course { id: "7".into(), name: "Programación de sistemas".into(), end: 0, favorite: true, current: true };
        let activity = std::sync::Arc::new(model::Activity { id: "3".into(), course_id: course.id.clone(), course_name: course.name.clone(), title: "Práctica de protocolos".into(), start: 0, extension: 0, deadline: text::now() + 3600, delivered: false, delivered_at: 0, grade: None, value: Some(10.0) });
        app.acts.insert(course.id.clone(), vec![activity]);
        app.courses.push(course);
        app.on_msg(0, eminus::api::Msg::Detail("7:3".into(), Ok(model::Detail { text: "Describe el protocolo y adjunta el informe.".into(), files: vec![model::AFile { id: "1".into(), name: "instrucciones.pdf".into(), size: "12 KB".into() }], blocked: None })));
        app.on_msg(0, eminus::api::Msg::Delivery("7:3".into(), Ok(model::DeliveryInfo { id: 41, text: "Primera línea\nSegunda línea".into(), feedback: "Revisa la explicación del protocolo.".into(), grade: Some(8.0), files: vec![model::AFile { id: "2".into(), name: "informe.pdf".into(), size: "20 KB".into() }], review_files: vec![model::AFile { id: "4".into(), name: "revisión.pdf".into(), size: "1 KB".into() }], ..Default::default() })));
        let mut gui = Gui::new(app, Appearance::default());
        let key = |key| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        let render = |gui: &mut Gui, ctx: &egui::Context, width, events| {
            ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 480.0))), events, ..Default::default() }, |ctx| gui.show(ctx))
        };
        for width in [600.0, 700.0, 720.0, 999.0, 1000.0, 1180.0, 1440.0] {
            let ctx = egui::Context::default();
            Palette::of(&THEMES[gui.theme]).apply(&ctx);
            for view in View::ALL {
                gui.app.set_view(view);
                for _ in 0..3 { render(&mut gui, &ctx, width, vec![]); }
                assert!(ctx.used_rect().right() <= width + 1.0, "{} desborda con {width} px: {:?}", view.label(), ctx.used_rect());
            }
            render(&mut gui, &ctx, width, vec![egui::Event::Text("2".into())]);
            assert!(gui.app.view == View::Courses);
            render(&mut gui, &ctx, width, vec![egui::Event::Text("l".into())]);
            assert!(gui.app.focus_mid);
            render(&mut gui, &ctx, width, vec![egui::Event::Text("/".into())]);
            assert!(ctx.wants_keyboard_input());
            assert_eq!(gui.app.filter, "");
            render(&mut gui, &ctx, width, vec![egui::Event::Text("program".into())]);
            assert_eq!(gui.app.filter, "program");
            render(&mut gui, &ctx, width, vec![key(Key::Escape)]);
            assert_eq!(gui.app.filter, "");
            assert!(!ctx.wants_keyboard_input());
            render(&mut gui, &ctx, width, vec![egui::Event::Text("?".into())]);
            assert!(gui.app.help);
            render(&mut gui, &ctx, width, vec![key(Key::Escape)]);
            assert!(!gui.app.help);
            render(&mut gui, &ctx, width, vec![egui::Event::Text("E".into())]);
            assert!(gui.app.delivery.is_some());
            assert_eq!(gui.app.delivery.as_ref().unwrap().text, "Primera línea\nSegunda línea");
            gui.app.delivery.as_mut().unwrap().confirm = true;
            render(&mut gui, &ctx, width, vec![egui::Event::Text("n".into())]);
            assert!(!gui.app.delivery.as_ref().unwrap().confirm);
            render(&mut gui, &ctx, width, vec![key(Key::Escape)]);
            assert!(gui.app.delivery.is_none());
        }
    }
}
