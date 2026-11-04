use slint::{ComponentHandle, VecModel};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use super::bridge::{KeybindItem, MacroItem, MainWindow, ProfileItem};
use super::macro_persistence::MacroEditState;
use super::models::{update_ui_keybind_list, update_ui_macro_list, update_ui_profile_list};
use crate::lghub::{
    AppProfile, LgHubSettings, LgMacro,
    keybinds::{ButtonBinding, KeybindListener},
    macro_engine::{MacroPlayer, PlaybackChange},
};

fn find_macro(
    name: &str,
    custom: &Mutex<Vec<LgMacro>>,
    settings: &Option<LgHubSettings>,
) -> Option<LgMacro> {
    let name = name.trim();
    custom
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|m| m.name.trim() == name)
        .cloned()
        .or_else(|| {
            settings.as_ref()?.cards.iter().find_map(|card| {
                let data = card.macro_data.as_ref()?;
                (data.name.trim() == name || card.name.as_deref().is_some_and(|n| n.trim() == name))
                    .then(|| data.clone())
            })
        })
}

fn toggle_macro(
    app: &MainWindow,
    player: &MacroPlayer,
    listener: &KeybindListener,
    data: LgMacro,
    generation: u64,
    from_trigger: bool,
) {
    let name = data.name.clone();
    let change = if from_trigger {
        player.trigger(data, generation)
    } else {
        player.toggle(data, generation)
    };
    match change {
        PlaybackChange::Started => {
            app.set_active_macro_name(name.clone().into());
            app.set_status_msg(format!("Запущен макрос '{name}'").into());
        }
        PlaybackChange::Stopped => {
            listener.invalidate_pending_events();
            app.set_active_macro_name("".into());
            app.set_status_msg(format!("Макрос '{name}' остановлен").into());
        }
        PlaybackChange::Stale | PlaybackChange::Released => {}
    }
}

pub fn setup_macro_triggers(
    listener: &Arc<KeybindListener>,
    player: &Arc<MacroPlayer>,
    custom: &Arc<Mutex<Vec<LgMacro>>>,
    settings: &Arc<Option<LgHubSettings>>,
    app: &MainWindow,
) {
    let weak = app.as_weak();
    let listener_weak = Arc::downgrade(listener);
    let runtime = player.clone();
    let custom = custom.clone();
    let settings = settings.clone();
    listener.set_transition_callback(move |name, event_generation, down, source| {
        let Some(listener) = listener_weak.upgrade() else {
            return;
        };
        if !listener.is_running() {
            return;
        }
        let generation = runtime.generation();
        let runtime = runtime.clone();
        let listener_weak = listener_weak.clone();
        let weak = weak.clone();
        let custom = custom.clone();
        let settings = settings.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(listener) = listener_weak.upgrade() else {
                return;
            };
            if !listener.is_running()
                || listener.event_generation() != event_generation
                || runtime.generation() != generation
            {
                return;
            }
            let Some(app) = weak.upgrade() else {
                return;
            };
            if !down {
                runtime.release(&source, generation);
                return;
            }
            match name.as_str() {
                "__PROFILE_NEXT__" => app.invoke_next_app_profile(),
                "__DPI_UP__" => app.invoke_dpi_up(),
                "__DPI_DOWN__" => app.invoke_dpi_down(),
                _ => {
                    if let Some(data) = find_macro(&name, &custom, &settings) {
                        let macro_name = data.name.clone();
                        match runtime.press(data, generation, source) {
                            PlaybackChange::Started => {
                                app.set_active_macro_name(macro_name.into());
                                app.set_status_msg("Макрос запущен".into());
                            }
                            PlaybackChange::Stopped => {
                                listener.invalidate_pending_events();
                                app.set_active_macro_name("".into());
                                app.set_status_msg("Макрос остановлен".into());
                            }
                            PlaybackChange::Stale | PlaybackChange::Released => {}
                        }
                    } else {
                        app.set_status_msg(format!("Макрос '{name}' не найден").into());
                    }
                }
            }
        });
    });

    // Stop on the panic worker, without waiting for the UI event loop.
    let runtime = player.clone();
    let weak = app.as_weak();
    listener.set_panic_callback(move || {
        let generation = runtime.stop();
        let runtime = runtime.clone();
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if runtime.is_idle_at(generation)
                && let Some(app) = weak.upgrade()
            {
                app.set_active_macro_name("".into());
                app.set_status_msg("⛔ Экстренная остановка (Esc): макросы остановлены".into());
            }
        });
    });

    let runtime = player.clone();
    let weak = app.as_weak();
    listener.set_context_cancel(move || {
        let generation = runtime.stop();
        let runtime = runtime.clone();
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if runtime.is_idle_at(generation)
                && let Some(app) = weak.upgrade()
            {
                app.set_active_macro_name("".into());
            }
        });
    });
}

pub struct MacroControllerContext<'a> {
    pub app: &'a MainWindow,
    pub macro_model: &'a Rc<VecModel<MacroItem>>,
    pub keybind_model: &'a Rc<VecModel<KeybindItem>>,
    pub builder_steps_model: &'a Rc<VecModel<slint::SharedString>>,
    pub custom_macros: &'a Arc<Mutex<Vec<LgMacro>>>,
    pub lghub_settings: &'a Arc<Option<LgHubSettings>>,
    pub keybinds: &'a Arc<Mutex<Vec<ButtonBinding>>>,
    pub player: &'a Arc<MacroPlayer>,
    pub profiles: &'a Arc<Mutex<Vec<AppProfile>>>,
    pub profile_model: &'a Rc<VecModel<ProfileItem>>,
    pub listener: &'a Arc<KeybindListener>,
}

