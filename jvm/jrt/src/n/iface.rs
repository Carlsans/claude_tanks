//! Native interfaces have no code of their own.

use crate::*;

// The default methods of `java.net.http.WebSocket.Listener`.

pub fn listener_on_open(_this: Ref, _ws: Ref) -> JResult<()> {
    Ok(())
}

pub fn listener_on_text(_this: Ref, _ws: Ref, _text: Ref, _last: i32) -> JResult<Ref> {
    Ok(None)
}

pub fn listener_on_close(_this: Ref, _ws: Ref, _code: i32, _reason: Ref) -> JResult<Ref> {
    Ok(None)
}

pub fn listener_on_error(_this: Ref, _ws: Ref, _error: Ref) -> JResult<()> {
    Ok(())
}
