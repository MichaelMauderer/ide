//! This module defines possible mouse events.

use crate::prelude::*;

use crate::control::io::mouse::button::*;


// =============
// === Event ===
// =============

macro_rules! define_events {
    ( $( $js_event:ident :: $name:ident ),* $(,)? ) => {$(
        /// Mouse event wrapper.
        #[derive(Debug,Clone,From,Shrinkwrap)]
        pub struct $name {
            raw: web_sys::$js_event
        }
        impl $name {
            /// Translation of the button property to Rust `Button` enum.
            pub fn button(&self) -> Button {
                Button::from_code(self.raw.button())
            }
        }
    )*};
}

define_events! {
    MouseEvent::OnDown,
    MouseEvent::OnUp,
    MouseEvent::OnMove,
    MouseEvent::OnLeave,
    WheelEvent::OnWheel,
}
