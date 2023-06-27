// extern crate tungstenite;
// extern crate url;
// use websocket::client;
use native_tls::{TlsConnector, TlsStream};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
// use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
// use std::thread;
// use tungstenite::client::AutoStream;
// use tungstenite::protocol::Message;
// use url::Url;
// use websocket::ClientBuilder;

struct DyDanmuMsgHandler;

impl DyDanmuMsgHandler {
    fn dy_encode(&self, msg: &str) -> Vec<u8> {
        // 头部8字节，尾部1字节，与字符串长度相加即数据长度
        // 为什么不加最开头的那个消息长度所占4字节呢？这得问问斗鱼^^
        let data_len = msg.len() + 9;

        // 字符串转化为字节流
        let msg_byte = msg.as_bytes();

        // 将数据长度转化为小端整数字节流
        let len_byte = (data_len as u32).to_le_bytes();

        // 前两个字节按照小端顺序拼接为0x02b1，转化为十进制即689（《协议》中规定的客户端发送消息类型）
        // 后两个字节即《协议》中规定的加密字段与保留字段，置0
        let send_byte: [u8; 4] = [0xB1, 0x02, 0x00, 0x00];

        // 尾部以'\0'结束
        let end_byte: [u8; 1] = [0x00];

        // 按顺序拼接在一起
        let mut data = Vec::new();
        data.extend_from_slice(&len_byte);
        data.extend_from_slice(&len_byte);
        data.extend_from_slice(&send_byte);
        data.extend_from_slice(&msg_byte);
        data.extend_from_slice(&end_byte);

        data
    }

    fn parse_msg(&self, raw_msg: &str) -> std::collections::HashMap<String, String> {
        let mut res = std::collections::HashMap::new();
        let attrs: Vec<&str> = raw_msg.split('/').collect();
        for attr in attrs.iter().take(attrs.len() - 1) {
            let attr = attr.replace("@s", "/");
            let attr = attr.replace("@A", "@");
            let couple: Vec<&str> = attr.split("@=").collect();
            if let Some(key) = couple.get(0) {
                if let Some(value) = couple.get(1) {
                    res.insert(key.to_string(), value.to_string());
                }
            }
        }
        res
    }
    fn dy_decode(&self, msg_byte: &[u8]) -> Vec<String> {
        let mut pos = 0;
        let mut msg = Vec::new();
        while pos < msg_byte.len() {
            let content_length_bytes = &msg_byte[pos..pos + 4];
            let content_length = u32::from_le_bytes([
                content_length_bytes[0],
                content_length_bytes[1],
                content_length_bytes[2],
                content_length_bytes[3],
            ]) as usize;

            let content_bytes = &msg_byte[pos + 12..pos + 3 + content_length];
            let content = String::from_utf8_lossy(content_bytes).to_string();

            msg.push(content);
            pos += 4 + content_length;
        }
        msg
    }

    fn get_chat_messages(&self, msg_byte: &[u8]) -> Vec<HashMap<String, String>> {
        let decode_msg = self.dy_decode(msg_byte);
        let mut messages = Vec::new();
        for msg in decode_msg {
            let res = self.parse_msg(&msg);
            if res.get("type") == Some(&"chatmsg".to_owned()) {
                messages.push(res);
            }
        }
        messages
    }
}

struct DyDanmuCrawler {
    room_id: String,
    heartbeat_timer: Option<std::time::Instant>,
    client: Arc<Mutex<TlsStream<TcpStream>>>,
    msg_handler: DyDanmuMsgHandler,
}

impl DyDanmuCrawler {
    fn new(room_id: String) -> Result<Self, tungstenite::error::Error> {
        let connector = TlsConnector::new().unwrap();
        println!("pre");
        let client = TcpStream::connect("danmuproxy.douyu.com:8506").unwrap();
        let client = Arc::new(Mutex::new(
            connector.connect("danmuproxy.douyu.com", client).unwrap(),
        ));
        client
            .lock()
            .unwrap()
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("Failed to set read timeout");
        println!("connected");
        Ok(DyDanmuCrawler {
            room_id,
            heartbeat_timer: None,
            client,
            msg_handler: DyDanmuMsgHandler,
        })
    }

