mod settings;
mod title;
mod update;
mod vlc;
mod vlc_setup;

use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde::Deserialize;
use settings::RunMode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use vlc::PlaybackState;

// TMDB Data Structures

#[derive(Deserialize)]
struct MovieData {
    title: String,
    genres: Vec<Genre>,
    poster_path: String,
    tmdb_id: u32,
    imdb_id: Option<String>,
}

#[derive(Deserialize)]
struct Genre {
    name: String,
}

#[derive(Deserialize)]
struct TVShowData {
    tmdb_id: u32,
    name: String,
    poster_path: String,
    imdb_id: Option<String>,
}

#[derive(Deserialize)]
struct EpisodeData {
    name: String,
}

// Cached Discord Presence

struct PresenceInfo {
    title: String,
    details: String,
    poster_url: String,
    imdb_url: Option<String>,
    tmdb_url: String,
}

// TMDB Fetch Functions

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '&' => "%26".to_string(),
            '?' => "%3F".to_string(),
            '#' => "%23".to_string(),
            '+' => "%2B".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

async fn fetch_movie_data(
    title: &str,
    year: Option<u32>,
    api_key: &str,
) -> anyhow::Result<MovieData> {
    println!("  Fetching movie data for: \"{}\"", title);

    let mut url = format!(
        "https://api.themoviedb.org/3/search/movie?api_key={}&query={}",
        api_key,
        urlencode(title)
    );
    if let Some(y) = year {
        url.push_str(&format!("&year={}", y));
    }

    let response: serde_json::Value = reqwest::get(&url).await?.json().await?;

    if let Some(movie) = response["results"].as_array().and_then(|a| a.first()) {
        let title = movie["title"].as_str().unwrap_or("").to_string();
        let genre_ids: Vec<i64> = movie["genre_ids"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|g| g.as_i64())
            .collect();
        let genres = fetch_genres(&genre_ids, api_key).await?;
        let poster_path = movie["poster_path"].as_str().unwrap_or("").to_string();
        let tmdb_id = movie["id"].as_u64().unwrap_or(0) as u32;

        // Fetch IMDB ID from movie details
        let detail_url = format!(
            "https://api.themoviedb.org/3/movie/{}?api_key={}",
            tmdb_id, api_key
        );
        let detail: serde_json::Value = reqwest::get(&detail_url).await?.json().await?;
        let imdb_id = detail["imdb_id"].as_str().map(|s| s.to_string());

        Ok(MovieData {
            title,
            genres: genres.into_iter().map(|name| Genre { name }).collect(),
            poster_path,
            tmdb_id,
            imdb_id,
        })
    } else {
        Err(anyhow::anyhow!("Movie not found"))
    }
}

