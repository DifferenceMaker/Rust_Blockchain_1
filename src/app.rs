use eframe::egui;
use egui::Ui;
use failure::{Error, Fail};
use crate::errors::Result;
use bitcoincash_addr::{Address, HashType, Scheme};
use crypto::{digest::Digest, ed25519, ripemd160::Ripemd160, sha2::Sha256};
use hex;
use log::error;

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::blockchain::Blockchain;
use crate::server::Server;
use crate::transaction::Transaction;
use crate::tx::TXOutputs;
use crate::utxoset::UTXOSet;
use crate::wallet::*;
use crate::runtime::RUNTIME; // Import the global runtime (tokio)


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
    Settings,
}

struct Notification {
    pub id: u32,              // Unique ID for each notification
    pub message: String,
    pub start_time: std::time::Instant,  // When the notification was created
    pub duration: u64,        // Duration in seconds before auto-dismissal
}

pub struct MyApp {
    // Blockchain specific
    wallets: Wallets,
    balances: Vec<i32>,
    utxo_set: Arc<RwLock<UTXOSet>>,
    server: Arc<Server>,

    // Tabbing
    active_tab: Tab, // Track which section is active

    // Transaction Tab
    selected_wallet: Option<String>,
    receiver_address: String,
    tx_amount: i32,
    tx_gas_price: i32,
    tx_gas_limit: i32,
    //send_transaction_result: Option<Result<bool>>, // To track transaction results
    //sending_transaction: bool,                            // To indicate ongoing async task


    // Notification Tab
    notifications: Vec<Notification>,
    notification_counter: u32,

    // Popups
    show_delete_popup: Option<String>,
    show_add_existing_wallet_popup: bool,
}

impl MyApp {
    pub async fn initialize_async() -> Result<Self> {
        // Load Settings to get preferences for node.
        /*
             - Regular node vs also miner node
             - connect node list? 
             - Use a default port like 8333 for Bitcoin-like blockchains.
             - Interval for blockchain check
             - How many miners to allow
        */

        // Load wallets
        let mut wallets = Wallets::new()?; 

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
        
        let server = Arc::new(Server::new("8334", &mining_address, Arc::clone(&utxo_set))?);

        // Pass an arc-clone to tokio::spawn which is in charge of server logic
        tokio::spawn({
            let server_clone = Arc::clone(&server);
            async move {
                if let Err(e) = server_clone.start_server().await {
                    error!("Server error: {}", e);
                }
            }
        });

        let mut app = MyApp {
            wallets,
            balances: Vec::new(),
            utxo_set: Arc::clone(&utxo_set),
            // pass an arc-clone to MyApp struct to have availability to the network
            server: Arc::clone(&server),
            active_tab: Tab::Blockchain,

            // Wallets Tab
            show_delete_popup: None,
            show_add_existing_wallet_popup: false,

            // Transaction Tab
            selected_wallet: None,
            receiver_address: String::from(""),
            tx_amount: 0,
            tx_gas_price: 0,
            tx_gas_limit: 0,

            // Notification Tab
            notifications: Vec::new(),
            notification_counter: 0,

        };
    
        // Update balances once during initialization
        app.update_balances().await?;
        Ok(app)
    }

    /// Updates the balances vector based on the current UTXO set.
    pub async fn update_balances(&mut self) -> Result<()> {
        let mut new_balances = Vec::new();

        for address in self.wallets.get_all_address() {
            
            let pub_key_hash = Address::decode(&address).unwrap().body;
            //println!("address: {}, pub_key_hash: {:?}", &address, &pub_key_hash);

            // Find all UTXOs for this address
            let utxos: TXOutputs = self.utxo_set.read().await.find_utxo(&pub_key_hash).unwrap_or_else(|_| {
                TXOutputs {
                    outputs: vec![],
                }
            });

            // Calculate the total balance for this address
            let balance: i32 = utxos.outputs.iter().map(|out| out.value).sum();

            // Add the balance to the vector
            new_balances.push(balance);
        }

        // Update the balances in the app state
        self.balances = new_balances;
        Ok(())
    }

    /// Retrieves the balance for a given wallet address.
    /// Returns `None` if the address is not found in the wallets list.
    pub fn get_balance(&self, address: &str) -> Option<i32> {
        if let Some(index) = self.wallets.get_all_address().iter().position(|a| a == address) {
            self.balances.get(index).copied()
        } else {
            None
        }
    }

    pub fn total_balance(&self) -> i32 {
        self.balances.iter().sum()
    }

    pub fn delete_wallet(&mut self, address: &str) -> Result<()> {
        self.wallets.delete_wallet(address)?;

        let message = format!("Wallet Deleted (Address): {}", &address);
        self.add_notification(message);

        // Update balances: Assuming balances align with wallet order
        if let Some(index) = self.wallets.get_all_address().iter().position(|a| a == address) {
            self.balances.remove(index);
        }

        let _ = self.update_balances();
        Ok(())
    }

