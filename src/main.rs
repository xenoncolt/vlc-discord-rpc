mod update;

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use std::vec;
use vlc_rc::Client as VlcClient;
use regex::Regex;
use tokio::sync::Mutex;

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


async fn fetch_movie_data(title: &str, api_key: &str) -> Result<MovieData> {
    println!("Fetching movie data for title: {}", title);
    let url = format!("https://api.themoviedb.org/3/search/movie?api_key={}&query={}", api_key, title);
    let response: serde_json::Value = reqwest::get(&url).await?.json().await?;
    // println!("Received response for movie data: {:?}", response);

    if let Some(movie) = response["results"].as_array().and_then(|a| a.get(0)) {
        let title = movie["title"].as_str().unwrap_or("").to_string();
        let genre_ids = movie["genre_ids"].as_array().unwrap_or(&vec![]).iter().map(|gen| gen.as_i64().unwrap_or(0)).collect::<Vec<_>>();
        let genres: Vec<Genre> = fetch_genres(&genre_ids, api_key).await?.into_iter().map(|name| Genre { name }).collect(); // Convert Vec<String> to Vec<Genre>

        let poster_path = movie["poster_path"].as_str().unwrap_or("").to_string();

        let tmdb_id = movie["id"].as_u64().unwrap_or(0) as u32;
        let movie_detail_url = format!("https://api.themoviedb.org/3/movie/{}?api_key={}", tmdb_id, api_key);
        let movie_detail_response: serde_json::Value = reqwest::get(&movie_detail_url).await?.json().await?;

        let imdb_id = movie_detail_response["imdb_id"].as_str().map(|s| s.to_string());

        Ok(MovieData {
            title,
            genres,
            poster_path,
            imdb_id,
            tmdb_id,
        })
    } else {
        Err(anyhow!("Movie not found"))
    }
}

async fn fetch_genres(genre_ids: &[i64], api_key: &str) -> Result<Vec<String>> {
    let url = format!("https://api.themoviedb.org/3/genre/movie/list?api_key={}", api_key);
    let response: serde_json::Value = reqwest::get(&url).await?.json().await?;

    let genres = genre_ids.iter()
        .filter_map(|id| response["genres"].as_array().unwrap_or(&vec![]).iter()
            .find(|g| g["id"].as_i64() == Some(*id)).and_then(|ge| ge["name"].as_str().map(|s| s.to_string())))
        .collect();
    Ok(genres)
}

async fn fetch_tv_show_data(name: &str, api_key: &str) -> Result<TVShowData> {
    println!("Fetching TV Show data for name: {}", name);
    let url = format!("https://api.themoviedb.org/3/search/tv?api_key={}&query={}", api_key, name);
    let response: serde_json::Value = reqwest::get(&url).await?.json().await?;
    // println!("Received response for TV Show data: {:?}", response);

    if let Some(show) = response["results"].as_array().and_then(|a| a.get(0)) {
        let tmdb_id =  show["id"].as_u64().unwrap_or(0) as u32;
        let name = show["name"].as_str().unwrap_or("").to_string();
        let poster_path = show["poster_path"].as_str().unwrap_or("").to_string();

        let show_detail_url = format!("https://api.themoviedb.org/3/tv/{}?api_key={}", tmdb_id, api_key);
        let show_detail_response: serde_json::Value = reqwest::get(&show_detail_url).await?.json().await?;
        let imdb_id = show_detail_response["external_ids"]["imdb_id"].as_str().map(|s|s.to_string());

        Ok(TVShowData {
            tmdb_id,
            name,
            poster_path,
            imdb_id,
        })
    } else {
        Err(anyhow!("TV Show not found"))
    }
}

async fn fetch_episode_data(tmdb_id: u32, season: u32, episode: u32, api_key: &str) -> Result<EpisodeData> {
    let url = format!("https://api.themoviedb.org/3/tv/{}/season/{}/episode/{}?api_key={}", tmdb_id, season, episode, api_key);
    let response: serde_json::Value = reqwest::get(&url).await?.json().await?;
    
    let name = response["name"].as_str().unwrap_or("").to_string();

    Ok(EpisodeData { name })
}

fn update_discord_presence(
    client: &mut DiscordIpcClient,
    title: &str,
    details: &str,
    large_image_key: &str,
    imdb_url: Option<&str>,
    tmdb_url: &str,
) {
    // println!("Update Discord Rich Presence: {}, Details: {}, Image: {}", title, details, large_image_key);
    let mut button: Vec<activity::Button> = vec![activity::Button::new("TMDB", tmdb_url)];
    if let Some(imdb_url) = imdb_url {
        button.push(activity::Button::new("IMDB", imdb_url));
    }
    let _ = client.set_activity(activity::Activity::new()
        .state(details)
        .details(title)
        .assets(activity::Assets::new().large_image(large_image_key).large_text(title))
        .buttons(button)
    );
}

