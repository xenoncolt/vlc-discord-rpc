use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result};

const PROMPT: u8 = b'>';

// The current playback state of VLC
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

// A lightweight TCP client for VLC's RC (Remote Control) interface
pub struct VlcClient {
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
}

impl VlcClient {
    // Connect to VLC's RC interface at the given host and port
    // Tries all resolved addresses (IPv4 first) to handle localhost ambiguity
    pub fn connect(host: &str, port: u16) -> Result<Self> {
        let addr_str = format!("{}:{}", host, port);
        let addrs: Vec<_> = addr_str.to_socket_addrs()
            .context(format!("Failed to resolve {}", addr_str))?
            .collect();

        // Sort so IPv4 addresses come first (more reliable on Windows)
        let mut sorted_addrs = addrs.clone();
        sorted_addrs.sort_by_key(|a| if a.is_ipv4() { 0 } else { 1 });

        let mut last_err = None;
        for addr in &sorted_addrs {
            match TcpStream::connect_timeout(addr, Duration::from_secs(2)) {
                Ok(stream) => {
                    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
                    stream.set_write_timeout(Some(Duration::from_secs(3)))?;

                    let mut reader = BufReader::new(stream.try_clone()?);

                    // Consume VLC's greeting message (ends with '>')
                    let mut greeting = Vec::new();
                    let _ = reader.read_until(PROMPT, &mut greeting);

                    let writer = BufWriter::new(stream);
                    return Ok(Self { reader, writer });
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        Err(anyhow::anyhow!(
            "Failed to connect to VLC at {}: {}",
            addr_str,
            last_err.map_or("no addresses".to_string(), |e| e.to_string())
        ))
    }

    // Send a command and read response until the next `>` prompt
    // Returns all text before the prompt, cleaned up
    fn command(&mut self, cmd: &str) -> Result<String> {
        writeln!(self.writer, "{}", cmd)?;
        self.writer.flush()?;

        let mut buf = Vec::new();
        self.reader.read_until(PROMPT, &mut buf)?;

        let raw = String::from_utf8_lossy(&buf);

        // Clean: remove trailing '>', trim whitespace
        let cleaned = raw
            .trim()
            .trim_end_matches('>')
            .trim()
            .to_string();
        Ok(cleaned)
    }

    // Check if VLC has media loaded (returns true for both playing and paused)
    pub fn is_playing(&mut self) -> Result<bool> {
        let resp = self.command("is_playing")?;
        // Response might contain multiple lines; take the last meaningful one
        let value = resp.lines().last().unwrap_or("").trim();
        Ok(value == "1")
    }

    // Get the title of the currently loaded media
    // Returns None if nothing is loaded
    pub fn get_title(&mut self) -> Result<Option<String>> {
        let resp = self.command("get_title")?;
        let value = resp.lines().last().unwrap_or("").trim().to_string();
        if value.is_empty() {
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }

    // Get the elapsed playback time in seconds
    pub fn get_time(&mut self) -> Result<Option<u64>> {
        let resp = self.command("get_time")?;
        let value = resp.lines().last().unwrap_or("").trim();
        Ok(value.parse().ok())
    }

    // Get the total duration of the current media in seconds
    pub fn get_length(&mut self) -> Result<Option<u64>> {
        let resp = self.command("get_length")?;
        let value = resp.lines().last().unwrap_or("").trim();
        Ok(value.parse().ok())
    }

    // Get the current playback state by parsing VLC's status output
    pub fn get_state(&mut self) -> Result<PlaybackState> {
        let output = self.command("status")?;

        for line in output.lines() {
            let trimmed = line.trim().to_lowercase();
            if trimmed.contains("state playing") {
                return Ok(PlaybackState::Playing);
            } else if trimmed.contains("state paused") {
                return Ok(PlaybackState::Paused);
            } else if trimmed.contains("state stopped") {
                return Ok(PlaybackState::Stopped);
            }
        }

        // Fallback: use is_playing
        if self.is_playing().unwrap_or(false) {
            Ok(PlaybackState::Playing)
        } else {
            Ok(PlaybackState::Stopped)
        }
    }

    // Quick check if the connection is still alive
    pub fn ping(&mut self) -> bool {
        self.is_playing().is_ok()
    }
}

impl Drop for VlcClient {
    fn drop(&mut self) {
        let _ = self.writer.get_ref().shutdown(Shutdown::Both);
    }
}
