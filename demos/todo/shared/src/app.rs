//! Todo — a projects/tasks app, ported to the generic Mobiler ABI.
//!
//! Showcases the full widget vocabulary (Scaffold with tabs + a pushed detail
//! screen, cards, checkboxes, chips, a color picker, badges, a switch), project
//! identity colors via `ColorDot`, and **state that survives cold restarts** via
//! the storage capability (`cx.save` + `restore`).

use mobiler_core::{
    ButtonStyle, CardStyle, Cx, Icon, InputValue, MobilerApp, MobilerShell, ProjectColor, Spacing,
    Tone, Widget, badge, button, caption, card, card_button, checkbox, chip, color_dot, column,
    grid, icon_button, row, scaffold, scaffold_back, spacer, subtitle, switch, tab, text, text_field,
    title,
};
use serde::{Deserialize, Serialize};

// ---- project identity colors (rotation order + picker labels) ----
const COLORS: [ProjectColor; 6] = [
    ProjectColor::Indigo,
    ProjectColor::Teal,
    ProjectColor::Coral,
    ProjectColor::Amber,
    ProjectColor::Lime,
    ProjectColor::Pink,
];

/// The next color in rotation, so each new project gets a fresh default.
fn next_color(c: ProjectColor) -> ProjectColor {
    let i = COLORS.iter().position(|x| *x == c).unwrap_or(0);
    COLORS[(i + 1) % COLORS.len()]
}

fn color_name(c: ProjectColor) -> &'static str {
    match c {
        ProjectColor::Indigo => "Indigo",
        ProjectColor::Teal => "Teal",
        ProjectColor::Coral => "Coral",
        ProjectColor::Amber => "Amber",
        ProjectColor::Lime => "Lime",
        ProjectColor::Pink => "Pink",
    }
}

// ============================ domain ============================

#[derive(Serialize, Deserialize, Clone)]
struct Project {
    id: u32,
    name: String,
    color: ProjectColor,
}

#[derive(Serialize, Deserialize, Clone)]
struct TaskItem {
    id: u32,
    project_id: u32,
    text: String,
    done: bool,
    today: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum TabKind {
    Today,
    Projects,
    Settings,
}

/// Your app's typed events. Mobiler serializes these into opaque tokens; the
/// native shell never sees this type.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    SelectTab(TabKind),
    OpenProject(u32),
    CloseProject,
    DeleteProject(u32),
    SelectColor(ProjectColor),
    AddProject,
    AddTask,
    ToggleToday(u32),
    DeleteTask(u32),
}

// ============================ model ============================

pub struct Model {
    // --- persisted (see `Persisted`) ---
    name: String,
    dark_mode: bool,
    projects: Vec<Project>,
    tasks: Vec<TaskItem>,
    new_color: ProjectColor,
    next_project_id: u32,
    next_task_id: u32,
    // --- transient UI state (reset on cold start) ---
    tab: TabKind,
    open_project: Option<u32>,
    project_input: String,
    task_input: String,
}

impl Default for Model {
    fn default() -> Self {
        // Seed two projects so a first launch has something to show.
        Self {
            name: String::new(),
            dark_mode: false,
            projects: vec![
                Project { id: 1, name: "Home".into(), color: ProjectColor::Indigo },
                Project { id: 2, name: "Mobiler".into(), color: ProjectColor::Teal },
            ],
            tasks: vec![
                TaskItem { id: 1, project_id: 1, text: "Buy milk".into(), done: false, today: true },
                TaskItem { id: 2, project_id: 1, text: "Cancel old subscription".into(), done: false, today: false },
                TaskItem { id: 3, project_id: 2, text: "Polish styling vocab".into(), done: true, today: false },
                TaskItem { id: 4, project_id: 2, text: "Write iOS Render".into(), done: false, today: false },
                TaskItem { id: 5, project_id: 2, text: "Ship v0.3".into(), done: false, today: true },
            ],
            // Seeds used Indigo + Teal; pre-pick Coral for the first hand-added project.
            new_color: ProjectColor::Coral,
            next_project_id: 3,
            next_task_id: 6,
            tab: TabKind::Today,
            open_project: None,
            project_input: String::new(),
            task_input: String::new(),
        }
    }
}

