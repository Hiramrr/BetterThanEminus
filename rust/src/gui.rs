// eminus con ventana: la misma app (mismo estado, misma red, misma sesión)
// dibujada con egui en vez de en la terminal. Los botones hacen lo que
// harían las teclas de la versión de terminal.

use eframe::egui::{self, Align, Color32, Key, Layout, RichText};
use eminus::app::{detail_key, App, Slot, View};
use eminus::model::{self, Entry, Row, Tone};
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
        v.panel_fill = self.bg;
        v.window_fill = self.bg;
        v.extreme_bg_color = self.sel;
        v.faint_bg_color = self.sel;
        v.override_text_color = Some(self.fg);
        v.hyperlink_color = self.accent;
        v.selection.bg_fill = self.sel;
        v.selection.stroke = egui::Stroke::new(1.0_f32, self.accent);
        v.window_stroke = egui::Stroke::new(1.0_f32, self.border);
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, self.border);
        v.widgets.inactive.weak_bg_fill = self.sel;
        v.widgets.inactive.bg_fill = self.sel;
        v.widgets.hovered.weak_bg_fill = self.border;
        v.widgets.active.weak_bg_fill = self.border;
        ctx.set_visuals(v);
    }
}

struct Gui {
    app: App,
    theme: usize,
    scroll_to_selection: bool,
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Eminus").with_inner_size([1180.0, 720.0]).with_min_inner_size([720.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "eminus",
        options,
        Box::new(|cc| {
            let app = App::new();
            Palette::of(&THEMES[app.theme]).apply(&cc.egui_ctx);
            cc.egui_ctx.style_mut(|style| {
                style.spacing.item_spacing = egui::vec2(8.0, 5.0);
                style.spacing.button_padding = egui::vec2(9.0, 4.0);
            });
            Ok(Box::new(Gui { theme: app.theme, app, scroll_to_selection: false }))
        }),
    )
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok((epoch, msg)) = self.app.pool.inbox.try_recv() {
            self.app.on_msg(epoch, msg);
        }
        self.app.tick();
        if self.theme != self.app.theme {
            self.theme = self.app.theme;
            Palette::of(&THEMES[self.theme]).apply(ctx);
        }
        let p = Palette::of(&THEMES[self.theme]);

        self.status(ctx, &p);
        if self.app.login.is_some() {
            egui::CentralPanel::default().show(ctx, |ui| self.login(ui, &p));
        } else {
            self.keyboard(ctx);
            self.top(ctx, &p);
            self.lists(ctx, &p);
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.detail(ui, &p));
            });
            self.delivery(ctx, &p);
            self.offer(ctx, &p);
        }
        // Sólo se vuelve a pintar cuando hace falta: mientras hay algo en la
        // red, o cuando toca mover el reloj.
        ctx.request_repaint_after(self.app.wait());
    }
}

impl Gui {
    fn press(&mut self, c: char) {
        self.app.press(KeyCode::Char(c));
    }

