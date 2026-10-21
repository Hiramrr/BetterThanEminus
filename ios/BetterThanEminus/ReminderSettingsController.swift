import ActivityKit
import UIKit
import UserNotifications

final class ReminderSettingsController: UITableViewController {
    var onChange: (() -> Void)?
    private var notificationStatus = "Consultando permiso…"
    private var pendingCount = 0

    override func viewDidLoad() {
        super.viewDidLoad()
        title = "Widgets y avisos"
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: "Listo", primaryAction: UIAction { [weak self] _ in self?.dismiss(animated: true) })
        tableView.rowHeight = UITableView.automaticDimension
        tableView.estimatedRowHeight = 60
        reloadStatus()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        reloadStatus()
    }

    private func reloadStatus() {
        Task {
            let center = UNUserNotificationCenter.current()
            let status = await center.notificationSettings().authorizationStatus
            notificationStatus = status == .denied ? "Permiso bloqueado. Ábrelo en Ajustes de iOS." : status == .authorized || status == .provisional ? "Permiso concedido" : "Activa los recordatorios para permitir avisos."
            pendingCount = await center.pendingNotificationRequests().filter { $0.identifier.hasPrefix("bte-task-") }.count
            tableView.reloadData()
        }
    }

    override func numberOfSections(in tableView: UITableView) -> Int { 3 }
    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { section == 0 ? 2 : section == 1 ? 2 : 3 }

    override func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? {
        ["Recordatorios", "Dynamic Island y widgets", "Sincronización y sesión"][section]
    }

    override func tableView(_ tableView: UITableView, titleForFooterInSection section: Int) -> String? {
        switch section {
        case 0: return "Avisos un día, dos horas y diez minutos antes. Los ya programados se entregan aunque cierres la app."
        case 1: return "La actividad muestra la próxima entrega si vence en ocho horas. Para añadir widgets, mantén pulsada la pantalla de inicio o de bloqueo y busca Eminus."
        default: return "iOS decide cuándo actualizar en segundo plano. La sesión se renueva con tu perfil guardado en el Llavero. Sin perfil, tendrás que iniciar sesión al vencer el token."
        }
    }

    override func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .subtitle, reuseIdentifier: nil)
        cell.textLabel?.numberOfLines = 0
        cell.detailTextLabel?.numberOfLines = 0
        cell.selectionStyle = .none
        switch (indexPath.section, indexPath.row) {
        case (0, 0):
            cell.textLabel?.text = "Recordatorios de entrega"
            cell.detailTextLabel?.text = "\(pendingCount) avisos programados"
            let toggle = UISwitch()
            toggle.isOn = ReminderScheduler.enabled
            toggle.accessibilityLabel = "Recordatorios de entrega"
            toggle.addAction(UIAction { [weak self, weak toggle] _ in
                guard let toggle else { return }
                let enabled = toggle.isOn
                Task {
                    if enabled {
                        _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])
                    }
                    ReminderScheduler.enabled = enabled
                    _ = await ReminderScheduler.synchronize(tasks: AgendaStore.load()?.tasks ?? [])
                    await AppExpiry.scheduleWarning()
                    self?.onChange?()
                    self?.reloadStatus()
                }
            }, for: .valueChanged)
            cell.accessoryView = toggle
        case (0, 1):
            cell.textLabel?.text = "Permisos de iOS"
            cell.detailTextLabel?.text = notificationStatus
            cell.accessoryType = .disclosureIndicator
            cell.selectionStyle = .default
        case (1, 0):
            cell.textLabel?.text = "Actividad en vivo"
            cell.detailTextLabel?.text = ActivityAuthorizationInfo().areActivitiesEnabled ? "Próxima entrega en Dynamic Island y pantalla bloqueada" : "Las actividades en vivo están desactivadas en iOS."
            let toggle = UISwitch()
            toggle.isOn = AgendaSync.liveEnabled
            toggle.accessibilityLabel = "Actividad en vivo"
            toggle.addAction(UIAction { [weak toggle] _ in
                guard let toggle else { return }
                AgendaSync.liveEnabled = toggle.isOn
                Task { await LiveActivityManager.sync(snapshot: AgendaStore.load(), enabled: AgendaSync.liveEnabled) }
            }, for: .valueChanged)
            cell.accessoryView = toggle
        case (1, 1):
            cell.textLabel?.text = "Agenda para widgets"
            cell.detailTextLabel?.text = AgendaStore.isShared ? "Disponible en pantalla de inicio y de bloqueo" : "La firma actual no permite compartir la agenda con widgets. Activa App Groups en Xcode."
        case (2, 0):
            cell.textLabel?.text = "Última actualización"
            if let snapshot = AgendaStore.load() {
                cell.detailTextLabel?.text = snapshot.updatedAt.formatted(date: .abbreviated, time: .shortened)
            } else { cell.detailTextLabel?.text = "Abre la agenda para cargar tus tareas." }
        case (2, 1):
            cell.textLabel?.text = "Segundo plano"
            cell.detailTextLabel?.text = UserDefaults.standard.string(forKey: "bte.sync.error") ?? (UIApplication.shared.backgroundRefreshStatus == .available ? "Permitido por iOS" : "Desactivado o restringido por iOS")
        default:
            cell.textLabel?.text = "Actualizar ahora"
            cell.textLabel?.textColor = .link
            cell.selectionStyle = .default
        }
        return cell
    }

    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        if indexPath.section == 0 && indexPath.row == 1, let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
        if indexPath.section == 2 && indexPath.row == 2 {
            Task {
                _ = await AgendaSync.refresh()
                reloadStatus()
            }
        }
    }
}
