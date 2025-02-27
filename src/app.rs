use chrono::{DateTime, NaiveDateTime, Utc};
use eframe::egui;
use egui::Ui;
use failure::Fail;
use reqwest;
use bitcoincash_addr::Address;
use crypto::ed25519;
use hex;
use log::error;
use std::sync::Arc;
use tokio::sync::{ oneshot, RwLock, mpsc };

// My Crates
use crate::blockchain::Blockchain;
use crate::block::Block;
use crate::errors::Result;
use crate::server::{self, Server};
use crate::transaction::Transaction;
use crate::tx::TXOutputs;
use crate::utxoset::UTXOSet;
use crate::wallet::*;
use crate::runtime::RUNTIME;    // Import the global runtime (tokio)
use crate::settings::SETTINGS;  // Application Settings

#[derive(Debug, Fail)]
pub enum WalletImportError {
    #[fail(display = "Invalid secret key format")]
    InvalidSecretKeyFormat,
    // Add other error types here as needed
}

enum Tab {
    Blockchain,
    Transactions,
    Wallets,
    Peers,
    Settings,
}

struct Notification {
    pub id: u32,              // Unique ID for each notification
    pub message: String,
    pub start_time: std::time::Instant,  // When the notification was created
    pub duration: u64,        // Duration in seconds before auto-dismissal
}

#[derive(Debug)]
pub enum TaskMessage {
    BalancesUpdated(Vec<i32>),
    Error(String),
    TransactionSent(bool),
    PeerAdded(String),
}

pub struct BlockchainModule {
    wallets: Wallets,
    balances: Vec<i32>,
    utxo_set: Arc<RwLock<UTXOSet>>,
}

pub struct NetworkModule {
    public_ip: Option<Result<String>>, // Use the custom Result type here
    server: Arc<RwLock<Server>>,
}

pub struct NotificationModule {
    notifications: Vec<Notification>,
    notification_counter: u32,
}

pub struct UIState {
    active_tab: Tab, // -

    // Blockchain Tab
    blocks: Vec<Block>,
    show_transactions: bool,
    blocks_to_display: usize,
    block_search_query: String,
    block_search_result: Option<Block>,

    // Transaction Tab
    selected_wallet: Option<String>,
    receiver_address: String,
    tx_amount: i32,
    tx_gas_price: i32,
    tx_gas_limit: i32,

    // Wallet Tab
    show_delete_popup: Option<String>,
    show_add_existing_wallet_popup: bool,

    // Peers Tab
    peer_ip_address_input: String,
    connected_peers_displayed: Vec<String>,
}

pub struct MyApp {
    bc_module: BlockchainModule,
    net_module: NetworkModule,
    ui_state: UIState,

    sender: mpsc::Sender<TaskMessage>,
    receiver: mpsc::Receiver<TaskMessage>,
    
    // the popups basically
    notif_module: NotificationModule,

}

impl MyApp {
    pub async fn initialize_async() -> Result<Self> {
        let wallets = Wallets::new()?; 

        let (sender, receiver) = mpsc::channel(100);

        // Retrieve first wallet and its address. 
        let mining_address =  wallets.get_all_address().get(0).cloned().unwrap_or_default();
        
        // Uncomment to create a new blockchain with a new genesis block and genesis address (Use for Custom)        
        /*
            let address = wallets.create_wallet();        
            let blockchain = Blockchain::create_blockchain(address.clone())?;
        */        

        // This can either load the existing blockchain or create a new genesis block. (Standard way)
        let blockchain = Arc::new(RwLock::new(Blockchain::new()?));
        let utxo_set = Arc::new(RwLock::new(UTXOSet::new(Arc::clone(&blockchain))));
        utxo_set.write().await.reindex().await?;

        let mut current_blocks:Vec<Block> = Vec::new();

        // Load node's blockchain blocks
        for block_hash in &blockchain.read().await.get_block_hashes() {
            current_blocks.push( blockchain.read().await.get_block(block_hash)?.clone() );
        }
        
        // Create a Server and loop it
        let server = Arc::new(RwLock::new(Server::new("8334", &mining_address, Arc::clone(&utxo_set))?));

        tokio::spawn({
            let server_clone = Arc::clone(&server);
            async move {
                //println!("Starting server with instance: {:?}", Arc::as_ptr(&server_clone));

                if let Err(e) = Server::start_server(server_clone).await {
                    error!("Server error: {}", e);
                }
            }
        });

        let mut connected_peer_ips: Vec<String> = Vec::new();
        for address_string in &server.read().await.get_known_nodes().await {
            connected_peer_ips.push(address_string.to_string());
        }

        // Fetch Public IP
        let public_ip_result = get_public_ip()
            .await
            .map_err(|e| failure::format_err!("Failed to retrieve public IP: {}", e));

        let public_ip = match public_ip_result {
            Ok(ip) => Some(Ok(ip)),
            Err(e) => Some(Err(e)),
        };
        
        // Update Balances
        let balances: Vec<i32> = Vec::new();
        let new_balances = MyApp::calculate_new_balances(&wallets, Arc::clone(&utxo_set)).await?;
        let _ = sender.send(TaskMessage::BalancesUpdated(new_balances)).await;


        //println!("Server instance: {:?} init_async", Arc::as_ptr(&server));

        let app = MyApp {
            bc_module: BlockchainModule{
                wallets: wallets,
                balances: balances,
                utxo_set: Arc::clone(&utxo_set),
            },
            net_module: NetworkModule {
                public_ip: public_ip, // Use the custom Result type here
                server: Arc::clone(&server),
            },

            ui_state: UIState {
                active_tab: Tab::Blockchain, 

                // Blockchain Tab
                blocks: current_blocks,
                show_transactions: false,
                blocks_to_display: 5,
                block_search_query: String::new(),
                block_search_result: None,

                // Transaction Tab
                selected_wallet: None,
                receiver_address: String::from(""),
                tx_amount: 0,
                tx_gas_price: 0,
                tx_gas_limit: 0,

                // Wallets Tab
                show_delete_popup: None,
                show_add_existing_wallet_popup: false, 

                // Peers Tab
                peer_ip_address_input: String::new(),
                connected_peers_displayed: connected_peer_ips,
            },

            notif_module: NotificationModule {
                notifications: Vec::new(),
                notification_counter: 0,
            },

            sender: sender,
            receiver: receiver,
        };

        Ok(app)
    }