/// The durable slice of the model, serialized to the storage capability after
/// every change and handed back to [`Todo::restore`] on startup.
#[derive(Serialize, Deserialize)]
struct Persisted {
    name: String,
    dark_mode: bool,
    projects: Vec<Project>,
    tasks: Vec<TaskItem>,
    new_color: ProjectColor,
    next_project_id: u32,
    next_task_id: u32,
}

impl Model {
    fn persisted(&self) -> Persisted {
        Persisted {
            name: self.name.clone(),
            dark_mode: self.dark_mode,
            projects: self.projects.clone(),
            tasks: self.tasks.clone(),
            new_color: self.new_color,
            next_project_id: self.next_project_id,
            next_task_id: self.next_task_id,
        }
    }

    fn project(&self, id: u32) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }
}

/// Persist the durable state. Called after every mutating event/input.
fn save(model: &Model, cx: &mut Cx<Msg>) {
    if let Ok(blob) = serde_json::to_string(&model.persisted()) {
        cx.save(blob);
    }
}

// ============================ app ============================

#[derive(Default)]
pub struct Todo;

impl MobilerApp for Todo {
    type Event = Msg;
    type Model = Model;

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::SelectTab(t) => {
                model.tab = t;
                if t != TabKind::Projects {
                    model.open_project = None;
                }
            }
            Msg::OpenProject(id) => {
                model.open_project = Some(id);
                model.task_input.clear();
            }
            Msg::CloseProject => {
                model.open_project = None;
                model.task_input.clear();
            }
            Msg::DeleteProject(id) => {
                model.projects.retain(|p| p.id != id);
                model.tasks.retain(|t| t.project_id != id);
                if model.open_project == Some(id) {
                    model.open_project = None;
                }
            }
            Msg::SelectColor(c) => model.new_color = c,
            Msg::AddProject => {
                let name = model.project_input.trim().to_string();
                if !name.is_empty() {
                    let id = model.next_project_id;
                    model.next_project_id += 1;
                    let color = model.new_color;
                    model.projects.push(Project { id, name, color });
                    model.project_input.clear();
                    // Advance the picker so the next project defaults to a new color.
                    model.new_color = next_color(color);
                }
            }
            Msg::AddTask => {
                if let Some(pid) = model.open_project {
                    let text = model.task_input.trim().to_string();
                    if !text.is_empty() {
                        let id = model.next_task_id;
                        model.next_task_id += 1;
                        model.tasks.push(TaskItem { id, project_id: pid, text, done: false, today: false });
                        model.task_input.clear();
                    }
                }
            }
            Msg::ToggleToday(id) => {
                if let Some(t) = model.tasks.iter_mut().find(|t| t.id == id) {
                    t.today = !t.today;
                }
            }
            Msg::DeleteTask(id) => model.tasks.retain(|t| t.id != id),
        }
        save(model, cx);
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, cx: &mut Cx<Msg>) {
        match value {
            InputValue::Text(v) => match id {
                "name" => model.name = v,
                "project_input" => model.project_input = v,
                "task_input" => model.task_input = v,
                _ => {}
            },
            InputValue::Bool(v) => {
                if id == "dark_mode" {
                    model.dark_mode = v;
                } else if let Some(rest) = id.strip_prefix("done:")
                    && let Ok(tid) = rest.parse::<u32>()
                    && let Some(t) = model.tasks.iter_mut().find(|t| t.id == tid)
                {
                    t.done = v;
                }
            }
            InputValue::Int(_) => {}
        }
        save(model, cx);
    }

    fn restore(&self, data: &str, model: &mut Model) {
        if let Ok(p) = serde_json::from_str::<Persisted>(data) {
            model.name = p.name;
            model.dark_mode = p.dark_mode;
            model.projects = p.projects;
            model.tasks = p.tasks;
            model.new_color = p.new_color;
            model.next_project_id = p.next_project_id;
            model.next_task_id = p.next_task_id;
        }
    }

    fn view(&self, model: &Model) -> Widget {
        let tabs = vec![
            tab("Today", model.tab == TabKind::Today, Msg::SelectTab(TabKind::Today)),
            tab("Projects", model.tab == TabKind::Projects, Msg::SelectTab(TabKind::Projects)),
            tab("Settings", model.tab == TabKind::Settings, Msg::SelectTab(TabKind::Settings)),
        ];

        match model.tab {
            TabKind::Today => scaffold("Today", model.dark_mode, tabs, today_screen(model)),
            TabKind::Settings => scaffold("Settings", model.dark_mode, tabs, settings_screen(model)),
            TabKind::Projects => match model.open_project.and_then(|id| model.project(id)) {
                Some(p) => scaffold_back(
                    p.name.clone(),
                    model.dark_mode,
                    tabs,
                    detail_screen(model, p),
                    Msg::CloseProject,
                ),
                None => scaffold("Projects", model.dark_mode, tabs, project_list(model)),
            },
        }
    }
}

