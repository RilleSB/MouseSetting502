pub mod bridge;
pub mod models;
pub mod device_worker;
pub mod macro_controller;
pub mod keybind_controller;

use std::rc::Rc;
use std::sync::{Arc, Mutex};
use slint::{ComponentHandle, ModelRc, VecModel};

use bridge::{MainWindow, MacroItem, KeybindItem};
use models::{update_ui_macro_list, update_ui_keybind_list};
use device_worker::init_device_worker;
use macro_controller::{setup_macro_controller, setup_macro_triggers, MacroControllerContext};
use keybind_controller::setup_keybind_controller;

use crate::lghub::{
    LgHubSettings, custom_macros::load_custom_macros,
    keybinds::{KeybindListener, load_bindings},
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

    // 1. Slint Models created ONCE, mutated in-place
    let macro_model = Rc::new(VecModel::<MacroItem>::default());
    app.set_macro_items(ModelRc::from(macro_model.clone()));

    let keybind_model = Rc::new(VecModel::<KeybindItem>::default());
    app.set_keybind_items(ModelRc::from(keybind_model.clone()));

    let builder_steps_model = Rc::new(VecModel::<slint::SharedString>::default());
    app.set_builder_steps(ModelRc::from(builder_steps_model.clone()));

    // 2. Data state
    let lghub_settings = Arc::new(LgHubSettings::load_from_db().ok());
    let custom_macros = Arc::new(Mutex::new(load_custom_macros()));
    let keybinds = Arc::new(Mutex::new(load_bindings()));

    // 3. Populate initial models
    update_ui_macro_list(&macro_model, &lghub_settings, &custom_macros);
    update_ui_keybind_list(&keybind_model, &keybinds);

    // 4. Macro execution & background listener state
    let active_macro: Arc<Mutex<Option<(String, macro_engine::ActiveMacro)>>> = Arc::new(Mutex::new(None));
    let initial_binds = if let Ok(guard) = keybinds.lock() {
        guard.clone()
    } else {
        Vec::new()
    };
    let listener = Arc::new(KeybindListener::new(initial_binds));

    // 5. Wire background trigger callback and Esc emergency panic stop
    setup_macro_triggers(&listener, &active_macro, &custom_macros, &lghub_settings, &app);

    // 6. Explicit hook activation (starts only on button click)
    app.set_listener_active(false);

    // 7. Initialize modular controllers
    init_device_worker(&app);
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
    setup_keybind_controller(&app, &keybind_model, &keybinds, &listener);

    // 8. Run Slint application loop
    if let Err(e) = app.run() {
        eprintln!("[!] Ошибка выполнения Slint: {e}");
    }

    // 9. Clean up running macro and background listener on window close
    listener.stop();
    let mut slot = active_macro.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, active)) = slot.take() {
        active.stop();
    }
}
