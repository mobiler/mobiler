//! The notes app core: one `MobilerApp` (logic + UI in Rust) that the stock shells render
//! on mobile and web. It reaches the Axum/SQLx server over HTTP through `cx`.

use domain::{NewNote, Note};
use mobiler_core::{
    ButtonStyle, CardStyle, Cx, HttpOutcome, InputValue, MobilerApp, MobilerShell, Widget, button,
    caption, card, column, emphasis, text, text_field, title,
};
use serde::{Deserialize, Serialize};

/// The notes API base. Point this at your deployed server for a real build.
const API: &str = "http://127.0.0.1:3000";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Refresh,
    GotNotes(String), // JSON body of Vec<Note>, only sent on a 2xx response
    Add,
    Added(String), // JSON body of the created Note, only sent on a 2xx response
    Failed(String),
    /// A failed *optimistic* delete: the note was already removed from `model.notes`
    /// before the request went out, so unlike a plain [`Msg::Failed`] this must
    /// reconcile the list with the server by refetching.
    DeleteFailed(String),
    Delete(i64),
}

/// A meaningful message for a non-success outcome: branches on the variant so a real
/// transport failure ("connection refused") is never rendered as the generic
/// "could not reach the server" guess a plain `.text()` call on an empty body used to
/// produce — TransportError and a non-2xx Response are distinguishable here.
fn err_of(resp: &HttpOutcome) -> String {
    match resp {
        HttpOutcome::TransportError { message } => format!("could not reach the server: {message}"),
        HttpOutcome::Response { status, .. } => {
            let body = resp.text().unwrap_or_default();
            if body.is_empty() {
                format!("server returned {status}")
            } else {
                format!("server returned {status}: {body}")
            }
        }
    }
}

#[derive(Default)]
pub struct Model {
    notes: Vec<Note>,
    draft_title: String,
    draft_body: String,
    error: Option<String>,
}

#[derive(Default)]
pub struct Notes;

impl MobilerApp for Notes {
    type Event = Msg;
    type Model = Model;

    fn init(&self, _model: &mut Model, cx: &mut Cx<Msg>) {
        cx.get(format!("{API}/notes"), |r| {
            // Restore the `if r.ok` gate the HttpOutcome migration dropped: only a 2xx
            // response carries a body worth parsing as JSON.
            if r.is_success() {
                Msg::GotNotes(r.text().unwrap_or_default().to_string())
            } else {
                Msg::Failed(err_of(&r))
            }
        });
    }

    fn update(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::Refresh => {
                cx.get(format!("{API}/notes"), |r| {
                    if r.is_success() {
                        Msg::GotNotes(r.text().unwrap_or_default().to_string())
                    } else {
                        Msg::Failed(err_of(&r))
                    }
                });
            }
            Msg::GotNotes(body) => match serde_json::from_str::<Vec<Note>>(&body) {
                Ok(notes) => {
                    model.notes = notes;
                    model.error = None;
                }
                Err(e) => model.error = Some(format!("bad response: {e}")),
            },
            Msg::Add => {
                let t = model.draft_title.trim();
                if !t.is_empty() {
                    let new = NewNote { title: t.to_string(), body: model.draft_body.trim().to_string() };
                    let payload = serde_json::to_string(&new).unwrap_or_default();
                    cx.post(format!("{API}/notes"), payload, |r| {
                        if r.is_success() {
                            Msg::Added(r.text().unwrap_or_default().to_string())
                        } else {
                            Msg::Failed(err_of(&r))
                        }
                    });
                }
            }
            Msg::Added(body) => {
                if let Ok(note) = serde_json::from_str::<Note>(&body) {
                    model.notes.push(note);
                    model.draft_title.clear();
                    model.draft_body.clear();
                    model.error = None;
                }
            }
            Msg::Failed(e) => {
                // A plain read (init/Refresh) or a non-optimistic mutation (Add) failed:
                // record the error and stop. There is nothing to reconcile — the list is
                // already whatever it was — and refetching here would turn a downed
                // server into an unbounded request loop (every failure re-triggers
                // Refresh, whose own failure re-triggers Refresh, ...).
                model.error = Some(e);
            }
            Msg::DeleteFailed(e) => {
                model.error = Some(e);
                // The delete was optimistic (see `Msg::Delete` below): the note is
                // already gone from `model.notes`, but the server-side delete failed, so
                // the in-memory list is now out of sync with the server. Reconcile by
                // refetching — same as a plain `Msg::Refresh`. Only this optimistic-
                // mutation failure needs that; see `Msg::Failed` above.
                self.update(Msg::Refresh, model, cx);
            }
            Msg::Delete(id) => {
                model.notes.retain(|n| n.id != id); // optimistic
                cx.delete(format!("{API}/notes/{id}"), |r| {
                    if r.is_success() { Msg::Refresh } else { Msg::DeleteFailed(err_of(&r)) }
                });
            }
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
        if let InputValue::Text(v) = value {
            match id {
                "title" => model.draft_title = v,
                "body" => model.draft_body = v,
                _ => {}
            }
        }
    }

