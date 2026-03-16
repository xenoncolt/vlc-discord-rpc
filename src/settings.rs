use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

// Run Mode 

// Controls app behavior when VLC disconnects.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    // Keep running forever. Reconnect automatically when VLC restarts.
    Continuous,
    // Exit the app after VLC disconnects (after being connected once).
    ExitOnClose,
}

impl Default for RunMode {
    fn default() -> Self {
        RunMode::Continuous
    }
}

impl std::fmt::Display for RunMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunMode::Continuous => write!(f, "continuous"),
            RunMode::ExitOnClose => write!(f, "exit_on_close"),
        }
    }
}

// Configuration struct

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub vlc: VlcConfig,
    pub discord: DiscordConfig,
    pub app: AppConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VlcConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiscordConfig {
    pub polling_interval: u64,
    pub show_time_remaining: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub check_updates: bool,
    pub auto_configure_vlc: bool,
    pub run_mode: RunMode,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            vlc: VlcConfig {
                host: "127.0.0.1".to_string(),
                port: 9090,
            },
            discord: DiscordConfig {
                polling_interval: 15,
                show_time_remaining: true,
            },
            app: AppConfig {
                check_updates: true,
                auto_configure_vlc: true,
                run_mode: RunMode::Continuous,
            },
        }
    }
}

impl Config {
    // Load configuration from config.toml next to the executable
    // If no config exists and `first_run_setup` is true, runs the interactive wizard
    pub fn load(first_run_setup: bool) -> Self {
        let config_path = Self::config_path();

        if !config_path.exists() {
            if first_run_setup {
                println!("  No config.toml found. Let's set things up!\n");
                let config = Self::setup_wizard();
                if let Err(e) = config.save() {
                    eprintln!("  Warning: Could not save config: {}", e);
                }
                return config;
            } else {
                println!("  Creating default config at: {}", config_path.display());
                let config = Config::default();
                if let Err(e) = config.save() {
                    eprintln!("  Warning: Could not create config file: {}", e);
                }
                return config;
            }
        }

        match fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!(
                        "  Warning: Could not parse config.toml: {}. Using defaults.",
                        e
                    );
                    Config::default()
                }
            },
            Err(e) => {
                eprintln!(
                    "  Warning: Could not read config.toml: {}. Using defaults.",
                    e
                );
                Config::default()
            }
        }
    }

    // Save configuration to config.toml with helpful comments
    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::config_path();
        let content = self.to_commented_toml();
        fs::write(&config_path, content)?;
        println!("  Config saved to: {}", config_path.display());
        Ok(())
    }

    // Generate a nicely commented TOML string.
    fn to_commented_toml(&self) -> String {
        format!(
            r#"# ╔═══════════════════════════════════════════╗
# ║   VLC Discord RPC - Configuration File   ║
# ╚═══════════════════════════════════════════╝
#
# Edit this file to customize behavior.
# Run with --setup to reconfigure interactively.
# Delete this file to reset all settings to defaults.

[vlc]
# Host and port for VLC's RC (Remote Control) interface.
# IMPORTANT: Use "127.0.0.1" (not "localhost") to avoid IPv6 issues on Windows.
host = "{}"
port = {}

[discord]
# How often to check VLC and update Discord status (in seconds).
# Lower values = more responsive, but more resource usage.
# Recommended: 10-30
polling_interval = {}

# Show time remaining (countdown) on Discord status.
# When enabled, Discord will display "XX:XX left" while watching.
show_time_remaining = {}

[app]
# Check for updates on startup.
check_updates = {}

# Automatically configure VLC's RC interface if not already set up.
# This modifies VLC's config file (vlcrc) to enable remote control.
# A backup is created before any changes.
auto_configure_vlc = {}

# Run mode: how the app behaves when VLC is not running.
#   "continuous"    - Keep running forever, reconnect when VLC restarts (default)
#   "exit_on_close" - Exit after VLC disconnects
run_mode = "{}"
"#,
            self.vlc.host,
            self.vlc.port,
            self.discord.polling_interval,
            self.discord.show_time_remaining,
            self.app.check_updates,
            self.app.auto_configure_vlc,
            self.app.run_mode,
        )
    }

    // Get the path to config.toml (next to the executable)
    pub fn config_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .join("config.toml")
    }

    // Get the VLC address string for TCP connection
    pub fn vlc_address(&self) -> String {
        format!("{}:{}", self.vlc.host, self.vlc.port)
    }

    // Interactive terminal setup wizard
    pub fn setup_wizard() -> Self {
        println!("  ╔═══════════════════════════════════╗");
        println!("  ║      Configuration Wizard         ║");
        println!("  ╚═══════════════════════════════════╝");
        println!();
        println!("  Press Enter to accept the default value shown in [brackets].\n");

        let defaults = Config::default();
        let mut config = Config::default();

        // VLC Host
        config.vlc.host = prompt_string("  VLC host address", &defaults.vlc.host);

        // VLC Port
        config.vlc.port =
            prompt_number("  VLC RC port", defaults.vlc.port as u64) as u16;

        // Polling interval
        config.discord.polling_interval = prompt_number(
            "  Discord update interval (seconds)",
            defaults.discord.polling_interval,
        );

        // Show time remaining
        config.discord.show_time_remaining = prompt_yes_no(
            "  Show time remaining on Discord?",
            defaults.discord.show_time_remaining,
        );

        // Check updates
        config.app.check_updates = prompt_yes_no(
            "  Check for updates on startup?",
            defaults.app.check_updates,
        );

        // Auto-configure VLC
        config.app.auto_configure_vlc = prompt_yes_no(
            "  Auto-configure VLC RC interface?",
            defaults.app.auto_configure_vlc,
        );

        // Run mode
        println!();
        println!("  Run mode:");
        println!("    1. Continuous   - Keep running, reconnect when VLC restarts");
        println!("    2. Exit on close - Exit after VLC disconnects");
        let mode_choice = prompt_number("  Choose (1-2)", 1);
        config.app.run_mode = match mode_choice {
            2 => RunMode::ExitOnClose,
            _ => RunMode::Continuous,
        };

        println!();
        println!("  Configuration complete!\n");
        config
    }
}

// Terminal Prompt

fn prompt_string(prompt: &str, default: &str) -> String {
    print!("{} [{}]: ", prompt, default);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn prompt_number(prompt: &str, default: u64) -> u64 {
    print!("{} [{}]: ", prompt, default);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();
    if trimmed.is_empty() {
        default
    } else {
        trimmed.parse().unwrap_or(default)
    }
}

fn prompt_yes_no(prompt: &str, default: bool) -> bool {
    let hint = if default { "Y/n" } else { "y/N" };
    print!("{} [{}]: ", prompt, hint);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        default
    } else {
        matches!(trimmed.as_str(), "y" | "yes")
    }
}
