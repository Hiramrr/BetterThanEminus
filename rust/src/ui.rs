// Dibujo. Nada aquí consulta la red ni recalcula modelos: pinta lo que la
// app ya tiene, y anota dónde quedó cada panel para el ratón.

use crate::app::{detail_key, line_of, App, ListState, Slot, View};
use crate::model::{self, Activity, ContentItem, Course, Entry, Row, Tone};
use crate::text::{self, Filter};
use crate::theme::{Theme, THEMES};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn spinner() -> &'static str {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    SPINNER[(ms / 80 % 10) as usize]
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let t = &THEMES[app.theme];
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(t.bg).fg(t.fg)), area);
    let [top, body, bottom] = Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas(area);
    app.hit.list = Rect::default();
    app.hit.mid = Rect::default();
    app.hit.detail = Rect::default();
    tabs(f, app, t, top);
    if app.login.is_some() {
        login(f, app, t, body);
    } else {
        match app.view {
            View::Courses => courses(f, app, t, body),
            View::Miyu => miyu(f, app, t, body),
            _ => two_panes(f, app, t, body),
        }
        if app.help {
            help(f, t, body);
        }
        if let Some((user, _)) = &app.offer {
            offer(f, t, body, user);
        }
        if app.delivery.is_some() {
            delivery(f, app, t, body);
        }
    }
    status(f, app, t, bottom);
}

fn tabs(f: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let mut spans = vec![Span::styled(" eminus ", Style::new().bg(t.accent).fg(t.on_accent).add_modifier(Modifier::BOLD)), Span::raw(" ")];
    let mut x = area.x + 9;
    app.hit.tabs.clear();
    for (i, view) in View::ALL.into_iter().enumerate() {
        let label = format!(" {} {} ", i + 1, view.label());
        let w = text::width(&label) as u16;
        if app.login.is_none() {
            app.hit.tabs.push((x, x + w, view));
        }
        x += w;
        let style = if view == app.view && app.login.is_none() { Style::new().fg(t.accent).add_modifier(Modifier::BOLD | Modifier::REVERSED) } else { Style::new().fg(t.dim) };
        spans.push(Span::styled(label, style));
    }
    f.render_widget(Line::from(spans), area);
    if app.login.is_none() && !app.account.is_empty() && area.width > x + text::width(&app.account) as u16 + 2 {
        f.render_widget(Line::styled(format!("{} ", app.account), Style::new().fg(t.dim)).right_aligned(), area);
    }
}

fn pane<'a>(t: &Theme, title: String, focused: bool) -> Block<'a> {
    Block::bordered()
        .border_style(Style::new().fg(if focused { t.accent } else { t.border }))
        .title(Span::styled(format!(" {title} "), Style::new().fg(if focused { t.accent } else { t.dim }).add_modifier(Modifier::BOLD)))
}

fn filter_suffix(app: &App, here: bool) -> String {
    if here && (!app.filter.is_empty() || app.filtering) { format!(" · /{}", app.filter) } else { String::new() }
}

// Título con las coincidencias del filtro resaltadas.
fn marked(title: &str, filter: &Filter, base: Style, t: &Theme) -> Vec<Span<'static>> {
    if filter.is_empty() {
        return vec![Span::styled(title.to_string(), base)];
    }
    let marks = filter.marks(title);
    let hit = base.fg(t.yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let mut spans = Vec::new();
    let mut chunk = String::new();
    let mut last = false;
    for (c, mark) in title.chars().zip(marks) {
        if mark != last && !chunk.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut chunk), if last { hit } else { base }));
        }
        last = mark;
        chunk.push(c);
    }
    spans.push(Span::styled(chunk, if last { hit } else { base }));
    spans
}

struct ListView<'a> {
    rows: &'a [Row],
    title: String,
    focused: bool,
    filter: &'a Filter,
    empty: String,
}