    // calculates and returns new balances (vector of i32)
    pub async fn calculate_new_balances(wallets: &Wallets, utxo_set: Arc<RwLock<UTXOSet>>) -> Result<Vec<i32>> {
        let mut new_balances = Vec::new();
        
        for address in wallets.get_all_address() {            
            let pub_key_hash = Address::decode(&address).unwrap().body;

            // Find all UTXOs for this address
            let utxos: TXOutputs = utxo_set.read().await.find_utxo(&pub_key_hash).unwrap_or_else(|_| {
                TXOutputs {
                    outputs: vec![],
                }
            });

            // Calculate the total balance for this address
            let balance: i32 = utxos.outputs.iter().map(|out| out.value).sum();
            
            //println!("address: {}, balance: {}", &address, &balance);

            // Add the balance to the vector
            new_balances.push(balance);
        }

        // Update the balances in the app state
        println!("Balances updated!");
        Ok(new_balances)
    }

    /// Retrieves the balance for a given wallet address.
    /// Returns `None` if the address is not found in the wallets list.
    pub fn get_balance(&self, address: &str) -> Option<i32> {
        if let Some(index) = self.bc_module.wallets.get_all_address().iter().position(|a| a == address) {
            self.bc_module.balances.get(index).copied()
        } else {
            None
        }
    }

    pub fn total_balance(&self) -> i32 {
        self.bc_module.balances.iter().sum()
    }

    pub fn delete_wallet(&mut self, address: &str) -> Result<()> {
        self.bc_module.wallets.delete_wallet(address)?;

        let message = format!("Wallet Deleted (Address): {}", &address);
        self.add_notification(message);

        // Update balances: Assuming balances align with wallet order
        if let Some(index) = self.bc_module.wallets.get_all_address().iter().position(|a| a == address) {
            self.bc_module.balances.remove(index);
        }

        let wallets = self.bc_module.wallets.clone(); // contains the new wallet
        let utxo_set = Arc::clone(&self.bc_module.utxo_set);
        let sender = self.sender.clone();

        RUNTIME.spawn(async move {
            match MyApp::calculate_new_balances(&wallets, utxo_set).await {
                Ok(new_balances) => {
                    sender.send(TaskMessage::BalancesUpdated(new_balances))
                        .await
                        .unwrap_or_else(|e| println!("Failed to send balances: {}", e));
                }
                Err(err) => {
                    sender.send(TaskMessage::Error(err.to_string()))
                        .await
                        .unwrap_or_else(|e| println!("Failed to send error: {}", e));
                }
            }
        });

        Ok(())
    }

    pub fn export_wallet_to_file(&self, address: &str, wallet: &Wallet) -> Result<()> {
        use std::fs::File;
        use std::io::Write;

        let file_name = format!("data/wallets/export/{}_wallet.dat", address);
        let mut file = File::create(&file_name)?;

        let serialized_wallet = bincode::serialize(wallet)?;
        file.write_all(&serialized_wallet)?;
        
        let msg = String::from(format!("Wallet exported to file: {}", file_name));
        println!("{}", msg);

        Ok(())
    }

     // Method for importing wallet from .dat file
    fn import_wallet_from_file(&self, path: std::path::PathBuf) -> Result<Wallet> {
        // Read the file content and deserialize it
        let file_content = std::fs::read(path).map_err(|_| WalletImportError::InvalidSecretKeyFormat)?;
        let wallet: Wallet = bincode::deserialize(&file_content).map_err(|_| WalletImportError::InvalidSecretKeyFormat)?;
        Ok(wallet)
    }

