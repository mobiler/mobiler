//! The notes app core: one `MobilerApp` (logic + UI in Rust) that the stock shells render
//! on mobile and web. It reaches the Axum/SQLx server over HTTP through `cx`.

use domain::{NewNote, Note};
use mobiler_core::{
    ButtonStyle, CardStyle, Cx, InputValue, MobilerApp, MobilerShell, Widget, button, caption,
    card, column, emphasis, scaffold, text, text_field, title,
};
use serde::{Deserialize, Serialize};

/// The notes API base. Point this at your deployed server for a real build.
const API: &str = "http://127.0.0.1:3000";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Refresh,
    GotNotes(String), // JSON body of Vec<Note>
    Add,
    Added(String), // JSON body of the created Note
    Delete(i64),
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
        cx.get(format!("{API}/notes"), |r| Msg::GotNotes(if r.ok { r.output } else { String::new() }));
    }

    fn update(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::Refresh => {
                cx.get(format!("{API}/notes"), |r| Msg::GotNotes(if r.ok { r.output } else { String::new() }));
            }
            Msg::GotNotes(body) => match serde_json::from_str::<Vec<Note>>(&body) {
                Ok(notes) => {
                    model.notes = notes;
                    model.error = None;
                }
                Err(_) if body.is_empty() => model.error = Some("could not reach the server".into()),
                Err(e) => model.error = Some(format!("bad response: {e}")),
            },
            Msg::Add => {
                let t = model.draft_title.trim();
                if !t.is_empty() {
                    let new = NewNote { title: t.to_string(), body: model.draft_body.trim().to_string() };
                    let payload = serde_json::to_string(&new).unwrap_or_default();
                    cx.post(format!("{API}/notes"), payload, |r| {
                        Msg::Added(if r.ok { r.output } else { String::new() })
                    });
                }
            }
            Msg::Added(body) => {
                if let Ok(note) = serde_json::from_str::<Note>(&body) {
                    model.notes.push(note);
                    model.draft_title.clear();
                    model.draft_body.clear();
                }
            }
            Msg::Delete(id) => {
                model.notes.retain(|n| n.id != id); // optimistic
                cx.delete(format!("{API}/notes/{id}"), |_| Msg::Refresh);
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
        scaffold("Notes", false, vec![], column(items))
    }
}

/// What the shells render — web (here) and mobile (via `mobiler new`).
pub type App = MobilerShell<Notes>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn got_notes_parses_then_handles_unreachable() {
        let app = Notes;
        let mut m = Model::default();
        // happy: a JSON array fills the list
        let json = serde_json::to_string(&vec![Note { id: 1, title: "a".into(), body: "b".into() }]).unwrap();
        app.update(Msg::GotNotes(json), &mut m, &mut Cx::default());
        assert_eq!(m.notes.len(), 1);
        assert!(m.error.is_none());
        // sad: an empty body (server unreachable) → error set, prior notes kept
        app.update(Msg::GotNotes(String::new()), &mut m, &mut Cx::default());
        assert!(m.error.is_some());
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
}
