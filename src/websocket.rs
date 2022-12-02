use super::utf8;

use actix::Actor;
use actix::ActorContext;
use actix::StreamHandler;
use actix_http::ws::CloseCode;
use actix_http::ws::CloseReason;
use actix_http::ws::Item;
use actix_http::ws::ProtocolError;
use actix_web::web;
use actix_web::Error as WebError;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web_actors::ws;
use actix_web_actors::ws::WebsocketContext;

use actix_web_actors::ws::WsResponseBuilder;
use bytes::BufMut;
use bytes::Bytes;
use bytes::BytesMut;

use bytestring::ByteString;
use utf8::validate_utf8_bytes;
use utf8::ValidUtf8;

enum ContinuationBuffer {
    Text {
        data: Vec<Bytes>,
        overflow: Option<Bytes>,
    },
    Binary(Vec<Bytes>),
    Empty,
}

impl ContinuationBuffer {
    fn is_empty(&self) -> bool {
        match self {
            Self::Text {
                data: _,
                overflow: _,
            } => false,
            Self::Binary(_) => false,
            Self::Empty => true,
        }
    }

    fn buffer_size(&self) -> usize {
        match self {
            Self::Text { data, overflow: _ } => data
                .iter()
                .fold(0, |accumulator, element| accumulator + element.len()),
            Self::Binary(buffer) => buffer
                .iter()
                .fold(0, |accumulator, element| accumulator + element.len()),
            Self::Empty => 0,
        }
    }

    fn append(&mut self, data: Bytes) -> Result<(), ws::ProtocolError> {
        println!("Appending: {:#?}", data);

        match self {
            Self::Binary(buffer) => {
                buffer.push(data);
                Ok(())
            }
            Self::Text {
                data: buffer,
                overflow,
            } => {
                let data = match overflow {
                    Some(overflow) => {
                        let new_data_len = data.len() + overflow.len();
                        let mut new_data = BytesMut::with_capacity(new_data_len);
                        new_data.put(overflow);
                        new_data.put(data);
                        new_data.freeze()
                    }
                    None => data,
                };

                println!("Entry data: {:#?}", data);
                let ValidUtf8 {
                    valid,
                    overflow: message_overflow,
                } = validate_utf8_bytes(data)?;

                let r = ByteString::try_from(valid.clone()).map_err(|e| {
                    println!("Should be a hit");

                    ProtocolError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{}", e),
                    ))
                })?;

                println!("Received: {:#?}", r);

                buffer.push(valid);

                match message_overflow {
                    Some(message_overflow) => {
                        _ = overflow.insert(message_overflow);
                    }
                    None => {}
                }

                Ok(())
            }
            Self::Empty => Err(ws::ProtocolError::ContinuationNotStarted),
        }
    }
}

enum ContinuationMessage {
    Text(ByteString),
    Binary(Bytes),
    Unfinished,
}

struct WebsocketActor {
    continuation_buffer: ContinuationBuffer,
}

impl WebsocketActor {
    fn continuation_handler(
        &mut self,
        item: Item,
    ) -> Result<ContinuationMessage, ws::ProtocolError> {
        println!("Received Continuation: {:#?}", item);
        match item {
            Item::FirstBinary(data) => {
                if self.continuation_buffer.is_empty() {
                    self.continuation_buffer = ContinuationBuffer::Binary(vec![data]);
                    Ok(ContinuationMessage::Unfinished)
                } else {
                    Err(ws::ProtocolError::ContinuationStarted)
                }
            }
            Item::FirstText(data) => {
                if self.continuation_buffer.is_empty() {
                    println!("Entry data: {:#?}", data);
                    let ValidUtf8 { valid, overflow } = validate_utf8_bytes(data)?;

                    ByteString::try_from(valid.clone()).map_err(|e| {
                        ProtocolError::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("{}", e),
                        ))
                    })?;

                    self.continuation_buffer = ContinuationBuffer::Text {
                        data: vec![valid],
                        overflow,
                    };
                    Ok(ContinuationMessage::Unfinished)
                } else {
                    Err(ws::ProtocolError::ContinuationStarted)
                }
            }
            Item::Continue(data) => {
                self.continuation_buffer.append(data)?;
                Ok(ContinuationMessage::Unfinished)
            }
            Item::Last(data) => {
                let size = self.continuation_buffer.buffer_size() + data.len();
                let mut message_data = BytesMut::with_capacity(size);
                match &mut self.continuation_buffer {
                    ContinuationBuffer::Text {
                        data: buffer,
                        overflow,
                    } => {
                        let data = match overflow {
                            Some(overflow) => {
                                let new_data_len = data.len() + overflow.len();
                                let mut new_data = BytesMut::with_capacity(new_data_len);
                                new_data.put(overflow);
                                new_data.put(data);
                                new_data.freeze()
                            }
                            None => data,
                        };

                        println!("Entry data: {:#?}", data);
                        let ValidUtf8 {
                            valid,
                            overflow: message_overflow,
                        } = validate_utf8_bytes(data)?;

                        match message_overflow {
                            Some(bytes) => {
                                return Err(ProtocolError::Io(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    format!(
                                        "invalid utf-8 sequence of {} bytes from index {}",
                                        bytes.len(),
                                        valid.len()
                                    ),
                                )));
                            }
                            _ => {}
                        }

                        ByteString::try_from(valid.clone()).map_err(|e| {
                            ProtocolError::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("{}", e),
                            ))
                        })?;

