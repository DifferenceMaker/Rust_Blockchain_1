use crate::errors::Result;
use eframe::egui;
use egui::{FontData, FontFamily};
use egui_extras::install_image_loaders;
use crate::settings::SETTINGS;

mod block;
mod transaction;
mod errors;
mod blockchain;
mod tx;
mod wallet;
mod utxoset;
mod server;
mod runtime;
mod app;
mod settings;

fn main() -> eframe::Result {
    env_logger::init();

    // Application options
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(egui::vec2(SETTINGS.resolution.0, SETTINGS.resolution.1))
            .with_icon(load_icon("resources/images/icon.png"))
            .with_min_inner_size([800.0, 400.0]),
        centered: true,
        ..Default::default()
    };    

    // Initialize the app asynchronously using the global runtime
    let app = runtime::RUNTIME.block_on(async {
        match app::MyApp::initialize_async().await {
            Ok(initialized_app) => initialized_app,
            Err(e) => {
                eprintln!("Failed to initialize app asynchronously: {}", e);
                app::MyApp::default()
            }
        }
    });

    eframe::run_native(
        "BlockJain",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx); // Custom font setup
            install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(app))
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
