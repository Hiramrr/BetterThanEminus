// BetterThanEminus nativo: la vista tui de la extensión como programa de
// terminal. Un solo hilo dibuja y atiende el teclado; la red va aparte.

mod api;
mod app;
mod model;
mod store;
mod text;
mod theme;
mod ui;

use ratatui::crossterm::event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyEventKind};
use ratatui::crossterm::execute;
use std::io::{self, stdout, IsTerminal};

const HELP: &str = "\
eminus: agenda, cursos, miyu, calificaciones y contenido de Eminus en la terminal

uso:
  eminus            abre la app (? muestra los atajos)
  eminus --version
  eminus --help

sesión:
  La app pide el token de tu sesión de Eminus la primera vez y lo guarda
  sólo en este equipo. También lo toma de la variable EMINUS_TOKEN.
";

fn run(terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
    let mut app = app::App::new();
    while !app.quit {
        while let Ok((epoch, msg)) = app.pool.inbox.try_recv() {
            app.on_msg(epoch, msg);
        }
        app.tick();
        // Sólo se pinta cuando algo cambió; ratatui además manda a la
        // terminal únicamente las celdas distintas.
        if app.dirty {
            app.dirty = false;
            terminal.draw(|f| ui::draw(f, &mut app))?;
        }
        if !event::poll(app.wait())? {
            // Reloj, cuenta regresiva o indicador de carga.
            app.dirty = true;
            continue;
        }
        // Todo lo que ya llegó se atiende antes de volver a pintar: con una
        // tecla sostenida no se dibuja un cuadro por repetición.
        loop {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                Event::Paste(pasted) => app.on_paste(&pasted),
                Event::Resize(..) => app.dirty = true,
                _ => {}
            }
            if app.quit || !event::poll(std::time::Duration::ZERO)? {
                break;
            }
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("eminus {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(_) => {
            print!("{HELP}");
            return Ok(());
        }
        None => {}
    }
    if !stdout().is_terminal() {
        eprintln!("eminus necesita una terminal interactiva");
        std::process::exit(1);
    }
    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture, EnableBracketedPaste)?;
    let result = run(&mut terminal);
    let _ = execute!(stdout(), DisableMouseCapture, DisableBracketedPaste);
    ratatui::restore();
    result
}

// Pinta cada vista contra un Eminus falso (EMINUS_ORIGIN) y la imprime:
//   EMINUS_ORIGIN=http://127.0.0.1:8765 EMINUS_TOKEN=… cargo test -- --ignored --nocapture
#[cfg(test)]
#[test]
#[ignore]
fn pantallas() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = app::App::new();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(150, 38)).unwrap();
    let mut settle = |app: &mut app::App, label: &str| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            while let Ok((epoch, msg)) = app.pool.inbox.try_recv() {
                app.on_msg(epoch, msg);
            }
            app.tick();
            if app.pool.pending == 0 && app.wait() >= std::time::Duration::from_secs(1) || std::time::Instant::now() > until {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        terminal.draw(|f| ui::draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer();
        println!("\n══ {label} ══");
        for y in 0..buffer.area.height {
            let line: String = (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect();
            println!("{}", line.trim_end());
        }
    };
    let press = |app: &mut app::App, keys: &str| keys.chars().for_each(|c| app.on_key(KeyEvent::new(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }, KeyModifiers::NONE)));
    settle(&mut app, "agenda");
    press(&mut app, "ajj");
    settle(&mut app, "agenda · todo, tercera fila");
    press(&mut app, "/red\n");
    settle(&mut app, "agenda filtrada");
    press(&mut app, "2l");
    settle(&mut app, "cursos");
    press(&mut app, "3");
    settle(&mut app, "miyu");
    press(&mut app, "4");
    settle(&mut app, "calificadas");
    press(&mut app, "5j");
    settle(&mut app, "contenido");
    press(&mut app, "D");
    settle(&mut app, "contenido tras D");
    press(&mut app, "?");
    settle(&mut app, "ayuda");
    press(&mut app, "?X");
    settle(&mut app, "login");
}