fn clean_title(title: &str) -> (String, Option<(u32, u32)>) {
    // Extract season and episode information if present
    let re = Regex::new(r"S(\d{2})E(\d{2})").unwrap();
    if let Some(caps) = re.captures(&title) {
        let season: u32 = caps[1].parse().unwrap();
        let episode: u32 = caps[2].parse().unwrap();

        // Extract and clean title by removing any text after the episode information
        let cleaned_title = &title[..caps.get(0).unwrap().end()];

        // Remove extra information after year
        let re = Regex::new(r"\d{4}").unwrap();
        let cleaned_title = &cleaned_title[..re.find(&cleaned_title).unwrap().end()];

        // Remove year
        let re = Regex::new(r"\d{4}").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, "");

        // Remove extra information in brackets or parentheses
        let re = Regex::new(r"[\[\(].*?[\]\)]").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, "");

        // Replace dots with spaces
        let re = Regex::new(r"\.").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, " ");

        // Remove parentheses
        let re = Regex::new(r"\{").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, "");
        
        let re = Regex::new(r"\}").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, "");

        // Remove brackets
        let re = Regex::new(r"\(").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, "");

        let re = Regex::new(r"\(").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, "");

        // Remove hyphens
        let re = Regex::new(r"\-").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, "");

        // Remove multiple spaces
        let re = Regex::new(r"\s+").unwrap();
        let cleaned_title = re.replace_all(&cleaned_title, " ").to_string();

        // Remove everything before hyphen
        // let re = Regex::new(r".*-\s*").unwrap();
        // let cleaned_title = re.replace_all(&cleaned_title, "");

        return (cleaned_title.trim().to_string(), Some((season, episode)));
    }

    // Proceed with normal cleaning if no season and episode info found
    // Remove file extension
    let re = Regex::new(r"\.[a-zA-Z0-9]+$").unwrap();
    let cleaned_title = re.replace_all(&title, "");

    // Remove extra information in brackets or parentheses
    let re = Regex::new(r"[\[\(].*?[\]\)]").unwrap();
    let cleaned_title = re.replace_all(&cleaned_title, "");

    // Remove extra information after year
    let re = Regex::new(r"\.\d{4}.*").unwrap();
    let cleaned_title = re.replace_all(&cleaned_title, "");

    // Replace dots with spaces
    let re = Regex::new(r"\.").unwrap();
    let cleaned_title = re.replace_all(&cleaned_title, " ");
    
    // Remove everything before hyphen
    let re = Regex::new(r".*-\s*").unwrap();
    let cleaned_title = re.replace_all(&cleaned_title, "");

    (cleaned_title.to_string(), None)
}

#[tokio::main]
async fn main() {
    // checking update
    
    update::update().await; // i know i waste my time 
    println!("Starting VLC Discord RPC...");

    // let client_id = env::var("CLIENT_ID").expect("CLIENT_ID must be set in .env file");
    // let api_key = env::var("API_KEY").expect("API_KEY must be set in .env file");
    let client_id = env!("CLIENT_ID");
    let api_key = env!("API_KEY");

    let mut discord_client = DiscordIpcClient::new(client_id).expect("Failed to create DiscordIpcClient");
    discord_client.connect().expect("Failed to connect to discord client.");
    println!("Discord client started.");

    let vlc_host = "127.0.0.1:9090";

    let vlc_client = Arc::new(Mutex::new(
        match VlcClient::connect(vlc_host) {
            Ok(client) => {
                println!("Connected to VLC at {}", vlc_host);
                client
            }
            Err(e) => {
                println!("Failed to connect to VLC: {:?}", e);
                return;
            }
        }
    ));

    loop {
        println!("Checking if VLC is playing...");
            if vlc_client.lock().await.is_playing().unwrap_or(false) {
                println!("VLC is playing...");

                if let Ok(Some(title)) = vlc_client.lock().await.get_title() {
                    let (cleaned_title, season_episode) = clean_title(&title);
                    // println!("Now playing: {:?}", title);
                    // println!("Cleaned title: {:?}", cleaned_title);


                    if let Some((season, episode)) = season_episode {
                        if let Ok(tv_show_data) = fetch_tv_show_data(&cleaned_title, &api_key).await {
                            if let Ok(episode_data) = fetch_episode_data(tv_show_data.tmdb_id, season, episode, &api_key).await {
                                let details = format!("{} S{:02}:E{:02}", tv_show_data.name, season, episode);
                                let episode_title = if episode_data.name.is_empty() {
                                    tv_show_data.name
                                } else {
                                    episode_data.name
                                };

                                let poster_url = format!("https://image.tmdb.org/t/p/w500{}", tv_show_data.poster_path);

                                let imdb_url = tv_show_data.imdb_id.as_deref().map(|id| format!("https://www.imdb.com/title/{}/", id));
                                let tmdb_url = format!("https://www.themoviedb.org/tv/{}", tv_show_data.tmdb_id);

                                update_discord_presence(&mut discord_client, &episode_title, &details, &poster_url, imdb_url.as_deref(), &tmdb_url)
                            } else {
                                println!("Could not find episode data for title: {:?}", title);
                            }
                        } else {
                            println!("Could not find TV show data for title: {:?}", title);
                        }
                    } else {
                        if let Ok(movie_data) = fetch_movie_data(&cleaned_title, &api_key).await {
                            // println!("Fetched movie data: {:?}", movie_data);
                            let genres: Vec<String> = movie_data.genres.iter().map(|gen| gen.name.clone()).collect();
                            let details = format!("Genres: {}", genres.join(", "));
                            let poster_url = format!("https://image.tmdb.org/t/p/w500{}", movie_data.poster_path);
            
                            let imdb_url = movie_data.imdb_id.as_deref().map(|id| format!("https://www.imdb.com/title/{}/", id));
                            let tmdb_url = format!("https://www.themoviedb.org/movie/{}", movie_data.tmdb_id);
            
                            update_discord_presence(&mut discord_client, &movie_data.title, &details, &poster_url, imdb_url.as_deref(), &tmdb_url);
                        } else {
                            println!("Could not find movie data for title: {:?}", title);
                        }
                    }
                } else {
                    println!("Could not retrieve title from VLC.");
                }
        } else {
            println!("VLC is not playing.");
        }
        sleep(Duration::from_secs(10)); // Adjust the sleep duration as needed
    }
}
