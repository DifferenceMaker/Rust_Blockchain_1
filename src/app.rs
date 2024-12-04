use eframe::egui;
use egui::Ui;
use failure::Fail;
use crate::errors::Result;
use bitcoincash_addr::{Address, HashType, Scheme};
use crypto::{digest::Digest, ed25519, ripemd160::Ripemd160, sha2::Sha256};
use hex;


use crate::blockchain::Blockchain;
use crate::server::Server;
use crate::transaction::Transaction;
use crate::tx::TXOutputs;
use crate::utxoset::UTXOSet;
use crate::wallet::*; 

use rfd::FileDialog;

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

pub struct MyApp {

    // Blockchain specific
    wallets: Wallets,
    balances: Vec<i32>,
    utxo_set: UTXOSet,

    // Tabbing
    active_tab: Tab, // Track which section is active

    // Popups
    show_delete_popup: Option<String>,
    show_add_existing_wallet_popup: bool,
}

impl MyApp {
    fn try_default() -> Result<Self> {

        // Load wallets
        let mut wallets = Wallets::new()?; 
        
        // Uncomment to create a new blockchain with a new genesis block and genesis address (Use for Custom)        
    
        /*  let address = wallets.create_wallet();
            let blockchain = Blockchain::create_blockchain(address.clone())?;
        */

        // This can either load the existing blockchain or create a new genesis block. (Standard way)
        let blockchain = Blockchain::new()?;
        let utxo_set = UTXOSet { blockchain };

        //utxo_set.reindex()?;

        // initialize also server, miner, blockchain and utxo?

        let mut app = MyApp {
            wallets,
            balances: Vec::new(),
            utxo_set,
            active_tab: Tab::Blockchain,

            show_delete_popup: None,
            show_add_existing_wallet_popup: false,
        };
    
        // Update balances once during initialization
        app.update_balances()?;
        Ok(app)
    }

    /// Updates the balances vector based on the current UTXO set.
    pub fn update_balances(&mut self) -> Result<()> {
        let mut new_balances = Vec::new();

        for address in self.wallets.get_all_address() {
            let pub_key_hash = Address::decode(&address).unwrap().body;

            // Find all UTXOs for this address
            let utxos: TXOutputs = self.utxo_set.find_utxo(&pub_key_hash).unwrap_or_else(|_| {
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
        println!("Wallet Deleted (Address): {}", &address);
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

}

impl Default for MyApp {
    fn default() -> Self {
        Self::try_default().unwrap_or_else(|e| {
            eprintln!("Failed to initialize MyApp: {}", e);

            // Provide a reasonable fallback state (blank app)
            Self {
                wallets: Wallets::default(),
                balances: Vec::new(),
                utxo_set: UTXOSet {
                    blockchain: Blockchain::default_empty(),
                },
                active_tab: Tab::Blockchain,
                show_delete_popup: None,
                show_add_existing_wallet_popup: false,
            }
        })
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
        });
    
        
    
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) { // automatically every 30 seconds
        /*  you can store user preferences, settings, or wallets. */
        // Use it to save non-critical data that doesn't need to be immediately updated on every change
        // Melnraksta Transactions... ; Mempool?
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
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

    fn render_transactions_section(&self, ui: &mut egui::Ui) {
        ui.heading("Transactions");
        ui.label("View and create transactions.");

        // forms for creating transactions or viewing transaction history
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
                let balance = self.get_balance(address).unwrap_or(0);
                
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
                                ui.label(format!("Address: {}", address));
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

                                    // wallet_selected_for_transaction = address.clone().
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
            self.delete_wallet(&wallet_to_delete);
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

    fn render_settings_section(&mut self, ui: &mut egui::Ui) {
       
        /*
         Allow customization of user preferences, themes, or application-specific settings.
         Theme? 

         */


    }

}