    // Method for importing wallet from secret key
    fn import_wallet_from_key(&self, secret_key: &str) -> Result<Wallet> {
        // parbaudit vai ir pareizs length
        // Convert the secret key into bytes and generate the corresponding public key
        let secret_key_bytes = hex::decode(secret_key).map_err(|_| WalletImportError::InvalidSecretKeyFormat)?;
       
        let (public_key, _) = ed25519::keypair(&secret_key_bytes);
        let wallet = Wallet {
            secret_key: secret_key_bytes,
            public_key: public_key.to_vec(),
        };
        Ok(wallet)
    }

    fn valid_tx_fields(&self) -> Result<(String, Wallet, String, i32)> {
        let selected_wallet_name = self
            .ui_state
            .selected_wallet
            .as_ref()
            .ok_or_else(|| failure::err_msg("No wallet selected"))?
            .clone();
    
        println!("From: {}", selected_wallet_name);
    
        let wallet = self
            .bc_module
            .wallets
            .get_wallet(&selected_wallet_name)
            .ok_or_else(|| failure::err_msg("Wallet not found for the selected address"))?;
    
        if self.ui_state.receiver_address.is_empty() {
            return Err(failure::err_msg("Receiver address cannot be empty"));
        }
    
        println!("To: {}", self.ui_state.receiver_address);
    
        if self.ui_state.tx_amount <= 0 {
            return Err(failure::err_msg("Transaction amount must be greater than zero"));
        }
    
        println!("Amount: {}", self.ui_state.tx_amount);
    
        Ok((
            selected_wallet_name,
            wallet.clone(),
            self.ui_state.receiver_address.clone(),
            self.ui_state.tx_amount,
        ))
    }

    pub async fn send_transaction(
        selected_wallet_name: String,
        wallet: Wallet,
        receiver_address: String,
        tx_amount: i32,
        utxo_set: Arc<RwLock<UTXOSet>>,
        server: Arc<RwLock<Server>>,
    ) -> Result<bool> {
        let tx = Transaction::new_utxo(&wallet, &receiver_address, tx_amount, &utxo_set)
            .await
            .map_err(|e| failure::err_msg(e))?;
    
        let mine_now = false;

        if mine_now {
            let cbtx = Transaction::new_coinbase(selected_wallet_name, String::from("reward!"))
                .map_err(|e| failure::err_msg(e))?;
    
            let new_block = utxo_set.write().await
                .blockchain.write().await
                .mine_block(vec![cbtx, tx])
                .map_err(|e| failure::err_msg(e))?;
    
            utxo_set.write().await
                .update(&new_block)
                .map_err(|e| failure::err_msg(e))?;

        } else {
            server.write().await.send_transaction(&tx).await?;
        }
    
        Ok(true)
    }
    
    
    fn preview_transaction(&self) {

        // display popup

    }

    fn clear_transaction_form(&mut self){
        // Transaction Tab
        self.ui_state.selected_wallet = None;
        self.ui_state.receiver_address = String::from("");
        self.ui_state.tx_amount = 0;
        self.ui_state.tx_gas_price = 0;
        self.ui_state.tx_gas_limit = 0;
    }

    pub fn add_notification(&mut self, message: String) {
        let notification = Notification {
            id: self.generate_notification_id(),
            message,
            start_time: std::time::Instant::now(),
            duration: 10, // 10 seconds
        };

        self.notif_module.notifications.push(notification);
    }

    fn generate_notification_id(&mut self) -> u32 {
        self.notif_module.notification_counter += 1;
        self.notif_module.notification_counter
    }


    fn add_peer(&mut self, new_peer: String) -> Result<()> {        
        let sender = self.sender.clone();
        let server_clone = Arc::clone(&self.net_module.server);

        //println!("Server instance: {:?} add_peer", Arc::as_ptr(&server_clone));

        let new_peer_ip = new_peer + ":8337";
        //println!("New_peer_ip: {}", new_peer_ip.clone());
        
        RUNTIME.spawn( async move {
            match server_clone.write().await.add_peer(new_peer_ip.clone()).await {
                Ok(_result) => {
                    /*println!("ok");

                    // gets stuck here.
                    let nodes = {
                        let guard = server_clone.read().await;
                        guard.get_known_nodes().await // Ensure this is necessary
                    };

                    for peer in nodes {
                        println!("Peer: {}", peer);
                    }

                    println!("ok2");*/
                    let _ = sender.send(TaskMessage::PeerAdded(new_peer_ip)).await;
                }
                Err(err) => {
                    println!("Error while adding peer: {}", err);
                }
            }
        });

        Ok(())
    }
}

impl Default for MyApp {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel(100);        
        
        // Create the `utxo_set` first, since it is needed by `server`
        let utxo_set = Arc::new(RwLock::new(UTXOSet {
            blockchain: Arc::new(RwLock::new(Blockchain::default_empty())),
        }));