    fn view(&self, model: &Model) -> Widget {
        let mut items = vec![
            title("Notes — Mobiler + Axum + SQLx"),
            text_field("title", "Title", model.draft_title.as_str()),
            text_field("body", "Body", model.draft_body.as_str()),
            button("Add note", ButtonStyle::Filled, Msg::Add),
        ];
        if let Some(e) = &model.error {
            items.push(caption(e.as_str()));
        }
        for n in &model.notes {
            items.push(card(
                column(vec![
                    emphasis(n.title.as_str()),
                    text(n.body.as_str()),
                    button("Delete", ButtonStyle::Text, Msg::Delete(n.id)),
                ]),
                CardStyle::Outlined,
            ));
        }
        // Deliberately NOT a Scaffold — keeps the shells' non-Scaffold root render path
        // (the scroll-container fallback) exercised. The in-body title above is the heading.
        column(items)
    }
}

/// What the shells render — web (here) and mobile (via `mobiler new`).
pub type App = MobilerShell<Notes>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn got_notes_parses_then_failed_handles_transport_error() {
        let app = Notes;
        let mut m = Model::default();
        // happy: a JSON array fills the list
        let json = serde_json::to_string(&vec![Note { id: 1, title: "a".into(), body: "b".into() }]).unwrap();
        app.update(Msg::GotNotes(json), &mut m, &mut Cx::default());
        assert_eq!(m.notes.len(), 1);
        assert!(m.error.is_none());
        // sad: a transport failure (server unreachable) surfaces its message, prior notes kept
        app.update(Msg::Failed(err_of(&HttpOutcome::TransportError { message: "connection refused".into() })), &mut m, &mut Cx::default());
        assert!(m.error.is_some());
        assert_eq!(m.error.as_deref(), Some("could not reach the server: connection refused"));
        assert_eq!(m.notes.len(), 1);
    }

    #[test]
    fn added_appends_and_clears_the_draft() {
        let app = Notes;
        let mut m = Model::default();
        m.draft_title = "x".into();
        m.draft_body = "y".into();
        let note = serde_json::to_string(&Note { id: 5, title: "x".into(), body: "y".into() }).unwrap();
        app.update(Msg::Added(note), &mut m, &mut Cx::default());
        assert_eq!(m.notes.len(), 1);
        assert!(m.draft_title.is_empty() && m.draft_body.is_empty());
    }

    #[test]
    fn delete_removes_the_note_optimistically() {
        let app = Notes;
        let mut m = Model::default();
        m.notes = vec![Note { id: 1, title: "a".into(), body: "".into() }, Note { id: 2, title: "b".into(), body: "".into() }];
        app.update(Msg::Delete(1), &mut m, &mut Cx::default());
        assert_eq!(m.notes.len(), 1);
        assert_eq!(m.notes[0].id, 2);
    }

    #[test]
    fn failed_delete_reports_the_error_and_reconciles_with_the_server() {
        let app = Notes;
        let mut m = Model::default();
        m.notes = vec![Note { id: 1, title: "a".into(), body: "".into() }];
        // The delete removed the note optimistically...
        app.update(Msg::Delete(1), &mut m, &mut Cx::default());
        assert!(m.notes.is_empty());
        // ...but the server-side delete failed: the error must be reported, and the
        // reconciling refetch restores the note (as if `Msg::Refresh` ran). Only the
        // optimistic-delete failure path (`Msg::DeleteFailed`) reconciles like this —
        // see `failed_read_does_not_trigger_a_second_refetch` below for the plain
        // `Msg::Failed` path, which must NOT do this.
        let mut cx = Cx::default();
        app.update(Msg::DeleteFailed("server returned 500".into()), &mut m, &mut cx);
        assert_eq!(m.error.as_deref(), Some("server returned 500"));
        app.update(Msg::GotNotes(serde_json::to_string(&vec![Note { id: 1, title: "a".into(), body: "".into() }]).unwrap()), &mut m, &mut cx);
        assert_eq!(m.notes.len(), 1, "the reconciling refresh must restore the note the server still has");
    }

    // ---- regression: a failed read must not loop ----
    //
    // `Msg::Failed` used to unconditionally re-issue `Msg::Refresh`, so every failure
    // path (including a plain failed read) fed back into another network request whose
    // own failure fed back into another, forever: with the server unreachable, init ->
    // Failed -> Refresh -> Failed -> ... never stops. These tests drive the app through
    // the real Crux ABI (`MobilerShell` as a `crux_core::App`) and count the *effects* a
    // single `update` call produces — the property that actually distinguishes "one
    // request" from "an unbounded retry loop" (a plain model-state assertion can't see
    // the difference, since both a fixed and a looping handler leave the same `model`
    // after just one step).

    use crux_core::App as _;
    use mobiler_core::{Action, Effect};

    /// How many outbound HTTP-request effects (`Effect::Plugin`) a `Command` carries,
    /// as opposed to bookkeeping effects like `Effect::Render` that every `update` call
    /// produces regardless of what happened.
    fn plugin_request_count(cmd: &mut crux_core::Command<Effect, Action>) -> usize {
        cmd.effects().filter(|e| matches!(e, Effect::Plugin(_))).count()
    }

    #[test]
    fn failed_read_does_not_trigger_a_second_refetch() {
        let shell = MobilerShell::<Notes>::default();
        let mut m = Model::default();

        // `init` issues exactly one GET.
        let mut start = shell.update(Action::Start, &mut m);
        assert_eq!(plugin_request_count(&mut start), 1, "init must issue exactly one request");

        // That GET fails and reaches `update` as `Msg::Failed` (the real path: the
        // shell resolves the `cx.get` continuation with a non-2xx/transport-error
        // outcome, which `init`'s closure turns into `Msg::Failed`).
        let token = serde_json::to_string(&Msg::Failed("could not reach the server: connection refused".into())).unwrap();
        let mut failed = shell.update(Action::Fired { token }, &mut m);

        // The bug: this used to be 1 (the recursive `Msg::Refresh`'s new GET), on top
        // of whatever `Render` also produces — an unbounded retry loop. Fixed: zero.
        assert_eq!(plugin_request_count(&mut failed), 0, "a failed read must not enqueue another request");
        assert_eq!(m.error.as_deref(), Some("could not reach the server: connection refused"));
    }

    #[test]
    fn failed_refresh_does_not_trigger_a_second_refetch() {
        let shell = MobilerShell::<Notes>::default();
        let mut m = Model::default();

        let token = serde_json::to_string(&Msg::Refresh).unwrap();
        let mut refresh = shell.update(Action::Fired { token }, &mut m);
        assert_eq!(plugin_request_count(&mut refresh), 1, "Refresh must issue exactly one request");

        let token = serde_json::to_string(&Msg::Failed("server returned 500".into())).unwrap();
        let mut failed = shell.update(Action::Fired { token }, &mut m);
        assert_eq!(plugin_request_count(&mut failed), 0, "a failed Refresh must not enqueue another request");
    }

    #[test]
    fn failed_optimistic_delete_still_reconciles_with_exactly_one_refetch() {
        // Unlike a plain failed read, `Msg::DeleteFailed` legitimately needs to
        // reconcile — but exactly once, not a loop.
        let shell = MobilerShell::<Notes>::default();
        let mut m = Model { notes: vec![Note { id: 1, title: "a".into(), body: "".into() }], ..Default::default() };

        let token = serde_json::to_string(&Msg::Delete(1)).unwrap();
        let mut delete = shell.update(Action::Fired { token }, &mut m);
        assert_eq!(plugin_request_count(&mut delete), 1, "Delete must issue exactly one request");

        let token = serde_json::to_string(&Msg::DeleteFailed("server returned 500".into())).unwrap();
        let mut failed = shell.update(Action::Fired { token }, &mut m);
        assert_eq!(plugin_request_count(&mut failed), 1, "a failed optimistic delete must reconcile with exactly one refetch");
    }

    #[test]
    fn err_of_distinguishes_transport_failure_from_bad_status() {
        let transport = HttpOutcome::TransportError { message: "connection refused".into() };
        assert_eq!(err_of(&transport), "could not reach the server: connection refused");

        let not_found = HttpOutcome::Response { status: 404, headers: vec![], body: b"missing".to_vec() };
        assert_eq!(err_of(&not_found), "server returned 404: missing");
    }
}
