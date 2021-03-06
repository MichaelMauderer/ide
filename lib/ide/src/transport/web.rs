//! web_sys::WebSocket-based `Transport` implementation.

use crate::prelude::*;

use basegl_system_web::closure::storage::OptionalFmMutClosure;
use basegl_system_web::js_to_string;
use failure::Error;
use futures::channel::mpsc;
use json_rpc::Transport;
use json_rpc::TransportEvent;
use utils::channel;
use web_sys::CloseEvent;
use web_sys::Event;
use web_sys::MessageEvent;



// ==============
// === Errors ===
// ==============

/// Errors that may happen when trying to establish WebSocket connection.
#[derive(Clone,Debug,Fail)]
pub enum ConnectingError {
    /// Failed to construct websocket. Usually this happens due to bad URL.
    #[fail(display = "Invalid websocket specification: {}.", _0)]
    ConstructionError(String),
    /// Failed to establish connection. Usually due to connectivity issues,
    /// wrong URL or server being down. Unfortunately, while the real error
    /// cause is usually logged down in js console, we have no reliable means of
    /// obtaining it programmatically. Reported error codes are utterly
    /// unreliable.
    #[fail(display = "Failed to establish connection.")]
    FailedToConnect,
}

/// Error that may occur when attempting to send the data over WebSocket
/// transport.
#[derive(Clone,Debug,Fail)]
enum SendingError {
    /// Calling `send` method has resulted in an JS exception.
    #[fail(display = "Failed to send message. Exception: {:?}.", _0)]
    FailedToSend(String),
    /// The socket was already closed, even before attempting sending a message.
    #[fail(display = "Failed to send message because socket state is {:?}.", _0)]
    NotOpen(State),
}



// =============
// === State ===
// =============

/// Describes the current state of WebSocket connection.
#[derive(Clone,Copy,Debug,PartialEq)]
pub enum State {
    /// Socket has been created. The connection is not yet open.
    Connecting,
    /// The connection is open and ready to communicate.
    Open,
    /// The connection is in the process of closing.
    Closing,
    /// The connection is closed or couldn't be opened.
    Closed,
    /// Any other, unknown condition.
    Unknown(u16),
}

impl State {
    /// Returns current state of the given WebSocket.
    pub fn query_ws(ws:&web_sys::WebSocket) -> State {
        State::from_code(ws.ready_state())
    }

    /// Translates code returned by `WebSocket.readyState` into our enum.
    /// cf https://developer.mozilla.org/en-US/docs/Web/API/WebSocket/readyState
    pub fn from_code(code:u16) -> State {
        match code {
            web_sys::WebSocket::CONNECTING => State::Connecting,
            web_sys::WebSocket::OPEN       => State::Open,
            web_sys::WebSocket::CLOSING    => State::Closing,
            web_sys::WebSocket::CLOSED     => State::Closed,
            num                            => State::Unknown(num), // impossible
        }
    }
}



// =================
// === WebSocket ===
// =================

/// Wrapper over JS `WebSocket` object and callbacks to its signals.
#[derive(Debug)]
pub struct WebSocket {
    /// Handle to the JS `WebSocket` object.
    pub ws         : web_sys::WebSocket,
    /// Handle to a closure connected to `WebSocket.onmessage`.
    pub on_message : OptionalFmMutClosure<MessageEvent>,
    /// Handle to a closure connected to `WebSocket.onclose`.
    pub on_close   : OptionalFmMutClosure<CloseEvent>,
    /// Handle to a closure connected to `WebSocket.onopen`.
    pub on_open    : OptionalFmMutClosure<Event>,
    /// Handle to a closure connected to `WebSocket.onerror`.
    pub on_error   : OptionalFmMutClosure<Event>,
}

impl WebSocket {
    /// Wraps given WebSocket object.
    pub fn new(ws:web_sys::WebSocket) -> WebSocket {
        WebSocket {
            ws,
            on_message : default(),
            on_close   : default(),
            on_open    : default(),
            on_error   : default(),
        }
    }