async fn fetch_genres(genre_ids: &[i64], api_key: &str) -> anyhow::Result<Vec<String>> {
    let url = format!(
        "https://api.themoviedb.org/3/genre/movie/list?api_key={}",
        api_key
    );
    let response: serde_json::Value = reqwest::get(&url).await?.json().await?;

    let genres = genre_ids
        .iter()
        .filter_map(|id| {
            response["genres"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .find(|g| g["id"].as_i64() == Some(*id))
                .and_then(|g| g["name"].as_str().map(|s| s.to_string()))
        })
        .collect();
    Ok(genres)
}

async fn fetch_tv_show_data(name: &str, api_key: &str) -> anyhow::Result<TVShowData> {
    println!("  Fetching TV show data for: \"{}\"", name);

    let url = format!(
        "https://api.themoviedb.org/3/search/tv?api_key={}&query={}",
        api_key,
        urlencode(name)
    );
    let response: serde_json::Value = reqwest::get(&url).await?.json().await?;

    if let Some(show) = response["results"].as_array().and_then(|a| a.first()) {
        let tmdb_id = show["id"].as_u64().unwrap_or(0) as u32;
        let name = show["name"].as_str().unwrap_or("").to_string();
        let poster_path = show["poster_path"].as_str().unwrap_or("").to_string();

        // Fetch IMDB ID from external IDs
        let detail_url = format!(
            "https://api.themoviedb.org/3/tv/{}/external_ids?api_key={}",
            tmdb_id, api_key
        );
        let detail: serde_json::Value = reqwest::get(&detail_url).await?.json().await?;
        let imdb_id = detail["imdb_id"].as_str().map(|s| s.to_string());

        Ok(TVShowData {
            tmdb_id,
            name,
            poster_path,
            imdb_id,
        })
    } else {
        Err(anyhow::anyhow!("TV show not found"))
    }
}

async fn fetch_episode_data(
    tmdb_id: u32,
    season: u32,
    episode: u32,
    api_key: &str,
) -> anyhow::Result<EpisodeData> {
    let url = format!(
        "https://api.themoviedb.org/3/tv/{}/season/{}/episode/{}?api_key={}",
        tmdb_id, season, episode, api_key
    );
    let response: serde_json::Value = reqwest::get(&url).await?.json().await?;
    let name = response["name"].as_str().unwrap_or("").to_string();
    Ok(EpisodeData { name })
}

// Discord Presence
/// Send a Discord activity with type=3 (Watching) using raw JSON.
/// The discord-rich-presence crate's Activity struct doesn't support the `type` field,
/// so we construct the payload manually and use the low-level `send()` method.
fn update_discord_presence(
    client: &mut DiscordIpcClient,
    info: &PresenceInfo,
    state: &PlaybackState,
    time_remaining_secs: Option<u64>,
    show_time: bool,
) {
    let state_text = match state {
        PlaybackState::Playing => info.details.as_str(),
        PlaybackState::Paused => "Paused",
        PlaybackState::Stopped => "Stopped",
    };

    // Build activity JSON with type=3 (Watching)
    let mut activity = serde_json::json!({
        "type": 3,
        "state": state_text,
        "details": info.title,
    });

    // Add poster image
    if !info.poster_url.is_empty() {
        activity["assets"] = serde_json::json!({
            "large_image": info.poster_url,
            "large_text": info.title,
        });
    }

    // Add TMDB/IMDB buttons
    let mut buttons = Vec::new();
    if !info.tmdb_url.is_empty() {
        buttons.push(serde_json::json!({"label": "TMDB", "url": info.tmdb_url}));
    }
    if let Some(ref imdb_url) = info.imdb_url {
        buttons.push(serde_json::json!({"label": "IMDB", "url": imdb_url}));
    }
    if !buttons.is_empty() {
        activity["buttons"] = serde_json::json!(buttons);
    }

    // Add countdown timestamp (only when actively playing)
    if show_time && *state == PlaybackState::Playing {
        if let Some(remaining) = time_remaining_secs {
            if remaining > 0 {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let end_time = now + remaining as i64;
                activity["timestamps"] = serde_json::json!({"end": end_time});
            }
        }
    }

    // Send via raw IPC (SET_ACTIVITY with opcode 1)
    let nonce = format!(
        "{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let data = serde_json::json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": std::process::id(),
            "activity": activity
        },
        "nonce": nonce
    });

    if let Err(e) = client.send(data, 1) {
        eprintln!("  Warning: Failed to update Discord presence: {}", e);
    }
}

/// Send a basic "Watching" activity for titles not found on TMDB.
fn update_basic_presence(
    client: &mut DiscordIpcClient,
    title: &str,
    details: &str,
) {
    let nonce = format!(
        "{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let data = serde_json::json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": std::process::id(),
            "activity": {
                "type": 3,
                "state": details,
                "details": title,
            }
        },
        "nonce": nonce
    });
    if let Err(e) = client.send(data, 1) {
        eprintln!("  Warning: Failed to update Discord presence: {}", e);
    }
}

fn clear_discord_presence(client: &mut DiscordIpcClient) {
    let _ = client.clear_activity();
}

// VLC Process Detection (Windows)

/// Check if vlc.exe is running (Windows only, uses tasklist).
#[cfg(windows)]
fn is_vlc_process_running() -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq vlc.exe", "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.to_lowercase().contains("vlc.exe")
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_vlc_process_running() -> bool {
    false
}

// TMDB to Presence Converters

