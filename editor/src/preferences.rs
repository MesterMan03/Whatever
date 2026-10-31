use super::*;
use serde_json::{Value, json};

pub(super) struct Preferences {
    pub editor: String,
    pub custom: String,
    pub recent: Vec<String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            editor: if cfg!(windows) {
                "Notepad"
            } else {
                "Visual Studio Code"
            }
            .into(),
            custom: String::new(),
            recent: Vec::new(),
        }
    }
}
impl Preferences {
    fn path() -> PathBuf {
        std::env::var_os("APPDATA")
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir)
                    .join(".config")
            })
            .join("Whatever/editor.json")
    }
    pub fn load() -> Self {
        let Ok(bytes) = std::fs::read(Self::path()) else {
            return Self::default();
        };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
            return Self::default();
        };
        Self {
            editor: v["script_editor"]
                .as_str()
                .unwrap_or(&Self::default().editor)
                .into(),
            custom: v["custom_editor"].as_str().unwrap_or("").into(),
            recent: v["recent_projects"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        std::fs::create_dir_all(path.parent().context("settings directory")?)?;
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"script_editor":self.editor,"custom_editor":self.custom,"recent_projects":self.recent}),
            )?,
        )?;
        Ok(())
    }
    pub fn remember(&mut self, path: &Path) {
        let path = display_path(path);
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(10);
        if let Err(e) = self.save() {
            tracing::warn!("editor settings: {e:#}");
        }
    }
    fn executable(&self) -> anyhow::Result<PathBuf> {
        if self.editor == "Custom" {
            ensure!(
                !self.custom.is_empty(),
                "choose a script editor executable in Editor Settings"
            );
            return Ok(PathBuf::from(&self.custom));
        }
        if self.editor == "Notepad" {
            return Ok(PathBuf::from(
                std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into()),
            )
            .join("System32/notepad.exe"));
        }
        let (command, relative) = match self.editor.as_str() {
            "Notepad++" => ("notepad++", "Notepad++/notepad++.exe"),
            "VSCodium" => ("codium", "VSCodium/VSCodium.exe"),
            "Sublime Text" => ("subl", "Sublime Text/sublime_text.exe"),
            _ => ("code", "Microsoft VS Code/Code.exe"),
        };
        if cfg!(windows) {
            for base in [
                std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs")),
                std::env::var_os("ProgramFiles").map(PathBuf::from),
                std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
            ]
            .into_iter()
            .flatten()
            {
                let path = base.join(relative);
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        if let Some(paths) = std::env::var_os("PATH") {
            for base in std::env::split_paths(&paths) {
                let path = base.join(format!("{command}{suffix}"));
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
        anyhow::bail!(
            "{} was not found. Choose another editor or a custom executable in Editor Settings.",
            self.editor
        )
    }
}
impl Editor {
    pub(super) fn open_script(&mut self, path: &Path) -> anyhow::Result<()> {
        let exe = self.preferences.executable()?;
        let mut command = Command::new(exe);
        if matches!(
            self.preferences.editor.as_str(),
            "Visual Studio Code" | "VSCodium"
        ) {
            command.arg("--reuse-window");
        }
        command.arg(engine::project::portable_path(path));
        command.spawn().context("open script editor")?;
        self.status = format!(
            "Opened {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        Ok(())
    }
    pub(super) fn preferences_ui(&mut self, ctx: &egui::Context) {
        if !self.show_preferences {
            return;
        }
        let mut close = false;
        let mut changed = false;
        egui::Modal::new(egui::Id::new("editor_preferences")).show(ctx,|ui| {ui.set_width(440.0);ui.heading("Editor Settings");ui.separator();ui.label("Open scripts with");
            egui::ComboBox::from_id_salt("script_editor").selected_text(&self.preferences.editor).show_ui(ui,|ui| {for name in ["Visual Studio Code","Notepad","Notepad++","VSCodium","Sublime Text","Custom"] {if !cfg!(windows)&&matches!(name,"Notepad"|"Notepad++"){continue;}changed|=ui.selectable_value(&mut self.preferences.editor,name.into(),name).changed();}});
            if self.preferences.editor=="Custom" {ui.label(display_path(Path::new(&self.preferences.custom)));if ui.button("Choose executable…").clicked()&&let Some(path)=self.file_dialog("Choose a text editor executable").pick_file(){self.preferences.custom=display_path(&path);changed=true;}}
            ui.weak("Scripts open as text in the selected editor. Double-click a script in the Scripts browser.");ui.separator();close=ui.button("Done").clicked();
        });
        close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if changed {
            let result = self.preferences.save();
            self.report(result);
        }
        if close {
            self.show_preferences = false;
        }
    }
    pub(super) fn welcome_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx,|ui| {if self.has_dialog(){ui.disable();} ui.add_space(65.0);ui.vertical_centered(|ui| {ui.heading(egui::RichText::new("Whatever").size(44.0).color(egui::Color32::from_rgb(129,174,248)));ui.label(egui::RichText::new("Make a mod. Build a game.").size(22.0));ui.add_space(12.0);ui.label("Every game is a mod. A project brings your mods, scenes and assets together.");ui.add_space(30.0);
            for (label,action) in [("Create a new project",Action::NewProject),("Open a project…",Action::OpenProject),("Open an existing game folder…",Action::OpenFolder),("Editor Settings…",Action::Preferences)] {if ui.add_sized([310.0,36.0],egui::Button::new(label)).clicked(){let result=self.action(action);self.report(result);}}
            if !self.preferences.recent.is_empty(){ui.add_space(25.0);ui.heading("Recent projects");for path in self.preferences.recent.clone(){if ui.button(&path).clicked(){let result=Project::open(Path::new(&path)).and_then(|p|self.change_project(p));self.report(result);}}}
            ui.add_space(20.0);ui.weak("Finished games open here using the same project format as mods in development.");if !self.status.is_empty(){ui.label(&self.status);}
        });});
    }
}
