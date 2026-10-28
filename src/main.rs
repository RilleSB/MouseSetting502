pub mod hidpp;

use std::env;
use hidapi::HidApi;
use hidpp::HidppDevice;
use hidpp::features::{
    get_device_name, get_current_dpi, set_dpi, get_dpi_range,
    set_led_effect, LedZone, LedEffect,
};

const LOGITECH_VID: u16 = 0x046D;
const G502_PID: u16 = 0xC08B;
const G435_PID: u16 = 0x0ACB;

fn parse_hex_color(hex: &str) -> Result<(u8, u8, u8), String> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Err("Color must be a 6-character hex code like FF0055".into());
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
    Ok(HidppDevice::new(dev, 0xFF))
}

fn print_banner() {
    println!("============================================================");
    println!("   🦀 GHUB-Rust: Ultra-fast Logitech Controller (Native)   ");
    println!("============================================================");
}

fn print_help() {
    println!("\nUsage:");
    println!("  ghub_rust                       Display current mouse status");
    println!("  ghub_rust --dpi <value>         Set active DPI (e.g. --dpi 800)");
    println!("  ghub_rust --color <HEX>         Set static RGB color (e.g. --color FF0055)");
    println!("  ghub_rust --cycle               Enable spectrum rainbow color cycle");
    println!("  ghub_rust --breathe <HEX>       Enable breathing effect with color");
    println!("  ghub_rust --off                 Turn off RGB lighting");
    println!("  ghub_rust --scan                Scan all Logitech devices & endpoints");
}

fn main() {
    print_banner();

    let api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("[!] Failed to initialize HID API: {e}");
            return;
        }
    };

    let args: Vec<String> = env::args().collect();

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

    if args.len() == 1 {
        // Default: Show status
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
                Ok(_) => {
                    let confirmed = get_current_dpi(&g502, 0).unwrap_or(target);
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
            match set_led_effect(&g502, LedZone::Logo, LedEffect::Static { r, g, b }) {
                Ok(_) => println!("[+] Color applied successfully!"),
                Err(e) => eprintln!("[-] Failed to set color: {e}"),
            }
        }
        "--cycle" => {
            println!("\n[*] Enabling rainbow color cycle on logo...");
            match set_led_effect(&g502, LedZone::Logo, LedEffect::ColorCycle { period_ms: 10000, brightness: 255 }) {
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
            match set_led_effect(&g502, LedZone::Logo, LedEffect::Breathing { r, g, b, period_ms: 3000 }) {
                Ok(_) => println!("[+] Breathing effect active!"),
                Err(e) => eprintln!("[-] Failed to set effect: {e}"),
            }
        }
        "--off" => {
            println!("\n[*] Turning off RGB lighting...");
            match set_led_effect(&g502, LedZone::All, LedEffect::Off) {
                Ok(_) => println!("[+] LEDs turned off!"),
                Err(e) => eprintln!("[-] Failed to turn off LEDs: {e}"),
            }
        }
        "--help" | "-h" => {
            print_help();
        }
        other => {
            eprintln!("[-] Unknown option: {other}");
            print_help();
        }
    }
}
