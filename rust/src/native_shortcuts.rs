// Atajos Command/Ctrl de la ventana (egui 0.29): parser puro, sin GUI ni App.
//
// Gui procesa estas acciones antes de los campos de texto y respeta el
// diálogo activo. Los eventos de edición quedan disponibles para egui.

use eframe::egui;

/// Acción nativa pedida por el usuario vía Command (macOS) o Ctrl (Win/Linux,
/// donde egui pone `command = true`). `View(n)` usa índice 0..4 para
/// `View::ALL` (Cmd+1 → 0 … Cmd+5 → 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    Close,
    Preferences,
    Find,
    Refresh,
    SaveDraft,
    Submit,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    View(usize),
}

/// Traduce un `egui::Event` a `Action`, o `None` si no es un atajo soportado.
///
/// Reglas: sólo `Key { pressed: true }` con `command || mac_cmd` (así vale el
/// Ctrl de Win/Linux que egui mapea a `command`) y sin `alt`. No intercepta
/// edición de texto: Cmd+A/C/X/V/Z (incluido Shift+Z = rehacer) ni los eventos
/// `Text`/`Copy`/`Cut`/`Paste` devuelven algo (siempre `None`), para que
/// `TextEdit` y la selección sigan funcionando.
pub fn action(event: &egui::Event) -> Option<Action> {
    let egui::Event::Key {
        key,
        pressed,
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    if !*pressed {
        return None;
    }
    if !(modifiers.command || modifiers.mac_cmd) {
        return None;
    }
    if modifiers.alt {
        return None;
    }
    // Zoom-in acepta Plus o Equals, con o sin Shift (en teclado US, Cmd+= es
    // Equals y Cmd+Shift+= llega como Plus según el layout).
    if matches!(key, egui::Key::Plus | egui::Key::Equals) {
        return Some(Action::ZoomIn);
    }
    // Fuera del zoom, Shift cambia el gesto (p. ej. Cmd+Shift+S no es guardar
    // borrador), así que no se reclama.
    if modifiers.shift {
        return None;
    }
    match key {
        egui::Key::Q => Some(Action::Quit),
        egui::Key::W => Some(Action::Close),
        egui::Key::Comma => Some(Action::Preferences),
        egui::Key::F => Some(Action::Find),
        egui::Key::R => Some(Action::Refresh),
        // Borrador local: guarda sin enviar (el envío sigue pidiendo
        // confirmación vía Submit).
        egui::Key::S => Some(Action::SaveDraft),
        // Cmd+Enter no entrega directo: sólo pide la confirmación de entrega.
        egui::Key::Enter => Some(Action::Submit),
        egui::Key::Minus => Some(Action::ZoomOut),
        egui::Key::Num0 => Some(Action::ZoomReset),
        egui::Key::Num1 => Some(Action::View(0)),
        egui::Key::Num2 => Some(Action::View(1)),
        egui::Key::Num3 => Some(Action::View(2)),
        egui::Key::Num4 => Some(Action::View(3)),
        egui::Key::Num5 => Some(Action::View(4)),
        // Edición/selección: se dejan a egui a propósito.
        egui::Key::A
        | egui::Key::C
        | egui::Key::X
        | egui::Key::V
        | egui::Key::Z
        | egui::Key::Copy
        | egui::Key::Cut
        | egui::Key::Paste => None,
        _ => None,
    }
}

pub fn normalize_editing(event: &mut egui::Event) {
    if let egui::Event::Key { key, pressed: true, modifiers, .. } = event {
        if *key == egui::Key::Z && modifiers.shift && (modifiers.command || modifiers.mac_cmd) && !modifiers.alt {
            // egui 0.29 interpreta Cmd+Shift+Z como deshacer antes de revisar
            // rehacer. Su gesto Cmd+Y sí llega al historial de rehacer.
            *key = egui::Key::Y;
            modifiers.shift = false;
            modifiers.command = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, Key, Modifiers};

    fn cmd() -> Modifiers {
        Modifiers::COMMAND
    }

    fn mac() -> Modifiers {
        Modifiers::MAC_CMD
    }

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn every_shortcut_maps() {
        let cases = [
            (Key::Q, Action::Quit),
            (Key::W, Action::Close),
            (Key::Comma, Action::Preferences),
            (Key::F, Action::Find),
            (Key::R, Action::Refresh),
            (Key::S, Action::SaveDraft),
            (Key::Enter, Action::Submit),
            (Key::Plus, Action::ZoomIn),
            (Key::Equals, Action::ZoomIn),
            (Key::Minus, Action::ZoomOut),
            (Key::Num0, Action::ZoomReset),
            (Key::Num1, Action::View(0)),
            (Key::Num2, Action::View(1)),
            (Key::Num3, Action::View(2)),
            (Key::Num4, Action::View(3)),
            (Key::Num5, Action::View(4)),
        ];
        for (k, expected) in cases {
            assert_eq!(action(&key(k, cmd())), Some(expected), "{k:?} con command");
            assert_eq!(action(&key(k, mac())), Some(expected), "{k:?} con mac_cmd");
            // En Win/Linux egui pone ctrl y command juntos: también vale.
            let both = cmd() | Modifiers::CTRL;
            assert_eq!(
                action(&key(k, both)),
                Some(expected),
                "{k:?} con ctrl+command"
            );
        }
    }

    #[test]
    fn shift_plus_or_equals_still_zooms_in() {
        let shift_cmd = cmd() | Modifiers::SHIFT;
        assert_eq!(action(&key(Key::Equals, shift_cmd)), Some(Action::ZoomIn));
        assert_eq!(action(&key(Key::Plus, shift_cmd)), Some(Action::ZoomIn));
    }

    #[test]
    fn bare_keys_and_ctrl_without_command_are_rejected() {
        // Teclas solas, sin modificador.
        for k in [Key::Q, Key::S, Key::F, Key::Enter, Key::Num1] {
            assert_eq!(action(&key(k, Modifiers::NONE)), None, "{k:?} sola");
        }
        // Ctrl solo sin command (integración que no mapea a command): no vale.
        for k in [Key::Q, Key::S, Key::F, Key::Enter, Key::Num1] {
            assert_eq!(action(&key(k, Modifiers::CTRL)), None, "{k:?} ctrl-only");
        }
        // Releases se ignoran aunque el modificador sea correcto.
        let released = Event::Key {
            key: Key::Q,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: cmd(),
        };
        assert_eq!(action(&released), None);
        // Alt no soportado.
        let alt_cmd = cmd() | Modifiers::ALT;
        assert_eq!(action(&key(Key::Q, alt_cmd)), None);
        assert_eq!(action(&key(Key::S, alt_cmd)), None);
    }

    #[test]
    fn editing_keys_and_text_events_pass_through() {
        // Cmd+A/C/X/V/Z y rehacer (Cmd+Shift+Z) siguen en manos de egui.
        for k in [Key::A, Key::C, Key::X, Key::V, Key::Z] {
            assert_eq!(action(&key(k, cmd())), None, "{k:?} edición");
            assert_eq!(action(&key(k, mac())), None, "{k:?} edición mac");
        }
        let shift_cmd = cmd() | Modifiers::SHIFT;
        assert_eq!(action(&key(Key::Z, shift_cmd)), None, "rehacer");
        // Eventos de texto/portapapeles nunca son atajos.
        assert_eq!(action(&Event::Text("q".into())), None);
        assert_eq!(action(&Event::Copy), None);
        assert_eq!(action(&Event::Cut), None);
        assert_eq!(action(&Event::Paste("x".into())), None);
        // Teclas sintéticas de portapapeles con command tampoco se reclaman.
        for k in [Key::Copy, Key::Cut, Key::Paste] {
            assert_eq!(action(&key(k, cmd())), None, "{k:?}");
        }
        // Mayús no convierte guardar en otra cosa: Cmd+Shift+S no es borrador.
        assert_eq!(action(&key(Key::S, shift_cmd)), None);
    }
}
