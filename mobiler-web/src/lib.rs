//! `mobiler-web` — Mobiler's web shell.
//!
//! Renders **any** Mobiler app's `Widget` tree to the DOM (Leptos / WASM), driving
//! the Rust core via crux's `Core` and fulfilling capabilities (HTTP) with the
//! browser's `fetch`. The web twin of the generic Android/SwiftUI shells: write
//! your app once as a `MobilerApp`, then
//!
//! ```ignore
//! fn main() { mobiler_web::run::<my_app::App>(); }
//! ```
//!
//! renders it on the web — fully styled, no CSS required: the shell ships its own
//! theme (`mobiler.css`) and injects it on mount, and `Scaffold.dark_mode` flips
//! the whole theme. Your crate only supplies a minimal `index.html` with the Trunk
//! entry point; an app may add its own stylesheet to override any widget class.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crux_core::{App, Core, Request};
use leptos::prelude::*;
use mobiler_core::{
    A11yRole, Action, BoxAlign, ButtonStyle, CardStyle, ChartBracket, ChartLegendItem, ChartRefLine, ChartRegion,
    ChartSeries, ChartStyle, ChartTick, Corner, Density, Effect, FieldKind, FontFamily, HttpHeader, HttpOutcome, Icon,
    ImageRatio, ImageShape, InputValue, PluginCall, PluginNotify, PluginResponse, PluginStreamCall, ProjectColor,
    Rgb, Spacing, TextStyle, Theme, Tone, TransferEvent, Widget,
};
use wasm_bindgen_futures::spawn_local;

/// The shell's own stylesheet — the web twin of the look the Android/SwiftUI shells
/// decide in code. Shipped with the crate and injected on mount, so `run::<App>()`
/// renders a fully styled, themeable app with no CSS required from the consuming
/// app (it can still override any class). Uses CSS variables so `Scaffold.dark_mode`
/// flips the whole theme by toggling one class.
const STYLE: &str = include_str!("mobiler.css");

/// Cloneable handle for sending an `Action` into the core. Leptos 0.7 view closures
/// require `Send`, so this is `Arc` + `Send + Sync` (the crux `Core` is both).
type Dispatch = Arc<dyn Fn(Action) + Send + Sync>;

/// What a Mobiler app must be to render on the web: a crux `App` speaking the fixed
/// ABI (`Action` in, `Widget` out, `Effect` for capabilities). `MobilerShell<_>`
/// satisfies this automatically.
pub trait WebApp:
    App<Event = Action, ViewModel = Widget, Effect = Effect> + Default + Send + Sync + 'static
where
    Self::Model: Default + Send + Sync,
{
}
impl<T> WebApp for T
where
    T: App<Event = Action, ViewModel = Widget, Effect = Effect> + Default + Send + Sync + 'static,
    T::Model: Default + Send + Sync,
{
}

/// Mount a Mobiler app into the document body. Call from your wasm `main`.
pub fn run<A: WebApp>()
where
    A::Model: Default + Send + Sync,
{
    console_error_panic_hook::set_once();
    inject_default_style();
    inject_hls_support();
    inject_maplibre_support();
    leptos::mount::mount_to_body(shell::<A>);
}

/// hls.js bootstrap for HLS (`.m3u8`) playback in browsers without native HLS
/// (Chrome/Firefox — Safari/iOS play HLS natively). A `<video>` whose source is an
/// `.m3u8` is rendered with `data-hls-src` and no `src`; this self-contained script
/// watches the DOM (a `MutationObserver`, so it also catches elements re-rendered on
/// each `update`) and, for each such `<video>`, either sets `src` directly (native
/// HLS, e.g. Safari) or lazily loads hls.js from a CDN and attaches it. If the CDN
/// fails it falls back to a plain `src`. Inert until an `.m3u8` `Video` appears, so
/// MP4/Bunny content (and non-video apps) pay nothing. Bunny content keeps using its
/// own player via `WebView`; this is for raw non-Bunny `.m3u8` on Chrome/Firefox.
fn inject_hls_support() {
    const BOOTSTRAP: &str = r#"(function(){
  function ensureHls(cb){
    if(window.Hls){return cb();}
    if(window.__mobilerHlsLoading){(window.__mobilerHlsCbs=window.__mobilerHlsCbs||[]).push(cb);return;}
    window.__mobilerHlsLoading=true;window.__mobilerHlsCbs=[cb];
    var s=document.createElement('script');
    s.src='https://cdn.jsdelivr.net/npm/hls.js@1';
    var flush=function(){var cbs=window.__mobilerHlsCbs||[];window.__mobilerHlsCbs=[];cbs.forEach(function(f){f();});};
    s.onload=flush;s.onerror=flush;
    document.head.appendChild(s);
  }
  function attach(v){
    if(v.__mobilerHlsDone){return;}v.__mobilerHlsDone=true;
    var url=v.getAttribute('data-hls-src');if(!url){return;}
    if(v.canPlayType('application/vnd.apple.mpegurl')){v.src=url;return;}
    ensureHls(function(){
      if(window.Hls&&window.Hls.isSupported()){var h=new window.Hls();h.loadSource(url);h.attachMedia(v);v.__mobilerHls=h;}
      else{v.src=url;}
    });
  }
  function scan(root){if(root&&root.querySelectorAll){root.querySelectorAll('video[data-hls-src]').forEach(attach);}}
  new MutationObserver(function(muts){muts.forEach(function(m){m.addedNodes.forEach(function(n){if(n.nodeType===1){if(n.matches&&n.matches('video[data-hls-src]')){attach(n);}scan(n);}});});}).observe(document.documentElement,{childList:true,subtree:true});
  scan(document);
})();"#;
    let document = leptos::prelude::document();
    let Some(head) = document.head() else { return };
    let Ok(script) = document.create_element("script") else { return };
    script.set_text_content(Some(BOOTSTRAP));
    let _ = head.append_child(&script);
}

/// MapLibre-GL bootstrap for [`Widget::Map`]. A self-contained script (mirrors `inject_hls_support`):
/// lazily loads maplibre-gl (JS + CSS) from a CDN the first time a `.mobiler-map` div appears, then for
/// each one inits a `maplibregl.Map` from its `data-*` attributes (center/zoom/style/markers/interactive)
/// and wires taps. The Rust render arm re-creates the map div on every `update`, so a `MutationObserver`
/// also REMOVES the map (`.remove()`) when its node is dropped — no leaked WebGL contexts. Map/marker
/// taps are reported to the core by writing `"tap|lat,lng"` / `"marker|id"` into the hidden sibling
/// `.mobiler-map-sink` input and firing its `input` event, which the render arm's `on:input` forwards as
/// `Action::Input`. Inert (and the CDN is never fetched) until a `Map` widget appears.
fn inject_maplibre_support() {
    const BOOTSTRAP: &str = r#"(function(){
  function ensureML(cb){
    if(window.maplibregl){return cb();}
    if(window.__mobilerMlLoading){(window.__mobilerMlCbs=window.__mobilerMlCbs||[]).push(cb);return;}
    window.__mobilerMlLoading=true;window.__mobilerMlCbs=[cb];
    var l=document.createElement('link');l.rel='stylesheet';l.href='https://cdn.jsdelivr.net/npm/maplibre-gl@4/dist/maplibre-gl.css';document.head.appendChild(l);
    var s=document.createElement('script');s.src='https://cdn.jsdelivr.net/npm/maplibre-gl@4/dist/maplibre-gl.js';
    var flush=function(){var cbs=window.__mobilerMlCbs||[];window.__mobilerMlCbs=[];cbs.forEach(function(f){f();});};
    s.onload=flush;s.onerror=flush;document.head.appendChild(s);
  }
  function emit(el,payload){
    var sink=el.parentElement&&el.parentElement.querySelector('.mobiler-map-sink');
    if(sink){sink.value=payload;sink.dispatchEvent(new Event('input',{bubbles:true}));}
  }
  function init(el){
    if(el.__mobilerMap){return;}el.__mobilerMap=true;
    ensureML(function(){
      try{
        var c=(el.getAttribute('data-center')||'0,0').split(',');
        var center=[parseFloat(c[1])||0,parseFloat(c[0])||0];
        var zoom=parseFloat(el.getAttribute('data-zoom'))||2;
        var style=el.getAttribute('data-style')||'https://tiles.openfreemap.org/styles/liberty';
        var interactive=el.getAttribute('data-interactive')!=='false';
        var map=new maplibregl.Map({container:el,style:style,center:center,zoom:zoom,interactive:interactive});
        el.__mobilerMapInstance=map;
        map.on('click',function(e){emit(el,'tap|'+e.lngLat.lat.toFixed(6)+','+e.lngLat.lng.toFixed(6));});
        var markers=[];try{markers=JSON.parse(el.getAttribute('data-markers')||'[]');}catch(_){}
        markers.forEach(function(mk){
          var m=new maplibregl.Marker().setLngLat([mk.lng,mk.lat]);
          if(mk.title){m.setPopup(new maplibregl.Popup({offset:24}).setText(mk.title));}
          m.addTo(map);
          m.getElement().addEventListener('click',function(ev){ev.stopPropagation();emit(el,'marker|'+mk.id);});
        });
      }catch(_){}
    });
  }
  function scan(root){if(root&&root.querySelectorAll){root.querySelectorAll('.mobiler-map[data-map]').forEach(init);}}
  new MutationObserver(function(muts){muts.forEach(function(m){
    m.addedNodes.forEach(function(n){if(n.nodeType===1){if(n.matches&&n.matches('.mobiler-map[data-map]')){init(n);}scan(n);}});
    m.removedNodes.forEach(function(n){if(n.nodeType===1){
      if(n.__mobilerMapInstance){try{n.__mobilerMapInstance.remove();}catch(_){}}
      if(n.querySelectorAll){n.querySelectorAll('.mobiler-map').forEach(function(x){if(x.__mobilerMapInstance){try{x.__mobilerMapInstance.remove();}catch(_){}}});}
    }});
  });}).observe(document.documentElement,{childList:true,subtree:true});
  scan(document);
})();"#;
    let document = leptos::prelude::document();
    let Some(head) = document.head() else { return };
    let Ok(script) = document.create_element("script") else { return };
    script.set_text_content(Some(BOOTSTRAP));
    let _ = head.append_child(&script);
}

/// Inject the shell's default stylesheet at the **front** of `<head>` so it's the
/// lowest-precedence baseline: an app that ships its own CSS (later in the document)
/// overrides any of these classes, while an app with no CSS still gets a full theme.
fn inject_default_style() {
    let document = leptos::prelude::document();
    let Some(head) = document.head() else { return };
    let Ok(style) = document.create_element("style") else { return };
    let _ = style.set_attribute("data-mobiler", "shell");
    style.set_text_content(Some(STYLE));
    let _ = head.insert_before(&style, head.first_child().as_ref());
}

fn shell<A: WebApp>() -> impl IntoView
where
    A::Model: Default + Send + Sync,
{
    let core = Arc::new(Core::<A>::new());
    let (view, set_view) = signal(core.view());

    let send: Dispatch = {
        let core = core.clone();
        Arc::new(move |action: Action| {
            let effects = core.process_event(action);
            drive(&core, set_view, effects);
        })
    };

    // Restore persisted state (localStorage), then fire Start — mirrors the native
    // shells (which restore before Start so the app sees its saved Model on launch).
    let saved = local_storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten()).unwrap_or_default();
    if !saved.is_empty() {
        send(Action::Restore { data: saved });
    }
    send(Action::Start);

    let send_for_view = send.clone();
    view! {
        <div class="app">
            {move || render(&view.get(), &send_for_view)}
        </div>
    }
}

