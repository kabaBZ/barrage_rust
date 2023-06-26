// package main

// import "github.com/kabaBZ/Sakula_Go/src/utils"

// func main() {
// 	utils.Struct()
// 	utils.Get()
// 	utils.Post()
// }

package main

import (
	"crypto/tls"
	"encoding/binary"
	"fmt"
	"log"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/gorilla/websocket"
)

type DyDanmuMsgHandler struct{}

func (h *DyDanmuMsgHandler) dyEncode(msg string) []byte {
	dataLen := len(msg) + 9
	lenByte := make([]byte, 4)
	msgByte := []byte(msg)
	sendByte := []byte{0xB1, 0x02, 0x00, 0x00}
	endByte := []byte{0x00}

	binary.LittleEndian.PutUint32(lenByte, uint32(dataLen))

	data := append(lenByte, lenByte...)
	data = append(data, sendByte...)
	data = append(data, msgByte...)
	data = append(data, endByte...)

	return data
}

func (h *DyDanmuMsgHandler) parseMsg(rawMsg string) map[string]string {
	res := make(map[string]string)
	attrs := strings.Split(rawMsg, "/")[0 : len(strings.Split(rawMsg, "/"))-1]
	for _, attr := range attrs {
		attr = strings.ReplaceAll(attr, "@s", "/")
		attr = strings.ReplaceAll(attr, "@A", "@")
		couple := strings.Split(attr, "@=")
		res[couple[0]] = couple[1]
	}
	return res
}

func (h *DyDanmuMsgHandler) dyDecode(msgByte []byte) []string {
	pos := 0
	msg := make([]string, 0)
	for pos < len(msgByte) {
		contentLength := int(binary.LittleEndian.Uint32(msgByte[pos : pos+4]))
		content := string(msgByte[pos+12 : pos+3+contentLength])
		msg = append(msg, content)
		pos += 4 + contentLength
	}
	return msg
}

func (h *DyDanmuMsgHandler) getChatMessages(msgByte []byte) []map[string]string {
	decodeMsg := h.dyDecode(msgByte)
	messages := make([]map[string]string, 0)
	for _, msg := range decodeMsg {
		res := h.parseMsg(msg)
		if res["type"] != "chatmsg" {
			continue
		}
		messages = append(messages, res)
	}
	return messages
}

type DyDanmuCrawler struct {
	roomID         string
	heartbeatTimer *time.Timer
	client         *DyDanmuWebSocketClient
	msgHandler     *DyDanmuMsgHandler
	keepHeartbeat  bool
}

func NewDyDanmuCrawler(roomID string) *DyDanmuCrawler {
	client := NewDyDanmuWebSocketClient()
	msgHandler := &DyDanmuMsgHandler{}
	fmt.Println("start")
	DyDanmuCrawler := &DyDanmuCrawler{
		roomID:         roomID,
		heartbeatTimer: nil,
		client:         client,
		msgHandler:     msgHandler,
		keepHeartbeat:  true,
	}
	fmt.Println("finish")
	return DyDanmuCrawler
}

func (c *DyDanmuCrawler) Start() {
	c.client.Start()
}

func (c *DyDanmuCrawler) Stop() {
	c.client.Stop()
	c.keepHeartbeat = false
}

func (c *DyDanmuCrawler) onError(err error) {
	fmt.Println(err)
}

func (c *DyDanmuCrawler) onClose() {
	fmt.Println("close")
}

func (c *DyDanmuCrawler) joinGroup() {
	joinGroupMsg := fmt.Sprintf("type@=joingroup/rid@=%s/gid@=1/", c.roomID)
	msgBytes := c.msgHandler.dyEncode(joinGroupMsg)
	c.client.Send(msgBytes)
}

func (c *DyDanmuCrawler) login() {
	loginMsg := fmt.Sprintf("type@=loginreq/roomid@=%s/dfl@=sn@AA=105@ASss@AA=1/username@=%s/uid@=%s/ver@=20190610/aver@=218101901/ct@=0/.",
		c.roomID, "99047358", "99047358")
	msgBytes := c.msgHandler.dyEncode(loginMsg)
	c.client.Send(msgBytes)
}

func (c *DyDanmuCrawler) startHeartbeat() {
	c.heartbeatTimer = time.NewTimer(0)
	go c.heartbeat()
}

func (c *DyDanmuCrawler) heartbeat() {
	heartbeatMsg := "type@=mrkl/"
	heartbeatMsgBytes := c.msgHandler.dyEncode(heartbeatMsg)
	for {
		select {
		case <-c.heartbeatTimer.C:
			c.client.Send(heartbeatMsgBytes)
			c.heartbeatTimer.Reset(30 * time.Second)
		default:
			time.Sleep(100 * time.Millisecond)
		}
		if !c.keepHeartbeat {
			return
		}
	}
}

func (c *DyDanmuCrawler) prepare() {
	c.login()
	c.joinGroup()
	c.startHeartbeat()
}

func (c *DyDanmuCrawler) receiveMsg(msg []byte) {
	chatMessages := c.msgHandler.getChatMessages(msg)
	for _, message := range chatMessages {
		fmt.Printf("%s: %s\n", message["nn"], message["txt"])
	}
}

type DyDanmuWebSocketClient struct {
	url       string
	websocket *websocket.Conn
	onOpen    func()
	onMessage func([]byte)
	onClose   func()
}

func NewDyDanmuWebSocketClient() *DyDanmuWebSocketClient {
	url := "wss://danmuproxy.douyu.com:8506/"
	return &DyDanmuWebSocketClient{
		url:       url,
		websocket: nil,
		onOpen:    nil,
		onMessage: nil,
		onClose:   nil,
	}
}

func (c *DyDanmuWebSocketClient) Start() {
	u, err := url.Parse(c.url)
	if err != nil {
		log.Fatal(err)
	}

	header := http.Header{}
	header.Add("Origin", "https://www.douyu.com")

	websocket.DefaultDialer.TLSClientConfig = &tls.Config{InsecureSkipVerify: true}
	conn, _, err := websocket.DefaultDialer.Dial(u.String(), header)
	if err != nil {
		log.Fatal(err)
	}
	c.websocket = conn

	if c.onOpen != nil {
		c.onOpen()
	}

	for {
		_, msg, err := c.websocket.ReadMessage()
		if err != nil {
			if c.onClose != nil {
				c.onClose()
			}
			return
		}
		if c.onMessage != nil {
			c.onMessage(msg)
		}
	}
}

func (c *DyDanmuWebSocketClient) Stop() {
	c.websocket.Close()
}

func (c *DyDanmuWebSocketClient) Send(msg []byte) {
	c.websocket.WriteMessage(websocket.BinaryMessage, msg)
}

func main() {
	roomID := "11144156"
	dyBarrageCrawler := NewDyDanmuCrawler(roomID)
	dyBarrageCrawler.Start()
}
