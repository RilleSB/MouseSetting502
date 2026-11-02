use slint::{ComponentHandle, VecModel};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use super::bridge::{KeybindItem, MacroItem, MainWindow, ProfileItem};
use super::macro_persistence::MacroEditState;
use super::models::{update_ui_keybind_list, update_ui_macro_list, update_ui_profile_list};
use crate::lghub::{
    AppProfile, LgHubSettings, LgMacro, LgMacroComponent,
    custom_macros::format_component,
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
        PlaybackChange::Stale => {}
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
    listener.set_event_callback(move |name, event_generation| {
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
            match name.as_str() {
                "__PROFILE_NEXT__" => app.invoke_next_app_profile(),
                "__DPI_UP__" => app.invoke_dpi_up(),
                "__DPI_DOWN__" => app.invoke_dpi_down(),
                _ => {
                    if let Some(data) = find_macro(&name, &custom, &settings) {
                        toggle_macro(&app, &runtime, &listener, data, generation, true);
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

    // Macro Builder in-memory sequence state
    let builder_steps: Arc<Mutex<Vec<LgMacroComponent>>> = Arc::new(Mutex::new(Vec::new()));

    // Open builder for new macro
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_open_macro_builder(move || {
        if let Some(app) = app_weak.upgrade() {
            if let Ok(mut steps) = steps_ref.lock() {
                steps.clear();
            }
            steps_model.set_vec(Vec::new());
            app.set_builder_error("".into());
            app.set_builder_editing_original("".into());
            app.set_builder_name("Мой новый автокликер".into());
            app.set_builder_is_toggle(true);
            app.set_show_macro_builder(true);
        }
    });

    // Edit existing custom macro
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    let custom_arc = custom_macros.clone();
    app.on_edit_custom_macro(move |name| {
        let target = if let Ok(custom) = custom_arc.lock() {
            custom.iter().find(|m| m.name == name.as_str()).cloned()
        } else {
            None
        };

        if let Some(m) = target
            && let Ok(mut steps) = steps_ref.lock()
        {
            *steps = m.sequence.clone();
            let slint_steps: Vec<slint::SharedString> =
                steps.iter().map(|s| format_component(s).into()).collect();
            steps_model.set_vec(slint_steps);
            if let Some(app) = app_weak.upgrade() {
                app.set_builder_error("".into());
                app.set_builder_editing_original(m.name.clone().into());
                app.set_builder_name(m.name.into());
                app.set_builder_is_toggle(m.is_toggle);
                app.set_show_macro_builder(true);
            }
        }
    });

    let app_weak = app.as_weak();
    app.on_close_macro_builder(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_show_macro_builder(false);
        }
    });

    // Builder Add Mouse
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_add_mouse(move |button, down, delay| {
        let comp = LgMacroComponent {
            mouse_button: Some(button as u8),
            mouse_down: Some(down),
            vk_code: None,
            key_down: None,
            delay_ms: if delay > 0 { Some(delay as u64) } else { None },
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps_model.push(format_component(&comp).into());
            steps.push(comp);
        }
    });

    // Builder Add Key
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_add_key(move |vk, down, delay| {
        let comp = LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: Some(vk as u16),
            key_down: Some(down),
            delay_ms: if delay > 0 { Some(delay as u64) } else { None },
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps_model.push(format_component(&comp).into());
            steps.push(comp);
        }
    });

    // Builder Add Delay
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_add_delay(move |delay| {
        let comp = LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: None,
            key_down: None,
            delay_ms: Some(delay as u64),
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps_model.push(format_component(&comp).into());
            steps.push(comp);
        }
    });

    // Builder Clear
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_clear_steps(move || {
        if let Ok(mut steps) = steps_ref.lock() {
            steps.clear();
        }
        steps_model.set_vec(Vec::new());
    });

    // Builder Remove Single Step
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_remove_step(move |idx| {
        if let Ok(mut steps) = steps_ref.lock() {
            let i = idx as usize;
            if i < steps.len() {
                steps.remove(i);
                steps_model.remove(i);
            }
        }
    });

    // Builder Move Step Up
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_move_step_up(move |idx| {
        let idx = idx as usize;
        if let Ok(mut steps) = steps_ref.lock()
            && idx > 0
            && idx < steps.len()
        {
            steps.swap(idx - 1, idx);
            let slint_steps: Vec<slint::SharedString> =
                steps.iter().map(|s| format_component(s).into()).collect();
            steps_model.set_vec(slint_steps);
        }
    });

    // Builder Move Step Down
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_move_step_down(move |idx| {
        let idx = idx as usize;
        if let Ok(mut steps) = steps_ref.lock()
            && idx + 1 < steps.len()
        {
            steps.swap(idx, idx + 1);
            let slint_steps: Vec<slint::SharedString> =
                steps.iter().map(|s| format_component(s).into()).collect();
            steps_model.set_vec(slint_steps);
        }
    });

    // Save edits and every profile reference before publishing new UI state.
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    let custom_arc = custom_macros.clone();
    let settings_arc = lghub_settings.clone();
    let keybinds_arc = keybinds.clone();
    let macro_model_ref = macro_model.clone();
    let keybind_model_ref = keybind_model.clone();
    let profile_model = ctx.profile_model.clone();
    let save_edits = edits.clone();
    app.on_builder_save_macro(move |orig_name, name, is_toggle| {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        let name_str = name.trim().to_string();
        app.set_builder_error("".into());
        let mut steps = steps_ref.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if steps.is_empty() {
            steps.push(LgMacroComponent {
                mouse_button: Some(1),
                mouse_down: Some(true),
                vk_code: None,
                key_down: None,
                delay_ms: Some(20),
            });
            steps.push(LgMacroComponent {
                mouse_button: Some(1),
                mouse_down: Some(false),
                vk_code: None,
                key_down: None,
                delay_ms: Some(20),
            });
        }

        let new_macro = LgMacro {
            name: name_str.clone(),
            macro_type: if is_toggle {
                "toggle".into()
            } else {
                "sequence".into()
            },
            action_name: Some(name_str.clone()),
            is_toggle,
            sequence: steps,
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
                app.set_show_macro_builder(false);
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
