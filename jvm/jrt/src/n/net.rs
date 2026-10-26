//! The slice of `java.net` and `java.net.http` the bot API uses to reach
//! its server: here the socket is in-process (`crate::robot`).
use crate::reflect::{self, Val};
use crate::*;

// java.net.URI: the text in refs[0].

pub fn uri_init(this: Ref, s: Ref) -> JResult<()> {
    *nn(&this)?.refs[0].borrow_mut() = s;
    Ok(())
}

pub fn uri_create(s: Ref) -> JResult<Ref> {
    let u = alloc(&C_java_net_URI);
    uri_init(u.clone(), s)?;
    Ok(u)
}

fn text(this: &Ref) -> JResult<String> {
    Ok(nn(this)?.refs[0]
        .borrow()
        .as_ref()
        .map_or(String::new(), |s| rust_string(s)))
}

pub fn uri_to_string(this: Ref) -> JResult<Ref> {
    Ok(jstr(&text(&this)?))
}

pub fn uri_get_scheme(this: Ref) -> JResult<Ref> {
    Ok(text(&this)?.split_once(':').and_then(|(s, _)| jstr(s)))
}

fn authority(t: &str) -> &str {
    let rest = t.split_once("://").map_or(t, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or("")
}

pub fn uri_get_host(this: Ref) -> JResult<Ref> {
    let t = text(&this)?;
    Ok(jstr(
        authority(&t)
            .rsplit_once(':')
            .map_or(authority(&t), |(h, _)| h),
    ))
}

pub fn uri_get_port(this: Ref) -> JResult<i32> {
    let t = text(&this)?;
    Ok(authority(&t)
        .rsplit_once(':')
        .and_then(|(_, p)| p.parse().ok())
        .unwrap_or(-1))
}

pub fn uri_get_path(this: Ref) -> JResult<Ref> {
    let t = text(&this)?;
    let rest = t.split_once("://").map_or(t.as_str(), |(_, r)| r);
    Ok(jstr(rest.find('/').map_or("", |i| &rest[i..])))
}

pub fn uri_equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok(
        matches!(&o, Some(x) if std::ptr::eq(x.class, &C_java_net_URI)) as i32
            * (text(&this)? == text(&o)?) as i32,
    )
}

pub fn uri_hash_code(this: Ref) -> JResult<i32> {
    let h = text(&this)?
        .encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)));
    Ok(h)
}

// java.net.http: builders that end in the in-process socket.

pub fn http_new_builder() -> JResult<Ref> {
    Ok(alloc(&C_jrt_net_HttpClientBuilder))
}

pub fn http_build(_this: Ref) -> JResult<Ref> {
    Ok(alloc(&C_jrt_net_HttpClientImpl))
}

pub fn http_new_websocket_builder(_this: Ref) -> JResult<Ref> {
    Ok(alloc(&C_jrt_net_WebSocketBuilder))
}

/// Builder methods that configure nothing here: return the builder.
pub fn builder_self(this: Ref, _x: Ref) -> JResult<Ref> {
    Ok(this)
}

/// A finished `CompletableFuture` holding `v` (the translated shim's).
pub fn completed(v: Ref) -> JResult<Ref> {
    let c = reflect::class_named("java/util/concurrent/CompletableFuture").ok_or_else(|| {
        throw(
            &C_java_lang_NoClassDefFoundError,
            "java/util/concurrent/CompletableFuture",
        )
    })?;
    Ok(reflect::call(
        c,
        "completedFuture(Ljava/lang/Object;)Ljava/util/concurrent/CompletableFuture;",
        &[Val::A(v)],
    )?
    .a())
}

/// `WebSocket.Builder.buildAsync(URI, Listener)`: connects the listener to
/// the in-process socket.
pub fn ws_build_async(_this: Ref, _uri: Ref, listener: Ref) -> JResult<Ref> {
    let socket = alloc(&C_jrt_net_InProcessSocket);
    crate::robot::attach(listener.clone(), socket.clone());
    let l = nn(&listener)?;
    let on_open: fn(Ref, Ref) -> JResult<()> =
        unsafe { std::mem::transmute(islot(l, &C_java_net_http_WebSocket_Listener, 0)?) };
    on_open(listener.clone(), socket.clone())?;
    completed(socket)
}

pub fn ws_send_text(this: Ref, text: Ref, _last: i32) -> JResult<Ref> {
    let s = String::from_utf16_lossy(&n::string::seq_chars(&text)?);
    crate::robot::send(s);
    completed(this)
}

pub fn ws_send_close(this: Ref, _code: i32, _reason: Ref) -> JResult<Ref> {
    crate::robot::close();
    completed(this)
}

pub fn ws_request(_this: Ref, _n: i64) -> JResult<()> {
    Ok(())
}

pub fn ws_abort(_this: Ref) -> JResult<()> {
    crate::robot::close();
    Ok(())
}

pub fn ws_closed(_this: Ref) -> JResult<i32> {
    Ok(crate::robot::is_closed() as i32)
}

pub fn ws_subprotocol(_this: Ref) -> JResult<Ref> {
    Ok(jstr(""))
}

pub fn getenv(_name: Ref) -> JResult<Ref> {
    Ok(None)
}