// ============================ screens ============================

fn today_screen(model: &Model) -> Widget {
    let today: Vec<&TaskItem> = model.tasks.iter().filter(|t| t.today).collect();
    let header = if model.name.trim().is_empty() {
        "Today's must-dos".to_string()
    } else {
        format!("{}, today's must-dos", model.name.trim())
    };
    let pending = today.iter().filter(|t| !t.done).count();

    let mut kids = vec![title(header), spacer(Spacing::Md)];
    if today.is_empty() {
        kids.push(card(
            column(vec![
                subtitle("Nothing scheduled for today"),
                spacer(Spacing::Xs),
                caption("Star tasks from a project to add them here."),
            ]),
            CardStyle::Outlined,
        ));
    } else {
        for t in &today {
            kids.push(card(today_row(model, t), CardStyle::Elevated));
        }
        kids.push(spacer(Spacing::Lg));
        let (label, tone) = if pending == 0 {
            ("All done for today!".to_string(), Tone::Success)
        } else {
            (format!("{pending} left for today"), Tone::Warning)
        };
        kids.push(row(vec![badge(label, tone)]));
    }
    column(kids)
}

fn today_row(model: &Model, t: &TaskItem) -> Widget {
    let proj = model.project(t.project_id);
    let name = proj.map_or_else(|| "(deleted project)".to_string(), |p| p.name.clone());
    let color = proj.map_or(ProjectColor::Indigo, |p| p.color);
    column(vec![
        row(vec![
            checkbox(format!("done:{}", t.id), t.text.clone(), t.done),
            chip("Today", true, Msg::ToggleToday(t.id)),
        ]),
        // Project context tucked under the task; the dot anchors its identity.
        row(vec![spacer(Spacing::Lg), color_dot(color), caption(format!("in {name}"))]),
    ])
}

fn project_list(model: &Model) -> Widget {
    let picker = grid(
        COLORS
            .iter()
            .map(|c| {
                card_button(
                    row(vec![color_dot(*c), text(color_name(*c))]),
                    if *c == model.new_color { CardStyle::Filled } else { CardStyle::Outlined },
                    Msg::SelectColor(*c),
                )
            })
            .collect(),
    );

    let input = card(
        column(vec![
            row(vec![
                text_field("project_input", "New project name", model.project_input.clone()),
                button("Add", ButtonStyle::Filled, Msg::AddProject),
            ]),
            spacer(Spacing::Sm),
            caption("Color"),
            picker,
        ]),
        CardStyle::Elevated,
    );

    let mut kids = vec![title("Projects"), spacer(Spacing::Md), input, spacer(Spacing::Lg)];
    if model.projects.is_empty() {
        kids.push(card(
            column(vec![
                subtitle("No projects yet"),
                spacer(Spacing::Xs),
                caption("Add one above to start organising your work."),
            ]),
            CardStyle::Outlined,
        ));
    } else {
        for p in &model.projects {
            kids.push(card(project_summary(model, p), CardStyle::Elevated));
        }
    }
    column(kids)
}