    // Flechas y j/k mueven la selección cuando no se está escribiendo.
    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() || self.app.delivery.is_some() || self.app.offer.is_some() {
            return;
        }
        let (down, up, enter, left, right) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::J),
                i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::K),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
            )
        });
        for (pressed, code) in [(down, KeyCode::Down), (up, KeyCode::Up), (enter, KeyCode::Enter), (left, KeyCode::Left), (right, KeyCode::Right)] {
            if pressed {
                self.app.press(code);
                self.scroll_to_selection = true;
            }
        }
    }

    fn top(&mut self, ctx: &egui::Context, p: &Palette) {
        egui::TopBottomPanel::top("barra").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for view in View::ALL {
                    let active = self.app.view == view;
                    let label = RichText::new(view.label()).strong().color(if active { p.accent } else { p.dim });
                    if ui.selectable_label(active, label).clicked() {
                        self.app.set_view(view);
                    }
                }
                ui.separator();
                let filter = egui::TextEdit::singleline(&mut self.app.filter).hint_text("filtrar…").desired_width(170.0);
                if ui.add(filter).changed() {
                    self.app.refilter();
                }
                let mut all = self.app.show_all;
                let what = if self.app.view == View::Courses { "cursos anteriores" } else { "entregadas" };
                if ui.checkbox(&mut all, what).changed() {
                    self.press('a');
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("cerrar sesión").clicked() {
                        self.press('X');
                    }
                    if ui.button(format!("tema: {}", THEMES[self.theme].name)).clicked() {
                        self.press('c');
                    }
                    if ui.button("⟳ actualizar").clicked() {
                        self.press('r');
                    }
                    ui.label(RichText::new(&self.app.account).color(p.dim));
                });
            });
            ui.add_space(2.0);
        });
    }

    fn status(&mut self, ctx: &egui::Context, p: &Palette) {
        egui::TopBottomPanel::bottom("estado").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.app.loading || self.app.content_loading || self.app.pool.pending > 0 {
                    ui.add(egui::Spinner::new().size(12.0).color(p.accent));
                }
                ui.label(RichText::new(self.app.message().replace(" · ? ayuda", "")).color(if self.app.error { p.red } else { p.dim }));
                if self.app.login.is_some() {
                    return;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(text::clock()).color(p.dim));
                    if let Some(left) = self.app.session_left().filter(|l| *l < 20 * 60) {
                        let tone = if left < 3 * 60 { p.red } else if left < 10 * 60 { p.yellow } else { p.dim };
                        ui.label(RichText::new(format!("sesión {} min", (left / 60).max(0))).color(tone));
                    }
                    let c = &self.app.counts;
                    for (n, label, tone) in [(c.week, "7 días", p.fg), (c.soon, "48 h", p.yellow), (c.today, "hoy", p.orange), (c.overdue, "vencidas", p.red)] {
                        ui.label(RichText::new(format!("{label} {n}")).color(if n > 0 { tone } else { p.dim }));
                    }
                });
            });
        });
    }

    // Lista de la izquierda y, en cursos, la de actividades del curso.
    fn lists(&mut self, ctx: &egui::Context, p: &Palette) {
        let empty = if self.app.loading && self.app.loaded_at == 0 {
            "cargando…"
        } else if !self.app.filter.is_empty() {
            "nada coincide con el filtro"
        } else {
            match self.app.view {
                View::Agenda => "✓ sin pendientes por ahora",
                View::Miyu => "✓ todo entregado",
                View::Grades => "aún no hay entregas calificadas",
                View::Content if self.app.content_loading || !self.app.content_tried => "cargando contenido…",
                View::Content => "tus cursos actuales no tienen contenido publicado",
                View::Courses => "sin cursos",
            }
        };
        let scroll = std::mem::take(&mut self.scroll_to_selection);
        let mut clicked = None;
        egui::SidePanel::left("lista").resizable(true).default_width(if self.app.view == View::Courses { 300.0 } else { 470.0 }).width_range(220.0..=760.0).show(ctx, |ui| {
            let sel = self.app.list().sel;
            if let Some(i) = rows(ui, p, &self.app.rows, sel, scroll && !self.app.focus_mid, empty) {
                clicked = Some((false, i));
            }
        });
        if self.app.view == View::Courses {
            let empty = match self.app.selected_course() {
                Some(c) if self.app.acts_pending(&c.id) => "cargando actividades…",
                Some(_) => "sin actividades publicadas",
                None => "",
            };
            egui::SidePanel::left("actividades").resizable(true).default_width(380.0).width_range(220.0..=700.0).show(ctx, |ui| {
                if let Some(i) = rows(ui, p, &self.app.mid_rows, self.app.mid.sel, scroll && self.app.focus_mid, empty) {
                    clicked = Some((true, i));
                }
            });
        }
        if let Some((mid, index)) = clicked {
            self.app.select(mid, index);
        }
    }

    fn detail(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(entry) = self.app.target().cloned() else {
            ui.add_space(24.0);
            ui.label(RichText::new("elige algo de la lista para ver su detalle").color(p.dim));
            return;
        };
        let now = text::now();
        let today = self.app.clock.today;
        ui.add_space(6.0);
        match entry {
            Entry::Act(act) => {
                let tone = model::urgency(&act, now);
                ui.label(RichText::new(&act.title).size(19.0).strong().color(p.accent));
                ui.label(RichText::new(act.course_name.to_lowercase()).color(p.dim));
                ui.add_space(6.0);
                let status = match tone {
                    Tone::Done => format!("✓ entregada {}", if act.delivered_at != 0 { text::full_date(act.delivered_at) } else { String::new() }),
                    Tone::Future => "por iniciar".into(),
                    Tone::Overdue => "vencida, sin entregar".into(),
                    Tone::Imminent => "vence en menos de 24 h".into(),
                    Tone::Urgent => "vence en menos de 48 h".into(),
                    _ => "pendiente".into(),
                };
                let slot = self.app.details.get(&detail_key(&act.course_id, &act.id));
                let (has_files, blocked) = match slot {
                    Some(Slot::Ready(d)) => (!d.files.is_empty(), d.blocked),
                    _ => (false, None),
                };
                egui::Grid::new("datos").num_columns(2).spacing([18.0, 4.0]).show(ui, |ui| {
                    let mut kv = |key: &str, value: RichText| {
                        ui.label(RichText::new(key).color(p.dim));
                        ui.label(value);
                        ui.end_row();
                    };
                    kv("estado", RichText::new(status).color(p.tone(tone)));
                    kv("vence", RichText::new(format!("{} ({})", text::full_date(act.deadline), text::rel_due(act.deadline, now, today))));
                    kv("inicia", RichText::new(text::full_date(act.start)));
                    if act.extension != 0 {
                        kv("prórroga", RichText::new(text::full_date(act.extension)));
                    }
                    kv("valor", RichText::new(act.value.map_or("—".into(), model::fmt_num)));
                    kv("calificación", RichText::new(act.grade.map_or("sin calificar".into(), model::fmt_num)).color(if act.grade.is_some() { p.tone(model::grade_tone(act.grade)) } else { p.dim }));
                });
                ui.add_space(8.0);
                let mut action = None;
                ui.horizontal(|ui| {
                    let deliver = egui::Button::new(RichText::new(if act.delivered { "volver a entregar" } else { "entregar" }).strong().color(p.on_accent)).fill(p.accent);
                    if ui.add_enabled(blocked.is_none(), deliver).on_disabled_hover_text(blocked.unwrap_or_default()).clicked() {
                        action = Some(KeyCode::Char('E'));
                    }
                    if ui.add_enabled(has_files, egui::Button::new("descargar archivos")).clicked() {
                        action = Some(KeyCode::Char('D'));
                    }
                    if ui.button("abrir en eminus").clicked() {
                        action = Some(KeyCode::Enter);
                    }
                });
                if let Some(reason) = blocked {
                    ui.label(RichText::new(reason).color(p.yellow));
                }
                ui.add_space(10.0);
                ui.label(RichText::new("descripción").strong().color(p.dim));
                match slot {
                    None | Some(Slot::Pending) => {
                        ui.spinner();
                    }
                    Some(Slot::Failed(note)) => {
                        ui.label(RichText::new(format!("no se pudo cargar la descripción ({note})")).color(p.dim));
                    }
                    Some(Slot::Ready(detail)) => {
                        if detail.text.is_empty() {
                            ui.label(RichText::new("sin descripción").color(p.dim));
                        } else {
                            ui.label(&detail.text);
                        }
                        if !detail.files.is_empty() {
                            ui.add_space(10.0);
                            ui.label(RichText::new("archivos").strong().color(p.dim));
                            for file in &detail.files {
                                ui.label(format!("⬇ {}   {}", file.name, file.size));
                            }
                        }
                    }
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
                ui.label(RichText::new(&item.title).size(19.0).strong().color(p.accent));
                ui.label(RichText::new(item.course_name.to_lowercase()).color(p.dim));
                ui.add_space(6.0);
                egui::Grid::new("datos").num_columns(2).spacing([18.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("tipo").color(p.dim));
                    ui.label(model::content_label(item.unit, files.len(), item.has_files));
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
                ui.add_space(8.0);
                let mut action = None;
                ui.horizontal(|ui| {
                    if ui.add_enabled(!files.is_empty(), egui::Button::new(RichText::new("descargar archivos").strong().color(p.on_accent)).fill(p.accent)).clicked() {
                        action = Some(KeyCode::Char('D'));
                    }
                    if ui.button("abrir en eminus").clicked() {
                        action = Some(KeyCode::Enter);
                    }
                });
                ui.add_space(10.0);
                ui.label(RichText::new("descripción").strong().color(p.dim));
                if description.is_empty() && loading {
                    ui.spinner();
                } else if description.is_empty() {
                    ui.label(RichText::new("sin descripción").color(p.dim));
                } else {
                    ui.label(description);
                }
                ui.add_space(10.0);
                ui.label(RichText::new("archivos").strong().color(p.dim));
                if files.is_empty() {
                    ui.label(RichText::new(if loading { "buscando archivos…" } else { "sin archivos" }).color(p.dim));
                }
                for file in files {
                    ui.label(format!("⬇ {}   {}", file.name, file.size));
                }
                if let Some(code) = action {
                    self.app.press(code);
                }
            }
            Entry::Course(course) => {
                let none = Vec::new();
                let acts = self.app.acts.get(&course.id).unwrap_or(&none);
                let done = acts.iter().filter(|a| a.delivered).count();
                let mut pending: Vec<_> = acts.iter().filter(|a| !a.delivered && a.deadline > now).collect();
                pending.sort_by_key(|a| a.deadline);
                let overdue = acts.iter().filter(|a| model::urgency(a, now) == Tone::Overdue).count();
                ui.label(RichText::new(course.name.to_lowercase()).size(19.0).strong().color(p.accent));
                ui.label(RichText::new(format!("{}{}", if course.current { "curso actual" } else { "curso anterior" }, if course.favorite { " · favorito" } else { "" })).color(p.dim));
                ui.add_space(6.0);
                egui::Grid::new("datos").num_columns(2).spacing([18.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("termina").color(p.dim));
                    ui.label(text::full_date(course.end));
                    ui.end_row();
                    ui.label(RichText::new("entregadas").color(p.dim));
                    ui.add(egui::ProgressBar::new(if acts.is_empty() { 0.0 } else { done as f32 / acts.len() as f32 }).desired_width(180.0).fill(p.green).text(format!("{done}/{}", acts.len())));
                    ui.end_row();
                    ui.label(RichText::new("pendientes").color(p.dim));
                    ui.label(pending.len().to_string());
                    ui.end_row();
                    ui.label(RichText::new("vencidas").color(p.dim));
                    ui.label(RichText::new(overdue.to_string()).color(if overdue > 0 { p.red } else { p.fg }));
                    ui.end_row();
                });
                if !pending.is_empty() {
                    ui.add_space(10.0);
                    ui.label(RichText::new("lo próximo").strong().color(p.dim));
                    for a in pending.iter().take(5) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(text::rel_due(a.deadline, now, today)).color(p.tone(model::urgency(a, now))));
                            ui.label(&a.title);
                        });
                    }
                }
                ui.add_space(8.0);
                if ui.button("abrir en eminus").clicked() {
                    self.app.press(KeyCode::Enter);
                }
            }
        }
    }

    fn login(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let profiles = self.app.profiles.perfiles.clone();
        let mut keys: Vec<KeyCode> = Vec::new();
        let mut pick = None;
        let Some(login) = &mut self.app.login else { return };
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.16);
            ui.label(RichText::new("eminus").size(30.0).strong().color(p.accent));
            ui.add_space(14.0);
            ui.allocate_ui_with_layout(egui::vec2(360.0, 0.0), Layout::top_down_justified(Align::Center), |ui| {
                ui.set_max_width(360.0);
                ui.add_enabled_ui(!login.busy, |ui| {
                    if login.picker {
                        ui.label(RichText::new("¿quién entra?").color(p.dim));
                        ui.add_space(4.0);
                        for (i, profile) in profiles.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let name = if profile.name == profile.user { profile.user.clone() } else { format!("{}  ·  {}", profile.name, profile.user) };
                                if ui.add_sized([318.0, 30.0], egui::Button::new(RichText::new(name).strong())).clicked() {
                                    pick = Some(i);
                                    keys.push(KeyCode::Enter);
                                }
                                if ui.add_sized([30.0, 30.0], egui::Button::new("✖")).on_hover_text("quitar este perfil (pide confirmar)").clicked() {
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
                        let user = ui.add(egui::TextEdit::singleline(&mut login.user).hint_text("usuario"));
                        let pass = ui.add(egui::TextEdit::singleline(&mut login.pass).password(true).hint_text("contraseña"));
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
                    ui.spinner();
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
        let Some(d) = &mut self.app.delivery else { return };
        let title = format!("entregar · {}", text::clip(&d.act.title, 56));
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new(d.act.course_name.to_lowercase()).color(p.dim));
            ui.add_space(4.0);
            match &d.info {
                Slot::Pending => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("consultando tu entrega en eminus…").color(p.dim));
                    });
                }
                Slot::Failed(note) => {
                    ui.label(RichText::new(format!("no se pudo consultar tu entrega ({note})")).color(p.yellow));
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
                        ui.label(RichText::new(format!("✓ {}   {} · ya en eminus", file.name, file.size)).color(p.green));
                    }
                }
            }
            ui.add_space(8.0);
            ui.add_enabled_ui(!d.busy && !d.confirm, |ui| {
                ui.label(RichText::new("archivos por subir").strong().color(p.dim));
                let mut remove = None;
                for (i, (path, size)) in d.files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("✖").clicked() {
                            remove = Some(i);
                        }
                        ui.label(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                        ui.label(RichText::new(model::file_size(*size as f64)).color(p.dim));
                    });
                }
                if let Some(i) = remove {
                    d.files.remove(i);
                }
                ui.horizontal(|ui| {
                    if ui.button("elegir archivos…").clicked() {
                        if let Some(paths) = rfd::FileDialog::new().set_title("Archivos para entregar").pick_files() {
                            add.extend(paths.into_iter().map(|path| path.to_string_lossy().into_owned()));
                        }
                    }
                    ui.label(RichText::new("o suéltalos sobre la ventana").color(p.dim));
                });
                ui.add_space(8.0);
                ui.label(RichText::new("comentario (eminus lo exige)").strong().color(p.dim));
                ui.add(egui::TextEdit::multiline(&mut d.text).desired_rows(3).desired_width(f32::INFINITY));
            });
            ui.add_space(8.0);
            if d.busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(&d.note).color(p.accent));
                });
            } else if d.confirm {
                let files = match d.files.len() {
                    0 => "sin archivos nuevos".to_string(),
                    1 => "con 1 archivo".to_string(),
                    n => format!("con {n} archivos"),
                };
                ui.label(RichText::new(format!("¿Entregar {files}? Se envía a tu docente en eminus.")).strong().color(p.orange));
                ui.horizontal(|ui| {
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
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("entregar…").strong().color(p.on_accent)).fill(p.accent)).clicked() {
                        // Enter en el campo de comentario pide la confirmación.
                        d.field = 1;
                        keys.push(KeyCode::Enter);
                    }
                    if ui.button("cancelar").clicked() {
                        keys.push(KeyCode::Esc);
                    }
                });
            }
        });
        if self.app.delivery.as_ref().is_some_and(|d| !d.busy && !d.confirm) {
            add.iter().for_each(|path| self.app.add_delivery_file(path));
        }
        for key in keys {
            self.app.press(key);
        }
    }

    fn offer(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some((user, _)) = &self.app.offer else { return };
        let mut key = None;
        egui::Window::new("perfiles").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(420.0);
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
        if let Some(c) = key {
            self.press(c);
        }
    }
}

