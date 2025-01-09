use serde::{ Serialize, Deserialize };
use std::fs;
use serde_json;
use once_cell::sync::Lazy;

#[derive(Serialize, Deserialize, Debug)]
pub enum NodeType {
    Regular, // Sends txs, blocks and is a miner
    Light, // Sends txs and browses blockchain
    Miner, // Mines blocks
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Settings {
    pub fullscreen: bool,
    pub resolution: (f32, f32),
    pub default_wallet: String,
    pub max_blocks_loaded: usize,

    // Node Settings
    pub node_type: NodeType,
    pub blockchain_state_check_interval: u64,
    pub preferred_miner_address: String,
    pub server_port: String,    // [PORT]
    pub bootstrap_node: String, // 198.2.2.5:[PORT]
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // Application Settings
            fullscreen: false,
            resolution: (1000.0, 600.0),
            default_wallet: String::new(),
            max_blocks_loaded: 50,

            // Node Settings
            node_type: NodeType::Regular,
            preferred_miner_address: String::new(),
            blockchain_state_check_interval: 20,
            server_port: String::from("8334"),
            bootstrap_node: String::from("127.0.0.1:8335"),
        }
    }
}

impl Settings {
    pub fn load(path: &str) -> Self {
        match fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|_| Self::default()),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &str) {
        if let Ok(contents) = serde_json::to_string_pretty(self) {
            println!("Saving Application's Settings.");
            let _ = fs::write(path, contents); // Handle errors as needed
        }
    }
}

// Define a globally accessible Settings instance
pub static SETTINGS: Lazy<Settings> = Lazy::new(|| {
    // Load settings from a file or use defaults
    println!("Loading global application SETTINGS");
    Settings::load("settings.json") // Replace with your desired settings file path
});