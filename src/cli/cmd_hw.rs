use crate::hardware::sysfs::identify;
use colored::*;

pub fn handle_diskinfo(parts: &[&str]) {
    if parts.len() < 2 {
        println!(
            "{}",
            "Usage: diskinfo <device_path> (e.g., diskinfo /dev/sda)".yellow()
        );
        return;
    }

    match identify(parts[1]) {
        Ok(device) => {
            println!("  type       : {}", device.kind);
            println!("  device     : {}", device.device_path.display());
            println!("  name       : {}", device.name);
            println!("  bus        : {}", device.bus);
            println!(
                "  model      : {}",
                device.model.as_deref().unwrap_or("unknown")
            );
            println!(
                "  vendor     : {}",
                device.vendor.as_deref().unwrap_or("unknown")
            );
            println!("  removable  : {}", device.removable);
            println!("  rotational : {}", device.rotational);
            println!("  sysfs      : {}", device.sysfs_path.display());
        }
        Err(err) => {
            eprintln!("{}", format!("[ERROR] Identify failed: {}", err).red());
        }
    }
}
