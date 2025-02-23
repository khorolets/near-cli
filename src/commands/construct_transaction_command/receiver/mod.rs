use dialoguer::Input;

#[derive(Debug, clap::Clap)]
pub enum CliSendTo {
    /// Specify a receiver
    Receiver(CliReceiver),
}

#[derive(Debug)]
pub enum SendTo {
    Receiver(Receiver),
}

impl From<CliSendTo> for SendTo {
    fn from(item: CliSendTo) -> Self {
        match item {
            CliSendTo::Receiver(cli_receiver) => {
                let receiver = Receiver::from(cli_receiver);
                Self::Receiver(receiver)
            }
        }
    }
}

impl SendTo {
    pub fn send_to() -> Self {
        Self::from(CliSendTo::Receiver(Default::default()))
    }

    pub async fn process(
        self,
        prepopulated_unsigned_transaction: near_primitives::transaction::Transaction,
        network_connection_config: Option<crate::common::ConnectionConfig>,
    ) -> crate::CliResult {
        match self {
            SendTo::Receiver(receiver) => {
                receiver
                    .process(prepopulated_unsigned_transaction, network_connection_config)
                    .await
            }
        }
    }
}

/// данные о получателе транзакции
#[derive(Debug, Default, clap::Clap)]
pub struct CliReceiver {
    receiver_account_id: Option<String>,
    #[clap(subcommand)]
    action: Option<super::transaction_actions::CliNextAction>,
}

#[derive(Debug)]
pub struct Receiver {
    pub receiver_account_id: String,
    pub action: super::transaction_actions::NextAction,
}

impl From<CliReceiver> for Receiver {
    fn from(item: CliReceiver) -> Self {
        let receiver_account_id: String = match item.receiver_account_id {
            Some(cli_receiver_account_id) => cli_receiver_account_id,
            None => Receiver::input_receiver_account_id(),
        };
        let action: super::transaction_actions::NextAction = match item.action {
            Some(cli_next_action) => super::transaction_actions::NextAction::from(cli_next_action),
            None => super::transaction_actions::NextAction::input_next_action(),
        };
        Self {
            receiver_account_id,
            action,
        }
    }
}

impl Receiver {
    pub fn input_receiver_account_id() -> String {
        Input::new()
            .with_prompt("What is the account ID of the receiver?")
            .interact_text()
            .unwrap()
    }

    pub async fn process(
        self,
        prepopulated_unsigned_transaction: near_primitives::transaction::Transaction,
        network_connection_config: Option<crate::common::ConnectionConfig>,
    ) -> crate::CliResult {
        let unsigned_transaction = near_primitives::transaction::Transaction {
            receiver_id: self.receiver_account_id.clone(),
            ..prepopulated_unsigned_transaction
        };
        self.action
            .process(unsigned_transaction, network_connection_config)
            .await
    }
}
