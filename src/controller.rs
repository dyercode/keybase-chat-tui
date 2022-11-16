use tokio::sync::mpsc::Receiver;

use crate::client::KeybaseClient;
use crate::state::ApplicationState;
use crate::types::{KeybaseConversation, ListenerEvent, UiEvent};
use anyhow::{anyhow, Result};

pub struct Controller<S, C> {
    client: C,
    state: S,
    ui_receiver: Receiver<UiEvent>,
}

impl<S: ApplicationState, C: KeybaseClient> Controller<S, C> {
    pub fn new(client: C, state: S, receiver: Receiver<UiEvent>) -> Self {
        Controller {
            client,
            state,
            ui_receiver: receiver,
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        let conversations = self
            .client
            .fetch_conversations()
            .await
            .map_err(|_| anyhow!("fetching conversations blew up"))?;
        if let Some(head) = conversations.get(0) {
            let first_id = head.id.clone();
            self.state.set_conversations(
                conversations
                    .into_iter()
                    .map(KeybaseConversation::into)
                    .collect(),
            );
            self.state.set_current_conversation(&first_id)?;
        };
        Ok(())
    }

    pub async fn process_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut client_receiver = self.client.get_receiver();
        loop {
            tokio::select! {
                msg = client_receiver.recv() => {
                    if let Some(value) = msg {
                        match value {
                            ListenerEvent::ChatMessage(message) => {
                                let conversation_id = &message.msg.conversation_id;
                                self.state.insert_message(conversation_id, message.msg.clone());
                            }
                        }
                    }
                },
                msg = self.ui_receiver.recv() => {
                    if let Some(value) = msg {
                        match value {
                            UiEvent::SendMessage(message) => {
                                if let Some(convo) = self.state.get_current_conversation() {
                                    let channel = &convo.data.channel;
                                    self.client.send_message(channel, message).await?;
                                }
                            },
                            UiEvent::SwitchConversation(conversation_id) => {
                                info!("received event to switch conversation {}", conversation_id);
                                switch_conversation(&mut self.client, &mut self.state, conversation_id).await?;
                            }
                        }
                    }
                },
            }
        }
    }
}

async fn switch_conversation<S: ApplicationState, C: KeybaseClient>(
    client: &mut C,
    state: &mut S,
    conversation_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(convo) = state
        .get_conversation_mut(&conversation_id)
        .filter(|c| !c.fetched)
    {
        info!("fetching messages for {:?}", &convo.data);
        let messages = client.fetch_messages(&convo.data, 20).await?;
        info!("fetched messages");
        convo.fetched = true;
        convo.insert_messages(messages);
    }

    info!("setting conversation to {}", &conversation_id);
    state.set_current_conversation(&conversation_id)?;
    Ok(())
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::client::MockKeybaseClient;
    use crate::conversation;
    use crate::state::ApplicationStateInner;
    use crate::types::*;

    #[tokio::test]
    async fn init() {
        let (_, r) = tokio::sync::mpsc::channel::<UiEvent>(32);
        let mut client = MockKeybaseClient::new();
        client
            .expect_fetch_conversations()
            .times(1)
            .return_once(|| Ok(vec![]));

        let state = ApplicationStateInner::default();

        let mut controller = Controller::new(client, state, r);
        controller.init().await.unwrap();
    }

    #[tokio::test]
    async fn switch_conversation() {
        let (s, r) = tokio::sync::mpsc::channel::<UiEvent>(32);
        let (_, c_recv) = tokio::sync::mpsc::channel::<ListenerEvent>(32);
        let mut client = MockKeybaseClient::new();
        let convo = conversation!("test1");
        let convo2 = conversation!("test2");
        let c1 = convo.clone();
        let c2 = convo2.clone();

        client
            .expect_get_receiver()
            .times(1)
            .return_once(move || c_recv);

        client
            .expect_fetch_conversations()
            .times(1)
            .return_once(move || Ok(vec![c1, c2]));

        client
            .expect_fetch_messages()
            .withf(move |c: &KeybaseConversation, _| c.id == "test1")
            .times(1)
            .return_once(|_, _| Ok(vec![]));

        let state = ApplicationStateInner::default();

        let mut controller = Controller::new(client, state, r);

        controller.init().await.unwrap();

        tokio::spawn(async move {
            s.send(UiEvent::SwitchConversation("test1".to_owned()))
                .await
                .ok();
        });

        tokio::select! {
            _ = controller.process_events() => {},
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {}
        }
    }
}
