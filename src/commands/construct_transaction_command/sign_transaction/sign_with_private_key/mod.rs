use dialoguer::{theme::ColorfulTheme, Input, Select};
use near_primitives::borsh::BorshSerialize;
use strum::{EnumDiscriminants, EnumIter, EnumMessage, IntoEnumIterator};

/// подписание сформированной транзакции с помощью личных ключей
#[derive(Debug, Default, clap::Clap)]
pub struct CliSignPrivateKey {
    #[clap(long)]
    signer_public_key: Option<near_crypto::PublicKey>,
    #[clap(long)]
    signer_secret_key: Option<near_crypto::SecretKey>,
    #[clap(subcommand)]
    submit: Option<Submit>,
}

#[derive(Debug)]
pub struct SignPrivateKey {
    pub signer_public_key: near_crypto::PublicKey,
    pub signer_secret_key: near_crypto::SecretKey,
    pub submit: Option<Submit>,
}

impl From<CliSignPrivateKey> for SignPrivateKey {
    fn from(item: CliSignPrivateKey) -> Self {
        let signer_public_key: near_crypto::PublicKey = match item.signer_public_key {
            Some(cli_public_key) => cli_public_key,
            None => SignPrivateKey::signer_public_key(),
        };
        let signer_secret_key: near_crypto::SecretKey = match item.signer_secret_key {
            Some(cli_secret_key) => cli_secret_key,
            None => SignPrivateKey::signer_secret_key(),
        };
        let submit: Option<Submit> = item.submit;
        let public_key_origin: near_crypto::PublicKey =
            near_crypto::SecretKey::public_key(&signer_secret_key);
        if &signer_public_key == &public_key_origin {
            Self {
                signer_public_key,
                signer_secret_key,
                submit,
            }
        } else {
            println!("\nError: The key pair does not match. Re-enter the keys.\n");
            let signer_public_key: near_crypto::PublicKey = Self::signer_public_key();
            let signer_secret_key: near_crypto::SecretKey = Self::signer_secret_key();
            Self::from(CliSignPrivateKey {
                signer_public_key: Some(signer_public_key),
                signer_secret_key: Some(signer_secret_key),
                submit: None,
            })
        }
    }
}

impl SignPrivateKey {
    pub fn signer_public_key() -> near_crypto::PublicKey {
        Input::new()
            .with_prompt("Enter sender's public key")
            .interact_text()
            .unwrap()
    }

    pub fn signer_secret_key() -> near_crypto::SecretKey {
        Input::new()
            .with_prompt("Enter sender's private key")
            .interact_text()
            .unwrap()
    }

    fn rpc_client(self, selected_server_url: &str) -> near_jsonrpc_client::JsonRpcClient {
        near_jsonrpc_client::new_client(&selected_server_url)
    }

    pub async fn process(
        self,
        prepopulated_unsigned_transaction: near_primitives::transaction::Transaction,
        network_connection_config: Option<crate::common::ConnectionConfig>,
    ) -> crate::CliResult {
        let public_key: near_crypto::PublicKey = self.signer_public_key.clone();
        let signer_secret_key: near_crypto::SecretKey = self.signer_secret_key.clone();
        let submit: Option<Submit> = self.submit.clone();
        match network_connection_config {
            None => {
                let unsigned_transaction = near_primitives::transaction::Transaction {
                    public_key,
                    ..prepopulated_unsigned_transaction
                };
                let signature =
                    signer_secret_key.sign(unsigned_transaction.get_hash_and_size().0.as_ref());
                let signed_transaction = near_primitives::transaction::SignedTransaction::new(
                    signature,
                    unsigned_transaction,
                );
                let serialize_to_base64 = near_primitives::serialize::to_base64(
                    signed_transaction
                        .try_to_vec()
                        .expect("Transaction is not expected to fail on serialization"),
                );
                println!(
                    "\n\n---  Signed transaction:   ---\n    {:#?}",
                    &signed_transaction
                );
                match submit {
                    Some(submit) => submit.process_offline(signed_transaction, serialize_to_base64),
                    None => {
                        let submit = Submit::choose_submit();
                        submit.process_offline(signed_transaction, serialize_to_base64)
                    }
                }
            }
            Some(network_connection_config) => {
                let online_signer_access_key_response = self
                    .rpc_client(network_connection_config.rpc_url().as_str())
                    .query(near_jsonrpc_primitives::types::query::RpcQueryRequest {
                        block_reference: near_primitives::types::Finality::Final.into(),
                        request: near_primitives::views::QueryRequest::ViewAccessKey {
                            account_id: prepopulated_unsigned_transaction.signer_id.clone(),
                            public_key: public_key.clone(),
                        },
                    })
                    .await
                    .map_err(|err| {
                        color_eyre::Report::msg(format!(
                            "Failed to fetch public key information for nonce: {:?}",
                            err
                        ))
                    })?;
                let current_nonce =
                    if let near_jsonrpc_primitives::types::query::QueryResponseKind::AccessKey(
                        online_signer_access_key,
                    ) = online_signer_access_key_response.kind
                    {
                        online_signer_access_key.nonce
                    } else {
                        return Err(color_eyre::Report::msg(format!("Error current_nonce")));
                    };
                let unsigned_transaction = near_primitives::transaction::Transaction {
                    public_key,
                    block_hash: online_signer_access_key_response.block_hash,
                    nonce: current_nonce + 1,
                    ..prepopulated_unsigned_transaction
                };
                let signature =
                    signer_secret_key.sign(unsigned_transaction.get_hash_and_size().0.as_ref());
                let signed_transaction = near_primitives::transaction::SignedTransaction::new(
                    signature,
                    unsigned_transaction,
                );
                let serialize_to_base64 = near_primitives::serialize::to_base64(
                    signed_transaction
                        .try_to_vec()
                        .expect("Transaction is not expected to fail on serialization"),
                );
                println!(
                    "\n\n---  Signed transaction:   ---\n    {:#?}",
                    &signed_transaction
                );
                match submit {
                    None => {
                        let submit = Submit::choose_submit();
                        submit
                            .process_online(
                                network_connection_config,
                                signed_transaction,
                                serialize_to_base64,
                            )
                            .await
                    }
                    Some(submit) => {
                        submit
                            .process_online(
                                network_connection_config,
                                signed_transaction,
                                serialize_to_base64,
                            )
                            .await
                    }
                }
            }
        }
    }
}