    pub fn export_wallet_to_file(&self, address: &str, wallet: &Wallet) -> Result<()> {
        use std::fs::File;
        use std::io::Write;

        let file_name = format!("data/wallets/export/{}_wallet.dat", address);
        let mut file = File::create(&file_name)?;

        let serialized_wallet = bincode::serialize(wallet)?;
        file.write_all(&serialized_wallet)?;

        println!("Wallet exported to file: {}", file_name);
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
            .selected_wallet
            .as_ref()
            .ok_or_else(|| failure::err_msg("No wallet selected"))?
            .clone();
    
        println!("From: {}", selected_wallet_name);
    
        let wallet = self
            .wallets
            .get_wallet(&selected_wallet_name)
            .ok_or_else(|| failure::err_msg("Wallet not found for the selected address"))?;
    
        if self.receiver_address.is_empty() {
            return Err(failure::err_msg("Receiver address cannot be empty"));
        }
    
        println!("To: {}", self.receiver_address);
    
        if self.tx_amount <= 0 {
            return Err(failure::err_msg("Transaction amount must be greater than zero"));
        }
    
        println!("Amount: {}", self.tx_amount);
    
        Ok((
            selected_wallet_name,
            wallet.clone(),
            self.receiver_address.clone(),
            self.tx_amount,
        ))
    }

    // (1) Checks for correct fields, (2) creates new utxo and (3) sends it with server.
    /*async fn send_transaction(&mut self) -> Result<bool> {

        /*
            1. Wallet name
            2. Wallet
            3. Receiver Address
            4. tx_amount
            5. utxo_set
            6. server
        */

        let selected_wallet_name = match &self.selected_wallet {
            Some(wallet_name) => wallet_name,
            None => return Err(failure::err_msg("No wallet selected")),
        };
        println!("From: {}", &selected_wallet_name);

        let wallet = match self.wallets.get_wallet(selected_wallet_name) {
            Some(wallet) => wallet,
            None => return Err(failure::err_msg("Wallet not found for the selected address")),
        };
        
        if self.receiver_address.is_empty() {
            return Err(failure::err_msg("Receiver address cannot be empty"));
        }
        println!("To: {}", &self.receiver_address);
        
        if self.tx_amount <= 0 {
            return Err(failure::err_msg("Transaction amount must be greater than zero"));
        }
        println!("Amount: {}", &self.tx_amount);
        
        let tx = Transaction::new_utxo(wallet, &self.receiver_address, self.tx_amount, &self.utxo_set).await
        .map_err(|e| failure::err_msg(e))?;

        // mine_now is just a bool variable for testing purposes
        let mine_now = false;

        if mine_now {
            let cbtx = Transaction::new_coinbase(selected_wallet_name.to_string(), String::from("reward!"))
            .map_err(|e| failure::err_msg(e))?;
        
            let new_block = self.utxo_set.write().await
                .blockchain.write().await.mine_block(vec![cbtx, tx])
                .map_err(|e|failure::err_msg(e))?;
    
            // Update the UTXO set with the new block
            self.utxo_set.write().await.update(&new_block)
                .map_err(|e| failure::err_msg(e))?;

        } else {
            // Propagate
            self.server.send_transaction(&tx).await?; // Create and pass in the sender.

            // Create new tokio::spawn for receiver which then adds a transaction msg.
        }

        Ok(true)

    }*/

    pub async fn send_transaction(
        selected_wallet_name: String,
        wallet: Wallet,
        receiver_address: String,
        tx_amount: i32,
        utxo_set: Arc<RwLock<UTXOSet>>,
        server: Arc<Server>,
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
            server.send_transaction(&tx).await?;
        }
    
        Ok(true)
    }
    
    

    fn preview_transaction(&self) {

        // display popup

    }

    fn clear_transaction_form(&mut self){
        // Transaction Tab
        self.selected_wallet = None;
        self.receiver_address = String::from("");
        self.tx_amount = 0;
        self.tx_gas_price = 0;
        self.tx_gas_limit = 0;
    }

    pub fn add_notification(&mut self, message: String) {
        let notification = Notification {
            id: self.generate_notification_id(),
            message,
            start_time: std::time::Instant::now(),
            duration: 10, // 10 seconds
        };

        self.notifications.push(notification);
    }

    // Generate a unique notification ID
    fn generate_notification_id(&mut self) -> u32 {
        self.notification_counter += 1;
        self.notification_counter
    }

}

