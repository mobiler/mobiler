use crux_core::{
    App, Command,
    macros::effect,
    render::{RenderOperation, render},
};
use facet::Facet;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// Holds the most-recent bincode-serialized Model. Written by Counter::update after
/// every event, read by CoreFFI::export_state. One global is fine because we have
/// one CoreFFI per Activity; if that assumption ever breaks we'd move this into a
/// stateful Counter struct.
pub(crate) fn snapshot_buffer() -> &'static Mutex<Vec<u8>> {
    static BUF: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    BUF.get_or_init(|| Mutex::new(Vec::new()))
}

// ============================================================
// Domain
// ============================================================

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Tab {
    Today,
    Projects,
    Settings,
}

/// Project identity colors. Distinct from `Tone` (which is for status/feedback);
/// these are pure identity — assigned to projects so the user can tell them apart
/// at a glance. Concrete RGB values decided per platform in the render layer.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ProjectColor {
    Indigo,
    Teal,
    Coral,
    Amber,
    Lime,
    Pink,
}

impl ProjectColor {
    /// Default rotation order. The (N+1)th project gets `ALL[N % ALL.len()]`
    /// unless the user overrides via the picker.
    pub const ALL: [ProjectColor; 6] = [
        ProjectColor::Indigo,
        ProjectColor::Teal,
        ProjectColor::Coral,
        ProjectColor::Amber,
        ProjectColor::Lime,
        ProjectColor::Pink,
    ];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|c| *c == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

impl Default for ProjectColor {
    fn default() -> Self {
        ProjectColor::Indigo
    }
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct Project {
    pub id: u32,
    pub name: String,
    pub color: ProjectColor,
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct Task {
    pub id: u32,
    pub project_id: u32,
    pub text: String,
    pub done: bool,
    pub must_do_today: bool,
}

// ============================================================
// Events + Effects
// ============================================================

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Event {
    SelectTab(Tab),
    TextChanged { id: String, value: String },
    Toggled { id: String, value: bool },

    AddProject,
    OpenProject(u32),
    CloseProject,
    DeleteProject(u32),
    SelectProjectColor(ProjectColor),

    AddTask,
    ToggleTaskDone(u32),
    DeleteTask(u32),
    ToggleMustDoToday(u32),

    /// Internal: rehydrate the model from a previously-saved snapshot.
    /// The shell crafts this in `CoreFFI::import_state` and routes it through Bridge.
    /// Visible in the generated Kotlin Event sum, but never sent from there.
    LoadFromSnapshot(Vec<u8>),
}

#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
}

// ============================================================
// Model
// ============================================================

#[derive(Serialize, Deserialize)]
pub struct Model {
    active_tab: ActiveTab,
    name: String,
    dark_mode: bool,

    projects: Vec<Project>,
    tasks: Vec<Task>,

    /// When Some, the Projects tab is showing the detail view for this project.
    /// When None, the Projects tab shows the project list.
    active_project_id: Option<u32>,

    /// Ephemeral input state — not persisted. Cleared on cold start.
    #[serde(skip)]
    project_input: String,
    #[serde(skip)]
    task_input: String,

    /// Color selected for the next project the user will add. Cycles through
    /// `ProjectColor::ALL` automatically; user can tap a swatch to override.
    /// Persisted because users expect their "in-progress" picker selection to
    /// survive a force-stop.
    selected_new_project_color: ProjectColor,

