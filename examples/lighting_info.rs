use ghub_rust::hidpp::HidppDevice;

// Read-only HID++ 0x8070 capability diagnostics for G502 HERO.
fn main() -> Result<(), String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let info = api
        .device_list()
        .find(|d| {
            d.vendor_id() == 0x046d
                && d.product_id() == 0xc08b
                && d.usage_page() == 0xff00
                && d.usage() == 2
        })
        .ok_or("G502 HERO not found")?;
    let dev = HidppDevice::new(api.open_path(info.path()).map_err(|e| e.to_string())?, 0xff);
    let feature = dev.get_feature_index(0x8070)?;
    let info = dev.call_function(feature, 0, &[])?.params;
    println!("Lighting info: {info:02x?}");
    for zone in 0..info[0].min(2) {
        let details = dev.call_function(feature, 1, &[zone])?.params;
        println!("Zone {zone}: {details:02x?}");
        for effect in 0..details[3].min(16) {
            match dev.call_function(feature, 2, &[zone, effect]) {
                Ok(reply) => println!("  Effect {effect}: {:02x?}", reply.params),
                Err(error) => println!("  Effect {effect}: {error}"),
            }
        }
    }
    Ok(())
}
