use eframe::egui;

use crate::blockchain::Blockchain;

/*
    What is possible with app?
     - See blockchain.
     - Make transactions.
     - See and create your wallets and its transactions
     - Start node ( mining and networking )
*/

/*
TO DO: 
    - connect to blockchain implementation
    
*/

pub struct MyApp {
    //bc: Blockchain,
    active_tab: Tab, // Track which section is active
}

enum Tab {
    Blockchain,
    Transactions,
    Wallets,
    Settings,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            active_tab: Tab::Blockchain,
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

    fn render_wallets_section(&self, ui: &mut egui::Ui) {
        ui.heading("Wallets");
        ui.label("Manage wallets and their transactions.");

        // Show a list of wallets, balances, and related transaction histories
    }

    fn render_settings_section(&mut self, ui: &mut egui::Ui) {
       
        /*
         Allow customization of user preferences, themes, or application-specific settings.
         Theme? 

         */
    }
}