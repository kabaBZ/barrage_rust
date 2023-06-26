use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::client::uri_mode::UriMode;
use tungstenite::client::uri_mode::WsMode;
use tungstenite::connect;
use tungstenite::handshake::client::Request;
use tungstenite::protocol::Message;

struct DyDanmuMsgHandler;

impl DyDanmuMsgHandler {
    fn dy_encode(&self, msg: &str) -> Vec<u8> {
        let data_len = msg.len() + 9;
        let len_byte = (data_len as u32).to_le_bytes();
        let msg_byte = msg.as_bytes();
        let send_byte = [0xB1, 0x02, 0x00, 0x00];
        let end_byte = [0x00];

        let mut data = Vec::new();
        data.extend_from_slice(&len_byte);
        data.extend_from_slice(&len_byte);
        data.extend_from_slice(&send_byte);
        data.extend_from_slice(&msg_byte);
        data.extend_from_slice(&end_byte);

        data
    }

    fn parse_msg(&self, raw_msg: &str) -> Vec<(String, String)> {
        let attrs: Vec<&str> = raw_msg.split("/").collect();
        let mut res = Vec::new();
        for attr in attrs {
            let attr = attr.replace("@s", "/");
            let attr = attr.replace("@A", "@");
            let couple: Vec<&str> = attr.split("@=").collect();
            res.push((couple[0].to_string(), couple[1].to_string()));
        }
        res
    }

    fn dy_decode(&self, msg_byte: &[u8]) -> Vec<String> {
        let mut pos = 0;
        let mut msg = Vec::new();
        while pos < msg_byte.len() {
            let content_length = u32::from_le_bytes([
                msg_byte[pos],
                msg_byte[pos + 1],
                msg_byte[pos + 2],
                msg_byte[pos + 3],
            ]);
            let content =
                String::from_utf8_lossy(&msg_byte[pos + 12..pos + 3 + content_length as usize])
                    .to_string();
            msg.push(content);
            pos += 4 + content_length as usize;
        }
        msg
    }

    fn get_chat_messages(&self, msg_byte: &[u8]) -> Vec<(String, String)> {
        let decode_msg = self.dy_decode(msg_byte);
        let mut messages = Vec::new();
        for msg in decode_msg {
            let res = self.parse_msg(&msg);
            if let Some(("type", value)) = res.get(0) {
                if value == "chatmsg" {
                    messages.push(res);
                }
            }
        }
        messages
    }
}

struct DyDanmuCrawler {
    room_id: String,
    client: Option<tungstenite::WebSocket<tungstenite::client::AutoStream>>,
    msg_handler: DyDanmuMsgHandler,
    keep_heartbeat: bool,
    heartbeat_timer: Option<tokio::time::Instant>,
}

impl DyDanmuCrawler {
    fn new(room_id: &str) -> DyDanmuCrawler {
        DyDanmuCrawler {
            room_id: room_id.to_string(),
            client: None,
            msg_handler: DyDanmuMsgHandler,
            keep_heartbeat: true,
            heartbeat_timer: None,
        }
    }

    async fn start(&mut self) -> Result<(), Box<dyn Error>> {
        let url = "wss://danmuproxy.douyu.com:8506/";
        let request = Request {
            uri: url.parse()?,
            extra_headers: Vec::new(),
            mode: UriMode::Ws(WsMode::Binary),
        };
        let (mut tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (response, _) = connect(request).await?;
        self.client = Some(tx);
        let room_id = self.room_id.clone();
        tokio::spawn(async move {
            let (mut write, mut read) = response.split();
            let join_group_msg = format!("type@=joingroup/rid@={}/gid@=1/", room_id);
            let msg_bytes = self.msg_handler.dy_encode(&join_group_msg);
            write.send(Message::Binary(msg_bytes)).await.unwrap();
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Binary(msg_bytes)) => {
                        let chat_messages = self.msg_handler.get_chat_messages(&msg_bytes);
                        for message in chat_messages {
                            println!("{}: {}", message[0].1, message[1].1);
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(err) => {
                        println!("Error: {}", err);
                        break;
                    }
                    _ => {}
                }
            }
        });
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let room_id = "11144156";
    let mut crawler = DyDanmuCrawler::new(room_id);
    let mut rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        crawler.start().await?;
    });
    Ok(())
}