/// Process effects: re-read the view on Render; fulfil HTTP via fetch and resolve.
fn drive<A: WebApp>(core: &Arc<Core<A>>, set_view: WriteSignal<Widget>, effects: Vec<Effect>)
where
    A::Model: Default + Send + Sync,
{
    for effect in effects {
        match effect {
            Effect::Render(_) => set_view.set(core.view()),
            Effect::PluginNotify(notify) => perform_notify(&notify.operation),
            Effect::Plugin(mut request) => {
                let core = core.clone();
                spawn_local(async move {
                    let response = perform(&request.operation).await;
                    if let Ok(next) = core.resolve(&mut request, response) {
                        drive(&core, set_view, next);
                    }
                });
            }
            // Long-lived subscription: start a native source that resolves the same
            // request repeatedly (one event per `core.resolve`). See `start_stream`.
            Effect::PluginStream(request) => start_stream(core, set_view, request),
        }
    }
}

/// Start a streaming subscription ([`Effect::PluginStream`]): begin a native source
/// that resolves `request` **repeatedly** (a [`PluginResponse`] per event), each
/// resolution re-entering the core. The source handle is parked in a per-key
/// registry so [`unsubscribe`](mobiler_core::Cx::unsubscribe) can stop it.
///
/// Web sources: `ticker`/`start` (a `setInterval` emitting an incrementing counter
/// every `input` ms — the deterministic demonstrator) and `websocket`/`stream`
/// (a `WebSocket`, a frame per `onmessage`).
fn start_stream<A: WebApp>(
    core: &Arc<Core<A>>,
    set_view: WriteSignal<Widget>,
    request: Request<PluginStreamCall>,
) where
    A::Model: Default + Send + Sync,
{
    use wasm_bindgen::{closure::Closure, JsCast};

    let call = request.operation.clone();

    // Each resolution of a `resolves_many_times` request yields the next stream item;
    // share the request across event closures via Rc<RefCell<_>>.
    let request = Rc::new(RefCell::new(request));
    let core = core.clone();
    let emit = move |resp: PluginResponse| {
        if let Ok(next) = core.resolve(&mut *request.borrow_mut(), resp) {
            drive(&core, set_view, next);
        }
    };

    let handle = match (call.plugin.as_str(), call.op.as_str()) {
        // Built-in deterministic demonstrator: emit an incrementing counter every
        // `input` ms. Dropping the Interval (on unsubscribe) stops it.
        ("ticker", "start") => {
            let ms: u32 = call.input.parse().unwrap_or(1000);
            let count = std::cell::Cell::new(0u32);
            let interval = gloo_timers::callback::Interval::new(ms, move || {
                count.set(count.get() + 1);
                emit(PluginResponse::text(true, count.get().to_string()));
            });
            StreamHandle::Ticker { _interval: interval }
        }
        ("websocket", "stream") => {
            let Ok(ws) = web_sys::WebSocket::new(&call.input) else { return };
            let onmessage = {
                let emit = emit.clone();
                Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
                    emit(PluginResponse::text(true, e.data().as_string().unwrap_or_default()));
                })
            };
            let onclose = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |_e| {
                emit(PluginResponse::text(false, "closed"));
            });
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
            StreamHandle::Ws(WsStream { ws, _onmessage: onmessage, _onclose: onclose })
        }
        // Built-in `system` source: deep-link URLs + app lifecycle. On the web a "deep link" is the
        // current URL (delivered on subscribe + on `popstate`) and "lifecycle" maps to page
        // visibility (`visibilitychange`). Listeners are dropped (removed) on unsubscribe.
        ("system", "events") => {
            let win = web_sys::window().expect("window");
            let doc = win.document().expect("document");
            // Initial: the current URL as a deeplink + current visibility as lifecycle.
            if let Ok(href) = win.location().href() {
                emit(PluginResponse::text(true, system_deeplink(&href)));
            }
            emit(PluginResponse::text(true, system_lifecycle(&doc)));
            let onpop = {
                let (emit, win) = (emit.clone(), win.clone());
                Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                    if let Ok(href) = win.location().href() {
                        emit(PluginResponse::text(true, system_deeplink(&href)));
                    }
                })
            };
            let onvis = {
                let (emit, doc) = (emit.clone(), doc.clone());
                Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                    emit(PluginResponse::text(true, system_lifecycle(&doc)));
                })
            };
            let _ = win.add_event_listener_with_callback("popstate", onpop.as_ref().unchecked_ref());
            let _ = doc.add_event_listener_with_callback("visibilitychange", onvis.as_ref().unchecked_ref());
            StreamHandle::System(SystemStream { win, doc, _onpop: onpop, _onvis: onvis })
        }
        // Streaming file transfer (`cx.upload` / `cx.download`, Release B). See
        // `start_web_upload` / `start_web_download` for the WEB ASYMMETRY: upload uses
        // XHR (the only web API with upload-progress events), download uses fetch +
        // ReadableStream (progress) and hands the app back a `blob:` handle.
        ("transfer", op @ ("upload" | "download")) => {
            let v: serde_json::Value = serde_json::from_str(&call.input).unwrap_or(serde_json::Value::Null);
            let url = v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let headers: Vec<(String, String)> = v
                .get("headers")
                .and_then(|x| x.as_array())
                .map(|hs| {
                    hs.iter()
                        .filter_map(|h| Some((h.get("name")?.as_str()?.to_string(), h.get("value")?.as_str()?.to_string())))
                        .collect()
                })
                .unwrap_or_default();

            if op == "upload" {
                let method = v.get("method").and_then(|x| x.as_str()).unwrap_or("PUT").to_string();
                let source = v.get("source").and_then(|x| x.as_str()).unwrap_or("").to_string();
                start_web_upload(url, method, headers, source, emit.clone())
            } else {
                start_web_download(url, headers, emit.clone())
            }
        }
        _ => return, // unknown / native-only source — ignore on web
    };

    STREAMS.with(|m| {
        m.borrow_mut().insert(call.key.clone(), handle);
    });
}

/// Monotonic milliseconds, for the ~10/sec progress throttle (`performance.now()`).
fn js_now() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}

/// Start a web upload via `XMLHttpRequest`.
///
/// DELIBERATE WEB ASYMMETRY (see `start_web_download` for the other half): upload uses
/// XHR because it is the *only* web API that reports upload progress
/// (`xhr.upload().onprogress`) — `fetch()` has no upload-progress signal at all. Do not
/// "unify" this with fetch; there is no fetch-based way to get upload progress in a
/// browser today.
fn start_web_upload(
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    source: String,
    emit: impl Fn(PluginResponse) + Clone + 'static,
) -> StreamHandle {
    use wasm_bindgen::{closure::Closure, JsCast};
    let xhr = web_sys::XmlHttpRequest::new().expect("xhr");
    let _ = xhr.open_with_async(&method, &url, true);
    for (n, val) in &headers {
        let _ = xhr.set_request_header(n, val);
    }

    // ~10/sec progress throttling, purely on elapsed time. (Gating on `loaded < total`
    // as well would be a no-op when `!length_computable`, since `total()` is then 0 and
    // `loaded() < 0` is always false.) The terminal Done is emitted by the
    // separate onload/onerror/onabort closures below, unthrottled, so completion is always
    // seen regardless of this gate.
    let last = std::rc::Rc::new(std::cell::Cell::new(0.0f64));
    let on_prog = {
        let (emit, last) = (emit.clone(), last.clone());
        Closure::<dyn FnMut(web_sys::ProgressEvent)>::new(move |e: web_sys::ProgressEvent| {
            let now = js_now();
            if now - last.get() < 100.0 {
                return;
            }
            last.set(now);
            let total = if e.length_computable() { Some(e.total() as u64) } else { None };
            emit(transfer_response(&TransferEvent::Progress { transferred: e.loaded() as u64, total }));
        })
    };
    if let Ok(upload) = xhr.upload() {
        upload.set_onprogress(Some(on_prog.as_ref().unchecked_ref()));
    }

    // Terminal event: a response (even non-2xx) is `Done { Response }`; only a failure
    // to obtain a response at all is `Done { TransportError }`.
    let on_done = {
        let (emit, xhr_c) = (emit.clone(), xhr.clone());
        Closure::<dyn FnMut()>::new(move || {
            let status = xhr_c.status().unwrap_or(0);
            let outcome = if status == 0 {
                HttpOutcome::TransportError { message: "upload failed".into() }
            } else {
                HttpOutcome::Response { status, headers: vec![], body: vec![] }
            };
            emit(transfer_response(&TransferEvent::Done { outcome, handle: None }));
        })
    };
    xhr.set_onload(Some(on_done.as_ref().unchecked_ref()));
    let on_err = {
        let emit = emit.clone();
        Closure::<dyn FnMut()>::new(move || {
            emit(transfer_response(&TransferEvent::Done {
                outcome: HttpOutcome::TransportError { message: "upload error".into() },
                handle: None,
            }));
        })
    };
    xhr.set_onerror(Some(on_err.as_ref().unchecked_ref()));
    let on_abort = {
        let emit = emit.clone();
        Closure::<dyn FnMut()>::new(move || {
            emit(transfer_response(&TransferEvent::Done {
                outcome: HttpOutcome::TransportError { message: "upload aborted".into() },
                handle: None,
            }));
        })
    };
    xhr.set_onabort(Some(on_abort.as_ref().unchecked_ref()));

    // The upload `source` is itself a `blob:` URL (e.g. produced by `photo`/`camera` or
    // `files`), so fetch it back into a `Blob` before sending — same shape a native
    // shell would read a file handle. A missing/unreadable source sends no body.
    //
    // Cancel race (see `TransferHandle::drop`): `open_with_async` above has already run,
    // but `send`/`send_with_opt_blob` is deferred behind the `fetch_blob` await. Per the
    // XHR spec, `abort()` before the send-flag is set (i.e. before `send` is called) is a
    // no-op, so if `cx.unsubscribe` fires in this window, `xhr.abort()` alone would not
    // stop the request from going out. `cancelled` is the second half of that guarantee:
    // it's checked right before `send`, after the await, so a drop that lands during the
    // fetch is still honored.
    let xhr_send = xhr.clone();
    let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
    let cancelled_send = cancelled.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let blob = fetch_blob(&source).await;
        if cancelled_send.get() {
            return;
        }
        if let Some(blob) = blob {
            let _ = xhr_send.send_with_opt_blob(Some(&blob));
        } else {
            let _ = xhr_send.send();
        }
    });

    StreamHandle::Transfer(TransferHandle {
        xhr: Some(xhr),
        abort: None,
        cancelled: Some(cancelled),
        _on_prog: Some(on_prog),
        _on_done: Some(on_done),
        _on_err: Some(on_err),
        _on_abort: Some(on_abort),
    })
}

/// Fetch a `blob:` (or any) URL back into a `Blob`, for handing to
/// `XmlHttpRequest::send_with_opt_blob`. `None` on any failure (network error, not a
/// Blob-shaped response, …) — the caller falls back to sending no body.
async fn fetch_blob(url: &str) -> Option<web_sys::Blob> {
    use wasm_bindgen::JsCast;
    let win = web_sys::window()?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(win.fetch_with_str(url)).await.ok()?;
    let resp: web_sys::Response = resp_value.dyn_into().ok()?;
    let blob_promise = resp.blob().ok()?;
    let blob_value = wasm_bindgen_futures::JsFuture::from(blob_promise).await.ok()?;
    blob_value.dyn_into().ok()
}