fn project_summary(model: &Model, p: &Project) -> Widget {
    let total = model.tasks.iter().filter(|t| t.project_id == p.id).count();
    let done = model.tasks.iter().filter(|t| t.project_id == p.id && t.done).count();
    let summary = if total == 0 {
        "No tasks yet".to_string()
    } else {
        format!("{done} of {total} done")
    };
    row(vec![
        color_dot(p.color),
        column(vec![subtitle(p.name.clone()), caption(summary)]),
        button("Open", ButtonStyle::Outlined, Msg::OpenProject(p.id)),
    ])
}

fn detail_screen(model: &Model, project: &Project) -> Widget {
    let tasks: Vec<&TaskItem> = model.tasks.iter().filter(|t| t.project_id == project.id).collect();

    let input = card(
        row(vec![
            text_field("task_input", "Add a task to this project", model.task_input.clone()),
            button("Add", ButtonStyle::Filled, Msg::AddTask),
        ]),
        CardStyle::Elevated,
    );

    let mut kids = vec![input, spacer(Spacing::Lg)];
    if tasks.is_empty() {
        kids.push(card(
            column(vec![
                subtitle("No tasks yet"),
                spacer(Spacing::Xs),
                caption("Add one above to get started."),
            ]),
            CardStyle::Outlined,
        ));
    } else {
        for t in &tasks {
            kids.push(card(detail_row(t), CardStyle::Elevated));
        }
    }
    kids.push(spacer(Spacing::Xl));
    kids.push(row(vec![button("Delete project", ButtonStyle::Text, Msg::DeleteProject(project.id))]));
    column(kids)
}

fn detail_row(t: &TaskItem) -> Widget {
    row(vec![
        checkbox(format!("done:{}", t.id), t.text.clone(), t.done),
        chip("Today", t.today, Msg::ToggleToday(t.id)),
        icon_button(Icon::Delete, Msg::DeleteTask(t.id)),
    ])
}

fn settings_screen(model: &Model) -> Widget {
    column(vec![
        title("Settings"),
        spacer(Spacing::Md),
        card(
            column(vec![
                subtitle("Profile"),
                spacer(Spacing::Sm),
                text("Your name"),
                text_field("name", "e.g. Milan", model.name.clone()),
            ]),
            CardStyle::Outlined,
        ),
        spacer(Spacing::Md),
        card(
            column(vec![
                subtitle("Appearance"),
                spacer(Spacing::Sm),
                switch("dark_mode", "Dark mode", model.dark_mode),
            ]),
            CardStyle::Outlined,
        ),
    ])
}

/// The Crux app the FFI + codegen target. `MobilerShell` over [`Todo`], so the
/// native shell stays generic.
pub type App = MobilerShell<Todo>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn add_project_rotates_color() {
        let app = Todo;
        let mut m = Model::default();
        let before = m.new_color;
        m.project_input = "Garden".into();
        app.update(Msg::AddProject, &mut m, &mut Cx::default());
        assert!(m.projects.iter().any(|p| p.name == "Garden"));
        assert_ne!(m.new_color, before, "picker should advance after adding");
    }

    #[test]
    fn toggle_done_via_checkbox_input() {
        let app = Todo;
        let mut m = Model::default();
        app.input("done:1", InputValue::Bool(true), &mut m, &mut Cx::default());
        assert!(m.tasks.iter().find(|t| t.id == 1).unwrap().done);
    }

    #[test]
    fn restore_round_trips_durable_state() {
        let app = Todo;
        let mut m = Model::default();
        m.name = "Ada".into();
        app.update(Msg::ToggleToday(2), &mut m, &mut Cx::default());

        let blob = serde_json::to_string(&m.persisted()).unwrap();
        let mut fresh = Model::default();
        app.restore(&blob, &mut fresh);

        assert_eq!(fresh.name, "Ada");
        assert!(fresh.tasks.iter().find(|t| t.id == 2).unwrap().today);
    }
}
