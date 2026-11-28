use velodock::config::defaults::detect_default_pinned;
use velodock::config::Config;
use velodock::desktop::DesktopIndex;
use velodock::platform::create_window_tracker;
use velodock::trash::TrashDir;
use velodock::ui::dock_window::DockWindow;
use velodock::ui::launchpad::LaunchpadWindow;
use velodock::ui::settings_window::SettingsWindow;
use velodock::ui::{install_global_css, AppState};
use gtk4::gio::ApplicationFlags;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

struct RunningHandles {
    state: Rc<RefCell<AppState>>,
    dock: Rc<DockWindow>,
    launchpad: Rc<LaunchpadWindow>,
    on_config_changed: Rc<dyn Fn()>,
    on_toggle_launchpad: Rc<dyn Fn()>,
}

fn main() -> glib::ExitCode {

    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if raw_args.iter().any(|a| a == "--version" || a == "-V") {
        println!("velodock {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    if raw_args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "VeloDock {} — Ultra-smooth desktop Dock for Arch Linux (Wayland & X11)\n\n\
             USAGE:\n  velodock [OPTIONS]\n\n\
             OPTIONS:\n  \
               (no flags)              Start the Dock and open Settings\n  \
               -d, --dock-only         Start only the Dock silently (for Autostart)\n  \
               -s, --settings          Open/focus the Settings window\n  \
               -l, --toggle-launchpad  Toggle the fullscreen Launchpad overlay\n  \
               -V, --version           Print version information\n  \
               -h, --help              Print this help message",
            env!("CARGO_PKG_VERSION")
        );
        return glib::ExitCode::SUCCESS;
    }

    let app = gtk4::Application::builder()
        .application_id("io.github.velodock.App")
        .flags(ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    let handles: Rc<RefCell<Option<RunningHandles>>> = Rc::new(RefCell::new(None));

    let handles_c = handles.clone();
    app.connect_command_line(move |app, cmdline| {
        let args: Vec<String> = cmdline
            .arguments()
            .into_iter()
            .filter_map(|os| os.into_string().ok())
            .collect();
        let dock_only = args
            .iter()
            .any(|a| a == "--dock-only" || a == "-d" || a == "--background");
        let open_settings = args.iter().any(|a| a == "--settings" || a == "-s");
        let toggle_launchpad = args.iter().any(|a| a == "--toggle-launchpad" || a == "-l");

        if handles_c.borrow().is_none() {
            install_global_css();

            let config_path = Config::default_config_path();
            let (mut config, was_created) = Config::load_from_path(&config_path);
            let desktop_index = DesktopIndex::scan_system();
            if was_created && config.apps.pinned.is_empty() {
                config.apps.pinned = detect_default_pinned(&desktop_index.all_ids());
                let _ = config.save_to_path(&config_path);
            }

            let tracker = create_window_tracker();
            let initial_windows = tracker.snapshot();
            let (tx, rx) = async_channel::unbounded();
            tracker.start_listener(tx);

            let mut app_state = AppState::new(
                config,
                config_path,
                desktop_index,
                TrashDir::system(),
                tracker,
            );
            app_state.windows = initial_windows;
            let state = Rc::new(RefCell::new(app_state));

            let app_for_settings = app.clone();
            let state_for_settings = state.clone();
            let dock_slot: Rc<RefCell<Option<Rc<DockWindow>>>> = Rc::new(RefCell::new(None));
            let lp_slot: Rc<RefCell<Option<Rc<LaunchpadWindow>>>> = Rc::new(RefCell::new(None));

            let dock_slot_cb = dock_slot.clone();
            let lp_slot_cb = lp_slot.clone();
            let on_config_changed: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(d) = dock_slot_cb.borrow().as_ref() {
                    d.apply_config();
                }
                if let Some(lp) = lp_slot_cb.borrow().as_ref() {
                    lp.apply_config();
                }
            });

            let launchpad = LaunchpadWindow::new(app, state.clone(), on_config_changed.clone());
            *lp_slot.borrow_mut() = Some(launchpad.clone());

            let lp_for_dock = launchpad.clone();
            let on_toggle_launchpad: Rc<dyn Fn()> = Rc::new(move || {
                lp_for_dock.toggle();
            });

            let on_cfg_for_settings = on_config_changed.clone();
            let on_lp_for_settings = on_toggle_launchpad.clone();
            let on_open_settings: Rc<dyn Fn()> = Rc::new(move || {
                SettingsWindow::show(
                    &app_for_settings,
                    state_for_settings.clone(),
                    on_cfg_for_settings.clone(),
                    on_lp_for_settings.clone(),
                );
            });

            let dock = DockWindow::new(
                app,
                state.clone(),
                on_open_settings,
                on_toggle_launchpad.clone(),
            );
            *dock_slot.borrow_mut() = Some(dock.clone());
            dock.present();

            let dock_weak = Rc::downgrade(&dock);
            glib::spawn_future_local(async move {
                while let Ok(windows) = rx.recv().await {
                    if let Some(d) = dock_weak.upgrade() {
                        d.update_windows(windows);
                    } else {
                        break;
                    }
                }
            });

            *handles_c.borrow_mut() = Some(RunningHandles {
                state,
                dock,
                launchpad,
                on_config_changed,
                on_toggle_launchpad,
            });
        }

        if let Some(h) = handles_c.borrow().as_ref() {
            if toggle_launchpad {
                h.launchpad.toggle();
            } else if open_settings {
                SettingsWindow::show(
                    app,
                    h.state.clone(),
                    h.on_config_changed.clone(),
                    h.on_toggle_launchpad.clone(),
                );
            } else if dock_only {

                h.dock.present();
            } else {

                h.dock.present();
                SettingsWindow::show(
                    app,
                    h.state.clone(),
                    h.on_config_changed.clone(),
                    h.on_toggle_launchpad.clone(),
                );
            }
        }

        0.into()
    });

    app.run()
}