impl Default for MyApp {
    fn default() -> Self {
        // Create the `utxo_set` first, since it is needed by `server`
        let utxo_set = Arc::new(RwLock::new(UTXOSet {
            blockchain: Arc::new(RwLock::new(Blockchain::default_empty())),
        }));

        // Use `utxo_set` to create the `server`
        let server:Arc<Server> = Arc::new(Server::new("8334", "", Arc::clone(&utxo_set)).unwrap());

        Self {
            wallets: Wallets::default(),
            balances: Vec::new(),
            utxo_set: utxo_set,
            server: server,
            active_tab: Tab::Blockchain,

            // Wallets Tab
            show_delete_popup: None,
            show_add_existing_wallet_popup: false,

            // Transaction Tab
            selected_wallet: None,
            receiver_address: String::new(),
            tx_amount: 0,
            tx_gas_price: 0,
            tx_gas_limit: 0,

            // Notification Tab
            notifications: Vec::new(),
            notification_counter: 0,
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
                    self.active_tab = Tab::Blockchain;
                }
                if ui.button(egui::RichText::new("Transactions").size(16.0)).clicked() {
                    self.active_tab = Tab::Transactions;
                }
                if ui.button(egui::RichText::new("Wallets").size(16.0)).clicked() {
                    self.active_tab = Tab::Wallets;
                }
                if ui.button(egui::RichText::new("Settings").size(16.0)).clicked() {
                    self.active_tab = Tab::Settings;
                }

                                // Spacer to push the following content to the rightmost side
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let wallet_count = self.wallets.get_all_address().len();
                    
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
                            self.active_tab = Tab::Wallets;
                        };

                    
                });
            
            
            });

            // Section rendering based on the active tab
            ui.separator(); // Add a visual separator

            match self.active_tab {
                Tab::Blockchain => self.render_blockchain_section(ui),
                Tab::Transactions => self.render_transactions_section(ui),
                Tab::Wallets => self.render_wallets_section(ui),
                Tab::Settings => self.render_settings_section(ui),
            }

            self.render_notifications(ctx);

            /*
                Render notifications
                // new coinbase added to your wallet 
                // Transaction successful / unsuccessful
                // new wallet created 
                // ..
                
             */
        });
    
        
    
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) { // automatically every 30 seconds
        /*  you can store user preferences, settings, or wallets. */
        // Use it to save non-critical data that doesn't need to be immediately updated on every change
        // Melnraksta Transactions... ; Mempool?
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        /* Double-check that all critical user data is saved (e.g., wallets, settings, blockchain state). */
        /* Close any open files, terminate threads, or gracefully shut down networking resources. */


        // Saves Wallets on disk
        if let Err(e) = self.wallets.save_all() {
            eprintln!("Failed to save wallets on exit: {}", e);
        } else {
            println!("Wallets successfully saved on exit.");
        }
        
        // Closes DB for Blockchain

        // Additional clean-up logic
        println!("Application exiting. Cleaning up resources...");
    }

}


