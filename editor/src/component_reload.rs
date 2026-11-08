use super::*;
use std::collections::BTreeMap;

type ComponentTypes = BTreeMap<String, engine::ecs::ComponentDefinition>;

struct Request {
    generation: u64,
    project: Project,
}

pub(super) struct Progress {
    label: String,
    completed: usize,
    total: usize,
}

enum Event {
    Progress(u64, Progress),
    Finished(u64, Result<ComponentTypes, String>),
}

/// One worker per editor; requests queue without blocking the event loop.
#[derive(Default)]
pub(super) struct ComponentReload {
    requests: Option<mpsc::Sender<Request>>,
    events: Option<mpsc::Receiver<Event>>,
    generation: u64,
    busy: bool,
    pub label: String,
    pub progress: Option<f32>,
    pub error: Option<String>,
}

impl ComponentReload {
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn ensure_ready(&self) -> anyhow::Result<()> {
        ensure!(
            !self.busy,
            "Component types are reloading in the background. Try again when the bottom status bar finishes."
        );
        if let Some(error) = &self.error {
            anyhow::bail!(
                "Fix the component reload error and reload component types first: {error}"
            );
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.busy = false;
        self.label.clear();
        self.progress = None;
        self.error = None;
    }

    pub fn request(&mut self, project: Project, ctx: egui::Context) -> anyhow::Result<()> {
        if self.requests.is_none() {
            let (requests, receive_requests) = mpsc::channel();
            let (events, receive_events) = mpsc::channel();
            std::thread::Builder::new()
                .name("component-reload".into())
                .spawn(move || worker(receive_requests, events, ctx, load_types))
                .context("start component reload worker")?;
            self.requests = Some(requests);
            self.events = Some(receive_events);
        }
        self.generation = self.generation.wrapping_add(1);
        self.requests
            .as_ref()
            .context("component reload worker")?
            .send(Request {
                generation: self.generation,
                project,
            })
            .context("queue component reload")?;
        self.busy = true;
        self.label = "Component reload queued…".into();
        self.progress = None;
        self.error = None;
        Ok(())
    }

    pub fn poll(&mut self) -> Option<Result<ComponentTypes, String>> {
        loop {
            match self.events.as_ref()?.try_recv() {
                Ok(Event::Progress(generation, progress)) if generation == self.generation => {
                    self.label = progress.label;
                    self.progress = (progress.total > 0)
                        .then(|| progress.completed as f32 / progress.total as f32);
                }
                Ok(Event::Finished(generation, result)) if generation == self.generation => {
                    self.busy = false;
                    self.progress = None;
                    self.error = result.as_ref().err().cloned();
                    return Some(result);
                }
                Ok(_) => {} // A superseded request or a project that has been closed.
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.requests = None;
                    self.events = None;
                    if !self.busy {
                        return None;
                    }
                    self.busy = false;
                    self.progress = None;
                    let error = "Component reload worker stopped unexpectedly. Reload to retry.";
                    self.error = Some(error.into());
                    return Some(Err(error.into()));
                }
            }
        }
    }
}

fn worker(
    requests: mpsc::Receiver<Request>,
    events: mpsc::Sender<Event>,
    ctx: egui::Context,
    mut load: impl FnMut(&Project, &mut dyn FnMut(Progress)) -> anyhow::Result<ComponentTypes>,
) {
    while let Ok(mut request) = requests.recv() {
        // Focus changes and repeated reload clicks collapse to the newest project snapshot.
        for next in requests.try_iter() {
            request = next;
        }
        let generation = request.generation;
        let result = load(&request.project, &mut |progress| {
            let _ = events.send(Event::Progress(generation, progress));
            ctx.request_repaint();
        })
        .map_err(|error| format!("{error:#}"));
        if events.send(Event::Finished(generation, result)).is_err() {
            break;
        }
        ctx.request_repaint();
    }
}

