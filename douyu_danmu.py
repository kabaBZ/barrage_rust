import websocket
import threading
import time


class DyDanmuMsgHandler:
    # 将字符串数据按照斗鱼协议封装为字节流
    def dy_encode(self, msg):
        # 头部8字节，尾部1字节，与字符串长度相加即数据长度
        # 为什么不加最开头的那个消息长度所占4字节呢？这得问问斗鱼^^
        data_len = len(msg) + 9
        # 字符串转化为字节流
        msg_byte = msg.encode("utf-8")
        # 将数据长度转化为小端整数字节流
        len_byte = int.to_bytes(data_len, 4, "little")
        # 前两个字节按照小端顺序拼接为0x02b1，转化为十进制即689（《协议》中规定的客户端发送消息类型）
        # 后两个字节即《协议》中规定的加密字段与保留字段，置0
        send_byte = bytearray([0xB1, 0x02, 0x00, 0x00])
        # 尾部以'\0'结束
        end_byte = bytearray([0x00])
        # 按顺序拼接在一起
        data = len_byte + len_byte + send_byte + msg_byte + end_byte
        return data

    def __parse_msg(self, raw_msg):
        """
        解析数据
        :param raw_msg: 原始response数据
        :return:
        """
        res = {}
        attrs = raw_msg.split("/")[0:-1]
        for attr in attrs:
            attr = attr.replace("@s", "/")
            attr = attr.replace("@A", "@")
            couple = attr.split("@=")
            res[couple[0]] = couple[1]
        return res

    def dy_decode(self, msg_byte):
        """
        解析斗鱼返回的数据
        :param msg_byte:
        :return:
        """
        pos = 0
        msg = []
        while pos < len(msg_byte):
            content_length = int.from_bytes(msg_byte[pos : pos + 4], byteorder="little")
            content = msg_byte[pos + 12 : pos + 3 + content_length].decode(
                encoding="utf-8", errors="ignore"
            )
            msg.append(content)
            pos += 4 + content_length
        return msg

    def get_chat_messages(self, msg_byte):
        """
        从数据获取chatmsg数据
        :param msg_byte:
        :return:
        """
        decode_msg = self.dy_decode(msg_byte)
        messages = []
        for msg in decode_msg:
            res = self.__parse_msg(msg)
            if res["type"] != "chatmsg":
                continue
            messages.append(res)
        return messages


class DyDanmuCrawler:
    def __init__(self, roomid):
        self.__room_id = roomid
        self.__heartbeat_thread = None
        self.__client = DyDanmuWebSocketClient(
            on_open=self.__prepare, on_message=self.__receive_msg, on_close=self.__stop
        )
        self.__msg_handler = DyDanmuMsgHandler()
        self.__keep_HeartBeat = True

    def start(self):
        """
        开启客户端
        :return:
        """
        self.__client.start()

    def __stop(self, *args):
        """
        登出
        停止客户端
        停止心跳线程
        :return:
        """
        # self.__logout()
        self.__client.stop()
        self.__keep_HeartBeat = False

    def on_error(self, *args, **kwargs):
        print(args[0])

    def on_close(self):
        print("close")

    # 发送入组消息
    def join_group(self):
        """
        发送群组消息
        :return:
        """
        join_group_msg = "type@=joingroup/rid@=%s/gid@=1/" % (self.__room_id)
        msg_bytes = self.__msg_handler.dy_encode(join_group_msg)
        self.__client.send(msg_bytes)

    # 发送登录请求消息
    def login(self):
        """
        登陆
        :return:
        """
        login_msg = (
            "type@=loginreq/roomid@=%s/dfl@=sn@AA=105@ASss@AA=1/"
            "username@=%s/uid@=%s/ver@=20190610/aver@=218101901/ct@=0/."
            % (self.__room_id, "99047358", "99047358")
        )
        msg_bytes = self.__msg_handler.dy_encode(login_msg)
        self.__client.send(msg_bytes)

    def __start_heartbeat(self):
        self.__heartbeat_thread = threading.Thread(target=self.__heartbeat)
        self.__heartbeat_thread.start()

    def __heartbeat(self):
        heartbeat_msg = "type@=mrkl/"
        heartbeat_msg_byte = self.__msg_handler.dy_encode(heartbeat_msg)
        while True:
            self.__client.send(heartbeat_msg_byte)
            for i in range(90):
                time.sleep(0.5)
                if not self.__keep_HeartBeat:
                    return

    def __prepare(self, *args):
        self.login()
        # 登录后发送入组消息
        self.join_group()
        self.__start_heartbeat()

    def __receive_msg(self, *args):
        """
        处理收到的信息
        :param msg:
        :return:
        """
        chat_messages = self.__msg_handler.get_chat_messages(args[1])
        for message in chat_messages:
            print(f"{message['nn']}:{message['txt']}")
        # 将字节流转化为字符串，忽略无法解码的错误（即斗鱼协议中的头部尾部）
        # print(message.decode(encoding='utf-8', errors='ignore'))


class DyDanmuWebSocketClient:
    def __init__(self, on_open, on_message, on_close):
        self.__url = "wss://danmuproxy.douyu.com:8506/"
        self.__websocket = websocket.WebSocketApp(
            self.__url,
            on_open=on_open,
            on_message=on_message,
            on_error=self.__on_error,
            on_close=on_close,
        )

    def start(self):
        self.__websocket.run_forever()

    def stop(self):
        self.__websocket.close()

    def send(self, msg):
        self.__websocket.send(msg)

    def __on_error(self, *args, **kwargs):
        print(args)


roomid = "11144156"
dy_barrage_crawler = DyDanmuCrawler(roomid)
dy_barrage_crawler.start()