async fn fetch_tv_presence(
    name: &str,
    season: u32,
    episode: u32,
    api_key: &str,
) -> Option<PresenceInfo> {
    let tv_data = fetch_tv_show_data(name, api_key).await.ok()?;
    let ep_data = fetch_episode_data(tv_data.tmdb_id, season, episode, api_key)
        .await
        .ok();

    let episode_title = ep_data
        .as_ref()
        .map(|e| e.name.as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or(&tv_data.name);

    let details = format!("{} - S{:02}E{:02}", tv_data.name, season, episode);
    let poster_url = if tv_data.poster_path.is_empty() {
        String::new()
    } else {
        format!("https://image.tmdb.org/t/p/w500{}", tv_data.poster_path)
    };
    let imdb_url = tv_data
        .imdb_id
        .as_deref()
        .map(|id| format!("https://www.imdb.com/title/{}/", id));
    let tmdb_url = format!("https://www.themoviedb.org/tv/{}", tv_data.tmdb_id);

    Some(PresenceInfo {
        title: episode_title.to_string(),
        details,
        poster_url,
        imdb_url,
        tmdb_url,
    })
}

async fn fetch_movie_presence(
    name: &str,
    year: Option<u32>,
    api_key: &str,
) -> Option<PresenceInfo> {
    let movie_data = fetch_movie_data(name, year, api_key).await.ok()?;

    let genres: Vec<String> = movie_data.genres.iter().map(|g| g.name.clone()).collect();
    let details = if genres.is_empty() {
        "Watching a movie".to_string()
    } else {
        genres.join(", ")
    };
    let poster_url = if movie_data.poster_path.is_empty() {
        String::new()
    } else {
        format!(
            "https://image.tmdb.org/t/p/w500{}",
            movie_data.poster_path
        )
    };
    let imdb_url = movie_data
        .imdb_id
        .as_deref()
        .map(|id| format!("https://www.imdb.com/title/{}/", id));
    let tmdb_url = format!(
        "https://www.themoviedb.org/movie/{}",
        movie_data.tmdb_id
    );

    Some(PresenceInfo {
        title: movie_data.title,
        details,
        poster_url,
        imdb_url,
        tmdb_url,
    })
}

// Main Entry Point 

#[tokio::main]
async fn main() {
    println!();
    println!("  ╔═══════════════════════════════════╗");
    println!("  ║     VLC Discord Rich Presence     ║");
    println!("  ╚═══════════════════════════════════╝");
    println!();

    // CLI argument handling 
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("  Usage: vlc-discord-rpc [OPTIONS]");
        println!();
        println!("  Options:");
        println!("    --setup    Run interactive configuration wizard");
        println!("    --help     Show this help message");
        println!();
        println!("  Config file: {}", settings::Config::config_path().display());
        println!();
        return;
    }

    let run_setup = args.iter().any(|a| a == "--setup");

    // Load or create configuration
    let config = if run_setup {
        let config = settings::Config::setup_wizard();
        if let Err(e) = config.save() {
            eprintln!("  Warning: Could not save config: {}", e);
        }
        println!();
        config
    } else {
        let first_run = !settings::Config::config_path().exists();
        settings::Config::load(first_run)
    };

    println!(
        "  Config loaded (polling every {}s, mode: {}).\n",
        config.discord.polling_interval, config.app.run_mode
    );

    // Check for updates
    if config.app.check_updates {
        update::update().await;
    }

    // Auto-configure VLC RC interface if enabled
    if config.app.auto_configure_vlc {
        vlc_setup::ensure_vlc_configured(config.vlc.port);
    }

    let client_id = env!("CLIENT_ID");
    let api_key = env!("API_KEY");

    // Connect to Discord (with retry loop)
    println!("  Connecting to Discord...");
    let mut discord_client = loop {
        match DiscordIpcClient::new(client_id) {
            Ok(mut client) => match client.connect() {
                Ok(_) => {
                    println!("  Connected to Discord.\n");
                    break client;
                }
                Err(e) => {
                    println!("  Waiting for Discord... ({})", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            },
            Err(e) => {
                println!("  Waiting for Discord... ({})", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    };

    let poll_interval = Duration::from_secs(config.discord.polling_interval);

    // State tracking
    let mut vlc_client: Option<vlc::VlcClient> = None;
    let mut last_title: Option<String> = None;
    let mut cached_presence: Option<PresenceInfo> = None;
    let mut was_connected = false;
    let mut waiting_logged = false;

    println!("  Ready! Waiting for VLC...\n");

    loop {
        // Checking VLC connection
        let vlc_alive = vlc_client.as_mut().is_some_and(|c| c.ping());

        if !vlc_alive {
            if was_connected {
                println!("  VLC disconnected. Clearing Discord status...");
                clear_discord_presence(&mut discord_client);
                last_title = None;
                cached_presence = None;
                was_connected = false;
                waiting_logged = false;

                // Exit if run mode is exit_on_close
                if config.app.run_mode == RunMode::ExitOnClose {
                    println!("  Run mode is 'exit_on_close'. Exiting...");
                    let _ = discord_client.close();
                    return;
                }
            }

            vlc_client = None;
            match vlc::VlcClient::connect(&config.vlc.host, config.vlc.port) {
                Ok(client) => {
                    println!("  Connected to VLC at {}.", config.vlc_address());
                    vlc_client = Some(client);
                    was_connected = true;
                    waiting_logged = false;
                }
                Err(_) => {
                    if !waiting_logged {
                        println!(
                            "  Waiting for VLC on port {}...",
                            config.vlc.port
                        );
                        // VLC process detected but RC not responding
                        if is_vlc_process_running() {
                            println!("  (VLC is running but RC interface isn't responding.)");
                            println!("  (Restart VLC for auto-configuration to take effect.)\n");
                        }
                        waiting_logged = true;
                    }
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }
            }
        }

        let vlc = vlc_client.as_mut().unwrap();

        // Get playback state
        let state = match vlc.get_state() {
            Ok(s) => s,
            Err(_) => {
                vlc_client = None;
                continue;
            }
        };

        match state {
            PlaybackState::Stopped => {
                if last_title.is_some() {
                    println!("  Playback stopped.");
                    clear_discord_presence(&mut discord_client);
                    last_title = None;
                    cached_presence = None;
                }
            }

            PlaybackState::Playing | PlaybackState::Paused => {
                // Get current media info 
                let current_title = match vlc.get_title() {
                    Ok(Some(t)) => t,
                    Ok(None) => {
                        tokio::time::sleep(poll_interval).await;
                        continue;
                    }
                    Err(_) => {
                        vlc_client = None;
                        continue;
                    }
                };

                // Get time info for timestamp
                let elapsed = vlc.get_time().unwrap_or(None);
                let total_length = vlc.get_length().unwrap_or(None);
                let time_remaining = match (elapsed, total_length) {
                    (Some(e), Some(t)) if t > e => Some(t - e),
                    _ => None,
                };

                // Fetch TMDB data if title changed
                let title_changed = last_title.as_ref() != Some(&current_title);

                if title_changed {
                    println!("  Now playing: {}", current_title);
                    let cleaned = title::clean_title(&current_title);
                    println!(
                        "  Cleaned: \"{}\" | Year: {:?} | Episode: {:?}",
                        cleaned.name, cleaned.year, cleaned.season_episode
                    );

                    let presence = if let Some((season, episode)) = cleaned.season_episode {
                        fetch_tv_presence(&cleaned.name, season, episode, api_key).await
                    } else {
                        fetch_movie_presence(&cleaned.name, cleaned.year, api_key).await
                    };

                    match presence {
                        Some(info) => {
                            println!("  Found: \"{}\" - {}", info.title, info.details);
                            update_discord_presence(
                                &mut discord_client,
                                &info,
                                &state,
                                time_remaining,
                                config.discord.show_time_remaining,
                            );
                            cached_presence = Some(info);
                        }
                        None => {
                            println!("  Not found on TMDB. Showing basic status.");
                            let basic_info = PresenceInfo {
                                title: cleaned.name.clone(),
                                details: "Watching".to_string(),
                                poster_url: String::new(),
                                imdb_url: None,
                                tmdb_url: String::new(),
                            };
                            update_basic_presence(
                                &mut discord_client,
                                &cleaned.name,
                                "Watching",
                            );
                            cached_presence = Some(basic_info);
                        }
                    }

                    last_title = Some(current_title);
                } else if let Some(ref info) = cached_presence {
                    // Same title — update timestamp/state
                    update_discord_presence(
                        &mut discord_client,
                        info,
                        &state,
                        time_remaining,
                        config.discord.show_time_remaining,
                    );
                }
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