fn list(f: &mut Frame, t: &Theme, area: Rect, view: ListView, state: &mut ListState) -> Rect {
    let ListView { rows, title, focused, filter, empty } = view;
    let mut block = pane(t, title, focused);
    let inner = block.inner(area);
    let height = inner.height as usize;
    let width = inner.width as usize;
    let sel_line = line_of(rows, state.sel);
    if let Some(line) = sel_line {
        // La cabecera del grupo se queda a la vista con su primera fila.
        let top = if line > 0 && matches!(rows[line - 1], Row::Group { .. }) { line - 1 } else { line };
        if top < state.offset {
            state.offset = top;
        } else if line >= state.offset + height {
            state.offset = line + 1 - height.max(1);
        }
        let items = rows.iter().filter(|r| matches!(r, Row::Item { .. })).count();
        block = block.title_bottom(Line::styled(format!(" {}/{} ", state.sel + 1, items), Style::new().fg(t.dim)).right_aligned());
    }
    state.offset = state.offset.min(rows.len().saturating_sub(height));
    f.render_widget(block, area);
    if rows.is_empty() {
        f.render_widget(Paragraph::new(empty).style(Style::new().fg(t.dim)).wrap(Wrap { trim: false }), inner.inner(ratatui::layout::Margin::new(1, 1)));
        return inner;
    }
    let due_w = rows.iter().map(|r| if let Row::Item { due, .. } = r { text::width(due) } else { 0 }).max().unwrap_or(0).min(16);
    let has_course = rows.iter().any(|r| matches!(r, Row::Item { course, .. } if !course.is_empty()));
    let course_w = if !has_course { 0 } else if width >= 70 { 20 } else if width >= 50 { 12 } else { 0 };
    let mut lines = Vec::with_capacity(height);
    for (i, row) in rows.iter().enumerate().skip(state.offset).take(height) {
        lines.push(match row {
            Row::Group { label, count, tone } => {
                let color = if *tone == Tone::Plain { t.accent } else { t.tone(*tone) };
                let label = text::clip(label, width.saturating_sub(text::width(count) + 4));
                let rule = width.saturating_sub(text::width(&label) + text::width(count) + 4);
                Line::from(vec![
                    Span::styled(format!(" {label} "), Style::new().fg(color).add_modifier(Modifier::BOLD)),
                    Span::styled("─".repeat(rule), Style::new().fg(t.border)),
                    Span::styled(format!(" {count} "), Style::new().fg(t.dim)),
                ])
            }
            Row::Item { glyph, tone, due, course, title, meta, .. } => {
                let selected = sel_line == Some(i);
                let base = if selected { Style::new().bg(t.sel).fg(t.fg).add_modifier(if focused { Modifier::BOLD } else { Modifier::empty() }) } else { Style::new().fg(t.fg) };
                let toned = base.fg(t.tone(*tone));
                let soft = if selected { base } else { base.fg(t.dim) };
                let mut spans = vec![Span::styled(if selected && focused { "▌" } else { " " }, base.fg(t.accent)), Span::styled(format!("{glyph} "), toned)];
                let mut used = 3;
                if due_w > 0 {
                    spans.push(Span::styled(format!("{} ", text::fit(due, due_w)), toned));
                    used += due_w + 1;
                }
                if course_w > 0 {
                    spans.push(Span::styled(format!("{} ", text::fit(course, course_w)), soft));
                    used += course_w + 1;
                }
                let meta_w = if meta.is_empty() { 0 } else { text::width(meta) + 1 };
                let room = width.saturating_sub(used + meta_w);
                let shown = text::clip(title, room);
                let pad = room.saturating_sub(text::width(&shown));
                spans.extend(marked(&shown, filter, if *tone == Tone::Done { soft } else { base }, t));
                spans.push(Span::styled(" ".repeat(pad), base));
                if meta_w > 0 {
                    spans.push(Span::styled(format!("{meta} "), soft));
                }
                Line::from(spans)
            }
        });
    }
    f.render_widget(Paragraph::new(lines), inner);
    inner
}

fn loading_note(app: &App) -> String {
    if app.loading && app.loaded_at == 0 { "cargando actividades…".into() } else { String::new() }
}

fn no_match(app: &App) -> Option<String> {
    (!app.filter.is_empty()).then(|| format!("nada coincide con «{}»", app.filter))
}

fn two_panes(f: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let [left, right] = if area.width >= 90 {
        Layout::horizontal([Constraint::Percentage(48), Constraint::Min(0)]).areas(area)
    } else {
        Layout::vertical([Constraint::Percentage(55), Constraint::Min(0)]).areas(area)
    };
    let filter = Filter::new(&app.filter);
    let suffix = filter_suffix(app, true);
    let (title, empty) = match app.view {
        View::Agenda => (
            format!("agenda{}{suffix}", if app.show_all { " · todo" } else { "" }),
            no_match(app).or(Some(loading_note(app)).filter(|n| !n.is_empty())).unwrap_or_else(|| "✓ sin pendientes por ahora.\n\n  a  ver entregadas\n  2  explorar cursos\n  r  actualizar".into()),
        ),
        View::Grades => (
            format!("tareas calificadas · promedio {}{suffix}", app.average.map_or("—".into(), |a| format!("{a:.1}"))),
            no_match(app).unwrap_or_else(|| "aún no hay entregas calificadas.\n\nlas actividades entregadas aparecen aquí como «en revisión» hasta que tengan nota.".into()),
        ),
        _ => (
            format!("contenido por materia{}{suffix}", if app.content_loading && !app.rows.is_empty() { " · actualizando…" } else { "" }),
            if app.content_loading || app.loading || !app.content_tried {
                "cargando contenido…".into()
            } else {
                no_match(app).unwrap_or_else(|| "tus cursos actuales no tienen contenido publicado.\n\n  r  actualizar".into())
            },
        ),
    };
    let mut state = app.list();
    app.hit.list = list(f, t, left, ListView { rows: &app.rows, title, focused: true, filter: &filter, empty }, &mut state);
    app.lists[app.view as usize] = state;
    detail(f, app, t, right);
}

