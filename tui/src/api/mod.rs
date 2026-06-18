pub mod mock;
pub mod types;

use types::*;
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait WalletApi {
    async fn get_setup(&self) -> SetupData;
    async fn submit_setup_create(&self, input: SetupCreateInput) -> Result<(), ApiError>;
    async fn submit_setup_import(&self, input: SetupImportInput) -> Result<(), ApiError>;

    async fn get_wallets(&self) -> WalletsData;
    async fn get_assets(&self, chain_id: &str) -> AssetsData;

    async fn get_home(&self) -> HomeData;
    async fn submit_home(&self, input: HomeInput) -> HomeData;
    async fn home_state(&self) -> ApiState<HomeData>;
    async fn refresh_home(&self);

    async fn get_receive(&self, chain_id: &str) -> ReceiveData;
    async fn submit_receive(&self, input: ReceiveInput) -> ReceiveData;
    async fn receive_state(&self, chain_id: &str) -> ApiState<ReceiveData>;
    async fn refresh_receive(&self, chain_id: &str);

    async fn get_send(&self, chain_id: &str) -> SendData;
    async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData;
    async fn submit_send_confirm(&self, input: SendConfirmInput) -> SendResult;
    async fn send_state(&self, chain_id: &str) -> ApiState<SendData>;
    async fn refresh_send(&self, chain_id: &str);

    async fn get_lock(&self) -> LockData;
    async fn submit_lock(&self, input: LockInput) -> Result<(), ApiError>;

    async fn get_settings(&self) -> SettingsData;
    async fn submit_settings(&self, input: SettingsInput) -> Result<(), ApiError>;
    async fn submit_reveal_phrase(&self, input: RevealPhraseInput) -> Result<Vec<String>, ApiError>;
}