    fn start(&mut self) {
        self.prepare();
        println!("prepared");
        self.receive_messages();
    }

    // fn stop(&mut self) {
    //     self.client.shutdown().unwrap();
    //     self.keep_heartbeat = false;
    // }

    // fn on_error(&self, err: tungstenite::error::Error) {
    //     println!("{}", err);
    // }

    // fn on_close(&self) {
    //     println!("close");
    // }

    fn join_group(&mut self) {
        println!("join");
        let join_group_msg = format!("type@=joingroup/rid@={}/gid@=1/", self.room_id);
        let msg_bytes = self.msg_handler.dy_encode(&join_group_msg);
        self.client.lock().unwrap().write_all(&msg_bytes).unwrap();
        println!("join:{:?}", msg_bytes);
    }

    fn login(&mut self) {
        println!("login");
        let login_msg = format!(
            "type@=loginreq/roomid@={}/dfl@=sn@AA=105@ASss@AA=1/username@={}/uid@={}/ver@=20190610/aver@=218101901/ct@=0/.",
            self.room_id, "99047358", "99047358"
        );
        let msg_bytes = self.msg_handler.dy_encode(&login_msg);
        self.client.lock().unwrap().write_all(&msg_bytes).unwrap();
        println!("login:{:?}", msg_bytes);
    }

    fn start_heartbeat(&mut self) {
        println!("heartbeat");
        self.heartbeat_timer = Some(std::time::Instant::now());
        self.heartbeat();
    }

    fn heartbeat(&mut self) {
        let heartbeat_msg = "type@=mrkl/";
        let heartbeat_msg_bytes = self.msg_handler.dy_encode(&heartbeat_msg);
        let thread_client = Arc::clone(&self.client);
        thread::spawn(move || loop {
            println!("beat");
            let mut x = thread_client.lock().unwrap();
            println!("beatlock");
            x.write_all(&heartbeat_msg_bytes.clone()).unwrap();
            println!("beat:{:?}", String::from_utf8_lossy(&heartbeat_msg_bytes));
            drop(x);
            thread::sleep(Duration::from_secs(10));
        });
    }

    fn prepare(&mut self) {
        self.login();
        self.join_group();
        self.start_heartbeat();
    }

    fn receive_messages(&mut self) {
        // let (tx, rx) = mpsc::channel();
        // let client_clone = self.client;

        // thread::spawn(move || {
        //     while let Ok(msg) = client_clone.() {
        //         if let Message::Binary(msg_bytes) = msg {
        //             tx.send(msg_bytes).unwrap();
        //         }
        //     }
        // });

        loop {
            println!("recieve");
            let mut buf = vec![];
            let mut x = self.client.lock().unwrap();
            println!("recieve locked");
            let res = x.read_to_end(&mut buf);
            if let Err(error) = res {
                println!("Error: {}", error);
            } else {
                let _ = res.unwrap();
                self.receive_msg(&mut buf);
            }
        }

        // for msg_bytes in rx {
        //     self.receive_msg(&msg_bytes);
        // }
    }

    fn receive_msg(&self, msg_bytes: &[u8]) {
        let chat_messages = self.msg_handler.get_chat_messages(msg_bytes);
        for message in chat_messages {
            if let Some(nn) = message.get("nn") {
                if let Some(txt) = message.get("txt") {
                    println!("{}: {}", nn, txt);
                }
            }
        }
    }
}

fn main() {
    let room_id = "11144156".to_owned();
    let mut dy_barrage_crawler = DyDanmuCrawler::new(room_id).unwrap();
    println!("newed");
    dy_barrage_crawler.start();
}