fn courses(f: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let [left, mid, right] = if area.width >= 110 {
        Layout::horizontal([Constraint::Percentage(28), Constraint::Percentage(36), Constraint::Min(0)]).areas(area)
    } else {
        let [top, bottom] = Layout::vertical([Constraint::Percentage(55), Constraint::Min(0)]).areas(area);
        let [left, mid] = Layout::horizontal([Constraint::Percentage(42), Constraint::Min(0)]).areas(top);
        [left, mid, bottom]
    };
    let filter = Filter::new(&app.filter);
    let none = Filter::default();
    let mut state = app.list();
    let view = ListView {
        rows: &app.rows,
        title: format!("cursos{}", filter_suffix(app, !app.focus_mid)),
        focused: !app.focus_mid,
        filter: if app.focus_mid { &none } else { &filter },
        empty: no_match(app).unwrap_or_else(|| if app.loading { "cargando cursos…".into() } else { "sin cursos".into() }),
    };
    app.hit.list = list(f, t, left, view, &mut state);
    app.lists[View::Courses as usize] = state;

    let course = app.selected_course();
    let view = ListView {
        rows: &app.mid_rows,
        title: format!("{}{}", course.map_or("actividades".into(), |c| model::short_course(&c.name)), filter_suffix(app, app.focus_mid)),
        focused: app.focus_mid,
        filter: if app.focus_mid { &filter } else { &none },
        empty: match course {
            None => String::new(),
            Some(c) if app.acts_pending(&c.id) => "cargando actividades…".into(),
            Some(_) if app.focus_mid && !app.filter.is_empty() => format!("nada coincide con «{}»", app.filter),
            Some(_) => "sin actividades publicadas".into(),
        },
    };
    let mut state = app.mid;
    app.hit.mid = list(f, t, mid, view, &mut state);
    app.mid = state;
    detail(f, app, t, right);
}

fn miyu(f: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let wide = area.width >= 124;
    let [left, mid, right] = if wide {
        Layout::horizontal([Constraint::Length(40), Constraint::Percentage(36), Constraint::Min(0)]).areas(area)
    } else if area.width >= 84 {
        let [left, mid] = Layout::horizontal([Constraint::Length(40), Constraint::Min(0)]).areas(area);
        [left, mid, Rect::default()]
    } else {
        [Rect::default(), area, Rect::default()]
    };
    if left.width > 0 {
        let block = pane(t, "miyu".into(), false);
        let inner = block.inner(left).inner(ratatui::layout::Margin::new(1, 0));
        f.render_widget(block, left);
        f.render_widget(Paragraph::new(dash(app, t, inner.width as usize)), inner);
    }
    let filter = Filter::new(&app.filter);
    let view = ListView {
        rows: &app.rows,
        title: format!("pendientes por materia{}", filter_suffix(app, true)),
        focused: true,
        filter: &filter,
        empty: no_match(app).or(Some(loading_note(app)).filter(|n| !n.is_empty())).unwrap_or_else(|| "✓ todo entregado.".into()),
    };
    let mut state = app.list();
    app.hit.list = list(f, t, mid, view, &mut state);
    app.lists[View::Miyu as usize] = state;
    if right.width > 0 {
        detail(f, app, t, right);
    }
}

fn label(t: &Theme, text: &str) -> Line<'static> {
    Line::styled(text.to_string(), Style::new().fg(t.dim).add_modifier(Modifier::BOLD))
}

fn bar(t: &Theme, done: usize, total: usize, width: usize) -> Vec<Span<'static>> {
    let (full, empty, count) = model::progress_bar(done, total, width);
    vec![Span::styled(full, Style::new().fg(t.green)), Span::styled(empty, Style::new().fg(t.border)), Span::styled(count, Style::new().fg(t.dim))]
}

