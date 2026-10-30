pub mod bridge;
pub mod models;
pub mod device_worker;
mod device_connection;
mod device_runtime;
mod binding_persistence;
mod autostart;
pub mod updater;
pub mod macro_controller;
pub mod keybind_controller;
pub mod profile_controller;

use std::rc::Rc;
use std::sync::{Arc, Mutex};
use slint::{ComponentHandle, ModelRc, VecModel};

use bridge::{MainWindow, MacroItem, KeybindItem, ProfileItem};
use models::{update_ui_macro_list, update_ui_keybind_list, update_ui_profile_list};
use device_worker::init_device_worker;
use macro_controller::{setup_macro_controller, setup_macro_triggers, MacroControllerContext};
use keybind_controller::setup_keybind_controller;
use profile_controller::{setup_profile_controller, ProfileControllerContext};

use crate::lghub::{
    LgHubSettings, custom_macros::load_custom_macros,
    keybinds::{KeybindListener, load_bindings},
    profiles::{load_profiles, get_active_profile_id, get_active_profile, get_g9_lock},
    macro_engine,
};

pub fn run() {
    let app = match MainWindow::new() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("[!] Ошибка инициализации MainWindow: {e}");
            return;
        }
    };

    fn startup_number(state: autostart::StartupState) -> i32 {
        match state {
            autostart::StartupState::Disabled => 0,
            autostart::StartupState::Current => 1,
            autostart::StartupState::OtherCopy => 2,
            autostart::StartupState::Occupied => 3,
        }
    }
    match autostart::state() {
        Ok(state) => app.set_autostart_state(startup_number(state)),
        Err(error) => { app.set_autostart_state(-1); app.set_autostart_error(error.into()); }
    }
    let startup_weak = app.as_weak();
    app.on_set_autostart(move |enabled| {
        if let Some(app) = startup_weak.upgrade() {
            match autostart::set(enabled) {
                Ok(state) => {
                    app.set_autostart_state(startup_number(state));
                    app.set_autostart_error("".into());
                    app.set_status_msg(if enabled { "Автозагрузка включена" } else { "Автозагрузка выключена" }.into());
                }
                Err(error) => {
                    if let Ok(state) = autostart::state() { app.set_autostart_state(startup_number(state)); }
                    app.set_autostart_error(format!("Не удалось изменить автозагрузку: {error}").into());
                }
            }
        }
    });

    // Updater & Repository integration
    app.set_app_version(updater::APP_VERSION.into());
    app.set_build_git_hash(updater::BUILD_GIT_HASH.into());
    if let Ok(hash) = updater::current_executable_hash() {
        app.set_local_sha256(hash.into());
    } else {
        app.set_local_sha256("не удалось вычислить".into());
    }

    let updater_weak = app.as_weak();
    app.on_check_for_updates(move || {
        let weak = updater_weak.clone();
        if let Some(app) = weak.upgrade() {
            app.set_update_status_kind(1);
            app.set_update_status_msg("Запрос информации о релизе с GitHub...".into());
        }
        std::thread::spawn(move || {
            let result = updater::check_for_updates();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    match result {
                        Ok(info) => {
                            app.set_remote_tag(info.tag_name.clone().into());
                            app.set_release_url(info.release_url.clone().into());
                            if let Some(ref remote_hash) = info.asset_sha256 {
                                app.set_remote_sha256(remote_hash.clone().into());
                            } else {
                                app.set_remote_sha256("".into());
                            }

                            if info.is_newer {
                                app.set_update_status_kind(3);
                                app.set_update_status_msg(format!(
                                    "Доступна новая версия: {}! Нажмите «Страница релизов», чтобы скачать.",
                                    info.tag_name
                                ).into());
                            } else if info.hash_matches {
                                app.set_update_status_kind(2);
                                app.set_update_status_msg(
                                    "У вас установлена последняя официальная сборка. Хэш исполняемого файла полностью совпадает с релизом GitHub!".into()
                                );
                            } else {
                                app.set_update_status_kind(2);
                                let remote_sub = info.asset_sha256.as_deref().unwrap_or("не указан");
                                app.set_update_status_msg(format!(
                                    "Версия {} актуальна. Хэш локального файла совпадает с текущим коммитом (релиз: {:.8}...).",
                                    info.tag_name,
                                    remote_sub
                                ).into());
                            }
                        }
                        Err(err) => {
                            app.set_update_status_kind(4);
                            app.set_update_status_msg(format!("Ошибка проверки: {err}").into());
                        }
                    }
                }
            });
        });
    });

    app.on_open_github_repo(move || {
        updater::open_repository();
    });

    let release_weak = app.as_weak();
    app.on_open_releases_page(move || {
        if let Some(app) = release_weak.upgrade() {
            let url = app.get_release_url();
            let url_str = url.as_str();
            updater::open_releases(if url_str.is_empty() { None } else { Some(url_str) });
        }
    });

    // 1. Slint Models created ONCE, mutated in-place
    let macro_model = Rc::new(VecModel::<MacroItem>::default());
    app.set_macro_items(ModelRc::from(macro_model.clone()));

    let keybind_model = Rc::new(VecModel::<KeybindItem>::default());
    app.set_keybind_items(ModelRc::from(keybind_model.clone()));

    let builder_steps_model = Rc::new(VecModel::<slint::SharedString>::default());
    app.set_builder_steps(ModelRc::from(builder_steps_model.clone()));

    let profile_model = Rc::new(VecModel::<ProfileItem>::default());
    app.set_profile_items(ModelRc::from(profile_model.clone()));

    let initial_profiles = load_profiles();
    let active_prof = get_active_profile(&initial_profiles);
    let initial_dpi_presets = active_prof.dpi_presets.clone();
    let dpi_presets_model = Rc::new(VecModel::<i32>::from(
        initial_dpi_presets.iter().map(|&x| x as i32).collect::<Vec<_>>()
    ));
    app.set_dpi_presets(ModelRc::from(dpi_presets_model.clone()));
    let dpi_presets = Arc::new(Mutex::new(initial_dpi_presets));

    // 2. Data state
    let lghub_settings = Arc::new(LgHubSettings::load_from_db().ok());
    let custom_macros = Arc::new(Mutex::new(load_custom_macros()));
    let keybinds = Arc::new(Mutex::new(load_bindings()));

    let active_id = get_active_profile_id();
    app.set_active_profile_name(active_prof.name.into());
    let profiles = Arc::new(Mutex::new(initial_profiles));
    let g9_locked = Arc::new(std::sync::atomic::AtomicBool::new(get_g9_lock()));

    // 3. Populate initial models
    update_ui_macro_list(&macro_model, &lghub_settings, &custom_macros);
    update_ui_keybind_list(&keybind_model, &keybinds);
    if let Ok(profs) = profiles.lock() {
        update_ui_profile_list(&profile_model, &profs, &active_id);
    }

    // 4. Macro execution & background listener state
    let active_macro: Arc<Mutex<Option<(String, macro_engine::ActiveMacro)>>> = Arc::new(Mutex::new(None));
    let initial_binds = keybinds.lock().map(|g| g.clone()).unwrap_or_default();
    let listener = Arc::new(KeybindListener::new(initial_binds));

    // 5. Wire background trigger callback and Esc emergency panic stop
    setup_macro_triggers(&listener, &active_macro, &custom_macros, &lghub_settings, &app);

    // 6. Start keybind and macro trigger listener by default
    listener.restart();
    app.set_listener_active(listener.is_running());

    // 7. Initialize modular controllers
    let device_worker = init_device_worker(&app, &dpi_presets_model, &dpi_presets, &profiles, &g9_locked, &listener);
    setup_macro_controller(&MacroControllerContext {
        app: &app,
        macro_model: &macro_model,
        keybind_model: &keybind_model,
        builder_steps_model: &builder_steps_model,
        custom_macros: &custom_macros,
        lghub_settings: &lghub_settings,
        keybinds: &keybinds,
        active_macro: &active_macro,
        listener: &listener,
    });
    setup_keybind_controller(&app, &keybind_model, &keybinds, &listener, &profiles, &profile_model);
    setup_profile_controller(&ProfileControllerContext {
        app: &app,
        profile_model: &profile_model,
        dpi_presets_model: &dpi_presets_model,
        keybind_model: &keybind_model,
        profiles: &profiles,
        dpi_presets: &dpi_presets,
        keybinds: &keybinds,
        listener: &listener,
    });

    // 8. Run Slint application loop
    if let Err(e) = app.run() {
        eprintln!("[!] Ошибка выполнения Slint: {e}");
    }

    // 9. Clean up running macro and background listener on window close
    drop(device_worker);
    listener.stop();
    let mut slot = active_macro.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, active)) = slot.take() {
        active.stop();
    }
}