/// Start a web download via `fetch` + a `ReadableStream` reader.
///
/// DELIBERATE WEB ASYMMETRY (see `start_web_upload` for the other half): download uses
/// `fetch`'s streaming response body to report progress as chunks arrive, then hands
/// the app back a `blob:` handle for the assembled bytes — the same handle shape
/// `take_image`/`photo.pick` returns via `Url::create_object_url_with_blob`. (XHR could
/// also do a download, but fetch + ReadableStream is the standard/ergonomic way to get
/// mid-transfer download progress on the web.)
fn start_web_download(
    url: String,
    headers: Vec<(String, String)>,
    emit: impl Fn(PluginResponse) + Clone + 'static,
) -> StreamHandle {
    let ctrl = web_sys::AbortController::new().expect("abortcontroller");
    let signal = ctrl.signal();
    let emit2 = emit.clone();
    wasm_bindgen_futures::spawn_local(async move {
        match fetch_stream(&url, &headers, &signal).await {
            Ok((status, resp_headers, total, mut reader)) => {
                let mut got: u64 = 0;
                let mut chunks: Vec<u8> = Vec::new();
                let mut last = js_now();
                loop {
                    match reader.next().await {
                        Ok(Some(chunk)) => {
                            got += chunk.len() as u64;
                            chunks.extend_from_slice(&chunk);
                            let now = js_now();
                            // ~10/sec progress throttling (see `start_web_upload`).
                            if now - last >= 100.0 {
                                last = now;
                                emit2(transfer_response(&TransferEvent::Progress { transferred: got, total }));
                            }
                        }
                        Ok(None) => break, // stream finished
                        Err(msg) => {
                            emit2(transfer_response(&TransferEvent::Done {
                                outcome: HttpOutcome::TransportError { message: msg },
                                handle: None,
                            }));
                            return;
                        }
                    }
                }
                let handle = make_blob_url(&chunks);
                let outcome = HttpOutcome::Response { status, headers: resp_headers, body: vec![] };
                emit2(transfer_response(&TransferEvent::Done { outcome, handle: Some(handle) }));
            }
            Err(msg) => emit2(transfer_response(&TransferEvent::Done {
                outcome: HttpOutcome::TransportError { message: msg },
                handle: None,
            })),
        }
    });
    StreamHandle::Transfer(TransferHandle {
        xhr: None,
        abort: Some(ctrl),
        cancelled: None,
        _on_prog: None,
        _on_done: None,
        _on_err: None,
        _on_abort: None,
    })
}

/// Begin a GET (with the given headers) via `fetch` under `signal` and return the
/// response's status, headers, `Content-Length` (if present) and a chunk [`Reader`]
/// over its body stream.
async fn fetch_stream(
    url: &str,
    headers: &[(String, String)],
    signal: &web_sys::AbortSignal,
) -> Result<(u16, Vec<HttpHeader>, Option<u64>, Reader), String> {
    use wasm_bindgen::JsCast;
    let win = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let js_headers = web_sys::Headers::new().map_err(|e| js_err(&e))?;
    for (n, v) in headers {
        js_headers.append(n, v).map_err(|e| js_err(&e))?;
    }
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    init.set_headers_headers(&js_headers);
    init.set_signal(Some(signal));
    let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(|e| js_err(&e))?;

    let resp_value = wasm_bindgen_futures::JsFuture::from(win.fetch_with_request(&request))
        .await
        .map_err(|e| js_err(&e))?;
    let resp: web_sys::Response = resp_value.dyn_into().map_err(|_| "fetch: not a Response".to_string())?;
    let status = resp.status();
    let resp_headers = response_headers(&resp.headers());
    let total = resp_headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-length"))
        .and_then(|h| h.value.parse().ok());

    let Some(stream) = resp.body() else {
        // No body (e.g. 204/304, or a HEAD-like response) — an empty reader is correct:
        // the caller's loop immediately sees "finished" and moves straight to Done.
        return Ok((status, resp_headers, total, Reader::empty()));
    };
    let reader = web_sys::ReadableStreamDefaultReader::new(&stream).map_err(|e| js_err(&e))?;
    Ok((status, resp_headers, total, Reader::new(reader)))
}

/// A `web_sys::Headers` iterable (Fetch's `Headers` implements `Symbol.iterator` over
/// `[name, value]` pairs) collected into our wire [`HttpHeader`] shape.
fn response_headers(headers: &web_sys::Headers) -> Vec<HttpHeader> {
    use wasm_bindgen::JsCast;
    let mut out = Vec::new();
    if let Ok(Some(iter)) = js_sys::try_iter(headers) {
        for entry in iter.flatten() {
            let arr: js_sys::Array = entry.unchecked_into();
            let name = arr.get(0).as_string().unwrap_or_default();
            let value = arr.get(1).as_string().unwrap_or_default();
            out.push(HttpHeader { name, value });
        }
    }
    out
}

/// Best-effort stringification of a `JsValue` error (e.g. a `DOMException`) for
/// `TransferEvent::Done { outcome: HttpOutcome::TransportError { message } }`.
fn js_err(e: &wasm_bindgen::JsValue) -> String {
    use wasm_bindgen::JsCast;
    e.as_string()
        .or_else(|| e.dyn_ref::<js_sys::Error>().map(|err| String::from(err.message())))
        .unwrap_or_else(|| "transfer error".to_string())
}

/// A minimal async chunk reader over a `ReadableStreamDefaultReader`. `next()` resolves
/// to `Ok(Some(bytes))` per chunk, `Ok(None)` when the stream is done, or `Err(message)`
/// if the underlying `read()` rejects (e.g. the fetch was aborted mid-stream).
struct Reader(Option<web_sys::ReadableStreamDefaultReader>);
impl Reader {
    fn new(reader: web_sys::ReadableStreamDefaultReader) -> Self {
        Self(Some(reader))
    }
    /// A reader over no stream at all (e.g. a bodiless response) — always "done".
    fn empty() -> Self {
        Self(None)
    }
    async fn next(&mut self) -> Result<Option<Vec<u8>>, String> {
        use wasm_bindgen::JsCast;
        let Some(reader) = &self.0 else { return Ok(None) };
        let result = wasm_bindgen_futures::JsFuture::from(reader.read()).await.map_err(|e| js_err(&e))?;
        let result: web_sys::ReadableStreamReadResult = result.unchecked_into();
        if result.get_done().unwrap_or(true) {
            return Ok(None);
        }
        let value = result.get_value();
        let bytes = js_sys::Uint8Array::new(&value).to_vec();
        Ok(Some(bytes))
    }
}

/// Assemble bytes into a `Blob` and return an object URL — the download's `handle`. The
/// same shape [`take_image`]'s `Url::create_object_url_with_blob` returns for a picked
/// photo, so an app can render/save a downloaded file the same way.
fn make_blob_url(bytes: &[u8]) -> String {
    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array);
    web_sys::Blob::new_with_u8_array_sequence(&parts)
        .ok()
        .and_then(|blob| web_sys::Url::create_object_url_with_blob(&blob).ok())
        .unwrap_or_default()
}

/// A `system` deeplink event payload (the push-style tagged JSON the app demuxes by `type`).
fn system_deeplink(url: &str) -> String {
    format!("{{\"type\":\"deeplink\",\"url\":{}}}", serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into()))
}
/// A `system` lifecycle event payload — page visibility maps to active/background.
fn system_lifecycle(doc: &web_sys::Document) -> String {
    let state = if doc.visibility_state() == web_sys::VisibilityState::Visible { "active" } else { "background" };
    format!("{{\"type\":\"lifecycle\",\"state\":\"{state}\"}}")
}

/// An open streaming source, parked by subscription key for teardown. Dropping the
/// entry stops the source (the `Interval` cancels on drop; the `WebSocket` is closed
/// explicitly in the `unsubscribe` handler and its closures drop here).
enum StreamHandle {
    /// A `ticker` interval — held only so dropping it (on unsubscribe) cancels it.
    Ticker { _interval: gloo_timers::callback::Interval },
    Ws(WsStream),
    /// The built-in `system` source — holds its JS listeners alive; `Drop` removes them on
    /// unsubscribe (the handle is dropped when removed from `STREAMS`). Never pattern-matched.
    #[allow(dead_code)]
    System(SystemStream),
    /// An in-flight transfer — held so dropping it (on unsubscribe) aborts the XHR /
    /// cancels the fetch reader. Never pattern-matched.
    #[allow(dead_code)]
    Transfer(TransferHandle),
}

/// Holds a web transfer so unsubscribe can abort it. For upload we keep the
/// `XmlHttpRequest` (call `.abort()` on drop via the Drop impl); for download we keep an
/// `AbortController` whose `.abort()` cancels the fetch + reader.
struct TransferHandle {
    xhr: Option<web_sys::XmlHttpRequest>,
    abort: Option<web_sys::AbortController>,
    // Upload-cancel race guard (see the comment at `start_web_upload`'s `spawn_local`):
    // `xhr.abort()` before `send()` has been called is a spec no-op, so this flag is the
    // half that actually stops a not-yet-sent upload. `None` for download, which has no
    // such window (its `AbortController` is wired into the fetch before any async work).
    cancelled: Option<std::rc::Rc<std::cell::Cell<bool>>>,
    // Typed closure fields (not `Closure::into_js_value`, which leaks permanently — see
    // `WsStream`/`SystemStream` above for the same pattern): held here so they free when
    // the handle drops, on unsubscribe or transfer completion. Download wires no XHR
    // event closures, so its fields are `None`.
    _on_prog: Option<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::ProgressEvent)>>,
    _on_done: Option<wasm_bindgen::closure::Closure<dyn FnMut()>>,
    _on_err: Option<wasm_bindgen::closure::Closure<dyn FnMut()>>,
    _on_abort: Option<wasm_bindgen::closure::Closure<dyn FnMut()>>,
}
impl Drop for TransferHandle {
    fn drop(&mut self) {
        if let Some(c) = &self.cancelled {
            c.set(true);
        }
        if let Some(x) = &self.xhr {
            let _ = x.abort();
        }
        if let Some(a) = &self.abort {
            a.abort();
        }
    }
}

/// Bincode a `TransferEvent` into a stream `PluginResponse` (mirrors Release A's `http` encode).
fn transfer_response(ev: &TransferEvent) -> PluginResponse {
    PluginResponse {
        ok: matches!(ev, TransferEvent::Done { outcome, .. } if outcome.is_success()),
        output: ev.encode(),
    }
}

/// The `system` subscription's event listeners — removed from the DOM when dropped (unsubscribe).
struct SystemStream {
    win: web_sys::Window,
    doc: web_sys::Document,
    _onpop: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    _onvis: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for SystemStream {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let _ = self.win.remove_event_listener_with_callback("popstate", self._onpop.as_ref().unchecked_ref());
        let _ = self.doc.remove_event_listener_with_callback("visibilitychange", self._onvis.as_ref().unchecked_ref());
    }
}

/// An open web `WebSocket` subscription — holds its JS closures so they stay alive.
struct WsStream {
    ws: web_sys::WebSocket,
    _onmessage: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>,
    _onclose: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::CloseEvent)>,
}

