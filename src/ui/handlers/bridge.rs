use crate::app::DeploymentManager;
use crate::config::{AppType, DeploymentConfig};
use crate::utils::{format_env_string, parse_env_string};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Include compiled Slint UI modules
slint::include_modules!();

pub fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let Ok(num) = u32::from_str_radix(hex, 16) {
            let r = ((num >> 16) & 0xFF) as u8;
            let g = ((num >> 8) & 0xFF) as u8;
            let b = (num & 0xFF) as u8;
            return Color::from_argb_u8(255, r, g, b);
        }
    }
    Color::from_argb_u8(255, 148, 163, 184)
}

pub struct UiBridge {
    window: MainWindow,
    manager: Arc<Mutex<DeploymentManager>>,
}

impl UiBridge {
    pub fn new(window: MainWindow, manager: Arc<Mutex<DeploymentManager>>) -> Self {
        Self { window, manager }
    }

    pub fn setup_handlers(self) {
        let window = self.window.clone_strong();
        let manager = self.manager.clone();

        // Initial UI population
        Self::refresh_ui(&window, &manager);

        // Sidebar toggle callback
        {
            window.on_toggle_sidebar(move || {
                // UI state root.sidebar_collapsed is directly toggled in Slint
            });
        }

        // Open Dashboard callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_open_dashboard(move || {
                if let Some(w) = w.upgrade() {
                    w.set_active_view(0);
                    Self::refresh_ui(&w, &mgr);
                }
            });
        }

        // Open Create Form callback
        {
            let w = window.as_weak();
            window.on_open_create_form(move || {
                if let Some(w) = w.upgrade() {
                    w.set_active_view(2);
                    w.set_form_is_new(true);
                    w.set_form_id(SharedString::from(""));
                    w.set_form_name(SharedString::from(""));
                    w.set_form_type(SharedString::from("FastAPI (Python)"));
                    w.set_form_work_dir(SharedString::from(""));
                    w.set_form_command(SharedString::from(AppType::FastApi.default_command()));
                    w.set_form_env_text(SharedString::from("PORT=8000\nPYTHONUNBUFFERED=1"));
                    w.set_form_auto_restart(false);
                }
            });
        }

        // App type preset selected callback
        {
            let w = window.as_weak();
            window.on_app_type_selected(move |app_type_str| {
                if let Some(w) = w.upgrade() {
                    let t = AppType::from_str(&app_type_str);
                    w.set_form_command(SharedString::from(t.default_command()));
                    match t {
                        AppType::FastApi => {
                            w.set_form_env_text(SharedString::from("PORT=8000\nPYTHONUNBUFFERED=1"));
                        }
                        AppType::GoBinary => {
                            w.set_form_env_text(SharedString::from("PORT=8080\nENV=development"));
                        }
                        AppType::ReactApp => {
                            w.set_form_env_text(SharedString::from("PORT=3000\nNODE_ENV=development"));
                        }
                        AppType::Custom => {}
                    }
                }
            });
        }

        // Select deployment from sidebar
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_select_deployment_item(move |id| {
                if let Some(w) = w.upgrade() {
                    let mgr_lock = mgr.lock().unwrap();
                    if let Some(config) = mgr_lock.get_deployment(&id) {
                        w.set_selected_deployment_id(SharedString::from(&config.id));
                        w.set_selected_deployment_name(SharedString::from(&config.name));
                        w.set_selected_is_running(mgr_lock.is_running(&config.id));
                        w.set_active_view(1); // switch to logs view
                        drop(mgr_lock);
                        Self::refresh_logs(&w, &mgr, &id);
                    }
                }
            });
        }

        // Start deployment callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_start_app(move |id| {
                let res = {
                    let mut mgr_lock = mgr.lock().unwrap();
                    mgr_lock.start(&id)
                };
                if let Err(e) = res {
                    eprintln!("Failed to start deployment {}: {}", id, e);
                }
                if let Some(w) = w.upgrade() {
                    Self::refresh_ui(&w, &mgr);
                }
            });
        }

        // Stop deployment callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_stop_app(move |id| {
                let res = {
                    let mut mgr_lock = mgr.lock().unwrap();
                    mgr_lock.stop(&id)
                };
                if let Err(e) = res {
                    eprintln!("Failed to stop deployment {}: {}", id, e);
                }
                if let Some(w) = w.upgrade() {
                    Self::refresh_ui(&w, &mgr);
                }
            });
        }

        // Restart deployment callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_restart_app(move |id| {
                let res = {
                    let mut mgr_lock = mgr.lock().unwrap();
                    mgr_lock.restart(&id)
                };
                if let Err(e) = res {
                    eprintln!("Failed to restart deployment {}: {}", id, e);
                }
                if let Some(w) = w.upgrade() {
                    Self::refresh_ui(&w, &mgr);
                }
            });
        }

        // View logs callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_view_logs(move |id| {
                if let Some(w) = w.upgrade() {
                    let mgr_lock = mgr.lock().unwrap();
                    if let Some(config) = mgr_lock.get_deployment(&id) {
                        w.set_selected_deployment_id(SharedString::from(&config.id));
                        w.set_selected_deployment_name(SharedString::from(&config.name));
                        w.set_selected_is_running(mgr_lock.is_running(&config.id));
                        w.set_active_view(1); // Logs view
                        drop(mgr_lock);
                        Self::refresh_logs(&w, &mgr, &id);
                    }
                }
            });
        }

        // Clear logs callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_clear_logs_clicked(move |id| {
                {
                    let mut mgr_lock = mgr.lock().unwrap();
                    mgr_lock.clear_logs(&id);
                }
                if let Some(w) = w.upgrade() {
                    Self::refresh_logs(&w, &mgr, &id);
                }
            });
        }

        // Edit deployment callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_edit_app(move |id| {
                if let Some(w) = w.upgrade() {
                    let mgr_lock = mgr.lock().unwrap();
                    if let Some(config) = mgr_lock.get_deployment(&id) {
                        w.set_form_is_new(false);
                        w.set_form_id(SharedString::from(&config.id));
                        w.set_form_name(SharedString::from(&config.name));
                        w.set_form_type(SharedString::from(config.app_type.as_str()));
                        w.set_form_work_dir(SharedString::from(&config.work_dir));
                        w.set_form_command(SharedString::from(&config.command));
                        w.set_form_env_text(SharedString::from(format_env_string(&config.env_vars)));
                        w.set_form_auto_restart(config.auto_restart);
                        w.set_active_view(2); // Form view
                    }
                }
            });
        }

        // Save deployment callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_save_deployment(move |id, name, app_type_str, work_dir, command, env_text, auto_restart| {
                let id_str = if id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    id.to_string()
                };

                let config = DeploymentConfig {
                    id: id_str,
                    name: if name.trim().is_empty() { "Untitled App".to_string() } else { name.to_string() },
                    app_type: AppType::from_str(&app_type_str),
                    work_dir: work_dir.to_string(),
                    command: command.to_string(),
                    env_vars: parse_env_string(&env_text),
                    auto_restart,
                    port: None,
                    created_at: chrono::Local::now().to_rfc3339(),
                };

                {
                    let mut mgr_lock = mgr.lock().unwrap();
                    let _ = mgr_lock.add_or_update(config);
                }

                if let Some(w) = w.upgrade() {
                    w.set_active_view(0); // return to dashboard
                    Self::refresh_ui(&w, &mgr);
                }
            });
        }

        // Delete deployment callback
        {
            let w = window.as_weak();
            let mgr = manager.clone();
            window.on_delete_app(move |id| {
                {
                    let mut mgr_lock = mgr.lock().unwrap();
                    let _ = mgr_lock.delete_deployment(&id);
                }
                if let Some(w) = w.upgrade() {
                    w.set_active_view(0);
                    Self::refresh_ui(&w, &mgr);
                }
            });
        }

        // Background polling timer for process exits and real-time streaming logs
        let timer = slint::Timer::default();
        let w_weak = window.as_weak();
        let mgr_bg = manager;

        timer.start(slint::TimerMode::Repeated, Duration::from_millis(250), move || {
            let (status_changed, new_logs) = {
                let mut mgr_lock = mgr_bg.lock().unwrap();
                let new_logs = mgr_lock.tick();
                (!new_logs.is_empty(), new_logs)
            };

            if let Some(w) = w_weak.upgrade() {
                if status_changed {
                    Self::refresh_ui(&w, &mgr_bg);
                }

                let current_id = w.get_selected_deployment_id().to_string();
                if !current_id.is_empty() && new_logs.contains_key(&current_id) {
                    Self::refresh_logs(&w, &mgr_bg, &current_id);
                }
            }
        });

        // Keep timer alive
        Box::leak(Box::new(timer));
    }

    pub fn refresh_ui(window: &MainWindow, manager: &Arc<Mutex<DeploymentManager>>) {
        let mgr = manager.lock().unwrap();
        let deployments = mgr.all_deployments();

        let mut sidebar_items = Vec::new();
        let mut cards = Vec::new();

        for d in &deployments {
            let status = mgr.get_status(&d.id);
            let status_color = hex_to_color(status.hex_color());
            let is_running = status.is_running();

            sidebar_items.push(SidebarItemData {
                id: SharedString::from(&d.id),
                name: SharedString::from(&d.name),
                app_type: SharedString::from(d.app_type.as_str()),
                status_text: SharedString::from(status.badge_text()),
                status_color,
            });

            cards.push(DeploymentCardData {
                id: SharedString::from(&d.id),
                name: SharedString::from(&d.name),
                app_type: SharedString::from(d.app_type.as_str()),
                work_dir: SharedString::from(&d.work_dir),
                command: SharedString::from(&d.command),
                status_text: SharedString::from(status.badge_text()),
                status_color,
                dot_color: status_color,
                is_running,
            });
        }

        window.set_sidebar_items(ModelRc::new(VecModel::from(sidebar_items)));
        window.set_dashboard_cards(ModelRc::new(VecModel::from(cards)));
    }

    pub fn refresh_logs(window: &MainWindow, manager: &Arc<Mutex<DeploymentManager>>, id: &str) {
        let mgr = manager.lock().unwrap();
        let entries = mgr.get_logs(id);

        let lines: Vec<LogLineData> = entries
            .into_iter()
            .map(|e| LogLineData {
                timestamp: SharedString::from(e.timestamp),
                stream: SharedString::from(e.log_type.as_str()),
                message: SharedString::from(e.message),
                line_color: hex_to_color(e.log_type.color_hex()),
            })
            .collect();

        window.set_current_logs(ModelRc::new(VecModel::from(lines)));
        window.set_selected_is_running(mgr.is_running(id));
    }
}

