use eframe::egui;
use crate::errors::Result;
use bitcoincash_addr::Address;

use crate::blockchain::Blockchain;
use crate::server::Server;
use crate::transaction::Transaction;
use crate::tx::TXOutputs;
use crate::utxoset::UTXOSet;
use crate::wallet::Wallets; 

pub struct MyApp {

    // utxo is necessary to efficiently find unspent transaction outputs
    wallets: Wallets,
    //bc: Blockchain,
    utxo_set: UTXOSet,
    active_tab: Tab, // Track which section is active
}

enum Tab {
    Blockchain,
    Transactions,
    Wallets,
    Settings,
}

impl MyApp {
    fn try_default() -> Result<Self> {

        // Load wallets
        let wallets = Wallets::new()?; 

        // This can either load the existing blockchain or create a new genesis block.
        let blockchain = Blockchain::new()?;

        let utxo_set = UTXOSet { blockchain };

        // Retrieve all balances for wallets
        /*for address in &wallets.get_all_address() {
            println!("Address: {}", &address);
            let pub_key_hash = Address::decode(&address).unwrap().body;

            // Find all UTXOs for this address
            let utxos: TXOutputs = utxo_set.find_utxo(&pub_key_hash).unwrap_or_else(|_|{
                TXOutputs {
                    outputs: vec![]
                }
            });

            // Calculate the total balance for this address
            let mut balance = 0;
            for out in utxos.outputs {
                balance += out.value;
            }

            for wallet in wallets.get_wallets_mut().iter_mut() {
                // Assuming Wallet has a method to derive its address from its public key
                if &wallet.1.get_address() == address {
                    wallet.1.balance = balance;
                    break;
                }
            }

        }*/

        // initialize also server, miner, blockchain and utxo?

        // Create the app instance
        Ok( Self {
            wallets,
            utxo_set,
            active_tab: Tab::Blockchain,
        })
    }

}

impl Default for MyApp {
    fn default() -> Self {
        Self::try_default().unwrap_or_else(|e| {
            eprintln!("Failed to initialize MyApp: {}", e);

            // Provide a reasonable fallback state (blank app)
            Self {
                wallets: Wallets::default(),
                utxo_set: UTXOSet {
                    blockchain: Blockchain::default_empty(),
                },
                active_tab: Tab::Blockchain,
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
        // Save application state here
        // self.wallets.save_all();
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        // Perform clean-up tasks here
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
        
            let mut total_balance: i32 = 0;

            // Add space to separate the heading and balance
            ui.add_space(20.0);        
            ui.add(
                egui::Label::new(egui::RichText::new(format!("Total Balance: {}", &total_balance)))            
            );
                

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Create New Wallet")
                .clicked() {
                    /*
                        Usual step-by-step
                         - new Wallets instance
                         - Wallets::create_wallet()
                         - Wallets::save_all()

                    */
                    println!("Create New Wallet clicked");

                    let new_address = self.wallets.create_wallet();
                    println!("new wallet address: {}", new_address);
                    
                }
        
                ui.add_space(10.0); // Space between buttons
        
                if ui.button("Add Existing Wallet").clicked() {
                    println!("Add Existing Wallet clicked");
                    // Logic for adding an existing wallet
                }
            });
        });

        ui.label("Manage wallets and their transactions.");

        // total balance | create new wallet | add existing one?


        // displays each wallet saved on the device
        egui::ScrollArea::vertical().show(ui, |ui| {
            for address in &self.wallets.get_all_address() {
                /*let mut balance = 0;
                for wallet in self.wallets.get_wallets().iter() {
                    // Assuming Wallet has a method to derive its address from its public key
                    if &wallet.1.get_address() == address {
                        balance = wallet.1.balance;
                        break;
                    }
                }*/
                egui::Frame::none()
                    .rounding(egui::Rounding::same(5.0))
                    .fill(egui::Color32::from_rgb(20 ,20 , 20 ))
                    .inner_margin(egui::Margin::symmetric(20.0, 20.0)) 
                    .stroke(egui::Stroke::new(1.0, egui::Color32::BLACK))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width()); // Make the frame take the entire available width
                        ui.vertical(|ui| {
                            ui.label(format!("Address: {}", address));

                            ui.label(format!("Balance: {} coins", 150));

                            /*
                                Delete Wallet utility 
                                - "Are you sure you want to delete your wallet? All funds will be lost 
                                   if public key is not retrievable. "
                             
                             */

                            ui.horizontal(|ui| {
                                if ui.button("Send").clicked() {
                                    println!("Send button clicked for wallet: {}", address);
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

        
    }

    fn render_settings_section(&mut self, ui: &mut egui::Ui) {
       
        /*
         Allow customization of user preferences, themes, or application-specific settings.
         Theme? 

         */


    }
}