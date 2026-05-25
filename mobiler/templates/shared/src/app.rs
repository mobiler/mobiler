use crux_core::{
    App, Command,
    macros::effect,
    render::{RenderOperation, render},
};
use facet::Facet;
use serde::{Deserialize, Serialize};

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Event {
    Increment,
    DialogDismissed,
}

#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
}

#[derive(Default)]
pub struct Model {
    count: i32,
    dialog_message: Option<String>,
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Widget {
    Text { content: String },
    Button { label: String, on_press: Event },
    Column { children: Vec<Widget> },
    AlertDialog { message: String, on_dismiss: Event },
}

pub type ViewModel = Widget;

#[derive(Default)]
pub struct {{NAME}}App;

impl App for {{NAME}}App {
    type Event = Event;
    type Model = Model;
    type ViewModel = ViewModel;
    type Effect = Effect;

    fn update(&self, event: Event, model: &mut Model) -> Command<Effect, Event> {
        match event {
            Event::Increment => {
                model.count += 1;
                model.dialog_message = Some("confirmed".to_string());
            }
            Event::DialogDismissed => {
                model.dialog_message = None;
            }
        }
        render()
    }

    fn view(&self, model: &Model) -> ViewModel {
        let mut children = vec![
            Widget::Text {
                content: format!("Count: {}", model.count),
            },
            Widget::Button {
                label: "Increment".to_string(),
                on_press: Event::Increment,
            },
        ];
        if let Some(msg) = &model.dialog_message {
            children.push(Widget::AlertDialog {
                message: msg.clone(),
                on_dismiss: Event::DialogDismissed,
            });
        }
        Widget::Column { children }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn increment_shows_dialog() {
        let app = {{NAME}}App;
        let mut model = Model::default();
        app.update(Event::Increment, &mut model).expect_only_render();
        assert_eq!(model.count, 1);
        assert_eq!(model.dialog_message.as_deref(), Some("confirmed"));
    }

    #[test]
    fn dismiss_clears_dialog_keeps_count() {
        let app = {{NAME}}App;
        let mut model = Model::default();
        app.update(Event::Increment, &mut model);
        app.update(Event::DialogDismissed, &mut model).expect_only_render();
        assert_eq!(model.count, 1);
        assert!(model.dialog_message.is_none());
    }
}
