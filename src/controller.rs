use tokio::sync::mpsc::{Receiver};

use crate::client::Client;
use crate::state::ApplicationState;
use crate::types::{ListenerEvent, UiMessage};

pub struct Controller<S> {
    client: Client,
    state: S,
    client_receiver: Receiver<ListenerEvent>,
    ui_receiver: Receiver<UiMessage>,
    listener: Option<tokio::process::Child>
}

impl<S> Drop for Controller<S> {
    fn drop(&mut self) {
        if let Some(mut child) = self.listener.take() {
            child.kill().unwrap();
        }
    }
}

impl<S: ApplicationState> Controller<S> {
    pub fn new(mut client: Client, state: S, receiver: Receiver<UiMessage>) -> Self {
        let r = client.register();
        Controller {
            client,
            state,
            listener: None, 
            client_receiver: r,
            ui_receiver: receiver
                
        }
    }

    pub async fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.listener = Some(self.client.start_listener().await?);
        let conversations = self.client.fetch_conversations().await?;
        if !conversations.is_empty() {
            let first_id = conversations[0].id.clone();
            self.state.set_conversations(conversations.into_iter().map(|c| c.into()).collect());
            self.state.set_current_conversation(&first_id);
        }
        Ok(())
    }

    pub async fn process_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            tokio::select! {
                msg = self.client_receiver.recv() => {
                    if let Some(value) = msg {
                        match value {
                            ListenerEvent::ChatMessage(msg) => {
                                let conversation_id = &msg.msg.conversation_id;
                                self.state.insert_message(conversation_id, msg.msg.clone());
                            }
                        }
                    }
                },
                msg = self.ui_receiver.recv() => {
                    if let Some(value) = msg {
                        match value {
                            UiMessage::SendMessage(msg) => {
                                if let Some(convo) = self.state.get_current_conversation() {
                                    let channel = &convo.data.channel;
                                    self.client.send_message(channel, msg).await?;
                                }
                            },
                            UiMessage::SwitchConversation(conversation_id) => {
                                switch_conversation(&mut self.client, &mut self.state, conversation_id).await?;
                            }
                        }
                    }
                },
            }
        }
    }
}

async fn switch_conversation<S: ApplicationState>(client: &mut Client, state: &mut S, conversation_id: String) -> Result<(), Box<dyn std::error::Error>>{
    let (convo_id, should_fetch) = {
        if let Some(mut convo) = state.get_conversation_mut(&conversation_id){
            if !convo.fetched {
                convo.fetched = true;
                (Some(convo.id.clone()), true)
            } else {
                (Some(convo.id.clone()), false)
            }
        } else {
            (None, false)
        }
    };

    if should_fetch {
        let id = &convo_id.unwrap();
        let convo = state.get_conversation(id).unwrap();
        let messages = client.fetch_messages(&convo.data, 20).await?;
                
        state.get_conversation_mut(id).unwrap().insert_messages(messages);
    }

    state.set_current_conversation(&conversation_id);
    Ok(())
}