#[derive(Debug, EnumDiscriminants, Clone, clap::Clap)]
#[strum_discriminants(derive(EnumMessage, EnumIter))]
pub enum Submit {
    #[strum_discriminants(strum(
        message = "Do you want send the transaction to the server (it's works only for online mode)"
    ))]
    Send,
    #[strum_discriminants(strum(message = "Do you want show the transaction on display?"))]
    Display,
}

impl Submit {
    pub fn choose_submit() -> Self {
        println!();
        let variants = SubmitDiscriminants::iter().collect::<Vec<_>>();
        let submits = variants
            .iter()
            .map(|p| p.get_message().unwrap().to_owned())
            .collect::<Vec<_>>();
        let select_submit = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select an action that you want to add to the action:")
            .items(&submits)
            .default(0)
            .interact()
            .unwrap();
        match variants[select_submit] {
            SubmitDiscriminants::Send => Submit::Send,
            SubmitDiscriminants::Display => Submit::Display,
        }
    }

    pub fn process_offline(
        self,
        signed_transaction: near_primitives::transaction::SignedTransaction,
        serialize_to_base64: String,
    ) -> crate::CliResult {
        println!("\n\n\n===========  DISPLAY  ==========");
        println!(
            "\n\n---  Signed transaction:   ---\n    {:#?}",
            &signed_transaction
        );
        println!(
            "\n\n---  serialize_to_base64:   --- \n   {:#?}",
            &serialize_to_base64
        );
        Ok(())
    }

    pub async fn process_online(
        self,
        network_connection_config: crate::common::ConnectionConfig,
        signed_transaction: near_primitives::transaction::SignedTransaction,
        serialize_to_base64: String,
    ) -> crate::CliResult {
        match self {
            Submit::Send => {
                println!("\n\n\n========= SENT =========");
                println!(
                    "\n\n---  Signed transaction:   ---\n    {:#?}",
                    &signed_transaction
                );
                println!(
                    "\n\n---  serialize_to_base64:   --- \n   {:#?}",
                    &serialize_to_base64
                );
                let json_rcp_client =
                    near_jsonrpc_client::new_client(network_connection_config.rpc_url().as_str());
                let transaction_info = loop {
                    let transaction_info_result = json_rcp_client
                        .broadcast_tx_commit(near_primitives::serialize::to_base64(
                            signed_transaction
                                .try_to_vec()
                                .expect("Transaction is not expected to fail on serialization"),
                        ))
                        .await;
                    match transaction_info_result {
                        Ok(response) => {
                            break response;
                        }
                        Err(err) => {
                            if let Some(serde_json::Value::String(data)) = &err.data {
                                if data.contains("Timeout") {
                                    println!("Error transaction: {:?}", err);
                                    continue;
                                }
                            }
                            return Err(color_eyre::Report::msg(format!(
                                "Error transaction: {:?}",
                                err
                            )));
                        }
                    };
                };
                println!("\n\n---  Success:  ---\n {:#?}", &transaction_info);
            }
            Submit::Display => {
                println!("\n\n\n===========  DISPLAY  ==========");
                println!(
                    "\n\n---  Signed transaction:   ---\n {:#?}",
                    &signed_transaction
                );
                println!(
                    "\n\n---  serialize_to_base64:   --- \n {:#?}",
                    &serialize_to_base64
                );
            }
        }
        Ok(())
    }
}