/// Fulfil a request/response capability. `http` via `fetch`; `device` via the
/// browser's user-agent string (the web analogue of a device model).
async fn perform(call: &PluginCall) -> PluginResponse {
    if call.plugin == "device" {
        let nav = web_sys::window().map(|w| w.navigator());
        let output = if call.op == "locale" {
            // The browser's preferred language as a BCP-47 tag (e.g. "de-CH").
            nav.and_then(|n| n.language()).unwrap_or_else(|| "en-US".into())
        } else {
            nav.and_then(|n| n.user_agent().ok()).unwrap_or_default()
        };
        return PluginResponse::text(true, output);
    }
    if call.plugin == "photo" && call.op == "pick" {
        return take_image(false).await;
    }
    if call.plugin == "camera" && call.op == "capture" {
        return take_image(true).await;
    }
    if call.plugin == "datetime" {
        return match call.op.as_str() {
            "date" => take_datetime("date").await,
            "time" => take_datetime("time").await,
            other => PluginResponse::text(false, format!("unknown datetime op '{other}'")),
        };
    }
    if call.plugin == "dialog" && call.op == "confirm" {
        let v: serde_json::Value = serde_json::from_str(&call.input).unwrap_or(serde_json::Value::Null);
        let title = v.get("title").and_then(serde_json::Value::as_str).unwrap_or("");
        let message = v.get("message").and_then(serde_json::Value::as_str).unwrap_or("");
        let prompt = if title.is_empty() { message.to_string() } else { format!("{title}\n\n{message}") };
        let ok = web_sys::window()
            .and_then(|w| w.confirm_with_message(&prompt).ok())
            .unwrap_or(false);
        return PluginResponse::text(ok, if ok { "ok" } else { "cancel" });
    }
    if call.plugin != "http" {
        return PluginResponse::text(false, format!("plugin '{}' not available", call.plugin));
    }
    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap_or(serde_json::Value::Null);
    let url = v.get("url").and_then(serde_json::Value::as_str).unwrap_or("");
    let body = v.get("body").and_then(serde_json::Value::as_str);
    let req_headers: Vec<(String, String)> = v
        .get("headers")
        .and_then(serde_json::Value::as_array)
        .map(|hs| {
            hs.iter()
                .filter_map(|h| {
                    Some((
                        h.get("name")?.as_str()?.to_string(),
                        h.get("value")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    use gloo_net::http::{Method, Request};

    // Exhaustive: an unknown verb is an error, never a silent GET. The previous
    // `_ => Request::get(url)` fallthrough turned every PUT into a GET.
    let builder = match call.op.as_str() {
        "GET" => Request::get(url),
        "POST" => Request::post(url),
        "PUT" => Request::put(url),
        "PATCH" => Request::patch(url),
        "DELETE" => Request::delete(url),
        "HEAD" => Request::get(url).method(Method::HEAD),
        "OPTIONS" => Request::get(url).method(Method::OPTIONS),
        other => return http_transport_error(format!("unsupported HTTP method '{other}'")),
    };

    // Only default Content-Type when the caller did not set one.
    let caller_set_content_type =
        req_headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("content-type"));

    // `RequestBuilder::header` maps to `web_sys::Headers::set`, which REPLACES
    // any existing value for that name — unlike iOS's `addValue` and Android's
    // `addHeader`, which both APPEND. Build a `gloo_net::http::Headers` and
    // `append` into it instead, so repeated names (Set-Cookie, Accept) survive
    // on web the same way they do on the native shells.
    let gloo_headers = gloo_net::http::Headers::new();
    for (name, value) in &req_headers {
        gloo_headers.append(name, value);
    }
    if body.is_some() && !caller_set_content_type {
        gloo_headers.append("Content-Type", "application/json");
    }
    let builder = builder.headers(gloo_headers);

    let request = match body {
        Some(b) => builder.body(b),
        None => builder.build(),
    };
    let request = match request {
        Ok(r) => r,
        Err(e) => return http_transport_error(e.to_string()),
    };

    match request.send().await {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp
                .headers()
                .entries()
                .map(|(name, value)| HttpHeader { name, value })
                .collect();
            match resp.binary().await {
                Ok(bytes) => {
                    let outcome = HttpOutcome::Response { status, headers, body: bytes };
                    PluginResponse { ok: (200..300).contains(&status), output: outcome.encode() }
                }
                // A body-read failure (truncated/aborted stream) is a transport
                // failure, not a successful empty response — match native shells.
                Err(e) => http_transport_error(e.to_string()),
            }
        }
        Err(e) => http_transport_error(e.to_string()),
    }
}

/// A failure where no HTTP response was obtained. `ok` is false and there is no status.
fn http_transport_error(message: String) -> PluginResponse {
    PluginResponse { ok: false, output: HttpOutcome::TransportError { message }.encode() }
}

/// Pick or capture an image via a hidden `<input type=file accept=image/*>`, clicked
/// to open the browser's file dialog — or, with `capture`, to hint the device camera on
/// supporting mobile browsers (desktop falls back to the file dialog). Awaits the
/// `change` event and returns a `blob:` object URL the `<img>` renderer loads. No
/// permission needed (the picker/camera prompt is the browser's). Backs both the
/// `photo`/`pick` and `camera`/`capture` capabilities.
async fn take_image(capture: bool) -> PluginResponse {
    use wasm_bindgen::{closure::Closure, JsCast};
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return PluginResponse::text(false, "no document");
    };
    let Some(input) = doc.create_element("input").ok().and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok()) else {
        return PluginResponse::text(false, "no input element");
    };
    input.set_type("file");
    input.set_accept("image/*");
    if capture {
        // Hints the environment-facing camera on mobile browsers that support it.
        let _ = input.set_attribute("capture", "environment");
    }

    let (tx, rx) = futures_channel::oneshot::channel::<Option<String>>();
    let tx = std::cell::RefCell::new(Some(tx));
    let input_for_cb = input.clone();
    let on_change = Closure::wrap(Box::new(move || {
        let url = input_for_cb
            .files()
            .and_then(|files| files.get(0))
            .and_then(|file| web_sys::Url::create_object_url_with_blob(&file).ok());
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(url);
        }
    }) as Box<dyn FnMut()>);
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    input.click();
    on_change.forget(); // keep the handler alive until `change` fires

    match rx.await {
        Ok(Some(url)) => PluginResponse::text(true, url),
        _ => PluginResponse::text(false, "cancelled"),
    }
}

/// Pick a date (`kind = "date"`) or time (`kind = "time"`) via a hidden native
/// `<input>`, opening the browser's picker with `showPicker()`. Returns the value
/// (`YYYY-MM-DD` for date, 24-hour `HH:MM` for time); `ok=false` on cancel/dismiss.
/// Backs the `datetime` capability (`cx.pick_date` / `cx.pick_time`).
async fn take_datetime(kind: &str) -> PluginResponse {
    use wasm_bindgen::{closure::Closure, JsCast};
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return PluginResponse::text(false, "no document");
    };
    let Some(input) = doc.create_element("input").ok().and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok()) else {
        return PluginResponse::text(false, "no input element");
    };
    input.set_type(kind); // "date" or "time"
    // showPicker() needs a connected element; keep it in the DOM but out of sight.
    let _ = input.set_attribute("style", "position:fixed;left:-9999px;opacity:0");
    if let Some(body) = doc.body() {
        let _ = body.append_child(&input);
    }

    let (tx, rx) = futures_channel::oneshot::channel::<Option<String>>();
    let tx = std::rc::Rc::new(std::cell::RefCell::new(Some(tx)));
    let input_for_change = input.clone();
    let tx_change = tx.clone();
    let on_change = Closure::wrap(Box::new(move || {
        let v = input_for_change.value();
        if let Some(tx) = tx_change.borrow_mut().take() {
            let _ = tx.send(if v.is_empty() { None } else { Some(v) });
        }
    }) as Box<dyn FnMut()>);
    let tx_cancel = tx.clone();
    let on_cancel = Closure::wrap(Box::new(move || {
        if let Some(tx) = tx_cancel.borrow_mut().take() {
            let _ = tx.send(None);
        }
    }) as Box<dyn FnMut()>);
    let _ = input.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref());
    let _ = input.add_event_listener_with_callback("cancel", on_cancel.as_ref().unchecked_ref());
    if input.show_picker().is_err() {
        input.click(); // older browsers: focus the field so the user can type a value
    }
    on_change.forget(); // keep the handlers alive until an event fires
    on_cancel.forget();

    let result = rx.await;
    input.remove();
    match result {
        Ok(Some(v)) => PluginResponse::text(true, v),
        _ => PluginResponse::text(false, "cancelled"),
    }
}

const STORAGE_KEY: &str = "mobiler.state";

/// `window.localStorage`, if available.
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Fulfil a fire-and-forget capability in the browser — the web twin of the native
/// shells' notify handlers (storage/clipboard/share/browser). None block; an unknown
/// capability is a graceful no-op.
fn perform_notify(notify: &PluginNotify) {
    let win = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    match (notify.plugin.as_str(), notify.op.as_str()) {
        // Persist the state blob (paired with cx.save + restore-on-startup above).
        ("storage", "save") => {
            if let Some(s) = local_storage() {
                let _ = s.set_item(STORAGE_KEY, &notify.input);
            }
        }
        // Copy to the clipboard (write_text returns a Promise we let run).
        ("clipboard", "copy") => {
            let _ = win.navigator().clipboard().write_text(&notify.input);
        }
        // Open a URL in a new tab.
        ("browser", "open") => {
            let _ = win.open_with_url_and_target(&notify.input, "_blank");
        }
        // No reliable cross-browser share sheet (navigator.share is mobile-only and
        // gesture-gated), so degrade to copying — a sane universal fallback.
        ("share", _) => {
            let _ = win.navigator().clipboard().write_text(&notify.input);
        }
        // Tear down a streaming subscription: close the WebSocket parked under this
        // key (input = the subscription key) and drop its closures. Paired with
        // cx.unsubscribe; the matching source was opened in `start_stream`.
        ("stream", "unsubscribe") => {
            // Removing the entry drops the source (a `ticker` Interval cancels on
            // drop); for a WebSocket we also close it explicitly.
            if let Some(StreamHandle::Ws(ws)) = STREAMS.with(|m| m.borrow_mut().remove(&notify.input)) {
                let _ = ws.ws.close();
            }
        }
        // Transient toast: a styled div appended to <body>, auto-removed after a beat.
        ("toast", _) => show_toast(&notify.input),
        // Haptic tap. navigator.vibrate is unsupported on iOS Safari (a graceful no-op).
        ("haptics", style) => {
            let ms = match style {
                "light" => 15,
                "heavy" => 50,
                _ => 30, // medium / unknown
            };
            let _ = win.navigator().vibrate_with_duration(ms);
        }
        _ => {} // unknown capability: ignore
    }
}

/// Append a transient toast to `<body>` (styled by `.toast` in mobiler.css) and
/// remove it after ~2.6 s — the web twin of the native toast/snackbar.
fn show_toast(text: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let (Ok(el), Some(body)) = (doc.create_element("div"), doc.body()) else { return };
    el.set_class_name("toast");
    el.set_text_content(Some(text));
    let _ = body.append_child(&el);
    gloo_timers::callback::Timeout::new(2600, move || el.remove()).forget();
}

// ---------------- Widget → DOM ----------------

