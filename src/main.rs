pub mod hidpp;
pub mod lghub;
pub mod ui;

use std::env;
use std::thread;
use std::time::Duration;
use hidapi::HidApi;
use hidpp::HidppDevice;
use hidpp::features::{
    get_device_name, get_current_dpi, set_dpi, get_dpi_range,
    set_led_effect, turn_off_all_leds,
    LedZone, LedEffect,
};
use lghub::{LgHubSettings, macro_engine};

const LOGITECH_VID: u16 = 0x046D;
const G502_PID: u16 = 0xC08B;
const G435_PID: u16 = 0x0ACB;

fn parse_hex_color(hex: &str) -> Result<(u8, u8, u8), String> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 || !hex.is_ascii() {
        return Err("Color must be a 6-character ASCII hex code like 00FFCC or FF0055".into());
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "Invalid Red channel")?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| "Invalid Green channel")?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "Invalid Blue channel")?;
    Ok((r, g, b))
}

fn open_g502(api: &HidApi) -> Result<HidppDevice, String> {
    let info = api
        .device_list()
        .find(|d| {
            d.vendor_id() == LOGITECH_VID
                && d.product_id() == G502_PID
                && d.usage_page() == 0xFF00
                && d.usage() == 0x0002
        })
        .ok_or("G502 HERO not found on HID++ interface")?;

    let dev = api.open_path(info.path()).map_err(|e| format!("Failed to open G502: {e}"))?;
    let hidpp = HidppDevice::new(dev, 0xFF);
    // Inspecting or opening the device must preserve its memory mode.
    Ok(hidpp)
}

fn print_banner() {
    println!("============================================================");
    println!("   🖱 Mouse Settings 502   ");
    println!("============================================================");
}

