import SwiftUI
import UIKit

/// Colores de la base de diseño: superficies planas, un solo acento y
/// colores de estado sólo para lo que vence pronto.
enum WidgetPalette {
    static let background = dynamic(light: 0xFFFFFF, dark: 0x222222)
    static let text = dynamic(light: 0x222222, dark: 0xEEEEEE)
    static let secondary = dynamic(light: 0x595959, dark: 0xBBBBBB)
    static let separator = dynamic(light: 0xDDDDDD, dark: 0x444444)
    static let accent = dynamic(light: 0x234A99, dark: 0x9DBFFF)
    static let urgent = dynamic(light: 0xB3261E, dark: 0xFF8A80)
    static let soon = dynamic(light: 0x8A5300, dark: 0xFFC46B)

    private static func dynamic(light: UInt32, dark: UInt32) -> Color {
        Color(uiColor: UIColor { $0.userInterfaceStyle == .dark ? rgb(dark) : rgb(light) })
    }

    private static func rgb(_ hex: UInt32) -> UIColor {
        UIColor(red: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }
}

/// Qué tan cerca está una entrega. El texto siempre dice la hora; el color
/// sólo la refuerza.
enum Urgency {
    case urgent, soon, later

    init(_ deadline: Date, now: Date) {
        let left = deadline.timeIntervalSince(now)
        self = left <= 2 * 3600 ? .urgent : left <= 24 * 3600 ? .soon : .later
    }

    var color: Color {
        switch self {
        case .urgent: WidgetPalette.urgent
        case .soon: WidgetPalette.soon
        case .later: WidgetPalette.secondary
        }
    }
}

enum DueFormat {
    /// La app sólo está en español; los widgets también, sin importar el idioma del teléfono.
    static let locale = Locale(identifier: "es_MX")

    private static func formatter(_ format: String) -> DateFormatter {
        let f = DateFormatter()
        f.locale = locale
        f.dateFormat = format
        return f
    }

    private static let time = formatter("HH:mm")
    private static let weekday = formatter("EEE HH:mm")
    private static let day = formatter("d MMM HH:mm")
    private static let shortDay = formatter("d MMM")

    /// "hoy 23:59", "mañana 08:00", "jue 23:59" o "12 nov 23:59".
    static func label(_ date: Date, now: Date) -> String {
        let cal = Calendar.current
        if cal.isDate(date, inSameDayAs: now) { return "hoy " + time.string(from: date) }
        if let tomorrow = cal.date(byAdding: .day, value: 1, to: now), cal.isDate(date, inSameDayAs: tomorrow) {
            return "mañana " + time.string(from: date)
        }
        if date.timeIntervalSince(now) < 6 * 86400 { return weekday.string(from: date).replacingOccurrences(of: ".", with: "") }
        return day.string(from: date).replacingOccurrences(of: ".", with: "")
    }

    /// "act. 10:32" o "act. 18 oct".
    static func updated(_ date: Date, now: Date) -> String {
        let text = Calendar.current.isDate(date, inSameDayAs: now) ? time.string(from: date) : shortDay.string(from: date)
        return "act. " + text.replacingOccurrences(of: ".", with: "")
    }

    static func timeOnly(_ date: Date) -> String { time.string(from: date) }
}

/// "en 3 h 20 min" mientras falte menos de un día; después, la fecha.
struct DueText: View {
    let deadline: Date
    let now: Date

    var body: some View {
        if deadline.timeIntervalSince(now) < 24 * 3600 {
            Text("en \(Text(deadline, style: .relative))")
        } else {
            Text(DueFormat.label(deadline, now: now))
        }
    }
}