    /// Establish connection with endpoint defined by the given URL and wrap it.
    /// Asynchronous, because it waits until connection is established.
    pub async fn new_opened(url:impl Str) -> Result<WebSocket,ConnectingError> {
        let     ws  = web_sys::WebSocket::new(url.as_ref());
        let mut wst = WebSocket::new(ws.map_err(|e| {
            ConnectingError::ConstructionError(js_to_string(e))
        })?);

        wst.wait_until_open().await?;
        Ok(wst)
    }

    /// Awaits until `open` signal has been emitted. Clears any callbacks on
    /// this `WebSocket`, if any has been set.
    async fn wait_until_open(&mut self) -> Result<(),ConnectingError> {
        // Connecting attempt shall either emit on_open or on_close.
        // We shall wait for whatever comes first.
        let (transmitter, mut receiver) = mpsc::unbounded::<Result<(),()>>();
        let transmitter_clone = transmitter.clone();
        self.set_on_close(move |_| {
            // Note [mwu] Ignore argument, `CloseEvent` here contains rubbish
            // anyway, nothing useful to pass to caller. Error code or reason
            // string should not be relied upon.
            utils::channel::emit(&transmitter_clone, Err(()));
        });
        self.set_on_open(move |_| {
            utils::channel::emit(&transmitter, Ok(()));
        });

        match receiver.next().await {
            Some(Ok(())) => {
                self.clear_callbacks();
                Ok(())
            }
            _ => Err(ConnectingError::FailedToConnect)
        }
    }

    /// Checks the current state of the connection.
    pub fn state(&self) -> State {
        State::query_ws(&self.ws)
    }

    /// Sets callback for the `close` event.
    pub fn set_on_close(&mut self, f:impl FnMut(CloseEvent) + 'static) {
        self.on_close.wrap(f);
        self.ws.set_onclose(self.on_close.js_ref());
    }

    /// Sets callback for the `error` event.
    pub fn set_on_error(&mut self, f:impl FnMut(Event) + 'static) {
        self.on_error.wrap(f);
        self.ws.set_onerror(self.on_error.js_ref());
    }

    /// Sets callback for the `message` event.
    pub fn set_on_message(&mut self, f:impl FnMut(MessageEvent) + 'static) {
        self.on_message.wrap(f);
        self.ws.set_onmessage(self.on_message.js_ref());
    }

    /// Sets callback for the `open` event.
    pub fn set_on_open(&mut self, f:impl FnMut(Event) + 'static) {
        self.on_open.wrap(f);
        self.ws.set_onopen(self.on_open.js_ref());
    }

    /// Clears all the available callbacks.
    pub fn clear_callbacks(&mut self) {
        self.on_close  .clear();
        self.on_error  .clear();
        self.on_message.clear();
        self.on_open   .clear();
        self.ws.set_onclose(None);
        self.ws.set_onerror(None);
        self.ws.set_onmessage(None);
        self.ws.set_onopen(None);
    }
}

impl Transport for WebSocket {
    fn send_text(&mut self, message:String) -> Result<(), Error> {
        // Sending through the closed WebSocket can return Ok() with error only
        // appearing in the log. We explicitly check for this to get failure as
        // early as possible.
        //
        // If WebSocket closes after the check, caller will be able to handle it
        // when receiving `TransportEvent::Closed`.
        let state = self.state();
        if state != State::Open {
            Err(SendingError::NotOpen(state).into())
        } else {
            self.ws.send_with_str(&message).map_err(|e| {
                SendingError::FailedToSend(js_to_string(e)).into()
            })
        }
    }

    fn set_event_transmitter(&mut self, transmitter:mpsc::UnboundedSender<TransportEvent>) {
        let transmitter_copy = transmitter.clone();
        self.set_on_message(move |e| {
            let data = e.data();
            if let Some(text) = data.as_string() {
                channel::emit(&transmitter_copy,TransportEvent::TextMessage(text));
            }
        });

        let transmitter_copy = transmitter.clone();
        self.set_on_close(move |_e| {
            channel::emit(&transmitter_copy,TransportEvent::Closed);
        });

        self.set_on_open(move |_e| {
            channel::emit(&transmitter, TransportEvent::Opened);
        });
    }
}