// Tablero de Miyu: siguiente entrega, carga de la semana, avance y reciente.
fn dash(app: &App, t: &Theme, width: usize) -> Vec<Line<'static>> {
    let (now, today) = (text::now(), app.clock.today);
    let acts: Vec<&Activity> = model::current_acts(&app.courses, &app.acts).map(|a| &**a).collect();
    let dim = Style::new().fg(t.dim);
    let mut lines = vec![label(t, "siguiente entrega")];
    match acts.iter().filter(|a| !a.delivered && a.deadline >= now && a.start <= now).min_by_key(|a| a.deadline) {
        Some(next) => {
            let tone = t.tone(model::urgency(next, now));
            let start = if next.start != 0 { next.start } else { next.deadline - 7 * 86400 };
            let elapsed = ((now - start) as f64 / (next.deadline - start).max(1) as f64).clamp(0.0, 1.0);
            let bar_w = width.saturating_sub(6).min(24);
            let filled = (elapsed * bar_w as f64).round() as usize;
            lines.push(Line::styled(text::clip(&next.title, width), Style::new().add_modifier(Modifier::BOLD)));
            lines.push(Line::styled(text::clip(&model::short_course(&next.course_name), width), dim));
            lines.push(Line::styled(text::countdown(next.deadline, now, true), Style::new().fg(tone).add_modifier(Modifier::BOLD)));
            lines.push(Line::styled(text::full_date(next.deadline), dim));
            lines.push(Line::from(vec![
                Span::styled("█".repeat(filled), Style::new().fg(tone)),
                Span::styled("░".repeat(bar_w - filled), Style::new().fg(t.border)),
                Span::styled(format!(" {}%", (elapsed * 100.0).round()), dim),
            ]));
        }
        None => lines.push(Line::styled("nada por entregar. disfruta.", dim)),
    }

    lines.extend([Line::raw(""), label(t, "carga de la semana")]);
    let mut per_day = [0usize; 7];
    for a in acts.iter().filter(|a| !a.delivered && a.deadline >= today) {
        if let Some(n) = per_day.get_mut(((a.deadline - today) / 86400) as usize) {
            *n += 1;
        }
    }
    for (i, n) in per_day.into_iter().enumerate() {
        let day = text::fit(&text::day_label(today + i as i64 * 86400, i), 8);
        let blocks = if n == 0 { Span::styled("·", dim) } else { Span::styled(text::clip(&"■ ".repeat(n), width.saturating_sub(12)), Style::new().fg(if n >= 3 { t.red } else { t.accent })) };
        let count = if n == 0 { String::new() } else { format!(" {n}") };
        lines.push(Line::from(vec![Span::styled(day, if i == 0 { Style::new().add_modifier(Modifier::BOLD) } else { dim }), blocks, Span::styled(count, dim)]));
    }

    let mut progress: Vec<(&str, usize, usize)> = Vec::new();
    for a in &acts {
        match progress.iter_mut().find(|(name, ..)| *name == &*a.course_name) {
            Some(entry) => (entry.1, entry.2) = (entry.1 + usize::from(a.delivered), entry.2 + 1),
            None => progress.push((&a.course_name, usize::from(a.delivered), 1)),
        }
    }
    if !progress.is_empty() {
        lines.extend([Line::raw(""), label(t, "avance por materia")]);
        for (name, done, total) in progress {
            let mut spans = vec![Span::raw(text::fit(&model::short_course(name), width.saturating_sub(15)))];
            spans.push(Span::raw(" "));
            spans.extend(bar(t, done, total, 8));
            lines.push(Line::from(spans));
        }
    }

    lines.extend([Line::raw(""), label(t, "reciente")]);
    let mut feed: Vec<(i64, bool, &Activity)> = Vec::new();
    for a in &acts {
        if a.delivered && a.delivered_at != 0 && now - a.delivered_at < 7 * 86400 {
            feed.push((a.delivered_at, true, a));
        }
        if a.start != 0 && a.start <= now && now - a.start < 3 * 86400 {
            feed.push((a.start, false, a));
        }
    }
    feed.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    if feed.is_empty() {
        lines.push(Line::styled("sin movimiento en los últimos días", dim));
    }
    for (at, done, act) in feed.into_iter().take(6) {
        let when = text::fit(&text::rel_due(at, now, today), 9);
        lines.push(Line::from(vec![
            Span::styled(if done { "✓ " } else { "+ " }, Style::new().fg(if done { t.green } else { t.accent })),
            Span::styled(when, dim),
            Span::raw(text::clip(&act.title, width.saturating_sub(12))),
        ]));
    }
    lines
}

// ───────────── detalle ─────────────

fn kv(t: &Theme, key: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::styled(text::fit(key, 14), Style::new().fg(t.dim))];
    spans.extend(value);
    Line::from(spans)
}

fn plain(text: impl Into<String>) -> Vec<Span<'static>> {
    vec![Span::raw(text.into())]
}

fn body(lines: &mut Vec<Line<'static>>, text: &str) {
    lines.extend(text.lines().map(|l| Line::raw(l.to_string())));
}

fn hints(t: &Theme, lines: &mut Vec<Line<'static>>, hints: &[(&str, &str)]) {
    let mut spans = Vec::new();
    for (key, what) in hints {
        spans.push(Span::styled(format!(" {key} "), Style::new().bg(t.sel).fg(t.accent).add_modifier(Modifier::BOLD)));
        spans.push(Span::styled(format!(" {what}   "), Style::new().fg(t.dim)));
    }
    lines.extend([Line::raw(""), Line::from(spans)]);
}