/// `Widget` → DOM. **Exhaustive** by construction — the `match` has no catch-all,
/// so (like the Compose/SwiftUI shells) it won't compile until every `Widget`
/// variant is handled. Style *intent* (TextStyle, Tone, …) becomes a CSS class;
/// the concrete look lives in `mobiler.css`.
fn render(widget: &Widget, send: &Dispatch) -> AnyView {
    match widget {
        // ---- content ----
        Widget::Text { content, style } => {
            let (class, content) = (text_class(*style), content.clone());
            view! { <p class=class>{content}</p> }.into_any()
        }
        Widget::Image { source, shape, ratio } => {
            let (class, source) = (image_class(*shape, *ratio), source.clone());
            view! { <img class=class src=source /> }.into_any()
        }
        Widget::Badge { label, tone } => {
            let (class, label) = (format!("badge {}", tone_class(*tone)), label.clone());
            view! { <span class=class>{label}</span> }.into_any()
        }
        Widget::ColorDot { color } => {
            view! { <span class=format!("dot {}", dot_class(*color))></span> }.into_any()
        }
        Widget::Avatar { source, status } => {
            let dot = status.map(|t| view! { <span class=format!("avatar-status {}", tone_class(t))></span> });
            view! {
                <span class="avatar">
                    <img class="avatar-img" src=source.clone() />
                    {dot}
                </span>
            }
            .into_any()
        }
        Widget::PdfView { url } => {
            // Browsers render PDFs natively in an iframe (remote URL or local blob/file URL).
            view! { <iframe class="pdfview" src=url.clone() title="PDF"></iframe> }.into_any()
        }
        Widget::WebView { url } => {
            // General embedded web content (incl. hosted player embeds like Bunny.net). `allow`
            // permits autoplay / fullscreen / PiP / encrypted-media so hosted players work.
            view! {
                <iframe
                    class="webview"
                    src=url.clone()
                    title="Web"
                    allow="autoplay; fullscreen; picture-in-picture; encrypted-media"
                    allowfullscreen=true
                ></iframe>
            }.into_any()
        }
        // Interactive map (MapLibre-GL, no key). The div carries the config as data-* attrs;
        // `inject_maplibre_support` inits the map + reports taps by firing `input` on the hidden sink,
        // which this `on:input` forwards as Action::Input { "{id}.tap" | "{id}.marker", Text(...) }.
        Widget::Map { id, center_lat, center_lng, zoom, markers, style_url, interactive } => {
            let send = send.clone();
            let id = id.clone();
            let center = format!("{center_lat},{center_lng}");
            let markers_json = serde_json::to_string(markers).unwrap_or_else(|_| "[]".to_string());
            let style = style_url.clone().unwrap_or_default();
            view! {
                <div class="mobiler-map-wrap">
                    <div
                        class="mobiler-map"
                        data-map="1"
                        data-center=center
                        data-zoom=zoom.to_string()
                        data-style=style
                        data-markers=markers_json
                        data-interactive=interactive.to_string()
                    ></div>
                    <input
                        class="mobiler-map-sink"
                        type="text"
                        tabindex="-1"
                        aria-hidden="true"
                        on:input=move |ev| {
                            let raw = event_target_value(&ev);
                            if let Some((suffix, value)) = raw.split_once('|') {
                                send(Action::Input {
                                    id: format!("{id}.{suffix}"),
                                    value: InputValue::Text(value.to_string()),
                                });
                            }
                        }
                    />
                </div>
            }.into_any()
        }
        Widget::Video { url, playing, controls, looping, muted, on_ended, poster, start_at_ms, captions, rate, volume, urls, start_index, .. } => {
            // Web = a native-controls `<video>`. App-driven play/pause + seek + position/state events
            // are iOS/Android only: the web shell rebuilds the whole tree on each `update`, which would
            // reset the element ~every tick — so we don't pump those here (poster/captions/rate/volume
            // ARE declarative attributes, so they're safe). `muted && playing` → autoplay. MP4 plays
            // everywhere; HLS (.m3u8) plays natively on Safari and, on Chrome/Firefox, via the hls.js
            // bootstrap (`inject_hls_support`). A non-empty `urls` is a playlist (best-effort: starts at
            // `start_index`, advances on `ended` within this element's lifetime — no index pump back).
            use wasm_bindgen::JsCast;
            let (send, ended) = (send.clone(), on_ended.clone());
            let autoplay = *playing && *muted;
            let playlist = urls.clone();
            let start_index = (*start_index).max(0) as usize;
            let effective = if playlist.is_empty() { url.clone() }
                else { playlist.get(start_index).cloned().unwrap_or_else(|| url.clone()) };
            let is_hls = effective.to_ascii_lowercase().ends_with(".m3u8");
            let src = (!is_hls).then(|| effective.clone());
            let hls_src = is_hls.then(|| effective.clone());
            let poster_attr = poster.clone();
            let start_at = *start_at_ms;
            let rate = *rate as f64;
            let volume = (*volume as f64).clamp(0.0, 1.0);
            let tracks: Vec<_> = captions.iter().map(|c| view! {
                <track kind="subtitles" src=c.url.clone() srclang=c.language.clone() label=c.label.clone() default=c.default_on />
            }).collect();
            let next_idx = std::rc::Rc::new(std::cell::Cell::new(start_index));
            view! {
                <video
                    class="video"
                    src=src
                    data-hls-src=hls_src
                    poster=poster_attr
                    controls=*controls
                    autoplay=autoplay
                    prop:loop=*looping
                    prop:playbackRate=rate
                    prop:volume=volume
                    muted=*muted
                    playsinline=true
                    on:loadedmetadata=move |ev| {
                        if start_at >= 0 {
                            if let Some(v) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlVideoElement>().ok()) {
                                v.set_current_time(start_at as f64 / 1000.0);
                            }
                        }
                    }
                    on:ended=move |ev| {
                        let nxt = next_idx.get() + 1;
                        if !playlist.is_empty() && nxt < playlist.len() {
                            next_idx.set(nxt);
                            if let Some(v) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlVideoElement>().ok()) {
                                v.set_src(&playlist[nxt]);
                                let _ = v.play();
                            }
                        } else if let Some(t) = ended.clone() {
                            send(Action::Fired { token: t });
                        }
                    }
                >{tracks}</video>
            }.into_any()
        }
        Widget::Rating { value, max, on_rate } => {
            let value = *value;
            let stars: Vec<AnyView> = (1..=*max)
                .map(|i| {
                    let threshold = u32::from(i) * 10;
                    // filled / half / empty by tenths.
                    let glyph = if value >= threshold { "★" } else if value + 5 >= threshold { "⯨" } else { "☆" };
                    match on_rate {
                        Some(tokens) => {
                            let (send, token) = (send.clone(), tokens.get(usize::from(i - 1)).cloned().unwrap_or_default());
                            view! {
                                <button class="star star-tappable" on:click=move |_| send(Action::Fired { token: token.clone() })>
                                    {glyph}
                                </button>
                            }
                            .into_any()
                        }
                        None => view! { <span class="star">{glyph}</span> }.into_any(),
                    }
                })
                .collect();
            view! { <span class="rating">{stars}</span> }.into_any()
        }
        Widget::Divider => view! { <hr class="divider" /> }.into_any(),
        Widget::Progress { value } => match value {
            Some(v) => {
                let pct = (v.clamp(0.0, 1.0) * 100.0) as u32;
                view! { <div class="progress"><div class="progress-bar" style=format!("width:{pct}%")></div></div> }.into_any()
            }
            None => view! { <div class="progress progress-indeterminate"><div class="progress-bar"></div></div> }.into_any(),
        },
        Widget::Skeleton => view! { <div class="skeleton"></div> }.into_any(),
        Widget::Chart { series, labels, style, axis, legend } => {
            chart_view(series, labels, *style, *axis, *legend)
        }
        Widget::RegionChart { regions, ticks, x_max, y_max, ref_lines, bracket, legend } => {
            region_chart_view(regions, ticks, *x_max, *y_max, ref_lines, bracket, legend)
        }
        Widget::Calendar { year, month, first_weekday, selected, on_day } => {
            const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June",
                "July", "August", "September", "October", "November", "December"];
            let head_label = format!("{} {year}", MONTHS.get((*month as usize).saturating_sub(1)).copied().unwrap_or(""));
            let weekdays = ["S", "M", "T", "W", "T", "F", "S"];
            let heads: Vec<_> = weekdays.iter().map(|w| view! { <div class="cal-head">{*w}</div> }).collect();
            let blanks: Vec<_> = (0..*first_weekday).map(|_| view! { <div class="cal-blank"></div> }).collect();
            let selected = *selected;
            let days: Vec<_> = on_day.iter().enumerate().map(|(i, token)| {
                let day = (i + 1) as u8;
                let token = token.clone();
                let send = send.clone();
                let cls = if selected == Some(day) { "cal-day cal-sel" } else { "cal-day" };
                view! { <button class=cls on:click=move |_| send(Action::Fired { token: token.clone() })>{day.to_string()}</button> }
            }).collect();
            view! {
                <div class="calendar">
                    <div class="cal-title">{head_label}</div>
                    <div class="cal-grid">{heads}{blanks}{days}</div>
                </div>
            }.into_any()
        }
        Widget::SwipeAction { child, actions } => {
            // Web has no swipe gesture — render the actions inline as a trailing button row.
            let acts: Vec<_> = actions.iter().map(|a| {
                let token = a.on_tap.clone();
                let send = send.clone();
                let cls = format!("swipe-act {}", tone_class(a.tone));
                let label = a.label.clone();
                view! { <button class=cls on:click=move |_| send(Action::Fired { token: token.clone() })>{label}</button> }
            }).collect();
            view! {
                <div class="swipe-row">
                    <div class="swipe-content">{render(child, send)}</div>
                    <div class="swipe-actions">{acts}</div>
                </div>
            }.into_any()
        }
        Widget::Spacer { size } => {
            view! { <div class=format!("spacer {}", spacer_class(*size))></div> }.into_any()
        }

        // ---- layout ----
        Widget::Row { children } => {
            let kids = render_all(children, send);
            view! { <div class="row">{kids}</div> }.into_any()
        }
        Widget::Column { children } => {
            let kids = render_all(children, send);
            view! { <div class="col">{kids}</div> }.into_any()
        }
        Widget::Card { child, style, on_press, on_long_press } => {
            let class = format!("card {}", card_class(*style));
            let body = render(child, send);
            match (on_press, on_long_press) {
                // Plain, non-interactive card.
                (None, None) => view! { <div class=class>{body}</div> }.into_any(),
                // Tappable and/or long-pressable — render a button with the relevant handlers.
                (tap, long) => {
                    let send = send.clone();
                    // Web has no native long-press; shim it with a pointer-hold timer (~500 ms),
                    // cancelled on pointerup/leave/cancel. A `long_fired` flag suppresses the
                    // click that follows a successful hold so it doesn't also fire the tap.
                    let timer: Rc<RefCell<Option<gloo_timers::callback::Timeout>>> =
                        Rc::new(RefCell::new(None));
                    let long_fired = Rc::new(RefCell::new(false));

                    let on_pointerdown = {
                        let (send, long, timer, long_fired) =
                            (send.clone(), long.clone(), timer.clone(), long_fired.clone());
                        move |_: web_sys::PointerEvent| {
                            let Some(token) = long.clone() else { return };
                            *long_fired.borrow_mut() = false;
                            let (send, long_fired) = (send.clone(), long_fired.clone());
                            *timer.borrow_mut() = Some(gloo_timers::callback::Timeout::new(
                                500,
                                move || {
                                    *long_fired.borrow_mut() = true;
                                    send(Action::Fired { token: token.clone() });
                                },
                            ));
                        }
                    };
                    let cancel = {
                        let timer = timer.clone();
                        // Dropping the `Timeout` cancels the pending fire.
                        move |_: web_sys::PointerEvent| { timer.borrow_mut().take(); }
                    };
                    let on_click = {
                        let (send, tap, long_fired) = (send.clone(), tap.clone(), long_fired.clone());
                        move |_| {
                            // Suppress the tap that trails a long-press.
                            if std::mem::take(&mut *long_fired.borrow_mut()) {
                                return;
                            }
                            if let Some(token) = tap.clone() {
                                send(Action::Fired { token });
                            }
                        }
                    };
                    view! {
                        <button
                            class=format!("{class} card-tappable")
                            on:pointerdown=on_pointerdown
                            on:pointerup=cancel.clone()
                            on:pointerleave=cancel.clone()
                            on:pointercancel=cancel
                            on:click=on_click
                        >
                            {body}
                        </button>
                    }
                    .into_any()
                }
            }
        }
        // Z-stack. With `scrim`, the first child is a background image, darkened
        // by an overlay, and the rest layer on top in light content — the DOM twin
        // of the Compose `matchParentSize` scrim / SwiftUI `.overlay` on the image.
        Widget::Box { children, align, scrim } => {
            let acls = align_class(*align);
            if *scrim && children.len() > 1 {
                let bg = render(&children[0], send);
                let content = render_all(&children[1..], send);
                view! {
                    <div class=format!("box box-scrim {acls}")>
                        {bg}
                        <div class="scrim"></div>
                        <div class="box-content">{content}</div>
                    </div>
                }
                .into_any()
            } else {
                let kids = render_all(children, send);
                view! { <div class=format!("box {acls}")>{kids}</div> }.into_any()
            }
        }
        Widget::Grid { children } => {
            let kids = render_all(children, send);
            view! { <div class="grid">{kids}</div> }.into_any()
        }
        Widget::Scroller { children } => {
            let kids = render_all(children, send);
            view! { <div class="scroller">{kids}</div> }.into_any()
        }
        // Two-pane master-detail. CSS does the adapting: wide (`@media min-width:768px`) shows both
        // panes side-by-side (back hidden); narrow shows one — primary by default, or detail (+ a
        // back chevron) when `data-detail` is set. `show_detail`/`on_back` only matter when narrow.
        Widget::Split { primary, detail, show_detail, on_back } => {
            let p = render(primary, send);
            let d = render(detail, send);
            let back_btn = on_back.clone().map(|t| {
                let send = send.clone();
                view! { <button class="split-back" on:click=move |_| send(Action::Fired { token: t.clone() })>"‹ Back"</button> }
            });
            view! {
                <div class="split" data-detail=show_detail.then_some("1")>
                    <div class="split-primary">{p}</div>
                    <div class="split-detail">{back_btn}{d}</div>
                </div>
            }.into_any()
        }
        // Accessibility wrapper: name the subtree for a screen reader (aria-label), give it a role,
        // and the hint via `title`. Best-effort web mapping of iOS traits / Android semantics.
        Widget::A11y { child, label, hint, role } => {
            let body = render(child, send);
            let role_attr = role.map(a11y_role_aria).unwrap_or("group");
            view! {
                <div class="a11y" role=role_attr aria-label=label.clone() title=hint.clone()>
                    {body}
                </div>
            }.into_any()
        }
        // A long/paged feed. Web has no pull gesture or reliable infinite-scroll on a sub-container,
        // so (like Scaffold pull-to-refresh) the gestures degrade to controls: a top "↻ Refresh"
        // button (while `on_refresh`), and a bottom "Load more" button (while `has_more && !loading`)
        // / loading bar / "end" caption. iOS/Android do true pull + scroll-near-end detection.
        Widget::LazyList { children, on_load_more, loading, has_more, on_refresh, refreshing } => {
            let kids = render_all(children, send);
            let refresh_btn = on_refresh.clone().map(|token| {
                let send = send.clone();
                view! { <button class="refresh-btn" on:click=move |_| send(Action::Fired { token: token.clone() })>"↻ Refresh"</button> }
            });
            let refresh_bar = refreshing.then(|| view! { <div class="progress progress-indeterminate"><div class="progress-bar"></div></div> });
            let loading_bar = loading.then(|| view! { <div class="progress progress-indeterminate"><div class="progress-bar"></div></div> });
            let load_more_btn = (!*loading && *has_more)
                .then(|| on_load_more.clone())
                .flatten()
                .map(|token| {
                    let send = send.clone();
                    view! { <button class="btn btn-outlined lazylist-more" on:click=move |_| send(Action::Fired { token: token.clone() })>"Load more"</button> }
                });
            let end_cap = (!*has_more && on_load_more.is_some()).then(|| view! { <div class="lazylist-end">"End of list"</div> });
            view! {
                <div class="lazylist">
                    {refresh_btn}
                    {refresh_bar}
                    {kids}
                    {loading_bar}
                    {load_more_btn}
                    {end_cap}
                </div>
            }.into_any()
        }

        // ---- input / actions ----
        Widget::Button { label, style, on_press } => {
            let (send, token, label) = (send.clone(), on_press.clone(), label.clone());
            let class = format!("btn {}", button_class(*style));
            view! {
                <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {label}
                </button>
            }
            .into_any()
        }
        Widget::IconButton { icon, on_press } => {
            let (send, token) = (send.clone(), on_press.clone());
            let glyph = icon_glyph(*icon);
            view! {
                <button class="iconbtn" on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {glyph}
                </button>
            }
            .into_any()
        }
        Widget::Chip { label, selected, on_press } => {
            let (send, token, label) = (send.clone(), on_press.clone(), label.clone());
            let class = if *selected { "chip selected" } else { "chip" };
            view! {
                <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {label}
                </button>
            }
            .into_any()
        }
        Widget::TextField { id, placeholder, value, kind, error } => {
            let (send, id) = (send.clone(), id.clone());
            let (placeholder, value) = (placeholder.clone(), value.clone());
            let invalid = error.is_some();
            let err_view = error.clone().map(|m| view! { <div class="field-error">{m}</div> });
            // (input type, inputmode) per FieldKind. Multiline renders a <textarea> below.
            let (itype, imode): (&str, &str) = match kind {
                FieldKind::Secure => ("password", ""),
                FieldKind::Email => ("email", "email"),
                FieldKind::Number => ("text", "numeric"),
                FieldKind::Decimal => ("text", "decimal"),
                FieldKind::Phone => ("tel", "tel"),
                FieldKind::Url => ("url", "url"),
                FieldKind::Text | FieldKind::Multiline => ("text", ""),
            };
            let field_class = if invalid { "field field-invalid" } else { "field" };
            let control = if matches!(kind, FieldKind::Multiline) {
                view! {
                    <textarea
                        class=field_class
                        rows="3"
                        placeholder=placeholder
                        prop:value=value
                        on:input=move |ev| send(Action::Input {
                            id: id.clone(),
                            value: InputValue::Text(event_target_value(&ev)),
                        })
                    ></textarea>
                }
                .into_any()
            } else {
                view! {
                    <input
                        class=field_class
                        r#type=itype
                        inputmode=imode
                        placeholder=placeholder
                        prop:value=value
                        on:input=move |ev| send(Action::Input {
                            id: id.clone(),
                            value: InputValue::Text(event_target_value(&ev)),
                        })
                    />
                }
                .into_any()
            };
            view! { <div class="field-wrap">{control}{err_view}</div> }.into_any()
        }
        Widget::SearchField { id, placeholder, value } => {
            let (send, id) = (send.clone(), id.clone());
            let (placeholder, value) = (placeholder.clone(), value.clone());
            view! {
                <div class="searchfield">
                    <span class="search-icon">{icon_glyph(Icon::Search)}</span>
                    <input
                        class="search-input"
                        placeholder=placeholder
                        prop:value=value
                        on:input=move |ev| send(Action::Input {
                            id: id.clone(),
                            value: InputValue::Text(event_target_value(&ev)),
                        })
                    />
                </div>
            }
            .into_any()
        }
        Widget::Segmented { segments } => {
            let segs: Vec<AnyView> = segments
                .iter()
                .map(|s| {
                    let (send, token) = (send.clone(), s.on_select.clone());
                    let class = if s.selected { "segment selected" } else { "segment" };
                    let label = s.label.clone();
                    view! {
                        <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                            {label}
                        </button>
                    }
                    .into_any()
                })
                .collect();
            view! { <div class="segmented">{segs}</div> }.into_any()
        }
        Widget::Toggle { id, label, value } => {
            let (send, id, label, checked) = (send.clone(), id.clone(), label.clone(), *value);
            view! {
                <label class="toggle">
                    {label}
                    <input
                        type="checkbox"
                        role="switch"
                        prop:checked=checked
                        on:change=move |ev| send(Action::Input {
                            id: id.clone(),
                            value: InputValue::Bool(event_target_checked(&ev)),
                        })
                    />
                </label>
            }
            .into_any()
        }
        Widget::Checkbox { id, label, value } => {
            let (send, id, label, checked) = (send.clone(), id.clone(), label.clone(), *value);
            view! {
                <label class="check">
                    <input
                        type="checkbox"
                        prop:checked=checked
                        on:change=move |ev| send(Action::Input {
                            id: id.clone(),
                            value: InputValue::Bool(event_target_checked(&ev)),
                        })
                    />
                    {label}
                </label>
            }
            .into_any()
        }
        Widget::Slider { id, value, max } => {
            let (send, id, value, max) = (send.clone(), id.clone(), *value, *max);
            view! {
                <input
                    class="slider"
                    type="range"
                    min="0"
                    max=max
                    prop:value=value
                    on:input=move |ev| send(Action::Input {
                        id: id.clone(),
                        value: InputValue::Int(event_target_value(&ev).parse().unwrap_or(0)),
                    })
                />
            }
            .into_any()
        }
        Widget::Stepper { value, on_decrement, on_increment } => {
            let send_dec = send.clone();
            let send_inc = send.clone();
            let (dec, inc) = (on_decrement.clone(), on_increment.clone());
            view! {
                <div class="stepper">
                    <button on:click=move |_| send_dec(Action::Fired { token: dec.clone() })>"−"</button>
                    <span class="stepper-value">{*value}</span>
                    <button on:click=move |_| send_inc(Action::Fired { token: inc.clone() })>"+"</button>
                </div>
            }
            .into_any()
        }

        // ---- shell ----
        Widget::Scaffold { title, body, tabs, back, dark_mode, theme, fab, sheet, on_refresh, refreshing, route, depth } => {
            let back_btn = back.clone().map(|token| {
                let send = send.clone();
                view! {
                    <button class="back" on:click=move |_| send(Action::Fired { token: token.clone() })>
                        "‹"
                    </button>
                }
            });
            let tabbar = (!tabs.is_empty()).then(|| {
                let tabs: Vec<AnyView> = tabs
                    .iter()
                    .map(|tab| {
                        let (send, token) = (send.clone(), tab.on_select.clone());
                        let class = if tab.selected { "tab selected" } else { "tab" };
                        let label = tab.label.clone();
                        // Optional leading icon → glyph above the label (icon tab bar).
                        let icon = tab.icon.map(|i| view! { <span class="tab-icon">{icon_glyph(i)}</span> });
                        view! {
                            <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                                {icon}
                                <span class="tab-label">{label}</span>
                            </button>
                        }
                        .into_any()
                    })
                    .collect();
                view! { <div class="tabbar">{tabs}</div> }
            });
            // Floating action button — the raised primary action, anchored over the body.
            let fab_btn = fab.clone().map(|f| {
                let (send, token) = (send.clone(), f.on_press.clone());
                view! {
                    <button class="fab" on:click=move |_| send(Action::Fired { token: token.clone() })>
                        {icon_glyph(f.icon)}
                    </button>
                }
            });
            // Modal bottom sheet — a scrim (tap to dismiss) + a panel rising from the bottom.
            let sheet_overlay = sheet.as_ref().map(|s| {
                let (send_scrim, dismiss) = (send.clone(), s.on_dismiss.clone());
                let (title, child) = (s.title.clone(), render(&s.child, send));
                view! {
                    <div class="sheet-scrim" on:click=move |_| send_scrim(Action::Fired { token: dismiss.clone() })></div>
                    <div class="sheet">
                        <div class="sheet-handle"></div>
                        <div class="sheet-title">{title}</div>
                        {child}
                    </div>
                }
            });
            // `theme-dark` flips the CSS variables for the whole shell — theme-as-data,
            // the web twin of the native shells' `preferredColorScheme`/Material theme.
            let class = if *dark_mode { "scaffold theme-dark" } else { "scaffold" };
            // Pull-to-refresh — web has no pull gesture, so expose a top-bar refresh button +
            // an indeterminate bar at the top of the body while `refreshing`.
            let refresh_btn = on_refresh.clone().map(|token| {
                let send = send.clone();
                view! {
                    <button class="refresh-btn" on:click=move |_| send(Action::Fired { token: token.clone() })>"↻"</button>
                }
            });
            let refresh_bar = refreshing.then(|| {
                view! { <div class="progress progress-indeterminate"><div class="progress-bar"></div></div> }
            });
            let body_class = format!("scaffold-body {}", nav_class(route, *depth));
            // An app `Theme` overrides the CSS variables inline (brand color, corner, density,
            // font) — the web twin of the native shells' brand/tint + shape + spacing + font.
            let theme_style = theme.as_ref().map(theme_css).unwrap_or_default();
            let (title, body) = (title.clone(), render(body, send));
            view! {
                <div class=class style=theme_style>
                    <div class="topbar">
                        {back_btn}
                        <span class="title">{title}</span>
                        {refresh_btn}
                    </div>
                    <div class=body_class data-route=route.clone()>{refresh_bar}{body}</div>
                    {fab_btn}
                    {tabbar}
                    {sheet_overlay}
                </div>
            }
            .into_any()
        }
    }
}