fn load_types(
    project: &Project,
    progress: &mut dyn FnMut(Progress),
) -> anyhow::Result<ComponentTypes> {
    progress(Progress {
        label: "Reading component mod manifests…".into(),
        completed: 0,
        total: 0,
    });
    let scripted: Vec<_> = project
        .manifests()?
        .into_iter()
        .filter(|(_, manifest)| manifest.script.is_some())
        .collect();
    let mut world = World::new();
    if !scripted.is_empty() {
        progress(Progress {
            label: "Checking the component scripting host…".into(),
            completed: 0,
            total: scripted.len(),
        });
        for (index, (root, manifest)) in scripted.iter().enumerate() {
            let owner = &manifest.meta.id;
            progress(Progress {
                label: format!(
                    "Reloading component types — {owner} ({}/{})…",
                    index + 1,
                    scripted.len()
                ),
                completed: index,
                total: scripted.len(),
            });
            for definition in engine::script::inspect_components(root, owner)? {
                world.register_component(owner, definition)?;
            }
        }
    }
    Ok(world.component_types)
}

impl Editor {
    pub(super) fn refresh_component_types(&mut self) -> anyhow::Result<()> {
        if self.live.is_some() || self.player.is_some() || self.job.is_some() {
            return Ok(());
        }
        if let Some(project) = &self.project {
            self.component_reload
                .request(project.clone(), self.ctx.clone())?;
        }
        Ok(())
    }