fn activity_detail(app: &App, t: &Theme, act: &Activity) -> Vec<Line<'static>> {
    let now = text::now();
    let dim = Style::new().fg(t.dim);
    let tone = model::urgency(act, now);
    let status = match tone {
        Tone::Done => format!("✓ entregada {}", if act.delivered_at != 0 { text::full_date(act.delivered_at) } else { String::new() }),
        Tone::Future => "○ por iniciar".into(),
        Tone::Overdue => "! vencida, sin entregar".into(),
        Tone::Imminent => "◉ vence en menos de 24 h".into(),
        Tone::Urgent => "● vence en menos de 48 h".into(),
        _ => "● pendiente".into(),
    };
    let mut lines = vec![
        Line::styled(act.title.clone(), Style::new().fg(t.accent).add_modifier(Modifier::BOLD)),
        Line::styled(act.course_name.to_lowercase(), dim),
        Line::raw(""),
        kv(t, "estado", vec![Span::styled(status, Style::new().fg(t.tone(tone)))]),
        kv(t, "vence", vec![Span::raw(text::full_date(act.deadline)), Span::styled(format!(" ({})", text::rel_due(act.deadline, now, app.clock.today)), dim)]),
        kv(t, "inicia", plain(text::full_date(act.start))),
    ];
    if act.extension != 0 {
        lines.push(kv(t, "prórroga", plain(text::full_date(act.extension))));
    }
    lines.push(kv(t, "valor", plain(act.value.map_or("—".into(), model::fmt_num))));
    lines.push(kv(t, "calificación", vec![Span::styled(act.grade.map_or("sin calificar".into(), model::fmt_num), Style::new().fg(if act.grade.is_some() { t.tone(model::grade_tone(act.grade)) } else { t.dim }))]));
    lines.extend([Line::raw(""), label(t, "descripción")]);
    match app.details.get(&detail_key(&act.course_id, &act.id)) {
        None | Some(Slot::Pending) => lines.push(Line::styled(format!("{} cargando…", spinner()), dim)),
        Some(Slot::Failed(note)) => lines.push(Line::styled(format!("no se pudo cargar la descripción ({note})"), dim)),
        Some(Slot::Ready(detail)) => {
            if detail.text.is_empty() {
                lines.push(Line::styled("sin descripción", dim));
            } else {
                body(&mut lines, &detail.text);
            }
            if !detail.files.is_empty() {
                lines.extend([Line::raw(""), label(t, "archivos")]);
                lines.extend(detail.files.iter().map(|file| Line::from(vec![Span::styled("⬇ ", Style::new().fg(t.accent)), Span::raw(format!("{} ", file.name)), Span::styled(file.size.clone(), dim)])));
            }
        }
    }
    let mut keys = vec![("enter", "abrir en eminus")];
    if let Some(Slot::Ready(detail)) = app.details.get(&detail_key(&act.course_id, &act.id)) {
        if !detail.files.is_empty() {
            keys.push(("D", "descargar"));
        }
        if detail.blocked.is_none() {
            keys.push(("E", if act.delivered { "volver a entregar" } else { "entregar" }));
        }
    }
    hints(t, &mut lines, &keys);
    lines
}

fn content_detail(app: &App, t: &Theme, item: &ContentItem) -> Vec<Line<'static>> {
    let dim = Style::new().fg(t.dim);
    let slot = app.content_details.get(&item.id);
    let loading = matches!(slot, None | Some(Slot::Pending));
    // Lo ya cargado o, si aún no llega, lo que venía en la lista.
    let (description, files) = match slot {
        Some(Slot::Ready(d)) => (d.text.as_str(), d.files.as_slice()),
        _ => (item.text.as_str(), item.files.as_slice()),
    };
    let mut lines = vec![
        Line::styled(item.title.clone(), Style::new().fg(t.accent).add_modifier(Modifier::BOLD)),
        Line::styled(item.course_name.to_lowercase(), dim),
        Line::raw(""),
        kv(t, "tipo", plain(model::content_label(item.unit, files.len(), item.has_files))),
    ];
    if !item.unit {
        lines.push(kv(t, "módulo", plain(item.unit_name.clone())));
    }
    lines.push(kv(t, "publicado", plain(text::full_date(item.published))));
    lines.extend([Line::raw(""), label(t, "descripción")]);
    if description.is_empty() {
        lines.push(Line::styled(if loading { format!("{} cargando…", spinner()) } else { "sin descripción".into() }, dim));
    } else {
        body(&mut lines, description);
    }
    lines.extend([Line::raw(""), label(t, "archivos")]);
    if files.is_empty() {
        let note = if loading { "buscando archivos…" } else if matches!(slot, Some(Slot::Failed(_))) { "no se pudieron cargar los archivos" } else { "sin archivos" };
        lines.push(Line::styled(note, dim));
    }
    lines.extend(files.iter().map(|file| Line::from(vec![Span::styled("⬇ ", Style::new().fg(t.accent)), Span::raw(format!("{} ", file.name)), Span::styled(file.size.clone(), dim)])));
    let mut keys = vec![("enter", "abrir en eminus")];
    if !files.is_empty() {
        keys.push(("D", "descargar"));
    }
    hints(t, &mut lines, &keys);
    lines
}

fn course_detail(app: &App, t: &Theme, course: &Course) -> Vec<Line<'static>> {
    let now = text::now();
    let dim = Style::new().fg(t.dim);
    let empty = Vec::new();
    let acts = app.acts.get(&course.id).unwrap_or(&empty);
    let mut pending: Vec<_> = acts.iter().filter(|a| !a.delivered && a.deadline > now).collect();
    pending.sort_by_key(|a| a.deadline);
    let overdue = acts.iter().filter(|a| model::urgency(a, now) == Tone::Overdue).count();
    let mut lines = vec![
        Line::styled(course.name.to_lowercase(), Style::new().fg(t.accent).add_modifier(Modifier::BOLD)),
        Line::styled(format!("{}{}", if course.current { "curso actual" } else { "curso anterior" }, if course.favorite { " · ★ favorito" } else { "" }), dim),
        Line::raw(""),
        kv(t, "termina", plain(text::full_date(course.end))),
        kv(t, "entregadas", bar(t, acts.iter().filter(|a| a.delivered).count(), acts.len(), 16)),
        kv(t, "pendientes", plain(pending.len().to_string())),
        kv(t, "vencidas", vec![Span::styled(overdue.to_string(), if overdue > 0 { Style::new().fg(t.red) } else { Style::new() })]),
    ];
    if !pending.is_empty() {
        lines.extend([Line::raw(""), label(t, "lo próximo")]);
        lines.extend(pending.iter().take(5).map(|a| Line::from(vec![Span::styled(text::fit(&text::rel_due(a.deadline, now, app.clock.today), 14), Style::new().fg(t.tone(model::urgency(a, now)))), Span::raw(a.title.clone())])));
    }
    hints(t, &mut lines, &[("enter", "abrir en eminus"), ("l", "ver actividades")]);
    lines
}