/// Render a slice of children as sibling views.
fn render_all(children: &[Widget], send: &Dispatch) -> Vec<AnyView> {
    children.iter().map(|c| render(c, send)).collect()
}

thread_local! {
    /// (previous route key, previous depth, alternating toggle). The render is a
    /// stateless whole-tree rebuild, so nav state lives here (wasm is single-
    /// threaded). Lets the Scaffold body animate on navigation — the web twin of
    /// the native shells keying their body on `route`.
    static NAV: RefCell<(String, u32, bool)> = const { RefCell::new((String::new(), 0, false)) };

    /// Open streaming subscriptions keyed by subscription key (wasm is single-
    /// threaded). Each [`Effect::PluginStream`] parks its source here so
    /// `cx.unsubscribe(key)` can stop it; dropping the entry stops the source.
    static STREAMS: RefCell<HashMap<String, StreamHandle>> = RefCell::new(HashMap::new());
}

/// Render an app [`Theme`] as inline CSS custom properties on the scaffold root — the web
/// twin of the native brand/tint + shape + spacing + font. Overrides `mobiler.css`'s defaults
/// (its rules read these via `var(--…)`); dark mode still works (it only swaps the colors the
/// seed doesn't pin).
fn theme_css(t: &Theme) -> String {
    let (r, g, b) = (t.seed.r, t.seed.g, t.seed.b);
    let radius = match t.corner {
        Corner::None => "0px",
        Corner::Small => "8px",
        Corner::Medium => "14px",
        Corner::Large => "22px",
    };
    let (gap, pad) = match t.density {
        Density::Compact => ("8px", "10px"),
        Density::Comfortable => ("12px", "14px"),
    };
    let font = match t.font {
        FontFamily::System => "system-ui, -apple-system, \"Segoe UI\", Roboto, sans-serif",
        FontFamily::Rounded => "ui-rounded, \"SF Pro Rounded\", \"Segoe UI\", system-ui, sans-serif",
        FontFamily::Serif => "ui-serif, Georgia, \"Times New Roman\", serif",
        FontFamily::Monospace => "ui-monospace, \"SF Mono\", \"Cascadia Code\", Menlo, monospace",
    };
    // Secondary brand color (for the CardStyle::Brand gradient); falls back to the seed.
    let (ar, ag, ab) = t.accent.map_or((r, g, b), |a| (a.r, a.g, a.b));
    format!(
        "--primary:rgb({r},{g},{b});--accent:rgb({r},{g},{b});\
         --accent2:rgb({ar},{ag},{ab});\
         --accent-soft:rgba({r},{g},{b},0.16);--radius:{radius};\
         --gap:{gap};--pad:{pad};--font:{font};"
    )
}

