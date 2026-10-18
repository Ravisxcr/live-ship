pub mod api;
pub mod app;
pub mod config;
pub mod ui;
pub mod utils;

use app::DeploymentManager;
use slint::ComponentHandle;
use std::sync::{Arc, Mutex};
use ui::handlers::bridge::MainWindow;
use ui::UiBridge;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting LiveShip GUI Application...");

    // Initialize the deployment manager and in-memory engine
    let manager = Arc::new(Mutex::new(DeploymentManager::new()));

    // Create the Slint main window
    let main_window = MainWindow::new()?;

    // Bind UI callbacks, models, and real-time streaming to the manager
    let bridge = UiBridge::new(main_window.clone_strong(), manager);
    bridge.setup_handlers();

    // Launch UI event loop
    main_window.run()?;

    println!("LiveShip exited cleanly.");
    Ok(())
}