fn print_help() {
    println!("\nUsage:");
    println!("  ghub_rust                       Launch Modern Desktop GUI");
    println!("  ghub_rust --cli                 Display mouse status in CLI");
    println!("  ghub_rust --dpi <value>         Set active DPI (100 - 25600, step 50)");
    println!("  ghub_rust --color <HEX>         Set static RGB color (e.g. --color 00FFCC)");
    println!("  ghub_rust --cycle               Enable spectrum rainbow color cycle");
    println!("  ghub_rust --breathe <HEX>       Enable breathing effect with color");
    println!("  ghub_rust --off                 Turn off RGB lighting");
    println!("  ghub_rust --scan                Scan all Logitech devices & endpoints");
    println!("  ghub_rust --macros              List all macros (G HUB & Custom) and keybinds");
    println!("  ghub_rust --run-macro <name>    Test play a macro (e.g. --run-macro lkm)");
    println!("  ghub_rust --help                Show this help message");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // 1. Check --help first without needing mouse connected!
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_banner();
        print_help();
        return;
    }

    // 2. Default: Launch GUI!
    if args.len() == 1 || (args.len() > 1 && (args[1] == "--gui" || args[1] == "--autostart")) {
        ui::run();
        return;
    }

    print_banner();

    // 3. Macro & Keybind Inspection command
    if args.len() > 1 && (args[1] == "--macros" || args[1] == "-m") {
        println!("\n[+] Custom User Macros (user_macros.json):");
        let custom = lghub::load_custom_macros();
        for (i, m) in custom.iter().enumerate() {
            let toggle_str = if m.is_toggle { " (Toggle / Auto-clicker)" } else { " (Sequence)" };
            println!("  #{} '{}' -> Type: {}{}, Steps: {}", i + 1, m.name, m.macro_type, toggle_str, m.sequence.len());
        }

        println!("\n[+] Reading original G HUB settings database...");
        match LgHubSettings::load_from_db() {
            Ok(settings) => {
                println!("Loaded {} cards and {} profiles from G HUB.", settings.cards.len(), settings.profiles.len());

                println!("\n--- G HUB Recorded Macros ---");
                let mut macro_count = 0;
                for card in &settings.cards {
                    if let Some(m) = &card.macro_data {
                        macro_count += 1;
                        let toggle_str = if m.is_toggle { " (Toggle / Auto-clicker)" } else { "" };
                        println!("  #{} '{}' -> Type: {}{}", macro_count, m.name, m.macro_type, toggle_str);
                        if let Some(action) = &m.action_name {
                            println!("       Action: {}", action);
                        }
                        println!("       Sequence length: {} steps", m.sequence.len());
                    }
                }
            }
            Err(e) => eprintln!("[-] G HUB database not loaded: {e}"),
        }

        println!("\n[+] Configured G502 & Hotkey Keybindings (keybinds.json):");
        let binds = lghub::load_bindings();
        for b in &binds {
            let status = if b.assigned_macro.is_empty() {
                format!("(Default: {})", b.default_action)
            } else {
                format!("-> ⚡ MACRO: '{}'", b.assigned_macro)
            };
            println!("  Button {:<26} {}", b.button_label, status);
        }
        return;
    }

    // 4. Run Macro command (proper argument check)
    if args.len() > 1 && args[1] == "--run-macro" {
        if args.len() < 3 {
            eprintln!("[-] Error: Missing macro name. Example: ghub_rust --run-macro lkm");
            return;
        }
        let macro_name = &args[2];
        println!("\n[*] Looking for macro '{}'...", macro_name);

        // Check custom macros first
        let custom = lghub::load_custom_macros();
        let target_custom = custom.iter().find(|m| m.name.contains(macro_name) || m.name == *macro_name).cloned();

        let target = if let Some(m) = target_custom {
            Some(m)
        } else {
            // Check G HUB DB
            match LgHubSettings::load_from_db() {
                Ok(settings) => settings.cards.iter().find_map(|c| {
                    if c.name.as_deref().unwrap_or("").contains(macro_name)
                        || c.macro_data.as_ref().map(|m| m.name.as_str()).unwrap_or("").contains(macro_name)
                    {
                        c.macro_data.clone()
                    } else {
                        None
                    }
                }),
                Err(_) => None,
            }
        };

        if let Some(m) = target {
            println!("[+] Found macro '{}' (Toggle: {})! Running for 3 seconds...", m.name, m.is_toggle);
            if m.is_toggle {
                let active = macro_engine::start_macro_loop(m.clone());
                thread::sleep(Duration::from_secs(3));
                active.stop();
                println!("[+] Finished test execution.");
            } else {
                macro_engine::play_macro_once(&m);
                println!("[+] Executed macro sequence once.");
            }
        } else {
            eprintln!("[-] Macro '{}' not found in custom macros or G HUB database.", macro_name);
        }
        return;
    }

    let api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("[!] Failed to initialize HID API: {e}");
            return;
        }
    };

    if args.len() > 1 && args[1] == "--scan" {
        println!("\n[+] Scanning Logitech Peripherals:");
        let mut g435_found = false;
        let mut g502_found = false;

        for dev in api.device_list() {
            if dev.vendor_id() == LOGITECH_VID {
                if dev.product_id() == G502_PID && !g502_found {
                    println!("  * Mouse: Logitech G502 HERO (VID: 0x{:04X}, PID: 0x{:04X})", dev.vendor_id(), dev.product_id());
                    g502_found = true;
                } else if dev.product_id() == G435_PID && !g435_found {
                    println!("  * Headset: Logitech G435 Lightspeed (VID: 0x{:04X}, PID: 0x{:04X})", dev.vendor_id(), dev.product_id());
                    g435_found = true;
                }
            }
        }
        return;
    }

    let g502 = match open_g502(&api) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("\n[-] Error: {e}");
            println!("Make sure the mouse is plugged in and recognized by Windows.");
            return;
        }
    };

    let device_name = get_device_name(&g502).unwrap_or_else(|_| "Logitech G502".into());

    if args[1] == "--cli" {
        println!("\nDevice: {}", device_name);
        match get_current_dpi(&g502, 0) {
            Ok(dpi) => println!("Current DPI       : {}", dpi),
            Err(e) => eprintln!("DPI read error    : {e}"),
        }
        match get_dpi_range(&g502, 0) {
            Ok(r) => println!("Supported DPI     : {} - {} (step: {})", r.min_dpi, r.max_dpi, r.step),
            Err(e) => eprintln!("Range query error : {e}"),
        }
        print_help();
        return;
    }

    match args[1].as_str() {
        "--dpi" => {
            if args.len() < 3 {
                eprintln!("[-] Error: Missing DPI value. Example: --dpi 800");
                return;
            }
            let target: u16 = match args[2].parse() {
                Ok(v) => v,
                Err(_) => {
                    eprintln!("[-] Error: Invalid number for DPI: {}", args[2]);
                    return;
                }
            };
            println!("\n[*] Setting {} DPI to {}...", device_name, target);
            match set_dpi(&g502, 0, target) {
                Ok(confirmed) => {
                    println!("[+] Successfully applied! Active DPI: {}", confirmed);
                }
                Err(e) => eprintln!("[-] Failed to set DPI: {e}"),
            }
        }
        "--color" => {
            if args.len() < 3 {
                eprintln!("[-] Error: Missing HEX color. Example: --color 00FFCC");
                return;
            }
            let (r, g, b) = match parse_hex_color(&args[2]) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[-] Error: {e}");
                    return;
                }
            };
            println!("\n[*] Setting RGB Logo color to #{:02X}{:02X}{:02X} (R: {}, G: {}, B: {})...", r, g, b, r, g, b);
            match set_led_effect(&g502, LedZone::Logo, LedEffect::Static { r, g, b }, false) {
                Ok(_) => println!("[+] Color applied successfully!"),
                Err(e) => eprintln!("[-] Failed to set color: {e}"),
            }
        }
        "--cycle" => {
            println!("\n[*] Enabling rainbow color cycle on logo...");
            match set_led_effect(&g502, LedZone::Logo, LedEffect::ColorCycle { period_ms: 10000, brightness: 255 }, false) {
                Ok(_) => println!("[+] Spectrum cycle active!"),
                Err(e) => eprintln!("[-] Failed to enable cycle: {e}"),
            }
        }
        "--breathe" => {
            if args.len() < 3 {
                eprintln!("[-] Error: Missing HEX color for breathing. Example: --breathe FF0055");
                return;
            }
            let (r, g, b) = match parse_hex_color(&args[2]) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[-] Error: {e}");
                    return;
                }
            };
            println!("\n[*] Enabling breathing effect (#{})...", args[2]);
            match set_led_effect(&g502, LedZone::Logo, LedEffect::Breathing { r, g, b, period_ms: 3000 }, false) {
                Ok(_) => println!("[+] Breathing effect active!"),
                Err(e) => eprintln!("[-] Failed to set effect: {e}"),
            }
        }
        "--off" => {
            println!("\n[*] Turning off RGB lighting...");
            match turn_off_all_leds(&g502, false) {
                Ok(_) => println!("[+] LEDs turned off!"),
                Err(e) => eprintln!("[-] Failed to turn off LEDs: {e}"),
            }
        }
        other => {
            eprintln!("[-] Unknown option: {other}");
            print_help();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("00FFCC"), Ok((0x00, 0xFF, 0xCC)));
        assert_eq!(parse_hex_color("#FF0055"), Ok((0xFF, 0x00, 0x55)));
        assert_eq!(parse_hex_color("123456"), Ok((0x12, 0x34, 0x56)));

        // Invalid length
        assert!(parse_hex_color("FFF").is_err());
        assert!(parse_hex_color("00FFCCA").is_err());

        // Multibyte characters should not panic
        assert!(parse_hex_color("日日").is_err());
        assert!(parse_hex_color("🦀").is_err());
        assert!(parse_hex_color("GGGGGG").is_err());
    }
}