// Methods for rendering each section
impl MyApp {
    fn render_blockchain_section(&self, ui: &mut egui::Ui) {
        ui.heading("Blockchain");
        ui.label("View and analyze the blockchain.");

        /*        
        block explorer,

        */
        
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
                    .selected_text(self.selected_wallet.clone().unwrap_or("Select Wallet".into()))
                    .show_ui(ui, |ui| {
                        for (address, display_text) in wallet_entries {
                            if ui.selectable_value(&mut self.selected_wallet, Some(address.clone()), display_text).clicked() {
                                self.selected_wallet = Some(address);
                            }
                        }
                    });

            });
            
            if let Some(wlt_address) = &self.selected_wallet {
                let available_funds = self.get_balance(&wlt_address).unwrap_or(0);
                ui.label(egui::RichText::new(format!("Available Funds: {}", available_funds)));
            }

            ui.separator();

            // Receiver Address
            ui.horizontal(|ui| {
                ui.label("To Address:");
                ui.text_edit_singleline(&mut self.receiver_address);
            });

            // Amount
            ui.horizontal(|ui| {
                ui.label("Amount:");
                ui.add(egui::DragValue::new(&mut self.tx_amount).speed(0.1));
                ui.label("coins");
            });

            ui.separator();

            // Gas and Gas Limit (Optional)
            ui.collapsing("Advanced Options", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Gas Price:");
                    ui.add(egui::DragValue::new(&mut self.tx_gas_price).speed(0.1));
                });
                ui.horizontal(|ui| {
                    ui.label("Gas Limit:");
                    ui.add(egui::DragValue::new(&mut self.tx_gas_limit).speed(0.1));
                });
            });

            ui.separator();

            // Buttons
            ui.horizontal(|ui| {
                if ui.button("Send Transaction").clicked() {

                    let (tx_complete, mut rx_complete) = tokio::sync::mpsc::channel::<bool>(1);

                    // Extract only the necessary references from `MyApp`
                    let server = Arc::clone(&self.server);
                    let utxo_set = Arc::clone(&self.utxo_set);

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
                            let _ = tx_complete.send(result).await;
                        });
                        
                        // Add a separate task to listen to rx_complete
                        RUNTIME.spawn(async move {
                            while let Some(success) = rx_complete.recv().await {
                                if success {
                                    println!("Transaction successfully propagated.");
                                    // It is propagated but not mined. 
                                } else {
                                    println!("Failed to propagate transaction.");
                                }
                            }
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
                    let new_address = self.wallets.create_wallet();
                    println!("new wallet address: {}", new_address);

                    if let Err(err) = self.wallets.save_all() {
                        println!("Error saving wallet: {}", err);
                    }                    

                    let _ = self.update_balances();
                    self.add_notification("New wallet created successfully.".to_string());

                }
        
                ui.add_space(10.0); // Space between buttons
        
                if ui.button("Add Existing Wallet").clicked() {
                    self.show_add_existing_wallet_popup = true;                    
                }
            });
        });

        ui.label("Manage wallets and their transactions.");

        // Get immutable data for the loop
        let all_addresses = self.wallets.get_all_address();

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
                                ui.scope(|ui|{
                                    ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(194, 42, 25);
                                    ui.style_mut().visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(194, 42, 25);
                                    ui.style_mut().visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(217, 47, 28);

                                    if ui.button(egui::RichText::new("Delete Wallet")).clicked() {
                                        // Set a flag or show a popup
                                        self.show_delete_popup = Some(address.clone());
                                    }
                                });
                                    
                                if ui.button("Export Wallet").clicked() {
                                    if let Some(wallet) = self.wallets.get_wallet(address) {
                                        if let Err(err) = self.export_wallet_to_file(address, wallet) {
                                            println!("Error exporting wallet: {}", err);
                                        }
                                    }
                                }

                                if ui.button("Send").clicked() {
                                    println!("Send button clicked for wallet: {}", address);
                                    
                                    self.active_tab = Tab::Transactions;

                                    self.selected_wallet = Some(address.clone());
                                }
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
        if let Some(wallet_to_delete) = &self.show_delete_popup.clone() {
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
                            self.show_delete_popup = None;
                        }
                        ui.scope(|ui|{
                            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(194, 42, 25);
                            ui.style_mut().visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(194, 42, 25);
                            ui.style_mut().visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(217, 47, 28);

                            if ui.button(egui::RichText::new("Proceed").color(egui::Color32::WHITE)).clicked() {
                                // Mark wallet for deletion outside this closure
                                delete_wallet_address = Some(wallet_to_delete.clone());
                                self.show_delete_popup = None; // Close the popup

                            }
                        });
                        
                    });
                });
        }

        // Handle wallet deletion after the popup UI
        if let Some(wallet_to_delete) = delete_wallet_address {
            let _ = self.delete_wallet(&wallet_to_delete);
        }

        if self.show_add_existing_wallet_popup {
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
                            self.wallets.insert(&wallet.get_address(), wallet);
                            println!("Wallet added from .dat file");
                            self.show_add_existing_wallet_popup = false;
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
                if ui.button("Retrieve Wallet").clicked() {
                    if let Ok(wallet) = self.import_wallet_from_key(&secret_key_input) {
                        self.wallets.insert(&wallet.get_address(), wallet);
                        println!("Wallet retrieved from private key");

                        self.show_add_existing_wallet_popup = false;
                    } else {
                        println!("Failed to retrieve wallet from the provided key");
                    }
                }
            });
        }

        
    }

    fn render_settings_section(&mut self, _ui: &mut egui::Ui) {
       
        /*
         Allow customization of user preferences, themes, or application-specific settings.
         Theme? 

         */


    }

    fn render_notifications(&mut self, ctx: &egui::Context) {
        // Calculate notification timeout and filter out expired notifications
        let now = std::time::Instant::now();
        self.notifications.retain(|n| now.duration_since(n.start_time).as_secs() < n.duration);
    
        // Bottom-right corner positioning
        let screen_rect = ctx.screen_rect();
        let mut y_offset = screen_rect.max.y - 15.0; // Start 15 px from the bottom
        let x_offset = screen_rect.max.x - 350.0 - 15.0;    // Notifications are 300 px wide + 15px margin
    
        let mut to_remove = Vec::new(); // Collect IDs of notifications to remove

        for notification in &self.notifications {
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

        self.notifications.retain(|n| !to_remove.contains(&n.id));

    }

}

/*
fn load_image(ctx: &egui::Context, src: &str, width: usize, height: usize) -> Result<egui::TextureHandle> {
    // Open the file at the provided path
    let mut file = File::open(src)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // Decode the image
    let image = image::load_from_memory(&buffer)?.into_rgba8();
    let size = [width as usize, height as usize];
    let image_buffer = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());

    // Load texture into egui
    Ok(ctx.load_texture("icon", image_buffer, egui::TextureOptions::default()))
}*/