        // Use `utxo_set` to create the `server`
        let server = Arc::new(RwLock::new(Server::new("8334", "", Arc::clone(&utxo_set)).unwrap()));

        
        Self {
            bc_module: BlockchainModule {
                wallets: Wallets::default(),
                balances: Vec::new(),
                utxo_set: utxo_set,
            },
    
            net_module: NetworkModule {
                public_ip: None,
                server: server,
            },
    
            ui_state: UIState {
                active_tab: Tab::Blockchain,
    
                // Blockchain Tab
                blocks: Vec::new(),
                show_transactions: false,
                blocks_to_display: 5,
                block_search_query: String::new(),
                block_search_result: None,
    
                // Transaction Tab
                selected_wallet: None,
                receiver_address: String::from(""),
                tx_amount: 0,
                tx_gas_price: 0,
                tx_gas_limit: 0,
    
                // Wallets Tab
                show_delete_popup: None,
                show_add_existing_wallet_popup: false, 

                // Peers Tab
                peer_ip_address_input: String::new(),
                connected_peers_displayed: Vec::new(),
            },
            
            notif_module: NotificationModule {
                // Notification Tab
                notifications: Vec::new(),
                notification_counter: 0,
            },

            sender: sender,
            receiver: receiver,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {       
        let bg_color = egui::Color32::from_rgb(30, 30, 30); // Dark gray
        let visuals = egui::Visuals {
            override_text_color: Some(egui::Color32::WHITE), // Optional: Ensure white text
            ..egui::Visuals::dark() // Base dark theme
        };
        ctx.set_visuals(visuals);

        ctx.set_style({
            let mut style = (*ctx.style()).clone();

            style.visuals.window_fill = bg_color;
            style.spacing.button_padding = egui::vec2(15.0, 10.0); // Custom padding

            style
        });

        // Render the UI
        egui::CentralPanel::default().show(ctx, |ui| {
            
            // Navigation bar at the top
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new("Blockchain").size(16.0)).clicked() {
                    self.ui_state.active_tab = Tab::Blockchain;
                }
                if ui.button(egui::RichText::new("Transactions").size(16.0)).clicked() {
                    self.ui_state.active_tab = Tab::Transactions;
                }
                if ui.button(egui::RichText::new("Wallets").size(16.0)).clicked() {
                    self.ui_state.active_tab = Tab::Wallets;
                }
                if ui.button(egui::RichText::new("Peers").size(16.0)).clicked() {
                    self.ui_state.active_tab = Tab::Peers;
                }
                if ui.button(egui::RichText::new("Settings").size(16.0)).clicked() {
                    self.ui_state.active_tab = Tab::Settings;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let wallet_count = self.bc_module.wallets.get_all_address().len();
                    
                    let text = if wallet_count > 0 {
                        format!("Connected Wallets: {}", wallet_count)
                    } else {
                        "No Wallets Connected".to_owned()
                    };

                    ui.add_space(10.0);
                    if ui.add(
                            egui::Label::new(egui::RichText::new(text))
                                .sense(egui::Sense::click()), // Make it interactive          
                        )
                        .on_hover_text("Go to Wallets tab") // Optional tooltip
                        .on_hover_cursor(egui::CursorIcon::PointingHand) // Change cursor to pointer
                        .clicked(){
                            self.ui_state.active_tab = Tab::Wallets;
                        };
                });
            });

            // Section rendering based on the active tab
            ui.separator(); // Add a visual separator

            match self.ui_state.active_tab {
                Tab::Blockchain => self.render_blockchain_section(ui),
                Tab::Transactions => self.render_transactions_section(ui),
                Tab::Wallets => self.render_wallets_section(ui),
                Tab::Peers => self.render_peers_section(ui),
                Tab::Settings => self.render_settings_section(ui),
            }

            // Channel message rendering
            self.render_channel_messages(ctx);

            // Notification rendering
            self.render_notifications(ctx);

        }); 
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) { // automatically every 30 seconds
        /*  you can store user preferences, settings, or wallets. */
        // Use it to save non-critical data that doesn't need to be immediately updated on every change
        // Melnraksta Transactions... ; Mempool?
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Saves Wallets on disk
        if let Err(e) = self.bc_module.wallets.save_all() {
            eprintln!("Failed to save wallets on exit: {}", e);
        } else {
            println!("Wallets successfully saved on exit.");
        }
        
        // Settings
        SETTINGS.save("settings.json");
        
        println!("Application exiting. Cleaning up resources...");
    }

}