/// Pick the Scaffold body's transition class for this render. Returns `""` for a
/// same-route data update (re-render in place, no transition). On a route change it
/// returns a directional class — slide-in from the right when `depth` grew (push),
/// from the left when it shrank (pop), a crossfade for a lateral move — and *alternates*
/// the `-a`/`-b` suffix each navigation so the CSS animation restarts even though
/// Leptos reuses the same DOM node.
fn nav_class(route: &str, depth: u32) -> &'static str {
    NAV.with_borrow_mut(|(prev_route, prev_depth, toggle)| {
        if route == prev_route {
            return "";
        }
        let dir = if depth > *prev_depth {
            ["nav-push-a", "nav-push-b"]
        } else if depth < *prev_depth {
            ["nav-pop-a", "nav-pop-b"]
        } else {
            ["nav-fade-a", "nav-fade-b"]
        };
        *toggle = !*toggle;
        *prev_route = route.to_string();
        *prev_depth = depth;
        dir[usize::from(*toggle)]
    })
}

// ---- style intent → CSS class / glyph (the only place that names the look) ----

fn text_class(s: TextStyle) -> &'static str {
    match s {
        TextStyle::Title => "t-title",
        TextStyle::Subtitle => "t-subtitle",
        TextStyle::Caption => "t-caption",
        TextStyle::Emphasis => "t-emphasis",
        TextStyle::Body => "t-body",
    }
}

fn button_class(s: ButtonStyle) -> &'static str {
    match s {
        ButtonStyle::Filled => "btn-filled",
        ButtonStyle::Outlined => "btn-outlined",
        ButtonStyle::Text => "btn-text",
    }
}

fn card_class(s: CardStyle) -> &'static str {
    match s {
        CardStyle::Elevated => "card-elevated",
        CardStyle::Outlined => "card-outlined",
        CardStyle::Filled => "card-filled",
        CardStyle::Brand => "card-brand",
    }
}

fn a11y_role_aria(role: A11yRole) -> &'static str {
    match role {
        A11yRole::Button => "button",
        A11yRole::Link => "link",
        A11yRole::Image => "img",
        A11yRole::Header => "heading",
        A11yRole::Adjustable => "slider",
    }
}

fn tone_class(t: Tone) -> &'static str {
    match t {
        Tone::Neutral => "tone-neutral",
        Tone::Success => "tone-success",
        Tone::Warning => "tone-warning",
        Tone::Danger => "tone-danger",
        Tone::Info => "tone-info",
    }
}

fn spacer_class(s: Spacing) -> &'static str {
    match s {
        Spacing::Xs => "sp-xs",
        Spacing::Sm => "sp-sm",
        Spacing::Md => "sp-md",
        Spacing::Lg => "sp-lg",
        Spacing::Xl => "sp-xl",
    }
}

fn icon_glyph(i: Icon) -> &'static str {
    match i {
        Icon::Delete => "🗑",
        Icon::Add => "＋",
        Icon::Edit => "✏️",
        Icon::Close => "✕",
        Icon::Settings => "⚙",
        Icon::Check => "✓",
        Icon::Star => "★",
        Icon::Info => "ℹ",
        Icon::Home => "⌂",
        Icon::Search => "🔍",
        Icon::Menu => "☰",
        Icon::Filter => "⚟",
        Icon::Back => "‹",
        Icon::Forward => "›",
        Icon::Down => "⌄",
        Icon::Bell => "🔔",
        Icon::Cart => "🛒",
        Icon::Share => "↗",
        Icon::Heart => "♡",
        Icon::HeartFilled => "♥",
        Icon::Person => "👤",
        Icon::People => "👥",
        Icon::Phone => "📞",
        Icon::Mail => "✉",
        Icon::Calendar => "📅",
        Icon::Clock => "🕑",
        Icon::MapPin => "📍",
        Icon::Camera => "📷",
        Icon::Photo => "🖼",
        Icon::Play => "▶",
        Icon::Scissors => "✂",
    }
}

fn image_class(shape: ImageShape, ratio: ImageRatio) -> String {
    let shape = match shape {
        ImageShape::Square => "img-square",
        ImageShape::Rounded => "img-rounded",
        ImageShape::Circle => "img-circle",
    };
    let ratio = match ratio {
        ImageRatio::Wide => "ratio-wide",
        ImageRatio::Square => "ratio-square",
        ImageRatio::Tall => "ratio-tall",
    };
    format!("img {shape} {ratio}")
}

fn dot_class(c: ProjectColor) -> &'static str {
    match c {
        ProjectColor::Indigo => "dot-indigo",
        ProjectColor::Teal => "dot-teal",
        ProjectColor::Coral => "dot-coral",
        ProjectColor::Amber => "dot-amber",
        ProjectColor::Lime => "dot-lime",
        ProjectColor::Pink => "dot-pink",
    }
}

fn align_class(a: BoxAlign) -> &'static str {
    match a {
        BoxAlign::TopStart => "align-top-start",
        BoxAlign::TopEnd => "align-top-end",
        BoxAlign::Center => "align-center",
        BoxAlign::BottomStart => "align-bottom-start",
        BoxAlign::BottomCenter => "align-bottom-center",
        BoxAlign::BottomEnd => "align-bottom-end",
    }
}

// ------------------------------- charts -------------------------------

/// Distinct fallback colors for series 1.. (series 0 with no override rides the theme accent).
const CHART_PALETTE: [&str; 6] = ["#E0772C", "#2EA06A", "#C0466B", "#8A5CC0", "#C9A227", "#3FA7D6"];

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Color for series `i`: explicit override → theme accent (i==0) → palette.
fn chart_color(i: usize, s: &ChartSeries) -> String {
    match s.color {
        Some(c) => hex(c),
        None if i == 0 => "var(--accent, #5C6BC0)".to_string(),
        None => CHART_PALETTE[(i - 1) % CHART_PALETTE.len()].to_string(),
    }
}

/// A series' single magnitude for circular charts (sum of its values).
fn chart_mag(s: &ChartSeries) -> f32 {
    s.values.iter().copied().sum()
}

/// Point on a circle: `ang` in radians, 0 = top (12 o'clock), increasing clockwise.
fn polar(cx: f32, cy: f32, r: f32, ang: f32) -> (f32, f32) {
    (cx + r * ang.sin(), cy - r * ang.cos())
}