fn detail(f: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let block = pane(t, "detalle".into(), false);
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(1, 0));
    f.render_widget(block, area);
    app.hit.detail = area;
    let lines = match app.target() {
        Some(Entry::Act(act)) => activity_detail(app, t, act),
        Some(Entry::Content(item)) => content_detail(app, t, item),
        Some(Entry::Course(course)) => course_detail(app, t, course),
        None => Vec::new(),
    };
    // Alto real con el ajuste de línea, para no desplazar más allá del final.
    let width = inner.width.max(1) as usize;
    let total: usize = lines.iter().map(|l| l.width().div_ceil(width).max(1)).sum();
    app.hit.detail_max = total.saturating_sub(inner.height as usize).min(u16::MAX as usize) as u16;
    app.detail_scroll = app.detail_scroll.min(app.hit.detail_max);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((app.detail_scroll, 0)), inner);
    if app.hit.detail_max > 0 {
        let more = if app.detail_scroll < app.hit.detail_max { " J ↓ más " } else { " K ↑ " };
        f.render_widget(Line::styled(more, Style::new().fg(t.dim)).right_aligned(), Rect { y: area.bottom().saturating_sub(1), height: 1, x: area.x + 1, width: area.width.saturating_sub(2) });
    }
}

// ───────────── barra de estado ─────────────

fn status(f: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let chip = |text: String, bg| Span::styled(text, Style::new().bg(bg).fg(t.on_accent).add_modifier(Modifier::BOLD));
    let busy = app.loading || app.content_loading || app.login.as_ref().is_some_and(|l| l.busy);
    let mut left = vec![if app.filtering {
        chip(" FILTRO ".into(), t.yellow)
    } else if busy {
        chip(format!(" {} CARGANDO ", spinner()), t.orange)
    } else if app.error {
        chip(" ERROR ".into(), t.red)
    } else if app.login.is_some() {
        chip(" LOGIN ".into(), t.accent)
    } else {
        chip(" NORMAL ".into(), t.accent)
    }];
    if app.filtering {
        left.push(Span::raw(format!(" /{}▏", app.filter)));
        left.push(Span::styled("  enter aplicar · esc quitar", Style::new().fg(t.dim)));
    } else {
        left.push(Span::styled(format!(" {}", app.message()), Style::new().fg(t.dim)));
    }

    let dim = Style::new().fg(t.dim);
    let mut right = Vec::new();
    if app.login.is_none() {
        let c = &app.counts;
        let count = |n: usize, text: String, color| Span::styled(text, if n > 0 { Style::new().fg(color).add_modifier(Modifier::BOLD) } else { dim });
        right.extend([
            count(c.overdue, format!("! {} ", c.overdue), t.red),
            count(c.today, format!(" hoy {} ", c.today), t.orange),
            count(c.soon, format!(" 48h {} ", c.soon), t.yellow),
            count(c.week, format!(" 7d {} ", c.week), t.fg),
        ]);
        // El token de Eminus dura 2 horas y Eminus no avisa.
        if let Some(left) = app.session_left().filter(|l| *l < 20 * 60) {
            let color = if left < 3 * 60 { t.red } else if left < 10 * 60 { t.yellow } else { t.dim };
            right.push(Span::styled(format!("│ sesión {} min ", (left / 60).max(0)), Style::new().fg(color)));
        }
    }
    right.push(Span::styled(format!("│ {} │ {} ", THEMES[app.theme].name, text::clock()), dim));
    let right_w: usize = right.iter().map(|s| s.width()).sum();
    let [l, r] = Layout::horizontal([Constraint::Min(0), Constraint::Length(right_w.min(area.width as usize / 2 * 2) as u16)]).areas(area);
    f.render_widget(Line::from(left), l);
    if r.width as usize >= right_w {
        f.render_widget(Line::from(right), r);
    }
}