    next_project_id: u32,
    next_task_id: u32,
}

impl Default for Model {
    fn default() -> Self {
        // Seed with two demo projects so the showcase has something to render
        // and there's an immediate sense of what the app is for.
        let projects = vec![
            Project { id: 1, name: "Home".to_string(), color: ProjectColor::Indigo },
            Project { id: 2, name: "Mobiler".to_string(), color: ProjectColor::Teal },
        ];
        let tasks = vec![
            Task { id: 1, project_id: 1, text: "Buy milk".to_string(), done: false, must_do_today: true },
            Task { id: 2, project_id: 1, text: "Cancel old subscription".to_string(), done: false, must_do_today: false },
            Task { id: 3, project_id: 2, text: "Polish styling vocab".to_string(), done: true, must_do_today: false },
            Task { id: 4, project_id: 2, text: "Write iOS Render".to_string(), done: false, must_do_today: false },
            Task { id: 5, project_id: 2, text: "Ship v0.2".to_string(), done: false, must_do_today: true },
        ];
        Self {
            active_tab: ActiveTab::default(),
            name: String::new(),
            dark_mode: false,
            projects,
            tasks,
            active_project_id: None,
            project_input: String::new(),
            task_input: String::new(),
            // Two seeded projects used Indigo + Teal; pre-select Coral so the user's
            // first hand-added project gets a fresh color by default.
            selected_new_project_color: ProjectColor::Coral,
            next_project_id: 3,
            next_task_id: 6,
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ActiveTab {
    #[default]
    Today,
    Projects,
    Settings,
}

impl From<ActiveTab> for Tab {
    fn from(t: ActiveTab) -> Self {
        match t {
            ActiveTab::Today => Tab::Today,
            ActiveTab::Projects => Tab::Projects,
            ActiveTab::Settings => Tab::Settings,
        }
    }
}

impl From<Tab> for ActiveTab {
    fn from(t: Tab) -> Self {
        match t {
            Tab::Today => ActiveTab::Today,
            Tab::Projects => ActiveTab::Projects,
            Tab::Settings => ActiveTab::Settings,
        }
    }
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct TabItem {
    pub label: String,
    pub key: Tab,
}

// ============================================================
// Styling primitives
// ============================================================

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum TextStyle {
    Body,
    Title,
    Subtitle,
    Caption,
    Emphasis,
}

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ButtonStyle {
    Filled,
    Outlined,
    Text,
}

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum CardStyle {
    Elevated,
    Outlined,
    Filled,
}

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Spacing {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Tone {
    Neutral,
    Success,
    Warning,
    Danger,
    Info,
}

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Icon {
    Delete,
    Add,
    Edit,
    Close,
    Settings,
    Check,
    Star,
    StarOutline,
}

// ============================================================
// Widget
// ============================================================

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Widget {
    Text { content: String, style: TextStyle },
    Spacer { size: Spacing },
    Divider,
    Row { children: Vec<Widget> },
    Column { children: Vec<Widget> },
    Card { child: Box<Widget>, style: CardStyle },
    Button { label: String, on_press: Event, style: ButtonStyle },
    IconButton { icon: Icon, on_press: Event },
    Badge { label: String, tone: Tone },
    /// Small non-interactive colored dot. Use for project identity hints in lists.
    ColorDot { color: ProjectColor },
    /// Tappable colored swatch used in pickers. Wider visual ring when `selected`.
    ColorSwatch { color: ProjectColor, selected: bool, on_press: Event },
    TextField {
        id: String,
        value: String,
        placeholder: String,
    },
    Switch {
        id: String,
        value: bool,
        label: String,
    },
    Checkbox {
        value: bool,
        label: String,
        on_change: Event,
    },
    Scaffold {
        title: String,
        /// When Some, the top bar shows a back arrow that fires this event.
        back_action: Option<Event>,
        body: Box<Widget>,
        bottom_tabs: Vec<TabItem>,
        active_tab: Tab,
        dark_mode: bool,
    },
}

pub type ViewModel = Widget;

// ============================================================
// App
// ============================================================

#[derive(Default)]
pub struct Counter;

impl App for Counter {
    type Event = Event;
    type Model = Model;
    type ViewModel = ViewModel;
    type Effect = Effect;

    fn update(&self, event: Event, model: &mut Model) -> Command<Effect, Event> {
        match event {
            Event::SelectTab(tab) => model.active_tab = tab.into(),
            Event::TextChanged { id, value } => match id.as_str() {
                "name" => model.name = value,
                "project_input" => model.project_input = value,
                "task_input" => model.task_input = value,
                _ => {}
            },
            Event::Toggled { id, value } => {
                if id == "dark_mode" {
                    model.dark_mode = value;
                }
            }

            Event::AddProject => {
                let name = model.project_input.trim().to_string();
                if !name.is_empty() {
                    let id = model.next_project_id;
                    model.next_project_id += 1;
                    let color = model.selected_new_project_color;
                    model.projects.push(Project { id, name, color });
                    model.project_input.clear();
                    // Advance the picker to the next color so the user's NEXT
                    // project gets a different default without an extra tap.
                    model.selected_new_project_color = color.next();
                }
            }
            Event::OpenProject(id) => model.active_project_id = Some(id),
            Event::CloseProject => {
                model.active_project_id = None;
                model.task_input.clear();
            }
            Event::DeleteProject(id) => {
                model.projects.retain(|p| p.id != id);
                model.tasks.retain(|t| t.project_id != id);
                if model.active_project_id == Some(id) {
                    model.active_project_id = None;
                }
            }
            Event::SelectProjectColor(c) => model.selected_new_project_color = c,

            Event::AddTask => {
                if let Some(project_id) = model.active_project_id {
                    let text = model.task_input.trim().to_string();
                    if !text.is_empty() {
                        let id = model.next_task_id;
                        model.next_task_id += 1;
                        model.tasks.push(Task {
                            id,
                            project_id,
                            text,
                            done: false,
                            must_do_today: false,
                        });
                        model.task_input.clear();
                    }
                }
            }
            Event::ToggleTaskDone(id) => {
                if let Some(t) = model.tasks.iter_mut().find(|t| t.id == id) {
                    t.done = !t.done;
                }
            }
            Event::DeleteTask(id) => {
                model.tasks.retain(|t| t.id != id);
            }
            Event::ToggleMustDoToday(id) => {
                if let Some(t) = model.tasks.iter_mut().find(|t| t.id == id) {
                    t.must_do_today = !t.must_do_today;
                }
            }

            Event::LoadFromSnapshot(bytes) => {
                if let Ok(loaded) = bincode::deserialize::<Model>(&bytes) {
                    *model = loaded;
                }
                // After loading, we still re-snapshot below so the static buffer
                // matches the (now-rehydrated) state.
            }
        }

        // Side-effect: snapshot the model to the static buffer for the shell to read.
        // Cheap (bincode + memcpy); only happens once per Event so the ~kilobytes of
        // model data don't hurt. Wrapped in best-effort: a serialization failure here
        // should not crash the app.
        if let Ok(bytes) = bincode::serialize(&*model) {
            *snapshot_buffer().lock().unwrap() = bytes;
        }

        render()
    }

    fn view(&self, model: &Model) -> ViewModel {
        let tabs = vec![
            TabItem { label: "Today".into(), key: Tab::Today },
            TabItem { label: "Projects".into(), key: Tab::Projects },
            TabItem { label: "Settings".into(), key: Tab::Settings },
        ];

        let (title, back_action, body) = match model.active_tab {
            ActiveTab::Today => ("Today".to_string(), None, today_screen(model)),
            ActiveTab::Projects => match model.active_project_id {
                Some(pid) => match model.projects.iter().find(|p| p.id == pid) {
                    Some(p) => (p.name.clone(), Some(Event::CloseProject), project_detail_screen(model, p)),
                    None => {
                        // Project disappeared; fall back to list.
                        ("Projects".to_string(), None, project_list_screen(model))
                    }
                },
                None => ("Projects".to_string(), None, project_list_screen(model)),
            },
            ActiveTab::Settings => ("Settings".to_string(), None, settings_screen(model)),
        };

        Widget::Scaffold {
            title,
            back_action,
            body: Box::new(body),
            bottom_tabs: tabs,
            active_tab: model.active_tab.into(),
            dark_mode: model.dark_mode,
        }
    }
}

// ============================================================
// View helpers
// ============================================================

fn text(content: impl Into<String>, style: TextStyle) -> Widget {
    Widget::Text { content: content.into(), style }
}
fn body(content: impl Into<String>) -> Widget { text(content, TextStyle::Body) }
fn title_text(content: impl Into<String>) -> Widget { text(content, TextStyle::Title) }
fn subtitle(content: impl Into<String>) -> Widget { text(content, TextStyle::Subtitle) }
fn caption(content: impl Into<String>) -> Widget { text(content, TextStyle::Caption) }

fn card(child: Widget) -> Widget { card_with(child, CardStyle::Elevated) }
fn outlined_card(child: Widget) -> Widget { card_with(child, CardStyle::Outlined) }
fn card_with(child: Widget, style: CardStyle) -> Widget {
    Widget::Card { child: Box::new(child), style }
}

fn column(children: Vec<Widget>) -> Widget { Widget::Column { children } }
fn row(children: Vec<Widget>) -> Widget { Widget::Row { children } }
fn spacer(size: Spacing) -> Widget { Widget::Spacer { size } }

fn filled_button(label: impl Into<String>, on_press: Event) -> Widget {
    Widget::Button { label: label.into(), on_press, style: ButtonStyle::Filled }
}
fn outlined_button(label: impl Into<String>, on_press: Event) -> Widget {
    Widget::Button { label: label.into(), on_press, style: ButtonStyle::Outlined }
}
fn icon_button(icon: Icon, on_press: Event) -> Widget {
    Widget::IconButton { icon, on_press }
}
fn badge(label: impl Into<String>, tone: Tone) -> Widget {
    Widget::Badge { label: label.into(), tone }
}

fn color_dot(color: ProjectColor) -> Widget {
    Widget::ColorDot { color }
}

fn color_swatch(color: ProjectColor, selected: bool) -> Widget {
    Widget::ColorSwatch { color, selected, on_press: Event::SelectProjectColor(color) }
}

// ============================================================
// Screens
// ============================================================

fn today_screen(model: &Model) -> Widget {
    let today_tasks: Vec<&Task> = model.tasks.iter().filter(|t| t.must_do_today).collect();
    let header_label = if model.name.trim().is_empty() {
        "Today's must-dos".to_string()
    } else {
        format!("{}, today's must-dos", model.name.trim())
    };
    let pending = today_tasks.iter().filter(|t| !t.done).count();

    let mut children = vec![title_text(header_label), spacer(Spacing::Md)];

    if today_tasks.is_empty() {
        children.push(outlined_card(column(vec![
            subtitle("Nothing scheduled for today"),
            spacer(Spacing::Xs),
            caption("Star tasks from the Projects tab to add them here."),
        ])));
    } else {
        for t in &today_tasks {
            children.push(card(today_task_row(model, t)));
        }
        children.push(spacer(Spacing::Lg));
        let (label, tone) = if pending == 0 {
            ("All done for today!".to_string(), Tone::Success)
        } else {
            (format!("{pending} left for today"), Tone::Warning)
        };
        children.push(row(vec![badge(label, tone)]));
    }

    column(children)
}

fn today_task_row(model: &Model, t: &Task) -> Widget {
    let project = model.projects.iter().find(|p| p.id == t.project_id);
    let project_name = project
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "(deleted project)".to_string());
    let project_color = project.map(|p| p.color).unwrap_or_default();

    column(vec![
        row(vec![
            Widget::Checkbox {
                value: t.done,
                label: t.text.clone(),
                on_change: Event::ToggleTaskDone(t.id),
            },
            icon_button(Icon::Star, Event::ToggleMustDoToday(t.id)),
        ]),
        // Project label tucked under the checkbox so the eye reads task -> context.
        // The colored dot anchors the project identity visually.
        row(vec![
            spacer(Spacing::Lg),
            color_dot(project_color),
            caption(format!("in {project_name}")),
        ]),
    ])
}

fn project_list_screen(model: &Model) -> Widget {
    let picker_row = row(
        ProjectColor::ALL
            .iter()
            .map(|c| color_swatch(*c, *c == model.selected_new_project_color))
            .collect(),
    );

    let input_card = card(column(vec![
        row(vec![
            Widget::TextField {
                id: "project_input".into(),
                value: model.project_input.clone(),
                placeholder: "New project name".into(),
            },
            filled_button("Add", Event::AddProject),
        ]),
        spacer(Spacing::Sm),
        caption("Color"),
        picker_row,
    ]));

    let mut children = vec![
        title_text("Projects"),
        spacer(Spacing::Md),
        input_card,
        spacer(Spacing::Lg),
    ];

    if model.projects.is_empty() {
        children.push(outlined_card(column(vec![
            subtitle("No projects yet"),
            spacer(Spacing::Xs),
            caption("Add one above to start organising your work."),
        ])));
    } else {
        for p in &model.projects {
            children.push(card(project_summary_row(model, p)));
        }
    }

    column(children)
}

fn project_summary_row(model: &Model, p: &Project) -> Widget {
    let total = model.tasks.iter().filter(|t| t.project_id == p.id).count();
    let done = model
        .tasks
        .iter()
        .filter(|t| t.project_id == p.id && t.done)
        .count();
    let summary = if total == 0 {
        "No tasks yet".to_string()
    } else {
        format!("{done} of {total} done")
    };

    row(vec![
        color_dot(p.color),
        column(vec![
            text(p.name.clone(), TextStyle::Subtitle),
            caption(summary),
        ]),
        outlined_button("Open", Event::OpenProject(p.id)),
    ])
}

fn project_detail_screen(model: &Model, project: &Project) -> Widget {
    let project_tasks: Vec<&Task> = model
        .tasks
        .iter()
        .filter(|t| t.project_id == project.id)
        .collect();

    let input_card = card(row(vec![
        Widget::TextField {
            id: "task_input".into(),
            value: model.task_input.clone(),
            placeholder: "Add a task to this project".into(),
        },
        filled_button("Add", Event::AddTask),
    ]));

    let mut children = vec![
        input_card,
        spacer(Spacing::Lg),
    ];

    if project_tasks.is_empty() {
        children.push(outlined_card(column(vec![
            subtitle("No tasks yet"),
            spacer(Spacing::Xs),
            caption("Add one above to get started."),
        ])));
    } else {
        for t in &project_tasks {
            children.push(card(project_task_row(t)));
        }
    }

    children.push(spacer(Spacing::Xl));
    children.push(row(vec![outlined_button("Delete project", Event::DeleteProject(project.id))]));

    column(children)
}

fn project_task_row(t: &Task) -> Widget {
    let star_icon = if t.must_do_today { Icon::Star } else { Icon::StarOutline };
    row(vec![
        Widget::Checkbox {
            value: t.done,
            label: t.text.clone(),
            on_change: Event::ToggleTaskDone(t.id),
        },
        icon_button(star_icon, Event::ToggleMustDoToday(t.id)),
        icon_button(Icon::Delete, Event::DeleteTask(t.id)),
    ])
}

fn settings_screen(model: &Model) -> Widget {
    column(vec![
        title_text("Settings"),
        spacer(Spacing::Md),
        outlined_card(column(vec![
            subtitle("Profile"),
            spacer(Spacing::Sm),
            body("Your name"),
            Widget::TextField {
                id: "name".into(),
                value: model.name.clone(),
                placeholder: "e.g. Milan".into(),
            },
        ])),
        spacer(Spacing::Md),
        outlined_card(column(vec![
            subtitle("Appearance"),
            spacer(Spacing::Sm),
            Widget::Switch {
                id: "dark_mode".into(),
                value: model.dark_mode,
                label: "Dark mode".into(),
            },
        ])),
    ])
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod test {
    use super::*;

    fn empty_model() -> Model {
        Model {
            active_tab: ActiveTab::default(),
            name: String::new(),
            dark_mode: false,
            projects: vec![],
            tasks: vec![],
            active_project_id: None,
            project_input: String::new(),
            task_input: String::new(),
            selected_new_project_color: ProjectColor::Indigo,
            next_project_id: 1,
            next_task_id: 1,
        }
    }

    fn scaffold_body(view: &Widget) -> &Widget {
        match view {
            Widget::Scaffold { body, .. } => body,
            other => panic!("expected Scaffold, got {other:?}"),
        }
    }

    fn scaffold_title(view: &Widget) -> &str {
        match view {
            Widget::Scaffold { title, .. } => title,
            other => panic!("expected Scaffold, got {other:?}"),
        }
    }

    fn scaffold_back(view: &Widget) -> Option<&Event> {
        match view {
            Widget::Scaffold { back_action, .. } => back_action.as_ref(),
            other => panic!("expected Scaffold, got {other:?}"),
        }
    }

    fn walk(w: &Widget, f: &mut impl FnMut(&Widget)) {
        f(w);
        match w {
            Widget::Column { children } | Widget::Row { children } => {
                for c in children { walk(c, f); }
            }
            Widget::Card { child, .. } => walk(child, f),
            Widget::Scaffold { body, .. } => walk(body, f),
            _ => {}
        }
    }

    fn count_task_checkboxes(w: &Widget) -> usize {
        let mut n = 0;
        walk(w, &mut |w| {
            if matches!(w, Widget::Checkbox { .. }) {
                n += 1;
            }
        });
        n
    }

    fn checkbox_labels(w: &Widget) -> Vec<String> {
        let mut out = Vec::new();
        walk(w, &mut |w| {
            if let Widget::Checkbox { label, .. } = w {
                out.push(label.clone());
            }
        });
        out
    }

    #[test]
    fn default_model_seeds_two_projects() {
        let m = Model::default();
        assert_eq!(m.projects.len(), 2);
        assert!(m.projects.iter().any(|p| p.name == "Home"));
        assert!(m.projects.iter().any(|p| p.name == "Mobiler"));
    }

    #[test]
    fn today_tab_shows_only_starred_tasks() {
        let app = Counter;
        let model = Model::default();
        let view = app.view(&model);

        let labels = checkbox_labels(scaffold_body(&view));
        // Seeded: tasks 1 and 5 are must_do_today.
        assert!(labels.contains(&"Buy milk".to_string()));
        assert!(labels.contains(&"Ship v0.2".to_string()));
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn today_task_row_shows_project_context() {
        let app = Counter;
        let model = Model::default();
        let view = app.view(&model);

        let mut found_project_label = false;
        walk(scaffold_body(&view), &mut |w| {
            if let Widget::Text { content, style: TextStyle::Caption } = w {
                if content.starts_with("in ") {
                    found_project_label = true;
                }
            }
        });
        assert!(found_project_label, "expected `in <project>` caption on Today rows");
    }

    #[test]
    fn opening_project_pushes_into_detail_view() {
        let app = Counter;
        let mut model = Model::default();
        app.update(Event::SelectTab(Tab::Projects), &mut model);
        let view = app.view(&model);
        assert_eq!(scaffold_title(&view), "Projects");
        assert!(scaffold_back(&view).is_none());

        app.update(Event::OpenProject(2), &mut model);
        let view = app.view(&model);
        assert_eq!(scaffold_title(&view), "Mobiler");
        assert!(matches!(scaffold_back(&view), Some(Event::CloseProject)));
        // Project detail shows all 3 Mobiler tasks (vs 2 in Today).
        assert_eq!(count_task_checkboxes(scaffold_body(&view)), 3);
    }

    #[test]
    fn closing_project_returns_to_list() {
        let app = Counter;
        let mut model = Model::default();
        app.update(Event::SelectTab(Tab::Projects), &mut model);
        app.update(Event::OpenProject(1), &mut model);
        app.update(Event::CloseProject, &mut model);
        let view = app.view(&model);
        assert_eq!(scaffold_title(&view), "Projects");
        assert!(scaffold_back(&view).is_none());
    }

    #[test]
    fn add_task_targets_active_project_only() {
        let app = Counter;
        let mut model = Model::default();
        app.update(Event::SelectTab(Tab::Projects), &mut model);
        // Without an active project, AddTask is a no-op.
        let before = model.tasks.len();
        app.update(
            Event::TextChanged { id: "task_input".into(), value: "stray".into() },
            &mut model,
        );
        app.update(Event::AddTask, &mut model);
        assert_eq!(model.tasks.len(), before);

        // Open Home, add a task — should land under project 1.
        app.update(Event::OpenProject(1), &mut model);
        app.update(
            Event::TextChanged { id: "task_input".into(), value: "vacuum".into() },
            &mut model,
        );
        app.update(Event::AddTask, &mut model);
        assert_eq!(model.tasks.len(), before + 1);
        let new = model.tasks.last().unwrap();
        assert_eq!(new.text, "vacuum");
        assert_eq!(new.project_id, 1);
        assert!(!new.must_do_today);
    }

    #[test]
    fn toggle_must_do_today_propagates_to_today_tab() {
        let app = Counter;
        let mut model = Model::default();
        // Task 4 (Mobiler / "Write iOS Render") starts unstarred.
        assert!(!model.tasks.iter().find(|t| t.id == 4).unwrap().must_do_today);

        app.update(Event::ToggleMustDoToday(4), &mut model);
        let view = app.view(&model);
        let labels = checkbox_labels(scaffold_body(&view));
        assert!(labels.contains(&"Write iOS Render".to_string()));
    }

    #[test]
    fn deleting_project_removes_its_tasks_and_closes_detail() {
        let app = Counter;
        let mut model = Model::default();
        let mobiler_task_count = model.tasks.iter().filter(|t| t.project_id == 2).count();
        assert!(mobiler_task_count > 0);

        app.update(Event::SelectTab(Tab::Projects), &mut model);
        app.update(Event::OpenProject(2), &mut model);
        app.update(Event::DeleteProject(2), &mut model);

        assert!(model.projects.iter().all(|p| p.id != 2));
        assert!(model.tasks.iter().all(|t| t.project_id != 2));
        assert!(model.active_project_id.is_none());
    }

    #[test]
    fn new_project_uses_selected_color_then_advances() {
        let app = Counter;
        let mut model = Model::default();
        // Default-seeded selection is Coral.
        assert_eq!(model.selected_new_project_color, ProjectColor::Coral);

        app.update(
            Event::TextChanged { id: "project_input".into(), value: "Garden".into() },
            &mut model,
        );
        app.update(Event::AddProject, &mut model);

        let added = model.projects.last().unwrap();
        assert_eq!(added.name, "Garden");
        assert_eq!(added.color, ProjectColor::Coral);
        // Picker advanced for the next project.
        assert_eq!(model.selected_new_project_color, ProjectColor::Amber);
    }

    #[test]
    fn select_project_color_overrides_default() {
        let app = Counter;
        let mut model = Model::default();
        app.update(Event::SelectProjectColor(ProjectColor::Pink), &mut model);
        assert_eq!(model.selected_new_project_color, ProjectColor::Pink);

        app.update(
            Event::TextChanged { id: "project_input".into(), value: "Birthday".into() },
            &mut model,
        );
        app.update(Event::AddProject, &mut model);
        assert_eq!(model.projects.last().unwrap().color, ProjectColor::Pink);
    }

    #[test]
    fn color_picker_marks_exactly_one_swatch_selected() {
        let app = Counter;
        let mut model = Model::default();
        app.update(Event::SelectTab(Tab::Projects), &mut model);
        app.update(Event::SelectProjectColor(ProjectColor::Lime), &mut model);

        let view = app.view(&model);
        let mut selected = Vec::new();
        walk(scaffold_body(&view), &mut |w| {
            if let Widget::ColorSwatch { color, selected: sel, .. } = w {
                if *sel {
                    selected.push(*color);
                }
            }
        });
        assert_eq!(selected, vec![ProjectColor::Lime]);
    }

    #[test]
    fn project_cards_carry_a_color_dot() {
        let app = Counter;
        let mut model = Model::default();
        app.update(Event::SelectTab(Tab::Projects), &mut model);
        let view = app.view(&model);
        let mut dot_colors = Vec::new();
        walk(scaffold_body(&view), &mut |w| {
            if let Widget::ColorDot { color } = w {
                dot_colors.push(*color);
            }
        });
        // Two seeded projects -> at least their two dots (Indigo, Teal).
        assert!(dot_colors.contains(&ProjectColor::Indigo));
        assert!(dot_colors.contains(&ProjectColor::Teal));
    }

    #[test]
    fn project_color_survives_round_trip_serialization() {
        let app = Counter;
        let mut model = Model::default();
        app.update(Event::SelectProjectColor(ProjectColor::Pink), &mut model);
        app.update(
            Event::TextChanged { id: "project_input".into(), value: "Trip".into() },
            &mut model,
        );
        app.update(Event::AddProject, &mut model);

        let bytes = bincode::serialize(&model).unwrap();
        let restored: Model = bincode::deserialize(&bytes).unwrap();
        let trip = restored.projects.iter().find(|p| p.name == "Trip").unwrap();
        assert_eq!(trip.color, ProjectColor::Pink);
    }

    #[test]
    fn empty_today_shows_helpful_card() {
        let app = Counter;
        let mut model = empty_model();
        // No projects, no tasks — Today should still render gracefully.
        let view = app.view(&model);
        let mut found = false;
        walk(scaffold_body(&view), &mut |w| {
            if let Widget::Text { content, style: TextStyle::Subtitle } = w {
                if content == "Nothing scheduled for today" { found = true; }
            }
        });
        assert!(found);
        // And the body should NOT contain any badge (we only show one when there ARE tasks).
        let mut badge_seen = false;
        walk(scaffold_body(&view), &mut |w| {
            if matches!(w, Widget::Badge { .. }) { badge_seen = true; }
        });
        assert!(!badge_seen);
        // Ditto active_tab manipulation should still work.
        app.update(Event::SelectTab(Tab::Projects), &mut model);
    }

    #[test]
    fn dark_mode_propagates_to_scaffold() {
        let app = Counter;
        let mut model = Model::default();
        app.update(
            Event::Toggled { id: "dark_mode".into(), value: true },
            &mut model,
        );
        match app.view(&model) {
            Widget::Scaffold { dark_mode, .. } => assert!(dark_mode),
            other => panic!("got {other:?}"),
        }
    }
}
