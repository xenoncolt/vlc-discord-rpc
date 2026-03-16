use std::env;
use std::fs;
use std::path::PathBuf;

// Returns the path to VLC's configuration file (vlcrc) on Windows
fn vlcrc_path() -> Option<PathBuf> {
    let appdata = env::var("APPDATA").ok()?;
    let path = PathBuf::from(appdata).join("vlc").join("vlcrc");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

// Check if VLC's RC interface is configured. If not, attempt to configure it
// Returns true if VLC RC is configured (either already or just now)
pub fn ensure_vlc_configured(port: u16) -> bool {
    println!("  Checking VLC RC interface configuration...");

    let vlcrc = match vlcrc_path() {
        Some(path) => path,
        None => {
            println!("  VLC config file not found at %APPDATA%\\vlc\\vlcrc");
            println!("  Please install VLC and run it once to create the config.\n");
            print_manual_instructions(port);
            return false;
        }
    };

    let content = match fs::read_to_string(&vlcrc) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("  Could not read vlcrc: {}", e);
            print_manual_instructions(port);
            return false;
        }
    };

    // Check if RC interface is already enabled
    let has_rc = content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#')
            && trimmed.starts_with("extraintf=")
            && trimmed.contains("rc")
    });

    let has_host = content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed.starts_with("rc-host=")
    });

    if has_rc && has_host {
        println!("  VLC RC interface is already configured.");
        return true;
    }

    println!("  Configuring VLC RC interface automatically...");

    // Create backup before modifying
    let backup_path = vlcrc.with_extension("bak");
    if let Err(e) = fs::copy(&vlcrc, &backup_path) {
        eprintln!("  Warning: Could not create backup of vlcrc: {}", e);
    } else {
        println!("  Backup saved to: {}", backup_path.display());
    }

    let mut new_content = String::new();
    let mut extraintf_set = false;
    let mut rc_host_set = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("#extraintf=") || trimmed == "#extraintf=" {
            // Uncomment and set to RC
            new_content.push_str("extraintf=rc\n");
            extraintf_set = true;
        } else if trimmed.starts_with("extraintf=") && !trimmed.contains("rc") {
            // Add RC to existing extra interfaces
            let value = trimmed.trim_start_matches("extraintf=").trim();
            if value.is_empty() {
                new_content.push_str("extraintf=rc\n");
            } else {
                new_content.push_str(&format!("extraintf={}:rc\n", value));
            }
            extraintf_set = true;
        } else if trimmed.starts_with("extraintf=") && trimmed.contains("rc") {
            // Already has RC, keep as-is
            new_content.push_str(line);
            new_content.push('\n');
            extraintf_set = true;
        } else if trimmed.starts_with("#rc-host=") || trimmed == "#rc-host=" {
            if !rc_host_set {
                new_content.push_str(&format!("rc-host=localhost:{}\n", port));
                rc_host_set = true;
            }
            // Skip duplicate commented rc-host lines
        } else if trimmed.starts_with("rc-host=") {
            if !rc_host_set {
                new_content.push_str(&format!("rc-host=localhost:{}\n", port));
                rc_host_set = true;
            }
            // Skip duplicate rc-host lines
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    // Append settings if they weren't found in the file
    if !extraintf_set {
        new_content.push_str("\n# Added by vlc-discord-rpc\n");
        new_content.push_str("extraintf=rc\n");
    }
    if !rc_host_set {
        new_content.push_str(&format!("rc-host=localhost:{}\n", port));
    }

    match fs::write(&vlcrc, &new_content) {
        Ok(_) => {
            println!("  VLC RC interface configured successfully!");
            println!("  Please restart VLC if it's currently running.\n");
            true
        }
        Err(e) => {
            eprintln!("  Failed to write vlcrc: {}", e);
            print_manual_instructions(port);
            false
        }
    }
}

// Print manual configuration instructions for users who need to set up VLC RC manually
fn print_manual_instructions(port: u16) {
    println!();
    println!("  ============ Manual VLC Setup ============");
    println!("  To enable VLC's remote control interface:");
    println!("  1. Open VLC media player");
    println!("  2. Go to Tools > Preferences");
    println!("  3. Click 'All' (bottom-left) to show advanced settings");
    println!("  4. Navigate to: Interface > Main interfaces");
    println!("  5. Check 'Remote control interface'");
    println!("  6. Expand 'Main interfaces' > 'RC'");
    println!("  7. Set 'TCP command input' to: localhost:{}", port);
    println!("  8. Save and restart VLC");
    println!();
    println!("  OR launch VLC from command line with:");
    println!(
        "    vlc --extraintf rc --rc-host localhost:{}",
        port
    );
    println!("  ==========================================");
    println!();
}