// ───────────── ventanas ─────────────

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let (w, h) = (width.min(area.width), height.min(area.height));
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn help(f: &mut Frame, t: &Theme, area: Rect) {
    const KEYS: [(&str, &str); 18] = [
        ("1 2 3 4 5", "agenda · cursos · miyu · calificadas · contenido"),
        ("j k  ↑ ↓", "mover"),
        ("g G", "primero · último"),
        ("ctrl d / u", "media página"),
        ("h l  tab", "cambiar de panel (cursos)"),
        ("J K", "desplazar el detalle"),
        ("enter  o", "abrir en eminus"),
        ("O", "abrir el curso"),
        ("/", "filtrar (esc lo quita)"),
        ("a", "mostrar entregadas o cursos anteriores"),
        ("D", "descargar los archivos de lo seleccionado"),
        ("E", "entregar la actividad seleccionada"),
        ("r", "actualizar"),
        ("c", "siguiente tema"),
        ("X", "cerrar sesión · cambiar de perfil"),
        ("q", "salir"),
        ("ratón", "clic selecciona · rueda desplaza"),
        ("", "shift/option + arrastrar para copiar texto"),
    ];
    let area = centered(area, 70, KEYS.len() as u16 + 4);
    let block = pane(t, "ayuda".into(), true).style(Style::new().bg(t.bg).fg(t.fg));
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(2, 1));
    f.render_widget(Clear, area);
    f.render_widget(block, area);
    let lines: Vec<Line> = KEYS.iter().map(|(key, what)| Line::from(vec![Span::styled(text::fit(key, 13), Style::new().fg(t.accent).add_modifier(Modifier::BOLD)), Span::raw(*what)])).collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn delivery(f: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let Some(d) = &app.delivery else { return };
    let dim = Style::new().fg(t.dim);
    let warn = Style::new().fg(t.yellow);
    let mut lines = vec![Line::styled(d.act.course_name.to_lowercase(), dim), Line::raw("")];
    match &d.info {
        Slot::Pending => lines.push(Line::styled(format!("{} consultando tu entrega en eminus…", spinner()), dim)),
        Slot::Failed(note) => lines.push(Line::styled(format!("no se pudo consultar tu entrega ({note})"), warn)),
        Slot::Ready(info) => {
            let state = if info.sent {
                "ya la entregaste: al enviar se actualiza el comentario y se agregan los archivos nuevos"
            } else if info.id > 0 {
                "tienes un borrador en eminus: se completa con lo de aquí"
            } else {
                "aún no hay entrega en eminus"
            };
            lines.push(Line::styled(state, if info.sent { warn } else { dim }));
            lines.extend(info.files.iter().map(|file| Line::from(vec![Span::styled("  ✓ ", Style::new().fg(t.green)), Span::raw(format!("{} ", file.name)), Span::styled(format!("{} · ya en eminus", file.size), dim)])));
        }
    }
    lines.extend([Line::raw(""), label(t, "archivos por subir")]);
    if d.files.is_empty() {
        lines.push(Line::styled("  ninguno todavía", dim));
    }
    lines.extend(d.files.iter().map(|(path, size)| {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Line::from(vec![Span::styled("  + ", Style::new().fg(t.accent)), Span::raw(format!("{name} ")), Span::styled(model::file_size(*size as f64), dim)])
    }));
    let area = centered(area, 84, lines.len() as u16 + 13);
    let block = pane(t, format!("entregar · {}", text::clip(&d.act.title, 60)), true).style(Style::new().bg(t.bg).fg(t.fg));
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(2, 1));
    f.render_widget(Clear, area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    let field = |name: &str, value: &str, index: usize| {
        let active = d.field == index && !d.confirm && !d.busy;
        let room = width.saturating_sub(15);
        let shown: String = value.chars().rev().take(room.saturating_sub(1)).collect::<Vec<_>>().into_iter().rev().collect();
        Line::from(vec![
            Span::styled(if active { "▌ " } else { "  " }, Style::new().fg(t.accent)),
            Span::styled(text::fit(name, 12), if active { Style::new().fg(t.accent).add_modifier(Modifier::BOLD) } else { dim }),
            Span::styled(text::fit(&format!("{shown}{}", if active { "▏" } else { "" }), room), Style::new().bg(t.sel)),
        ])
    };
    lines.push(Line::raw(""));
    lines.push(field("archivo", &d.path, 0));
    lines.push(Line::styled("              arrastra un archivo aquí o escribe su ruta · enter lo agrega", dim));
    lines.push(Line::raw(""));
    lines.push(field("comentario", &d.text, 1));
    lines.push(Line::raw(""));
    let key = |k: &str| Span::styled(format!(" {k} "), Style::new().bg(t.sel).fg(t.accent).add_modifier(Modifier::BOLD));
    if d.busy {
        lines.push(Line::styled(format!("{} {}", spinner(), d.note), Style::new().fg(t.accent)));
    } else if d.confirm {
        let files = match d.files.len() {
            0 => "sin archivos nuevos".to_string(),
            1 => "con 1 archivo".to_string(),
            n => format!("con {n} archivos"),
        };
        lines.push(Line::styled(format!("¿Entregar {files}? Se envía a tu docente en eminus."), Style::new().fg(t.orange).add_modifier(Modifier::BOLD)));
        lines.push(Line::from(vec![key("s"), Span::raw(" sí, entregar   "), key("n"), Span::raw(" volver")]));
    } else {
        if !d.note.is_empty() {
            lines.push(Line::styled(d.note.clone(), warn));
        }
        lines.push(Line::styled("enter entregar · tab cambiar de campo · ⌫ quita un archivo · esc cancelar", dim));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn offer(f: &mut Frame, t: &Theme, area: Rect, user: &str) {
    let area = centered(area, 64, 11);
    let block = pane(t, "perfiles".into(), true).style(Style::new().bg(t.bg).fg(t.fg));
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(2, 1));
    f.render_widget(Clear, area);
    f.render_widget(block, area);
    let key = |k: &str| Span::styled(format!(" {k} "), Style::new().bg(t.sel).fg(t.accent).add_modifier(Modifier::BOLD));
    let lines = vec![
        Line::from(vec![Span::raw("¿Guardar el perfil de "), Span::styled(user.to_string(), Style::new().fg(t.accent).add_modifier(Modifier::BOLD)), Span::raw("?")]),
        Line::raw(""),
        Line::styled("La contraseña queda en el llavero del sistema y la próxima vez entras sin escribirla. No lo hagas en una computadora compartida.", Style::new().fg(t.dim)),
        Line::raw(""),
        Line::from(vec![key("s"), Span::raw(" guardar   "), key("n"), Span::raw(" ahora no")]),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn picker(f: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let Some(login) = &app.login else { return };
    let dim = Style::new().fg(t.dim);
    let profiles = &app.profiles.perfiles;
    let area = centered(area, 64, profiles.len() as u16 + 9);
    let block = pane(t, "eminus · ¿quién entra?".into(), true);
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(2, 1));
    f.render_widget(block, area);
    let width = inner.width as usize;
    let row = |index: usize, number: String, name: &str, user: &str| {
        let active = login.pick == index;
        let base = if active { Style::new().bg(t.sel).add_modifier(Modifier::BOLD) } else { Style::new() };
        let user_w = text::width(user);
        Line::from(vec![
            Span::styled(if active { "▌ " } else { "  " }, base.fg(t.accent)),
            Span::styled(text::fit(&number, 3), base.fg(t.accent)),
            Span::styled(text::fit(name, width.saturating_sub(6 + user_w)), base),
            Span::styled(format!("{user} "), if active { base } else { dim }),
        ])
    };
    let mut lines: Vec<Line> = profiles.iter().enumerate().map(|(i, p)| row(i, (i + 1).to_string(), &p.name, if p.name == p.user { "" } else { &p.user })).collect();
    lines.push(Line::raw(""));
    lines.push(row(profiles.len(), "+".into(), "iniciar con otro perfil", ""));
    lines.push(Line::raw(""));
    if !login.note.is_empty() {
        let prefix = if login.busy { format!("{} ", spinner()) } else { String::new() };
        lines.push(Line::styled(format!("{prefix}{}", login.note), Style::new().fg(if login.busy { t.accent } else { t.yellow })));
    }
    lines.push(Line::styled("enter entrar · 1-9 directo · x quitar perfil · esc salir", dim));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn login(f: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let Some(login) = &app.login else { return };
    if login.picker {
        return picker(f, app, t, area);
    }
    let dim = Style::new().fg(t.dim);
    let area = centered(area, 72, 19);
    let block = pane(t, "eminus · iniciar sesión".into(), true);
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(2, 1));
    f.render_widget(block, area);
    let width = inner.width as usize;
    let field = |name: &str, value: String, index: usize| {
        let active = login.field == index;
        let room = width.saturating_sub(15);
        // Con textos largos se ve el final, que es donde se escribe.
        let shown: String = value.chars().rev().take(room.saturating_sub(1)).collect::<Vec<_>>().into_iter().rev().collect();
        Line::from(vec![
            Span::styled(if active { "▌ " } else { "  " }, Style::new().fg(t.accent)),
            Span::styled(text::fit(name, 12), if active { Style::new().fg(t.accent).add_modifier(Modifier::BOLD) } else { dim }),
            Span::styled(text::fit(&format!("{shown}{}", if active { "▏" } else { "" }), room), Style::new().bg(t.sel)),
        ])
    };
    let mut lines = vec![
        field("usuario", login.user.clone(), 0),
        Line::raw(""),
        field("contraseña", "•".repeat(login.pass.chars().count()), 1),
        Line::raw(""),
        Line::styled("  o pega un token de sesión:", dim),
        Line::raw(""),
    ];
    let masked = match login.token.chars().count() {
        0 => String::new(),
        n => format!("{}… ({n} caracteres)", login.token.chars().take(12).collect::<String>()),
    };
    lines.push(field("token", masked, 2));
    lines.push(Line::raw(""));
    lines.push(Line::styled("  (en la consola del navegador: copy(localStorage.accessToken))", dim));
    lines.push(Line::raw(""));
    lines.push(Line::styled("Al entrar puedes guardar el perfil (la contraseña va al llavero", dim));
    lines.push(Line::styled("del sistema) para no volver a escribirla.", dim));
    lines.push(Line::raw(""));
    if !login.note.is_empty() {
        let prefix = if login.busy { format!("{} ", spinner()) } else { String::new() };
        lines.push(Line::styled(format!("{prefix}{}", login.note), Style::new().fg(if login.busy { t.accent } else { t.yellow })));
    }
    lines.push(Line::styled(if app.profiles.perfiles.is_empty() { "enter entrar · tab cambiar de campo · esc salir" } else { "enter entrar · tab cambiar de campo · esc volver a los perfiles" }, dim));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
