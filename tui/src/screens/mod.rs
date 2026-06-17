pub mod assets;
pub mod component_demo;
pub mod help;
pub mod home;
pub mod lock;
pub mod receive;
pub mod send;
pub mod settings;
pub mod setup;
pub mod wallets;

use crate::api::WalletApi;
use crate::app::Nav;
use ratatui::Frame;

pub trait Screen {
    fn name(&self) -> &str;
    fn init(&mut self, _api: &dyn WalletApi) {}
    fn on_reactivate(&mut self, _api: &mut dyn WalletApi) {}
    fn render(&mut self, frame: &mut Frame, api: &dyn WalletApi);
    fn handle_input(&mut self, key: crossterm::event::KeyEvent, api: &mut dyn WalletApi) -> Nav;
    fn handle_paste(&mut self, _text: &str, _api: &mut dyn WalletApi) -> Nav { Nav::None }
}
