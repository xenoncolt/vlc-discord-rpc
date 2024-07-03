use std::{env, fs, process::Command, io::{stdin, stdout, Write}, path};
use semver::Version;

const GITHUB_API: &str = "https://api.github.com/repos/xenoncolt/vlc-discord-rpc/releases/latest";

pub async fn update() {
    println!("Checking version...");
    let current_version = env!("CARGO_PKG_VERSION");
    let current_version = Version::parse(current_version).expect("Failed to parse current version string to struct"); // parse version string to Version struct

    let client = reqwest::Client::builder().user_agent("vlc-discord-rpc").build().expect("Failed to build client");
    let response_client = client.get(GITHUB_API).send().await.expect("Github api is not responding"); // send request to github api
    let release: serde_json::Value = response_client.json().await.expect("Failed to parse response to json"); // parse response to json
    let latest_version = Version::parse(release["tag_name"].as_str().unwrap()).expect("Failed to get the latest version"); // parse latest version string to Version struct

    let exe_path = env::current_dir().unwrap().join("vlc-discord-rpc.exe");

    let temp_file = env::current_dir().unwrap().join("new-version.exe"); 
    let download_url = release["assets"][0]["browser_download_url"].as_str().unwrap();
    let response_url = reqwest::get(download_url).await.expect("Failed to get response from the download url"); 
    let content = response_url.bytes().await.expect("Something Wrong! Latest version is not right or broken");

    // don't know why i do this :<
    if latest_version > current_version {
        println!("You are using an older version v{}, updating...", current_version);
        
        fs::write(&temp_file, &content).expect("Failed to write new exe file"); // new version file as a temporary file

        Command::new("cmd").args(&["/C", "start", temp_file.to_str().unwrap()]).spawn().expect("Failed to start new-version.exe"); // open new-version.exe shell

        std::process::exit(0);
    }
        
    if path::Path::new(&temp_file).exists() { // new-version.exe not exist

        // asking stuff
        let mut input = String::new();
        print!("Do you want to install new version? (y/n): ");
        stdout().flush().unwrap();
        stdin().read_line(&mut input).unwrap();
        if input.trim() == "y" || input.trim() == "yes" {
            // changing file 
            fs::write(&exe_path, content).expect("Failed to replace exe file");
            println!("Updated to version {}", latest_version); //pure useless like me
            println!("Please stand by while we are starting RPC"); //pure useless like me
            Command::new("cmd").args(&["/C", "start", exe_path.to_str().unwrap()]).spawn().expect("Failed to start new version"); // open vlc-discord-rpc.exe shell
        } else {
            print!("Exiting....");
            Command::new("cmd").args(&["/C", "start", exe_path.to_str().unwrap()]).spawn().expect("Failed to restart program");
            return;
        }
        fs::remove_file(&temp_file).expect("Failed to delete new version"); // remove new-version.exe
        std::process::exit(0); 
    }
}
