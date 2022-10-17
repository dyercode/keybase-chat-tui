// # main.rs
//
// Contains the cli and high-level orchestration of other components.

#![feature(exact_size_is_empty)]
#[macro_use]
extern crate log;

use crate::client::{Client, ClientExecutor};
use crate::controller::Controller;
use crate::state::{ApplicationState, ApplicationStateInner};
use crate::ui::{UiBuilder, UiObserver};

mod client;
mod controller;
mod state;
mod types;
mod ui;
mod views;
#[macro_use]
mod macros;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only enable the logging when compiling in debug mode. This makes the difference between
    // `info!` and `debug!` somewhat moot, so I'm just using them to switch between a 'normal'
    // amount of logging and 'excessive'.
    //
    if cfg!(debug_assertions) {
        let mut builder = env_logger::Builder::from_default_env();
        builder.target(env_logger::Target::Stderr).init();
    }

    info!("Starting...");

    // The UI object has all of the cursive (rust tui library) logic.
    let (mut ui, ui_recv, ui_sender) = UiBuilder::new().build();
    let mut state = ApplicationStateInner::default();

    state.register_observer(Box::new(UiObserver { sender: ui_sender }));
    let client = Client::<ClientExecutor>::default();
    let mut controller = Controller::new(client, state, ui_recv);

    controller.init().await?;

    tokio::select! {
        _ = controller.process_events() => { info!("Exiting from process events")}
        _ = ui.run() => { info!("Exiting from cursive."); }
    }
    Ok(())
}
