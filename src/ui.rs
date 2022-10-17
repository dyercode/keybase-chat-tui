// # ui.rs
//
// Contains the main UI struct and all the views that don't exist in their own module.

use std::path::PathBuf;
use std::time::Duration;

use cursive::{event::*, view::*, views::*, CbSink, Cursive, CursiveRunnable};
use dirs::config_dir;
use log::debug;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::state::StateObserver;
use crate::types::{Conversation, Message, MessageType, UiEvent};
use crate::views::conversation::{ConversationName, ConversationView};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct UiBuilder {
    cursive: CursiveRunnable,
}

impl UiBuilder {
    pub fn new() -> Self {
        let mut cursive = cursive::default();

        // load a theme from `$HOME/.config/keybase-chat-tui/theme.toml` (on linux)
        if let Some(dir) = config_dir() {
            let theme_path = PathBuf::new().join(dir).join("keybase-chat-tui/theme.toml");
            if theme_path.exists() {
                cursive
                    .load_toml(theme_path.to_str().unwrap())
                    .expect("Failed to load theme");
            }
        }

        cursive.add_layer(
            Dialog::around(
                LinearLayout::horizontal()
                    .child(conversation_list())
                    .child(chat_area()),
            )
            .title(format!("keybase-chat-tui ({})", VERSION)),
        );

        // focus the edit view (where you type) on the initial render
        cursive.focus_name("edit").unwrap();

        UiBuilder { cursive }
    }

    pub fn build(mut self) -> (Ui, Receiver<UiEvent>, CbSink) {
        let (ui_send, ui_recv) = mpsc::channel(32);
        // let (tx, rx) = sync::mpsc::channel::<UiMessage>();
        let executor = UiExecutor { sender: ui_send };
        let sender: CbSink = self.cursive.cb_sink().clone();

        self.cursive.set_user_data(executor);

        (
            Ui {
                cursive: self.cursive,
            },
            ui_recv,
            sender.clone(),
        )
    }
}

pub struct Ui {
    cursive: CursiveRunnable,
}

impl Ui {
    pub async fn run(&mut self) {
        let mut runner = self.cursive.runner();
        loop {
            let sleeper = tokio::time::sleep(Duration::from_millis(1));
            if !runner.is_running() {
                break;
            }
            runner.step();
            sleeper.await;
        }
    }
}

// TODO: move this into a new view that inherits from TextView so we can color the username.
fn render_message(view: &mut TextView, message: &Message) {
    match message.content {
        MessageType::Text { ref text } => {
            view.append(&format!("{}: {}\n", message.sender.username, text.body));
        }
        MessageType::Unfurl {} => {
            view.append(&format!(
                "{} sent an Unfurl and I don't know how to render it\n",
                message.sender.username
            ));
        }
        MessageType::Join => {}
        MessageType::Attachment { .. } => {}
        MessageType::Metadata { .. } => {}
        MessageType::System { .. } => {}
        MessageType::Reaction { .. } => {}
    }
}

/* possible replacement for the Cursive-having Ui to pass into state */
pub struct UiObserver {
    pub sender: CbSink,
}
impl StateObserver for UiObserver {
    fn on_conversation_change(&mut self, data: &Conversation) {
        let name = data.get_name();
        let messages: Vec<Message> = data.messages.iter().rev().cloned().collect();
        self.sender
            .send(Box::new(move |cursive: &mut Cursive| {
                cursive.call_on_name("chat_container", move |view: &mut TextView| {
                    view.set_content("");
                    for msg in messages {
                        render_message(view, &msg);
                    }
                });
                cursive.call_on_name("chat_panel", move |view: &mut Panel<LinearLayout>| {
                    view.set_title(name);
                });
                cursive.focus_name("edit").unwrap();
            }))
            .unwrap();
    }

    fn on_conversations_added(&mut self, data: &[Conversation]) {
        let convos: Vec<Conversation> = data.to_vec();
        self.sender
            .send(Box::new(move |cursive: &mut Cursive| {
                cursive.call_on_name("conversation_list", |view: &mut ListView| {
                    view.clear();
                    for convo in convos {
                        debug!("Adding child: {}", &convo.get_name());
                        view.add_child("", conversation_view(convo.clone()))
                    }
                });
            }))
            .unwrap();
    }

    fn on_message(&mut self, message: &Message, conversation_id: &str, active: bool) {
        let ci = conversation_id.to_string();
        let message = message.clone();
        self.sender
            .send(Box::new(move |cursive: &mut Cursive| {
                if active {
                    // write the message in the chat box
                    cursive.call_on_name("chat_container", |view: &mut TextView| {
                        render_message(view, &message);
                    });
                    // highlight the conversation with unread messages
                    cursive.call_on_name(&ci, |view: &mut ConversationView| {
                        view.unread = true;
                    });
                }
            }))
            .unwrap();
    }
}

#[derive(Clone)]
struct UiExecutor {
    sender: Sender<UiEvent>,
}

// helper to create the view of available conversations on the left. Should probably go to its own
// module.
fn conversation_view(convo: Conversation) -> impl View {
    let id = convo.id.clone();
    let view = ConversationView::new(convo).with_name(id);
    OnEventView::new(view)
        // handle left clicking on a conversation name
        .on_event_inner(EventTrigger::mouse(), handle_switch)
        // handle pressing enter when a conversation name has focus
        .on_event_inner(Key::Enter, handle_switch)
}

fn handle_switch(v: &mut NamedView<ConversationView>, e: &Event) -> Option<EventResult> {
    if let Event::Mouse {
        event: MouseEvent::Release(MouseButton::Left),
        ..
    } = *e
    {
        let convo = v.conversation_id();

        Some(EventResult::with_cb(move |s| {
            s.with_user_data(|executor: &mut UiExecutor| {
                let exec = executor.clone();
                let c = convo.clone();
                tokio::spawn(async move {
                    exec.sender.send(UiEvent::SwitchConversation(c)).await.ok();
                });
            });
        }))
    } else {
        None
    }
}

fn send_chat_message(s: &mut Cursive, msg: &str) {
    if !msg.is_empty() {
        s.call_on_name("edit", |view: &mut EditView| view.set_content(""));
        s.with_user_data(|executor: &mut UiExecutor| {
            let exec = executor.clone();
            let c = msg.to_owned();
            tokio::spawn(async move {
                exec.sender.send(UiEvent::SendMessage(c)).await.ok();
            });
        });
    }
}

fn conversation_list() -> BoxedView {
    let convo_list =
        Panel::new(ListView::new().with_name("conversation_list")).title("Conversations");
    BoxedView::new(
        ResizedView::new(SizeConstraint::Free, SizeConstraint::Full, convo_list).into_boxed_view(),
    )
}

fn chat_area() -> BoxedView {
    let mut text = TextView::new("").with_name("chat_container").scrollable();
    text.set_scroll_strategy(ScrollStrategy::StickToBottom);

    let chat_layout = LinearLayout::vertical()
        .child(ResizedView::new(
            SizeConstraint::Full,
            SizeConstraint::Full,
            text,
        ))
        .child(
            EditView::new()
                .on_submit(send_chat_message)
                .with_name("edit"),
        );
    let chat = Panel::new(chat_layout).with_name("chat_panel");

    BoxedView::new(
        ResizedView::new(SizeConstraint::Full, SizeConstraint::Full, chat).into_boxed_view(),
    )
}
