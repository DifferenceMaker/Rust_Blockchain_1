use crate::cli::Cli;
use crate::errors::Result;
use eframe::egui;
//use eframe::egui::IconData;
use egui::{FontData, FontFamily};

mod block;
mod transaction;
mod errors;
mod blockchain;
mod cli; 
mod tx;
mod wallet;
mod utxoset;
mod server;
mod app;

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // Application options
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 600.0])
            .with_icon(load_icon("resources/images/icon.png")),
        centered: true,
        ..Default::default()
    };    


    /*let mut cli = match Cli::new() {
        Ok(cli_instance) => cli_instance,
        Err(e) => {
            eprintln!("Failed to initialize CLI: {}", e);
            return Err(e); // Or handle the error as needed
        }
    };
    
    if let Err(e) = cli.run() {
        eprintln!("Failed to run CLI: {}", e);
        return Err(e); // Or handle the error as needed
    }*/


    // Start program
    eframe::run_native(
        "BlockJain",
        options,
        Box::new(|cc| {
            // Setup
            setup_fonts(&cc.egui_ctx); // Custom font setup

            // Create blockchain here and pass to default?
            // Just follow cli and try shit out.


            // Returns the app that will be runned
            Ok(Box::<app::MyApp>::default())
        }),
    )
}





// Helpers

fn load_icon(path: &str) -> eframe::egui::IconData {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };

    eframe::egui::IconData {
        rgba: icon_rgba,
        width: icon_width,
        height: icon_height,
    }
}

fn setup_fonts(ctx: &egui::Context) {
    ctx.set_fonts({
        let mut fonts = egui::FontDefinitions::default();
    
        fonts.font_data.insert("my_font".to_owned(),
        FontData::from_static(include_bytes!("../resources/Poppins-Light.ttf"))); // .ttf and .otf supported

        // Put my font first (highest priority):
        fonts.families.get_mut(&FontFamily::Proportional).unwrap()
        .insert(0, "my_font".to_owned());

        // Put my font as last fallback for monospace:
        fonts.families.get_mut(&FontFamily::Monospace).unwrap()
        .push("my_font".to_owned());

        fonts
    });
}