// Pinta una lista y devuelve el índice de la fila en la que se hizo clic.
fn rows(ui: &mut egui::Ui, p: &Palette, rows: &[Row], selected: usize, scroll: bool, empty: &str) -> Option<usize> {
    let mut clicked = None;
    // El panel conserva su ancho aunque el contenido sea más angosto.
    ui.set_min_width(ui.available_width());
    if rows.is_empty() {
        ui.add_space(16.0);
        ui.label(RichText::new(empty).color(p.dim));
        return None;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
            // Una fila, una línea: lo que no quepa se corta con «…».
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            let mut index = 0;
            for row in rows {
                match row {
                    Row::Group { label, count, tone } => {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(label).strong().color(if *tone == Tone::Plain { p.accent } else { p.tone(*tone) }));
                            ui.label(RichText::new(count).color(p.dim));
                        });
                    }
                    Row::Item { tone, due, course, title, meta, .. } => {
                        let font = egui::FontId::proportional(14.0);
                        let mut job = egui::text::LayoutJob::default();
                        let part = |text: &str, color: Color32| (text.to_string(), egui::TextFormat { font_id: font.clone(), color, ..Default::default() });
                        let mut parts = Vec::new();
                        if !due.is_empty() {
                            parts.push(part(&format!("{due}   "), p.tone(*tone)));
                        }
                        parts.push(part(title, if *tone == Tone::Done { p.dim } else { p.fg }));
                        if !course.is_empty() {
                            parts.push(part(&format!("   {course}"), p.dim));
                        }
                        if !meta.is_empty() {
                            parts.push(part(&format!("   {meta}"), p.dim));
                        }
                        for (text, format) in parts {
                            job.append(&text, 0.0, format);
                        }
                        let response = ui.selectable_label(index == selected, job);
                        if response.clicked() {
                            clicked = Some(index);
                        }
                        if scroll && index == selected {
                            response.scroll_to_me(None);
                        }
                        index += 1;
                    }
                }
            }
        });
    });
    clicked
}
