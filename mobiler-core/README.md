# mobiler-core

> Mobiler's runtime — the developer-facing API.

Implement **`MobilerApp`** with your typed events, model, and a `view` built from the
widget [builders](https://docs.rs/mobiler-core). Mobiler wraps it in **`MobilerShell`**,
a [Crux](https://github.com/redbadger/crux) app speaking the fixed UI ABI
([`mobiler-ui`](https://crates.io/crates/mobiler-ui)) — so the native shell stays
generic and you never touch the wire protocol.

```rust
impl MobilerApp for Counter {
    type Event = Msg;
    type Model = Model;

    fn update(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::Increment => model.count += 1,
            Msg::Greet => cx.notify("toast", "show", "Hi from Rust!"),
        }
    }

    fn view(&self, model: &Model) -> Widget {
        column(vec![
            title("Counter"),
            text(format!("count: {}", model.count)),
            button("Increment", ButtonStyle::Filled, Msg::Increment),
        ])
    }
}

pub type App = MobilerShell<Counter>;
```

- **Capabilities** via `Cx` — device APIs as async effects (`cx.notify` for
  fire-and-forget, `cx.plugin`/`cx.http`/`cx.get`/… for request-response,
  `cx.save` for persistence).
- **Navigation** — a core-owned `Nav<Route>` stack + `nav_scaffold`.
- **Theme-as-data** — dark mode etc. flow through the `Widget` tree.

Most users go through the [`mobiler`](https://crates.io/crates/mobiler) CLI, which
scaffolds a project wired to this crate and a generic native shell.

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
