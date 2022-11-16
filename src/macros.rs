#[cfg(test)]
#[macro_use]
mod test {
    #[macro_export]
    macro_rules! conversation {
        ($id:expr) => {{
            KeybaseConversation {
                id: $id.to_owned(),
                unread: false,
                channel: Channel {
                    name: "channel".to_owned(),
                    topic_name: "".to_owned(),
                    members_type: MemberType::User,
                },
            }
        }};
    }

    #[macro_export]
    macro_rules! message {
        ($convo_id: expr, $text: expr) => {{
            use $crate::types::Sender;
            Message {
                conversation_id: $convo_id.to_owned(),
                content: MessageType::Text {
                    text: MessageBody {
                        body: $text.to_owned(),
                    },
                },
                channel: Channel {
                    name: "channel".to_owned(),
                    topic_name: "".to_owned(),
                    members_type: MemberType::User,
                },
                sender: Sender {
                    device_name: "My Device".to_owned(),
                    username: "Some Guy".to_owned(),
                },
            }
        }};
    }
}