    pub(super) fn poll_component_types(&mut self) {
        match self.component_reload.poll() {
            Some(Ok(types)) => {
                let count = types.len();
                self.world.component_types = types;
                self.preview_dirty = true;
                self.status = format!("Component types reloaded — {count} available");
            }
            Some(Err(error)) => {
                self.status = "Component reload failed — previous types kept".into();
                self.logs.push(format!("Component reload failed: {error}"));
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn project(name: &str) -> Project {
        Project {
            version: 1,
            name: name.into(),
            id: name.into(),
            mods: vec![],
            build: Default::default(),
            path: PathBuf::from(name).join("whatever.wproject"),
        }
    }

    fn types(name: &str) -> ComponentTypes {
        let definition = engine::ecs::ComponentDefinition {
            id: format!("{name}:Marker"),
            source: "scripts/components.ts".into(),
            export: "Marker".into(),
            fields: vec![],
        };
        BTreeMap::from([(definition.id.clone(), definition)])
    }

    #[test]
    fn slow_inspection_keeps_polling_available_and_coalesces_queued_requests() {
        let (requests, receive_requests) = mpsc::channel();
        let (events, receive_events) = mpsc::channel();
        let (started, receive_started) = mpsc::channel();
        let (release, receive_release) = mpsc::channel();
        let mut reload = ComponentReload {
            requests: Some(requests),
            events: Some(receive_events),
            ..Default::default()
        };
        let background = std::thread::spawn(move || {
            worker(
                receive_requests,
                events,
                egui::Context::default(),
                |project, progress| {
                    progress(Progress {
                        label: format!("Inspecting {}", project.name),
                        completed: 0,
                        total: 1,
                    });
                    started.send(project.name.clone())?;
                    if project.name == "first" {
                        receive_release.recv_timeout(Duration::from_secs(5))?;
                    }
                    Ok(types(&project.name))
                },
            );
        });
        reload
            .request(project("first"), egui::Context::default())
            .expect("queue first");
        assert_eq!(
            receive_started
                .recv_timeout(Duration::from_secs(5))
                .expect("worker started"),
            "first"
        );
        assert!(reload.poll().is_none());
        assert!(reload.is_busy());
        assert!(reload.label.contains("first"));
        assert!(reload.ensure_ready().is_err());

        reload
            .request(project("intermediate"), egui::Context::default())
            .expect("queue intermediate");
        reload
            .request(project("latest"), egui::Context::default())
            .expect("queue latest");
        release.send(()).expect("release first scan");
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            if let Some(result) = reload.poll() {
                break result.expect("latest result");
            }
            assert!(Instant::now() < deadline, "component worker did not finish");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(result.contains_key("latest:Marker"));
        assert_eq!(
            receive_started
                .recv_timeout(Duration::from_secs(5))
                .expect("latest started"),
            "latest"
        );
        assert!(receive_started.try_recv().is_err());
        assert!(!reload.is_busy());
        assert!(reload.ensure_ready().is_ok());
        drop(reload);
        background.join().expect("worker stopped");
    }

    #[test]
    fn closing_a_project_discards_in_flight_progress_results_and_errors() {
        let (events, receive_events) = mpsc::channel();
        let mut reload = ComponentReload {
            events: Some(receive_events),
            busy: true,
            ..Default::default()
        };
        reload.reset();
        events
            .send(Event::Progress(
                0,
                Progress {
                    label: "Old project".into(),
                    completed: 0,
                    total: 1,
                },
            ))
            .expect("progress");
        events
            .send(Event::Finished(0, Ok(types("old"))))
            .expect("result");
        events
            .send(Event::Finished(0, Err("old failure".into())))
            .expect("error");
        assert!(reload.poll().is_none());
        assert!(reload.label.is_empty());
        assert!(!reload.is_busy());
        assert!(reload.error.is_none());
    }

    #[test]
    fn failed_scans_keep_previous_types_and_success_swaps_types_without_losing_edits() {
        let mut editor = Editor::new(None, None);
        editor.world.component_types = types("previous");
        editor.inspector_dirty = true;
        let (events, receive_events) = mpsc::channel();
        editor.component_reload.events = Some(receive_events);
        editor.component_reload.busy = true;
        events
            .send(Event::Finished(0, Err("source syntax error".into())))
            .expect("failure");
        editor.poll_component_types();
        assert!(editor.world.component_types.contains_key("previous:Marker"));
        assert!(editor.component_reload.ensure_ready().is_err());
        assert!(
            editor
                .logs
                .last()
                .is_some_and(|line| line.contains("source syntax error"))
        );

        editor.component_reload.generation = 1;
        editor.component_reload.busy = true;
        events
            .send(Event::Progress(
                1,
                Progress {
                    label: "Inspecting next mod".into(),
                    completed: 1,
                    total: 2,
                },
            ))
            .expect("progress");
        editor.poll_component_types();
        assert_eq!(editor.component_reload.progress, Some(0.5));
        assert!(editor.world.component_types.contains_key("previous:Marker"));
        events
            .send(Event::Finished(1, Ok(types("next"))))
            .expect("success");
        editor.poll_component_types();
        assert!(editor.world.component_types.contains_key("next:Marker"));
        assert!(!editor.world.component_types.contains_key("previous:Marker"));
        assert!(editor.component_reload.ensure_ready().is_ok());
        assert!(editor.inspector_dirty);
        assert!(editor.preview_dirty);
    }

    #[test]
    fn a_stopped_worker_clears_busy_state_and_reports_a_retryable_error() {
        let (events, receive_events) = mpsc::channel();
        let mut reload = ComponentReload {
            events: Some(receive_events),
            busy: true,
            ..Default::default()
        };
        drop(events);
        assert!(reload.poll().is_some_and(|result| result.is_err()));
        assert!(!reload.is_busy());
        assert!(reload.requests.is_none());
        assert!(reload.error.is_some());
    }

    #[test]
    fn a_stopped_background_job_does_not_leave_editor_actions_blocked() {
        let mut editor = Editor::new(None, None);
        let (sender, receiver) = mpsc::channel();
        editor.job = Some(receiver);
        drop(sender);
        editor.poll_jobs();
        assert!(editor.job.is_none());
        assert!(editor.status.contains("stopped unexpectedly"));
        assert!(!editor.logs.is_empty());
    }
}
