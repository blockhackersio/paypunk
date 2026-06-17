pub mod mock;
pub mod types;

use types::*;

pub trait WalletApi {
    fn get_setup(&self) -> SetupData;
    fn submit_setup_create(&self, input: SetupCreateInput) -> Result<(), ApiError>;
    fn submit_setup_import(&self, input: SetupImportInput) -> Result<(), ApiError>;

    fn get_wallets(&self) -> WalletsData;
    fn get_assets(&self, chain_id: &str) -> AssetsData;

    fn get_home(&self) -> HomeData;
    fn submit_home(&self, input: HomeInput) -> HomeData;
    fn home_state(&self) -> ApiState<HomeData>;
    fn refresh_home(&self);

    fn get_receive(&self, chain_id: &str) -> ReceiveData;
    fn submit_receive(&self, input: ReceiveInput) -> ReceiveData;
    fn receive_state(&self, chain_id: &str) -> ApiState<ReceiveData>;
    fn refresh_receive(&self, chain_id: &str);

    fn get_send(&self, chain_id: &str) -> SendData;
    fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData;
    fn submit_send_confirm(&self, input: SendConfirmInput) -> SendResult;
    fn send_state(&self, chain_id: &str) -> ApiState<SendData>;
    fn refresh_send(&self, chain_id: &str);

    fn get_lock(&self) -> LockData;
    fn submit_lock(&self, input: LockInput) -> Result<(), ApiError>;

    fn get_settings(&self) -> SettingsData;
    fn submit_settings(&self, input: SettingsInput) -> Result<(), ApiError>;
    fn submit_reveal_phrase(&self, input: RevealPhraseInput) -> Result<Vec<String>, ApiError>;
}