/// An open arc path (for ring/donut/gauge strokes).
fn arc_path(cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> String {
    let (x0, y0) = polar(cx, cy, r, a0);
    let (x1, y1) = polar(cx, cy, r, a1);
    let large = if (a1 - a0).abs() > std::f32::consts::PI { 1 } else { 0 };
    format!("M {x0:.2} {y0:.2} A {r:.2} {r:.2} 0 {large} 1 {x1:.2} {y1:.2}")
}

/// A filled wedge from the center (for pie/donut slices).
fn wedge_path(cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> String {
    let (x0, y0) = polar(cx, cy, r, a0);
    let (x1, y1) = polar(cx, cy, r, a1);
    let large = if (a1 - a0).abs() > std::f32::consts::PI { 1 } else { 0 };
    format!("M {cx:.2} {cy:.2} L {x0:.2} {y0:.2} A {r:.2} {r:.2} 0 {large} 1 {x1:.2} {y1:.2} Z")
}

fn fmt_tick(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

fn is_cartesian(style: ChartStyle) -> bool {
    matches!(style, ChartStyle::Bar | ChartStyle::Line | ChartStyle::StackedBar | ChartStyle::StackedBar100)
}

/// The y-axis denominator for a cartesian chart.
fn cartesian_max(series: &[ChartSeries], style: ChartStyle, nslots: usize) -> f32 {
    match style {
        ChartStyle::StackedBar => (0..nslots)
            .map(|j| series.iter().map(|s| *s.values.get(j).unwrap_or(&0.0)).sum::<f32>())
            .fold(0.0, f32::max)
            .max(1e-6),
        ChartStyle::StackedBar100 => 1.0,
        _ => series.iter().flat_map(|s| s.values.iter().copied()).fold(0.0, f32::max).max(1e-6),
    }
}

fn cartesian_svg(series: &[ChartSeries], style: ChartStyle, axis: bool, max: f32, nslots: usize) -> AnyView {
    // plot area: y in [2, 48] of the 0..50 viewBox
    let mut nodes: Vec<AnyView> = Vec::new();
    if axis {
        for k in 0..=4 {
            let y = 2.0 + k as f32 * (46.0 / 4.0);
            nodes.push(view! { <line x1="0" y1=format!("{y:.2}") x2="100" y2=format!("{y:.2}") class="chart-gridline"></line> }.into_any());
        }
    }
    match style {
        ChartStyle::Line => {
            for (i, s) in series.iter().enumerate() {
                let n = s.values.len().max(1);
                let pts = s.values.iter().enumerate().map(|(j, v)| {
                    let x = if n == 1 { 50.0 } else { j as f32 * (100.0 / (n as f32 - 1.0)) };
                    let y = 2.0 + (1.0 - (v / max).clamp(0.0, 1.0)) * 46.0;
                    format!("{x:.2},{y:.2}")
                }).collect::<Vec<_>>().join(" ");
                let st = format!("fill:none;stroke:{};stroke-width:1.5;vector-effect:non-scaling-stroke", chart_color(i, s));
                nodes.push(view! { <polyline points=pts style=st></polyline> }.into_any());
            }
        }
        ChartStyle::Bar => {
            let sw = 100.0 / nslots as f32;
            let ns = series.len().max(1);
            for (i, s) in series.iter().enumerate() {
                let st = format!("fill:{}", chart_color(i, s));
                for (j, v) in s.values.iter().enumerate() {
                    let h = (v / max).clamp(0.0, 1.0) * 46.0;
                    let bw = sw * 0.8 / ns as f32;
                    let x = j as f32 * sw + sw * 0.1 + i as f32 * bw;
                    let y = 48.0 - h;
                    nodes.push(view! { <rect x=format!("{x:.2}") y=format!("{y:.2}") width=format!("{bw:.2}") height=format!("{h:.2}") style=st.clone()></rect> }.into_any());
                }
            }
        }
        ChartStyle::StackedBar | ChartStyle::StackedBar100 => {
            let sw = 100.0 / nslots as f32;
            for j in 0..nslots {
                let slot_total = series.iter().map(|s| *s.values.get(j).unwrap_or(&0.0)).sum::<f32>().max(1e-6);
                let denom = if matches!(style, ChartStyle::StackedBar100) { slot_total } else { max };
                let mut acc = 0.0_f32;
                for (i, s) in series.iter().enumerate() {
                    let v = *s.values.get(j).unwrap_or(&0.0);
                    let h = (v / denom).clamp(0.0, 1.0) * 46.0;
                    let x = j as f32 * sw + sw * 0.15;
                    let bw = sw * 0.7;
                    let y = 48.0 - acc - h;
                    let st = format!("fill:{}", chart_color(i, s));
                    nodes.push(view! { <rect x=format!("{x:.2}") y=format!("{y:.2}") width=format!("{bw:.2}") height=format!("{h:.2}") style=st></rect> }.into_any());
                    acc += h;
                }
            }
        }
        _ => {}
    }
    view! { <svg viewBox="0 0 100 50" preserveAspectRatio="none" class="chart-svg">{nodes}</svg> }.into_any()
}

fn circular_svg(series: &[ChartSeries], style: ChartStyle) -> AnyView {
    use std::f32::consts::PI;
    let mut nodes: Vec<AnyView> = Vec::new();
    match style {
        ChartStyle::Pie | ChartStyle::Donut => {
            let total = series.iter().map(chart_mag).sum::<f32>().max(1e-6);
            let mut a = 0.0_f32;
            for (i, s) in series.iter().enumerate() {
                let frac = chart_mag(s) / total;
                let st = format!("fill:{}", chart_color(i, s));
                if frac >= 0.999 {
                    nodes.push(view! { <circle cx="50" cy="50" r="45" style=st></circle> }.into_any());
                } else if frac > 0.0 {
                    let d = wedge_path(50.0, 50.0, 45.0, a, a + frac * 2.0 * PI);
                    nodes.push(view! { <path d=d style=st></path> }.into_any());
                }
                a += frac * 2.0 * PI;
            }
            if matches!(style, ChartStyle::Donut) {
                nodes.push(view! { <circle cx="50" cy="50" r="24" style="fill:var(--surface, #ffffff)"></circle> }.into_any());
            }
        }
        ChartStyle::Rings => {
            let n = series.len().max(1);
            for (i, s) in series.iter().enumerate() {
                let r = 45.0 - i as f32 * (34.0 / n as f32);
                let goal = s.goal.unwrap_or_else(|| chart_mag(s)).max(1e-6);
                let prog = (chart_mag(s) / goal).clamp(0.0, 1.0);
                nodes.push(view! { <circle cx="50" cy="50" r=format!("{r:.2}") style="fill:none;stroke:var(--border, #e6e6e6);stroke-width:6"></circle> }.into_any());
                let st = format!("fill:none;stroke:{};stroke-width:6;stroke-linecap:round", chart_color(i, s));
                if prog >= 0.999 {
                    nodes.push(view! { <circle cx="50" cy="50" r=format!("{r:.2}") style=st></circle> }.into_any());
                } else if prog > 0.0 {
                    let d = arc_path(50.0, 50.0, r, 0.0, prog * 2.0 * PI);
                    nodes.push(view! { <path d=d style=st></path> }.into_any());
                }
            }
        }
        ChartStyle::Gauge => {
            let s = match series.first() { Some(s) => s, None => return view! { <svg viewBox="0 0 100 100" class="chart-svg"></svg> }.into_any() };
            let goal = s.goal.unwrap_or_else(|| chart_mag(s)).max(1e-6);
            let prog = (chart_mag(s) / goal).clamp(0.0, 1.0);
            let a0 = -0.75 * PI; // 270° sweep, gap at the bottom
            let a1 = 0.75 * PI;
            nodes.push(view! { <path d=arc_path(50.0, 50.0, 42.0, a0, a1) style="fill:none;stroke:var(--border, #e6e6e6);stroke-width:8;stroke-linecap:round"></path> }.into_any());
            if prog > 0.0 {
                let st = format!("fill:none;stroke:{};stroke-width:8;stroke-linecap:round", chart_color(0, s));
                nodes.push(view! { <path d=arc_path(50.0, 50.0, 42.0, a0, a0 + prog * 1.5 * PI) style=st></path> }.into_any());
            }
            let pct = format!("{}%", (prog * 100.0).round() as i64);
            nodes.push(view! { <text x="50" y="56" style="fill:var(--fg, #222);font-size:20px;font-weight:700;text-anchor:middle">{pct}</text> }.into_any());
        }
        _ => {}
    }
    view! { <svg viewBox="0 0 100 100" preserveAspectRatio="xMidYMid meet" class="chart-svg">{nodes}</svg> }.into_any()
}

fn chart_view(series: &[ChartSeries], labels: &[String], style: ChartStyle, axis: bool, legend: bool) -> AnyView {
    let cartesian = is_cartesian(style);
    let nslots = series.iter().map(|s| s.values.len()).max().unwrap_or(0).max(1);
    let max = cartesian_max(series, style, nslots);

    let plot = if cartesian {
        let svg = cartesian_svg(series, style, axis, max, nslots);
        let yaxis = if axis {
            let ticks: Vec<_> = [max, max / 2.0, 0.0].iter()
                .map(|t| view! { <span class="chart-tick">{fmt_tick(*t)}</span> })
                .collect();
            Some(view! { <div class="chart-yaxis">{ticks}</div> })
        } else {
            None
        };
        view! { <div class="chart-plot">{yaxis}{svg}</div> }.into_any()
    } else {
        circular_svg(series, style).into_any()
    };

    let label_row = if cartesian && !labels.is_empty() {
        let items: Vec<_> = labels.iter().map(|l| view! { <span class="chart-label">{l.clone()}</span> }).collect();
        Some(view! { <div class="chart-labels">{items}</div> })
    } else {
        None
    };

    let legend_row = if legend {
        let items: Vec<_> = series.iter().enumerate().map(|(i, s)| {
            let sw = format!("background:{}", chart_color(i, s));
            let name = s.name.clone();
            view! { <span class="chart-legend-item"><span class="chart-swatch" style=sw></span>{name}</span> }
        }).collect();
        Some(view! { <div class="chart-legend">{items}</div> })
    } else {
        None
    };

    view! { <div class="chart">{plot}{label_row}{legend_row}</div> }.into_any()
}

// --------------------------- region chart ---------------------------

/// Palette as RGB (parallel to `CHART_PALETTE`) so region charts can compute label contrast.
const CHART_PALETTE_RGB: [(u8, u8, u8); 6] =
    [(0xE0, 0x77, 0x2C), (0x2E, 0xA0, 0x6A), (0xC0, 0x46, 0x6B), (0x8A, 0x5C, 0xC0), (0xC9, 0xA2, 0x27), (0x3F, 0xA7, 0xD6)];

/// The resolved fill RGB for region `i` (explicit override → palette).
fn region_rgb(i: usize, r: &ChartRegion) -> (u8, u8, u8) {
    match r.color {
        Some(c) => (c.r, c.g, c.b),
        None => CHART_PALETTE_RGB[i % CHART_PALETTE_RGB.len()],
    }
}

/// Black or white label text, whichever reads on the given fill (perceived luminance).
fn contrast_text((r, g, b): (u8, u8, u8)) -> &'static str {
    let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if lum > 140.0 { "#1a1a1a" } else { "#f5f5f5" }
}

fn region_color(i: usize, r: &ChartRegion) -> String {
    let (r8, g8, b8) = region_rgb(i, r);
    format!("#{r8:02x}{g8:02x}{b8:02x}")
}

// A variable-width stacked-region / coverage-gap chart: absolute-positioned region rectangles in
// the [0,x_max]×[0,y_max] plane, horizontal ref lines + chips, an irregular x-axis, an optional
// right-side bracket, and a legend. The web twin of the Compose/SwiftUI RegionChart renderers.
fn region_chart_view(
    regions: &[ChartRegion],
    ticks: &[ChartTick],
    x_max: f32,
    y_max: f32,
    ref_lines: &[ChartRefLine],
    bracket: &Option<ChartBracket>,
    legend: &[ChartLegendItem],
) -> AnyView {
    let xm = x_max.max(1e-6);
    let ym = y_max.max(1e-6);

    let region_divs: Vec<_> = regions.iter().enumerate().map(|(i, r)| {
        let left = (r.x0 / xm * 100.0).clamp(0.0, 100.0);
        let width = ((r.x1 - r.x0) / xm * 100.0).clamp(0.0, 100.0);
        let bottom = (r.y0 / ym * 100.0).clamp(0.0, 100.0);
        let height = ((r.y1 - r.y0) / ym * 100.0).clamp(0.0, 100.0);
        let style = format!("left:{left:.3}%;width:{width:.3}%;bottom:{bottom:.3}%;height:{height:.3}%;background:{}", region_color(i, r));
        let label_class = if r.vertical { "rchart-label rchart-label-v" } else { "rchart-label" };
        let label_style = format!("color:{}", contrast_text(region_rgb(i, r)));
        let label = r.label.clone();
        view! { <div class="rchart-region" style=style><span class=label_class style=label_style>{label}</span></div> }
    }).collect();

    // The reference lines span the full plot width; their value chips sit in the right margin
    // (outside the plot), like the original — so the line clearly runs to the plot's edge.
    let ref_line_divs: Vec<_> = ref_lines.iter().map(|rl| {
        let style = format!("bottom:{:.3}%", (rl.value / ym * 100.0).clamp(0.0, 100.0));
        let cls = if rl.dashed { "rchart-refline rchart-refline-dashed" } else { "rchart-refline" };
        view! { <div class=cls style=style></div> }
    }).collect();
    let chip_divs: Vec<_> = ref_lines.iter().map(|rl| {
        let style = format!("bottom:{:.3}%", (rl.value / ym * 100.0).clamp(0.0, 100.0));
        let label = rl.label.clone();
        view! { <div class="rchart-chip" style=style>{label}</div> }
    }).collect();

    let bracket_div = bracket.as_ref().map(|b| {
        let bottom = (b.y0 / ym * 100.0).clamp(0.0, 100.0);
        let height = ((b.y1 - b.y0) / ym * 100.0).clamp(0.0, 100.0);
        let style = format!("bottom:{bottom:.3}%;height:{height:.3}%");
        let label = if b.info { format!("ⓘ\n{}", b.label) } else { b.label.clone() };
        view! { <div class="rchart-bracket" style=style><span>{label}</span></div> }
    });

    let yticks: Vec<_> = (0..=4).rev().map(|k| {
        let v = ym * k as f32 / 4.0;
        view! { <span class="chart-tick">{fmt_tick(v)}</span> }
    }).collect();

    let xticks: Vec<_> = ticks.iter().map(|t| {
        let style = format!("left:{:.3}%", (t.at / xm * 100.0).clamp(0.0, 100.0));
        let label = t.label.clone();
        view! { <span class="rchart-xtick" style=style>{label}</span> }
    }).collect();

    // Axis tick marks (notches on the L-shaped axis): horizontal on the y-axis at each value,
    // vertical on the x-axis at each irregular break — drawn over the bands at the plot edges.
    let ytick_marks: Vec<_> = (0..=4).map(|k| {
        let style = format!("bottom:{:.3}%", k as f32 * 25.0);
        view! { <div class="rchart-ytick" style=style></div> }
    }).collect();
    let xtick_marks: Vec<_> = ticks.iter().map(|t| {
        let style = format!("left:{:.3}%", (t.at / xm * 100.0).clamp(0.0, 100.0));
        view! { <div class="rchart-xtickmark" style=style></div> }
    }).collect();

    let legend_row = if legend.is_empty() {
        None
    } else {
        let items: Vec<_> = legend.iter().map(|l| {
            let sw = format!("background:{}", hex(l.color));
            let name = l.label.clone();
            view! { <span class="chart-legend-item"><span class="chart-swatch" style=sw></span>{name}</span> }
        }).collect();
        Some(view! { <div class="chart-legend">{items}</div> })
    };

    view! {
        <div class="rchart">
            <div class="rchart-row">
                <div class="rchart-yaxis">{yticks}</div>
                <div class="rchart-plotwrap">
                    <div class="rchart-plot">{region_divs}{ytick_marks}{xtick_marks}{ref_line_divs}</div>
                    {chip_divs}{bracket_div}
                </div>
            </div>
            <div class="rchart-xaxis">{xticks}</div>
            {legend_row}
        </div>
    }.into_any()
}
