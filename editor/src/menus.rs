#[cfg(windows)]
use anyhow::Context;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    NewProject,
    OpenProject,
    OpenFolder,
    NewScene,
    OpenScene,
    Save,
    SaveAs,
    Quit,
    Undo,
    Redo,
    Delete,
    AddEntity,
    AddChild,
    Frame,
    NewMod,
    AddMod,
    Settings,
    Build,
}

type MenuEntry = (&'static str, &'static str, Action);
pub(super) const GROUPS: &[(&str, &[MenuEntry])] = &[
    (
        "File",
        &[
            ("new_project", "New Project…", Action::NewProject),
            ("open_project", "Open Project…", Action::OpenProject),
            ("open_folder", "Open Game Folder…", Action::OpenFolder),
            ("new_scene", "New Scene…", Action::NewScene),
            ("open_scene", "Open Scene…", Action::OpenScene),
            ("save", "Save Scene", Action::Save),
            ("save_as", "Save Scene As…", Action::SaveAs),
            ("quit", "Exit", Action::Quit),
        ],
    ),
    (
        "Edit",
        &[
            ("undo", "Undo", Action::Undo),
            ("redo", "Redo", Action::Redo),
            ("delete", "Delete Entity", Action::Delete),
        ],
    ),
    (
        "Scene",
        &[
            ("entity", "Create Entity", Action::AddEntity),
            ("child", "Create Child", Action::AddChild),
            ("frame", "Frame Selected", Action::Frame),
        ],
    ),
    (
        "Project",
        &[
            ("new_mod", "Create Mod…", Action::NewMod),
            ("add_mod", "Add Existing Mod…", Action::AddMod),
            ("settings", "Project & Build Settings…", Action::Settings),
        ],
    ),
    ("Build", &[("build", "Build Game…", Action::Build)]),
];

#[cfg(any(windows, target_os = "macos"))]
pub(super) struct Menus {
    pub bar: muda::Menu,
}

#[cfg(any(windows, target_os = "macos"))]
impl Menus {
    pub fn new() -> anyhow::Result<Self> {
        use muda::accelerator::{Accelerator, Code, Modifiers};
        use muda::{Menu, MenuItem, Submenu};
        let bar = Menu::new();
        for (name, items) in GROUPS {
            let submenu = Submenu::new(name, true);
            for (id, label, _) in *items {
                let code = match *id {
                    "new_project" => Some(Code::KeyN),
                    "open_project" => Some(Code::KeyO),
                    "save" => Some(Code::KeyS),
                    "undo" => Some(Code::KeyZ),
                    "redo" => Some(Code::KeyY),
                    "build" => Some(Code::KeyB),
                    _ => None,
                };
                let item = MenuItem::with_id(
                    *id,
                    *label,
                    true,
                    code.map(|code| Accelerator::new(Modifiers::CONTROL, code)),
                );
                submenu.append(&item)?;
            }
            bar.append(&submenu)?;
        }
        Ok(Self { bar })
    }
    pub fn attach(&self, window: &winit::window::Window) -> anyhow::Result<()> {
        #[cfg(windows)]
        {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() {
                // The native menu is owned for the lifetime of this window.
                unsafe { self.bar.init_for_hwnd(handle.hwnd.get()) }
                    .context("attach native menu")?;
                unsafe {
                    self.bar
                        .set_theme_for_hwnd(handle.hwnd.get(), muda::MenuTheme::Dark)
                }
                .context("theme native menu")?;
            }
        }
        #[cfg(target_os = "macos")]
        {
            let _ = window;
            self.bar.init_for_nsapp();
        }
        Ok(())
    }
    pub fn poll(&self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            if let Some((_, _, action)) = GROUPS
                .iter()
                .flat_map(|(_, items)| items.iter())
                .find(|(id, _, _)| event.id == *id)
            {
                actions.push(*action);
            }
        }
        actions
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(super) struct Menus;
#[cfg(not(any(windows, target_os = "macos")))]
impl Menus {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self)
    }
    pub fn attach(&self, _: &winit::window::Window) -> anyhow::Result<()> {
        Ok(())
    }
    pub fn poll(&self) -> Vec<Action> {
        Vec::new()
    }
}