                        for b in buffer {
                            message_data.put(b);
                        }
                        message_data.put(valid);

                        let text = ByteString::try_from(message_data.freeze()).map_err(|e| {
                            ProtocolError::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("{}", e),
                            ))
                        })?;

                        Ok(ContinuationMessage::Text(text))
                    }
                    ContinuationBuffer::Binary(buffer) => {
                        for b in buffer {
                            message_data.put(b);
                        }
                        message_data.put(data);

                        Ok(ContinuationMessage::Binary(message_data.freeze()))
                    }
                    ContinuationBuffer::Empty => Err(ws::ProtocolError::ContinuationNotStarted),
                }
            }
        }
    }

    fn binary(&mut self, bin: Bytes, ctx: &mut <Self as Actor>::Context) {
        println!("Received Binary: {:#?}", bin);
        ctx.binary(bin);
    }

    fn close(&mut self, reason: Option<CloseReason>, ctx: &mut <Self as Actor>::Context) {
        println!("Received Close\n\treason: {:#?}", reason);
        match reason {
            Some(CloseReason {
                code: CloseCode::Other(code),
                description: _,
            }) => {
                if (3000u16..5000u16).contains(&code) {
                    ctx.close(reason);
                } else {
                    ctx.close(Some(CloseReason::from(CloseCode::Protocol)));
                }
            }
            Some(CloseReason {
                code: CloseCode::Abnormal,
                description: _,
            }) => {
                ctx.close(Some(CloseReason::from(CloseCode::Protocol)));
            }
            reason => ctx.close(reason),
        }

        ctx.stop();
    }

    fn nop(&mut self, _ctx: &mut <Self as Actor>::Context) {
        println!("Received Nop");
    }

    fn ping(&mut self, data: Bytes, ctx: &mut <Self as Actor>::Context) {
        println!("Received Ping");
        ctx.pong(data.as_ref());
    }

    fn pong(&mut self, _data: Bytes, _ctx: &mut <Self as Actor>::Context) {
        println!("Received PONG");
    }

    fn text(&mut self, text: ByteString, ctx: &mut <Self as Actor>::Context) {
        let str = text.to_string();
        println!("Received text: {}", str);
        ctx.text(text);
    }

    fn protocol_error(&mut self, e: ProtocolError, ctx: &mut <Self as Actor>::Context) {
        println!("Error: {:#?}", e);
        ctx.stop();
    }
}

impl Default for WebsocketActor {
    fn default() -> Self {
        Self {
            continuation_buffer: ContinuationBuffer::Empty,
        }
    }
}

impl Actor for WebsocketActor {
    type Context = WebsocketContext<Self>;
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebsocketActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Binary(bin)) => {
                if self.continuation_buffer.is_empty() {
                    self.binary(bin, ctx)
                } else {
                    self.protocol_error(ws::ProtocolError::ContinuationStarted, ctx);
                }
            }
            Ok(ws::Message::Close(reason)) => self.close(reason, ctx),
            Ok(ws::Message::Continuation(item)) => {
                let result = self.continuation_handler(item);

                match result {
                    Err(e) => self.protocol_error(e, ctx),
                    Ok(ContinuationMessage::Binary(bin)) => self.binary(bin, ctx),
                    Ok(ContinuationMessage::Text(text)) => self.text(text, ctx),
                    Ok(ContinuationMessage::Unfinished) => {}
                }
            }
            Ok(ws::Message::Ping(data)) => self.ping(data, ctx),
            Ok(ws::Message::Pong(data)) => self.pong(data, ctx),
            Ok(ws::Message::Text(text)) => {
                if self.continuation_buffer.is_empty() {
                    self.text(text, ctx);
                } else {
                    self.protocol_error(ws::ProtocolError::ContinuationStarted, ctx);
                }
            }
            Ok(ws::Message::Nop) => self.nop(ctx),
            Err(e) => self.protocol_error(e, ctx),
        }
    }
}

pub async fn index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, WebError> {
    WsResponseBuilder::new(
        WebsocketActor::default(), 
        &req, stream
    )
        .frame_size(16_777_216)
        .start()
}