pub fn setup_macro_controller(ctx: &MacroControllerContext) -> slint::Timer {
    let app = ctx.app;
    let macro_model = ctx.macro_model;
    let keybind_model = ctx.keybind_model;
    let builder_steps_model = ctx.builder_steps_model;
    let custom_macros = ctx.custom_macros;
    let lghub_settings = ctx.lghub_settings;
    let keybinds = ctx.keybinds;
    let listener = ctx.listener;
    let edits = MacroEditState {
        custom: custom_macros.clone(),
        profiles: ctx.profiles.clone(),
        bindings: keybinds.clone(),
        listener: listener.clone(),
        player: ctx.player.clone(),
        imported: lghub_settings.clone(),
        store: crate::lghub::profile_store::ProfileStore::user(),
        macros_path: crate::lghub::storage::get_storage_path("user_macros.json"),
    };

    let runtime = ctx.player.clone();
    let weak = app.as_weak();
    let listener_ref = listener.clone();
    app.on_stop_all_macros(move || {
        listener_ref.invalidate_pending_events();
        runtime.stop();
        if let Some(app) = weak.upgrade() {
            app.set_active_macro_name("".into());
            app.set_status_msg("Все макросы остановлены".into());
        }
    });

    let runtime = ctx.player.clone();
    let custom = custom_macros.clone();
    let settings = lghub_settings.clone();
    let weak = app.as_weak();
    let listener_ref = listener.clone();
    app.on_toggle_macro(move |name| {
        let generation = runtime.generation();
        let Some(app) = weak.upgrade() else {
            return;
        };
        if let Some(data) = find_macro(name.as_str(), &custom, &settings) {
            toggle_macro(&app, &runtime, &listener_ref, data, generation, false);
        } else {
            app.set_status_msg(format!("Макрос '{name}' не найден").into());
        }
    });

    let completion_timer = slint::Timer::default();
    let runtime = ctx.player.clone();
    let weak = app.as_weak();
    completion_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            if let Some(name) = runtime.take_finished()
                && let Some(app) = weak.upgrade()
            {
                app.set_active_macro_name("".into());
                app.set_status_msg(format!("Макрос '{name}' выполнен").into());
            }
        },
    );

    let builder = super::macro_draft::setup(
        app,
        builder_steps_model.clone(),
        custom_macros.clone(),
        lghub_settings.clone(),
        listener.clone(),
        ctx.player.clone(),
    );

    // Save edits and every profile reference before publishing new UI state.
    let app_weak = app.as_weak();
    let custom_arc = custom_macros.clone();
    let settings_arc = lghub_settings.clone();
    let keybinds_arc = keybinds.clone();
    let macro_model_ref = macro_model.clone();
    let keybind_model_ref = keybind_model.clone();
    let profile_model = ctx.profile_model.clone();
    let save_edits = edits.clone();
    app.on_builder_save_macro(move |orig_name, name, _is_toggle| {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        let name_str = name.trim().to_string();
        app.set_builder_error("".into());
        let new_macro = match builder.build(&app, &name_str) {
            Ok(data) => data,
            Err(error) => {
                app.set_builder_error(error.into());
                return;
            }
        };

        match save_edits.save(
            orig_name.as_str(),
            new_macro,
            app.get_active_profile_id().as_str(),
        ) {
            Ok(stopped) => {
                if stopped {
                    app.set_active_macro_name("".into());
                }
                update_ui_macro_list(&macro_model_ref, &settings_arc, &custom_arc);
                update_ui_keybind_list(&keybind_model_ref, &keybinds_arc);
                let profiles = save_edits
                    .profiles
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                update_ui_profile_list(
                    &profile_model,
                    &profiles,
                    app.get_active_profile_id().as_str(),
                );
                app.invoke_close_macro_builder();
                app.set_status_msg(format!("Макрос '{name_str}' сохранён").into());
            }
            Err(error) => {
                app.set_builder_error(error.clone().into());
                app.set_status_msg(format!("Макрос не сохранён: {error}").into());
            }
        }
    });

    let app_weak = app.as_weak();
    let custom_arc = custom_macros.clone();
    let settings_arc = lghub_settings.clone();
    let keybinds_arc = keybinds.clone();
    let macro_model_ref = macro_model.clone();
    let keybind_model_ref = keybind_model.clone();
    let profile_model = ctx.profile_model.clone();
    app.on_delete_custom_macro(move |name| {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        match edits.delete(name.as_str(), app.get_active_profile_id().as_str()) {
            Ok(stopped) => {
                if stopped {
                    app.set_active_macro_name("".into());
                }
                update_ui_macro_list(&macro_model_ref, &settings_arc, &custom_arc);
                update_ui_keybind_list(&keybind_model_ref, &keybinds_arc);
                let profiles = edits
                    .profiles
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                update_ui_profile_list(
                    &profile_model,
                    &profiles,
                    app.get_active_profile_id().as_str(),
                );
                app.set_status_msg(format!("Макрос '{name}' удалён").into());
            }
            Err(error) => app.set_status_msg(format!("Макрос не удалён: {error}").into()),
        }
    });
    completion_timer
}