// Methods for rendering each section
impl MyApp {
    fn render_blockchain_section(&mut self, ui: &mut egui::Ui) {

        ui.horizontal(|ui|{
            ui.vertical(|ui|{
                ui.heading("Blockchain");
                ui.label("View and analyze the blockchain.");
            });
    
            if ui.button("Toggle Transactions").clicked() {
                self.ui_state.show_transactions = !self.ui_state.show_transactions;
            }
    
            ui.label(format!(" Current Height: {}", &self.ui_state.blocks.first().unwrap().get_height() ));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Search input
                ui.horizontal(|ui| {
                    let placeholder = "Enter Height or Hash"; // Placeholder text
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.ui_state.block_search_query)
                            .hint_text(placeholder),
                    );

                    // Update search result dynamically
                    if response.changed() {
                        if self.ui_state.block_search_query.trim().is_empty() {
                            self.ui_state.block_search_result = None;
                        } else {
                            self.ui_state.block_search_result = self
                                .ui_state.blocks
                                .iter()
                                .find(|block| {
                                    block.get_height().to_string() == self.ui_state.block_search_query
                                        || block.get_hash() == self.ui_state.block_search_query
                                })
                                .cloned();
                        }
                    }
                });
            });    
        });
        ui.add_space(5.0);

        // Scrollable display section
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.vertical(|ui| {
                match &self.ui_state.block_search_result {
                    Some(block) => {
                        // Render only the searched block
                        MyApp::render_block(ui, block, self.ui_state.show_transactions);
                    }
                    None => {
                        for block in self.ui_state.blocks.iter().take(self.ui_state.blocks_to_display) {
                            MyApp::render_block(ui, block, self.ui_state.show_transactions);
                            ui.add_space(15.0);
                        }
                    
                        // Load More button
                        if self.ui_state.blocks_to_display < self.ui_state.blocks.len() {
                            ui.vertical_centered(|ui| {
                                if ui.button("Load More Blocks").clicked() {
                                    self.ui_state.blocks_to_display += 20; // Increment by 20 blocks
                                }
                            });
                        }
                    }
                }
            });
        });
    }

    // Function to render a single block
    fn render_block(ui: &mut egui::Ui, block: &Block, show_transactions: bool) {
        egui::Frame::none()
            .rounding(egui::Rounding::same(5.0))
            .fill(egui::Color32::from_rgb(20, 20, 20))
            .inner_margin(egui::Margin::same(10.0))
            .stroke(egui::Stroke::new(2.0, egui::Color32::DARK_GRAY))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(format!("{}", block.get_height()));
                    ui.label(format!("Block Hash: {}", block.get_hash()));
                    ui.label(format!("Previous Hash: {}", block.get_prev_hash()));
                    ui.label(format!("Timestamp: {}", convert_timestamp(block.get_timestamp())));
                    ui.label(format!("Nonce: {}", block.get_nonce()));

                    

                    if show_transactions {
                        ui.add_space(10.0);
                        egui::Frame::none()
                            .rounding(egui::Rounding::same(5.0))
                            .fill(egui::Color32::from_rgb(50, 50, 50))
                            .inner_margin(egui::Margin::same(10.0))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::WHITE))
                            .show(ui, |ui| {
                                ui.label("Transactions:");
                                for tx in block.get_transactions() {
                                    ui.label(format!("Tx ID: {}", tx.id));
                                }
                            });
                    }
                });
            });
    }
    
    fn render_transactions_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Transactions");
        ui.label("View and create transactions.");

        egui::Frame::none()
        .rounding(egui::Rounding::same(5.0))
        .fill(egui::Color32::from_rgb(20 ,20 , 20 ))
        .inner_margin(egui::Margin::symmetric(20.0, 20.0)) 
        .stroke(egui::Stroke::new(1.0, egui::Color32::BLACK))
        .show(ui, |ui| {
            ui.heading("Create New Transaction");

            // Wallet Selection
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("From Wallet:"));
            
                // Borrow the wallets before the closure to avoid borrowing `self` inside
                let wallet_entries: Vec<(String, String)> = self
                    .bc_module
                    .wallets
                    .iter()
                    .map(|(address, _wallet)| {                        
                        let balance = self.get_balance(&address).unwrap_or(0);
                        let display_text = format!("{} - {} coins", address, balance);
                        (address.clone(), display_text)
                    })
                    .collect();
            
                // Use the collected data in the dropdown
                egui::ComboBox::from_label("")
                    .selected_text(self.ui_state.selected_wallet.clone().unwrap_or("Select Wallet".into()))
                    .show_ui(ui, |ui| {
                        for (address, display_text) in wallet_entries {
                            if ui.selectable_value(&mut self.ui_state.selected_wallet, Some(address.clone()), display_text).clicked() {
                                self.ui_state.selected_wallet = Some(address);
                            }
                        }
                    });
            });
            
            if let Some(wlt_address) = &self.ui_state.selected_wallet {
                let available_funds = self.get_balance(&wlt_address).unwrap_or(0);
                ui.label(egui::RichText::new(format!("Available Funds: {}", available_funds)));
            }

            ui.separator();

            // Receiver Address
            ui.horizontal(|ui| {
                ui.label("To Address:");
                ui.text_edit_singleline(&mut self.ui_state.receiver_address);
            });

            // Amount
            ui.horizontal(|ui| {
                ui.label("Amount:");
                ui.add(egui::DragValue::new(&mut self.ui_state.tx_amount).speed(0.1));
                ui.label("coins");
            });

            ui.separator();

            // Gas and Gas Limit (Optional)
            ui.collapsing("Advanced Options", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Gas Price:");
                    ui.add(egui::DragValue::new(&mut self.ui_state.tx_gas_price).speed(0.1));
                });
                ui.horizontal(|ui| {
                    ui.label("Gas Limit:");
                    ui.add(egui::DragValue::new(&mut self.ui_state.tx_gas_limit).speed(0.1));
                });
            });

            ui.separator();

            // Buttons
            ui.horizontal(|ui| {
                if ui.button("Send Transaction").clicked() {

                    let sender = self.sender.clone();

                    // Extract only the necessary references from `MyApp`
                    let server = Arc::clone(&self.net_module.server);
                    let utxo_set = Arc::clone(&self.bc_module.utxo_set);

                    if let Ok((selected_wallet_name, wallet, receiver_address, tx_amount)) = self.valid_tx_fields() {
                        
                        RUNTIME.spawn(async move {
                            let result = MyApp::send_transaction(
                                selected_wallet_name,
                                wallet,
                                receiver_address,
                                tx_amount,
                                utxo_set,
                                server,
                            )
                            .await
                            .unwrap_or(false);
                
                            // Send the result back to the main thread
                            let _ = sender.send(TaskMessage::TransactionSent(result)).await;
                        });
                        
                    } else {
                        // Handle validation errors here, such as showing a message to the user
                        println!("Invalid transaction fields!");
                    }
                    

                }
                if ui.button("Preview").clicked() {
                    self.preview_transaction();
                }
                if ui.button("Clear").clicked() {
                    self.clear_transaction_form();
                }
            });
        });

        /* Search transactions by id  */
        /* Search your transactions? */
    }


    fn render_wallets_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Left section: "Total Balance"
            ui.heading("Wallets");
        
            let total_balance = self.total_balance();

            // Add space to separate the heading and balance
            ui.add_space(20.0);        
            ui.add(
                egui::Label::new(egui::RichText::new(format!("Total Balance: {}", &total_balance)))            
            );
                
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {

                if ui.button("Create New Wallet").clicked() {
                    let sender = self.sender.clone(); // Clone the sender for the async task

                    let new_address = self.bc_module.wallets.create_wallet();
                    println!("New wallet address: {}", new_address);

                    if let Err(err) = self.bc_module.wallets.save_all() {
                        println!("Error saving wallet: {}", err);
                    }

                    let wallets = self.bc_module.wallets.clone(); // contains the new wallet
                    let utxo_set = Arc::clone(&self.bc_module.utxo_set);

                    RUNTIME.spawn(async move {
                        match MyApp::calculate_new_balances(&wallets, utxo_set).await {
                            Ok(new_balances) => {
                                sender.send(TaskMessage::BalancesUpdated(new_balances))
                                    .await
                                    .unwrap_or_else(|e| println!("Failed to send balances: {}", e));
                            }
                            Err(err) => {
                                sender.send(TaskMessage::Error(err.to_string()))
                                    .await
                                    .unwrap_or_else(|e| println!("Failed to send error: {}", e));
                            }
                        }
                    });


                    self.add_notification("New wallet created successfully.".to_string());

                }
        
                ui.add_space(10.0); // Space between buttons
        
                if ui.button("Add Existing Wallet").clicked() {
                    self.ui_state.show_add_existing_wallet_popup = true;                    
                }

            });
        });

        ui.label("Manage wallets and their transactions.");

        // Get immutable data for the loop
        let all_addresses = self.bc_module.wallets.get_all_address();

        // displays each wallet saved on the device
        egui::ScrollArea::vertical().show(ui, |ui: &mut Ui| {
            for address in &all_addresses {
                let balance = self.get_balance(&address).unwrap_or(0);
                
                egui::Frame::none()
                    .rounding(egui::Rounding::same(5.0))
                    .fill(egui::Color32::from_rgb(20 ,20 , 20 ))
                    .inner_margin(egui::Margin::symmetric(20.0, 20.0)) 
                    .stroke(egui::Stroke::new(1.0, egui::Color32::BLACK))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width()); // Make the frame take the entire available width

                        ui.horizontal(|ui| {
                            // Left side: Address and Balance
                            ui.vertical(|ui| {

                                
                                ui.horizontal(|ui| {
                                    // Add the label
                                    let label_response = ui.add(
                                        egui::Label::new(egui::RichText::new(format!("Address: {}", address)))
                                            .sense(egui::Sense::click())
                                    );

                                    let icon_response = ui.add(
                                        egui::Image::new(egui::include_image!("../resources/images/copy-to-clipboard-icon.png"))
                                            .max_width(15.0)
                                            .sense(egui::Sense::click())
                                    );                                      

                                    // Handle click behavior
                                    if icon_response.clicked() || label_response.clicked() {
                                        ui.output_mut(|o| o.copied_text = address.clone());
                                    }

                                    // Handle hover behavior
                                    if icon_response.hovered() || label_response.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        
                                        label_response
                                            .on_hover_text("Click to Copy").highlight();

                                        icon_response.on_hover_text("Click to Copy");
                                    }

                                });

                                ui.label(format!("Balance: {:?} coins", balance));
                            });

                            // Right side buttons
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                
                                // Delete Wallet
                                ui.scope(|ui|{
                                    ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(194, 42, 25);
                                    ui.style_mut().visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(194, 42, 25);
                                    ui.style_mut().visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(217, 47, 28);

                                    if ui.button(egui::RichText::new("Delete Wallet")).clicked() {
                                        // Set a flag or show a popup
                                        self.ui_state.show_delete_popup = Some(address.clone());
                                    }
                                });
                                    
                                // Export Wallet
                                if ui.button("Export Wallet").clicked() {
                                    if let Some(wallet) = self.bc_module.wallets.get_wallet(address) {
                                        if let Err(err) = self.export_wallet_to_file(address, wallet) {
                                            println!("Error exporting wallet: {}", err);
                                        }
                                    }
                                }

                                // Send Wallet
                                if ui.button("Send").clicked() {
                                    println!("Send button clicked for wallet: {}", address);
                                    
                                    self.ui_state.active_tab = Tab::Transactions;

                                    self.ui_state.selected_wallet = Some(address.clone());
                                }

                                // Receive - doesn't do anything
                                if ui.button("Receive").clicked() {
                                    println!("Receive button clicked for wallet: {}", address);
                                }
                                
                            });
                        });
                    });
                ui.add_space(10.0);
            }
        });

        // ----------- For Popups -----------

        let mut delete_wallet_address: Option<String> = None;

        // Handle Delete Wallet Popup
        if let Some(wallet_to_delete) = &self.ui_state.show_delete_popup.clone() {
            egui::Window::new("Confirm Wallet Deletion")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]) // Center the window
                .show(ui.ctx(), |ui| {
                    ui.label("Are you sure you want to delete your wallet?");
                    ui.label(format!("Address: {}", wallet_to_delete.clone()));
                    ui.label("All funds will be lost if the wallet is not retrievable.");

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            // Close the popup without deleting
                            self.ui_state.show_delete_popup = None;
                        }
                        ui.scope(|ui|{
                            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(194, 42, 25);
                            ui.style_mut().visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(194, 42, 25);
                            ui.style_mut().visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(217, 47, 28);

                            if ui.button(egui::RichText::new("Proceed").color(egui::Color32::WHITE)).clicked() {
                                // Mark wallet for deletion outside this closure
                                delete_wallet_address = Some(wallet_to_delete.clone());
                                self.ui_state.show_delete_popup = None; // Close the popup

                            }
                        });
                        
                    });
                });
        }

        // Handle wallet deletion after the popup UI
        if let Some(wallet_to_delete) = delete_wallet_address {
            let _ = self.delete_wallet(&wallet_to_delete);
        }

        if self.ui_state.show_add_existing_wallet_popup {
            // Start the window for adding an existing wallet
            egui::Window::new("Add Existing Wallet")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]) // Center the window
            .show(ui.ctx(), |ui| {
                ui.label("Select Wallet Method:");

                // Option 1: "Select Wallet (.dat file)"
                if ui.button("Select Wallet (.dat file)").clicked() {
                    // Open file explorer to select .dat file
                    if let Some(path) = rfd::FileDialog::new().add_filter("Wallet File", &["dat"]).pick_file() {
                        // Deserialize the .dat file to retrieve the wallet
                        if let Ok(wallet) = self.import_wallet_from_file(path) {
                            self.bc_module.wallets.insert(&wallet.get_address(), wallet);
                            println!("Wallet added from .dat file");
                            self.ui_state.show_add_existing_wallet_popup = false;
                        } else {
                            println!("Failed to import wallet from .dat file");
                        }
                    }
                }

                ui.add_space(20.0); // Add space between options

                // Option 2: "Provide Keys to Retrieve"
                ui.label("OR Provide Private Key:");

                // Input field for private key
                let mut secret_key_input = String::new();
                ui.text_edit_singleline(&mut secret_key_input);

                // Provide a button to submit the secret key
                ui.horizontal(|ui|{
                    if ui.button("Retrieve Wallet").clicked() {
                        if let Ok(wallet) = self.import_wallet_from_key(&secret_key_input) {
                            self.bc_module.wallets.insert(&wallet.get_address(), wallet);
                            println!("Wallet retrieved from private key");
    
                            self.ui_state.show_add_existing_wallet_popup = false;
                        } else {
                            println!("Failed to retrieve wallet from the provided key");
                        }
                    }
                    if ui.button("Cancel").clicked(){
                        self.ui_state.show_add_existing_wallet_popup = false;
                    }
                });
            });
        }

        
    }

    fn render_peers_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Peers");
        ui.horizontal(|ui| {
            ui.label("View And Manage Your Peers");

            ui.add_space(10.0);
            ui.label(format!("Peer Count: {}", 5));
        });

        ui.separator();

        match &self.net_module.public_ip {
            Some( Ok(ip) ) => {
                ui.label(format!("Your Public IP: {}", ip));
            },
            Some(Err(_)) => {
                ui.label(format!("Couldn't retrieve your Public IP"));
            },
            None => {
                ui.label("Wait...");
            }
        }

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.ui_state.peer_ip_address_input)
                .hint_text("Input Peer's IP Address"));


            if ui.button("Add Peer").clicked() {
                if !self.ui_state.peer_ip_address_input.is_empty() {
                    
                    
                    let _ = self.add_peer(self.ui_state.peer_ip_address_input.clone());
                    self.ui_state.peer_ip_address_input.clear();
                
                }
            }
        });

        // Display the list of connected peers
        ui.label("Connected Peers:");
        for peer in &self.ui_state.connected_peers_displayed {
            ui.label(peer);
        }
        // display connected peers - ip address, node type, Functionality (disconnect from peering, )

        

    }

    fn render_settings_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.label("Change Your Preferred Settings");


    }

    fn render_notifications(&mut self, ctx: &egui::Context) {
        // Calculate notification timeout and filter out expired notifications
        let now = std::time::Instant::now();
        self.notif_module.notifications.retain(|n| now.duration_since(n.start_time).as_secs() < n.duration);
    
        // Bottom-right corner positioning
        let screen_rect = ctx.screen_rect();
        let mut y_offset = screen_rect.max.y - 15.0; // Start 15 px from the bottom
        let x_offset = screen_rect.max.x - 350.0 - 15.0;    // Notifications are 300 px wide + 15px margin
    
        let mut to_remove = Vec::new(); // Collect IDs of notifications to remove

        for notification in &self.notif_module.notifications {
            // Calculate the position for this notification
            let notification_rect = egui::Rect::from_min_size(
                egui::pos2(x_offset, y_offset - 75.0), // Notification height = 50 px
                egui::vec2(350.0, 75.0),
            );
            
            egui::Area::new(egui::Id::new(notification.id))
                .fixed_pos(notification_rect.min)
                .show(ctx, |ui| {
                    // Draw the background and border
                    let painter = ui.painter();
                    painter.rect_filled(
                        notification_rect,
                        5.0, // Corner radius
                        egui::Color32::from_rgb(25, 25, 25), // Background color
                    );
                    painter.rect_stroke(
                        notification_rect,
                        5.0, // Corner radius
                        egui::Stroke::new(2.0, egui::Color32::WHITE), // Border width and color
                    );

                    // Constrain the UI to the rectangle width for wrapping
                    ui.set_min_width(notification_rect.width());
                    ui.set_max_width(notification_rect.width());

                    // Create the content
                    ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::RightToLeft), |ui| {
                        ui.horizontal(|ui| {
                            // Smaller "x" button
                            ui.add_space(15.0);
                            
                            ui.style_mut().spacing.button_padding = egui::Vec2::new(7.5, 7.5);


                            let close_button = ui.add_sized([5.0, 5.0], egui::Button::new("X"));
                  
                            if close_button.clicked() {
                                to_remove.push(notification.id); // Schedule for removal
                            }

                            // Centered, wrapped label
                            ui.add(egui::Label::new(egui::RichText::new(&notification.message)
                                .color(egui::Color32::WHITE)
                                .text_style(egui::TextStyle::Body))
                                .wrap()
                            ); // Enable text wrapping
                        });
                    });
                });

    
            y_offset -= 90.0; // Stack next notification 10 px above the current one
        }

        self.notif_module.notifications.retain(|n| !to_remove.contains(&n.id));

    }

    fn render_channel_messages(&mut self, ctx: &egui::Context) { 
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                TaskMessage::BalancesUpdated(new_balances) => {
                    self.bc_module.balances = new_balances;
                    println!("Balances updated: {:?}", &self.bc_module.balances);
                }
                TaskMessage::Error(err) => {
                    println!("Error occurred: {}", err);
                    self.add_notification(err); // Display error to the user
                }
                TaskMessage::TransactionSent(successful) => {
                    if successful {
                        self.add_notification(String::from("Successful Transaction!"));
                    } else {
                        self.add_notification(String::from("UNSUCCESSFUL Transaction."));
                    }
                }
                TaskMessage::PeerAdded(address) => {
                    println!("Successfully added: {}", address);

                    
                }
            }
        }
    }
}

fn convert_timestamp(timestamp: u128) -> String {
    let secs = (timestamp / 1000) as i64; // Convert milliseconds to seconds
    let naive_datetime = NaiveDateTime::from_timestamp_opt(secs, 0)
        .unwrap_or_else(|| Utc::now().naive_utc());
    
    // Convert NaiveDateTime to DateTime<Utc>
    let datetime: DateTime<Utc> = DateTime::from_utc(naive_datetime, Utc);
    datetime.format("%d-%m-%Y %H:%M:%S").to_string()
}

async fn get_public_ip() -> Result<String> {
    let response = reqwest::get("https://ipinfo.io/ip").await?.text().await?;
    Ok(response)
}