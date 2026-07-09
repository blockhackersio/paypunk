use paypunk_chains_zcash::signer::ZcashSignerProtocol;
use paypunk_chains_zcash::to_local_params;
use paypunk_types::{
    ArtifactSummary, ChainId, KeypunkdRequest, KeypunkdResponse, ProtocolId, SignerProtocol,
};
use zcash_protocol::consensus::{Network, NetworkType};

pub struct SignerState {
    pub seed: [u8; 64],
    pub mnemonic: String,
    zcash_signer: Option<ZcashSignerProtocol>,
    pub status: SignerStatus,
}

pub enum SignerStatus {
    Idle,
    Previewing {
        raw_artifact: Vec<u8>,
        summary: ArtifactSummary,
        derivation_path: String,
        protocol: ProtocolId,
    },
    Signing,
    Signed {
        signed_artifact: Vec<u8>,
    },
    Error(String),
}

impl SignerState {
    pub fn create() -> Self {
        let mnemonic =
            "ribbon velvet ocean puzzle harvest guitar shadow ladder comfort raven spring anchor"
                .to_string();
        let seed = bip39::Mnemonic::parse(&mnemonic)
            .expect("valid mnemonic")
            .to_seed("");

        Self {
            seed,
            mnemonic,
            zcash_signer: None,
            status: SignerStatus::Idle,
        }
    }

    fn get_or_init_zcash(&mut self, chain_id: &ChainId) -> Result<&ZcashSignerProtocol, String> {
        if self.zcash_signer.is_none() {
            let (network, network_type) = match chain_id.reference.as_str() {
                "mainnet" => (Network::MainNetwork, NetworkType::Main),
                "testnet" => (Network::TestNetwork, NetworkType::Test),
                "regtest" => (Network::TestNetwork, NetworkType::Regtest),
                _ => return Err(format!("unsupported zcash network: {}", chain_id.reference)),
            };
            let params = to_local_params(network, network_type);
            self.zcash_signer = Some(ZcashSignerProtocol::new(params, network_type));
        }
        Ok(self.zcash_signer.as_ref().unwrap())
    }

    pub fn handle_request(&mut self, request_bytes: &[u8]) -> Vec<u8> {
        let request: KeypunkdRequest = match postcard::from_bytes(request_bytes) {
            Ok(r) => r,
            Err(e) => {
                let resp = KeypunkdResponse::Error {
                    message: format!("deserialize failed: {e}"),
                };
                return postcard::to_allocvec(&resp).unwrap_or_default();
            }
        };

        let response = match request {
            KeypunkdRequest::PreviewArtifact {
                raw_artifact,
                protocol,
                chain_id,
                derivation_path,
            } => match protocol {
                ProtocolId::Zcash => {
                    let signer = match self.get_or_init_zcash(&chain_id) {
                        Ok(s) => s,
                        Err(e) => {
                            return postcard::to_allocvec(&KeypunkdResponse::Error { message: e })
                                .unwrap_or_default();
                        }
                    };

                    let parsed = match signer.parse_artifact(&raw_artifact) {
                        Ok(p) => p,
                        Err(e) => {
                            return postcard::to_allocvec(&KeypunkdResponse::Error { message: e })
                                .unwrap_or_default();
                        }
                    };

                    let summary: ArtifactSummary = match postcard::from_bytes(&parsed) {
                        Ok(s) => s,
                        Err(e) => {
                            return postcard::to_allocvec(&KeypunkdResponse::Error {
                                message: format!("summary deserialize: {e}"),
                            })
                            .unwrap_or_default();
                        }
                    };

                    self.status = SignerStatus::Previewing {
                        raw_artifact,
                        summary,
                        derivation_path,
                        protocol,
                    };

                    KeypunkdResponse::ArtifactPreview {
                        raw_artifact: vec![],
                        parsed_summary: parsed,
                        signature: vec![],
                        keypunkd_public_key: [0u8; 32],
                    }
                }
                ProtocolId::Ethereum => KeypunkdResponse::Error {
                    message: "Ethereum signing not yet supported in signer".to_string(),
                },
            },
            _ => KeypunkdResponse::Error {
                message: "unsupported request".to_string(),
            },
        };

        postcard::to_allocvec(&response).unwrap_or_default()
    }

    pub fn approve_and_sign(&mut self) -> Result<Vec<u8>, String> {
        let (raw_artifact, derivation_path, protocol) = match &self.status {
            SignerStatus::Previewing {
                raw_artifact,
                derivation_path,
                protocol,
                ..
            } => (raw_artifact.clone(), derivation_path.clone(), *protocol),
            _ => return Err("no preview to sign".to_string()),
        };

        self.status = SignerStatus::Signing;

        let signed = match protocol {
            ProtocolId::Zcash => {
                let signer = self
                    .zcash_signer
                    .as_ref()
                    .ok_or("zcash signer not initialized")?;
                signer.sign(&self.seed, &derivation_path, &raw_artifact)?
            }
            ProtocolId::Ethereum => {
                return Err("Ethereum signing not yet supported".to_string());
            }
        };

        self.status = SignerStatus::Signed {
            signed_artifact: signed.clone(),
        };

        Ok(signed)
    }
}
