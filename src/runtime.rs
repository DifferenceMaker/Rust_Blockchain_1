use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

// Define a globally accessible runtime
pub static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    println!("RUNTIME initialized");
    Runtime::new().expect("Failed to create Tokio runtime")
});
